#!/usr/bin/env python3
"""Offline schema checks; full mode builds a small host tool and uses loopback HTTP."""

import copy
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import threading
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SUMMARY = runpy.run_path(str(ROOT / "scripts/provider-bench-summary.py"))
FIXTURES = runpy.run_path(str(ROOT / "scripts/provider-fixtures.py"))


def row():
    return {
        "schema": 1, "sample_kind": "synthetic-http-fixture", "provider": "fake-http",
        "source_commit": "a" * 40, "source_dirty": False, "case_id": "ok",
        "temperature": "adapter-cold", "temperature_scope": "fake_adapter_cache", "startup_included": False,
        "search_ms": 1.0, "resolve_ms": 2.0, "transport_ms": 3.0, "total_ms": 7.0,
        "outcome": "ok", "error": None, "first_pcm_ms": None, "audible_ms": None, "cancel_ms": None,
        "peak_rss_bytes": None, "cpu_user_ms": None, "cpu_system_ms": None,
    }


class SchemaTests(unittest.TestCase):
    def test_sample_and_summary_keep_success_denominators_and_failures(self):
        bad = row()
        bad.update(outcome="error", error="Timeout", resolve_ms=None, transport_ms=None)
        summary = SUMMARY["summarize"]([row(), bad])
        group = summary["groups"]["ok:adapter-cold"]
        self.assertEqual(group["n_failed"], 1)
        self.assertEqual(group["errors"], {"Timeout": 1})
        self.assertEqual(group["successful_stage_percentiles_only"]["total_ms"]["n_success"], 1)

    def test_missing_revision_hidden_failures_secrets_and_audio_claims_are_rejected(self):
        for patch in [
            {"source_commit": ""}, {"error": "Timeout"}, {"audible_ms": 0.1},
            {"uri": "https://private"}, {"total_ms": 0.1}, {"search_ms": float("nan")},
        ]:
            sample = copy.deepcopy(row())
            sample.update(patch)
            with self.assertRaises(ValueError):
                SUMMARY["validate"](sample)


def integration():
    subprocess.run(["cargo", "build", "--locked", "-p", "doremi-provider-harness"], cwd=ROOT, check=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    if not target.is_absolute():
        target = ROOT / target
    binary = target / "debug" / ("doremi-provider-harness.exe" if os.name == "nt" else "doremi-provider-harness")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip())
    output = ROOT / os.environ.get("PROVIDER_BENCH_OUTPUT_DIR", "target/provider-fixtures")
    output.mkdir(parents=True, exist_ok=True)
    expected = {
        "ok": None, "denied": "AccessDenied", "rate-limited": "RateLimited",
        "malformed": "InvalidResponse", "too-many": "TooLarge", "unavailable": "Unavailable",
        "range-ignored": "InvalidResponse", "truncated": "InvalidResponse",
        "oversized": "TooLarge", "stalled": "Timeout",
    }
    rows = []
    with FIXTURES["server"]() as http:
        worker = threading.Thread(target=http.serve_forever, daemon=True)
        worker.start()
        try:
            for case, error in expected.items():
                for mode in ("cold", "warm"):
                    command = [str(binary), "--fixture", f"127.0.0.1:{http.server_address[1]}",
                               "--case", case, "--commit", commit, "--iterations", "2", "--mode", mode,
                               "--budget-ms", "50" if case == "stalled" else "2000"]
                    if dirty:
                        command.append("--dirty")
                    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=10, check=True)
                    samples = [SUMMARY["validate"](json.loads(line)) for line in result.stdout.splitlines()]
                    assert len(samples) == 2
                    for sample in samples:
                        assert sample["source_commit"] == commit
                        assert sample["outcome"] == ("error" if error else "ok"), (case, sample)
                        assert error is None or sample["error"].startswith(error), (case, sample)
                    if case == "ok" and mode == "warm":
                        assert not samples[0]["search_cache_hit"] and samples[1]["search_cache_hit"]
                        assert not samples[0]["resolve_cache_hit"] and samples[1]["resolve_cache_hit"]
                    rows.extend(samples)
            rejected = subprocess.run([str(binary), "--fixture", "192.0.2.1:80"], capture_output=True, text=True)
            assert rejected.returncode != 0 and not rejected.stdout
        finally:
            http.shutdown()
            worker.join(timeout=2)
    (output / "samples.jsonl").write_text("".join(json.dumps(sample, sort_keys=True) + "\n" for sample in rows))
    (output / "summary.json").write_text(json.dumps(SUMMARY["summarize"](rows), indent=2) + "\n")
    (output / "fixture-manifest.json").write_text(json.dumps(FIXTURES["MANIFEST"], indent=2) + "\n")
    print(f"Validated {len(rows)} synthetic samples, including all expected failure cases. Results: {output}")


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SchemaTests)
    if not unittest.TextTestRunner().run(suite).wasSuccessful():
        sys.exit(1)
    if "--unit-only" not in sys.argv:
        integration()
