#!/usr/bin/env python3
"""Validate synthetic JSONL and summarize its measured stages, never music latency."""

import json
import math
from pathlib import Path
import re
import sys

STAGES = ("search_ms", "resolve_ms", "transport_ms", "total_ms")


def validate(row):
    if row.get("schema") != 1 or row.get("sample_kind") != "synthetic-http-fixture" or row.get("provider") != "fake-http":
        raise ValueError("Expected an explicitly synthetic fixture sample")
    if not re.fullmatch(r"[0-9a-fA-F]{40}", row.get("source_commit", "")):
        raise ValueError("Missing source revision")
    if not isinstance(row.get("source_dirty"), bool):
        raise ValueError("Missing dirty-source status")
    if row.get("temperature") not in ("adapter-cold", "adapter-warm") or row.get("temperature_scope") != "fake_adapter_cache":
        raise ValueError("Unknown cold/warm scope")
    if row.get("startup_included") is not False:
        raise ValueError("This harness does not measure startup")
    if row.get("outcome") not in ("ok", "error") or (row["outcome"] == "ok") != (row.get("error") is None):
        raise ValueError("Hidden or inconsistent failure")
    for field in STAGES:
        value = row.get(field)
        if value is None and row["outcome"] == "error" and field != "total_ms":
            continue
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError("Invalid measured stage")
    if sum(row.get(stage) or 0 for stage in STAGES[:-1]) > row["total_ms"] + 0.01:
        raise ValueError("Impossible stage timings")
    for unmeasured in ("first_pcm_ms", "audible_ms", "cancel_ms", "peak_rss_bytes", "cpu_user_ms", "cpu_system_ms"):
        if unmeasured not in row or row[unmeasured] is not None:
            raise ValueError("Unmeasured audio/resource metric presented as a result")
    encoded = json.dumps(row)
    for secret in ("do-not-log", "fixture-private-cookie", "fixture_token", "http://", "https://"):
        if secret in encoded:
            raise ValueError("Sensitive transport metadata escaped into results")
    for forbidden in ("url", "uri", "headers", "cookies", "snapshot"):
        if forbidden in row:
            raise ValueError("Transport metadata is not a benchmark record")
    return row


def summarize(rows):
    for row in rows:
        validate(row)
    groups = {}
    for row in rows:
        key = f"{row['case_id']}:{row['temperature']}"
        groups.setdefault(key, []).append(row)
    result = {}
    for key, samples in groups.items():
        successful = [row for row in samples if row["outcome"] == "ok"]
        stages = {}
        for stage in STAGES:
            values = sorted(row[stage] for row in successful)
            stages[stage] = {
                "n_success": len(values),
                "p50": values[math.ceil(0.50 * len(values)) - 1] if values else None,
                "p95": values[math.ceil(0.95 * len(values)) - 1] if values else None,
            }
        errors = {}
        for row in samples:
            if row["outcome"] == "error":
                errors[row["error"]] = errors.get(row["error"], 0) + 1
        result[key] = {"n_all": len(samples), "n_failed": len(samples) - len(successful),
                       "errors": errors, "successful_stage_percentiles_only": stages}
    return {"schema": 1, "sample_kind": "synthetic-http-fixture", "groups": result,
            "warning": "Synthetic adapter/loopback timings only. No first PCM, audible latency, CPU or RSS collected."}


if __name__ == "__main__":
    rows = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
    print(json.dumps(summarize(rows), indent=2, sort_keys=True))
