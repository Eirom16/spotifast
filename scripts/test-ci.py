#!/usr/bin/env python3
"""Offline tests for reporting, runner-only cache bounds and CI separation."""

from pathlib import Path
import json
import os
import runpy
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
REPORT = runpy.run_path(str(ROOT / "scripts/ci-report.py"))
BUDGET = runpy.run_path(str(ROOT / "scripts/ci-cache-budget.py"))
VERIFY = runpy.run_path(str(ROOT / "scripts/verify-artifacts.py"))


class CITests(unittest.TestCase):
    def test_native_manifest_binds_the_new_executable_and_every_license(self):
        previous=Path.cwd()
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            package=root/'crates/doremi-app';package.mkdir(parents=True)
            (package/'Cargo.toml').write_text('[package]\nversion="0.1.0"\n')
            auth_package=root/'crates/doremi-auth';auth_package.mkdir(parents=True)
            (auth_package/'Cargo.toml').write_text('[package]\nversion="0.1.0"\n')
            artifact=root/'artifact';artifact.mkdir()
            binary=artifact/'doremi';binary.write_bytes(b'own synthetic executable')
            helper=artifact/'doremi-auth';helper.write_bytes(b'own synthetic authentication host')
            licenses=artifact/'licenses';licenses.mkdir()
            (licenses/'notice.txt').write_text('preserve the upstream license')
            manifest=artifact/'manifest.json'
            def output(command,**kwargs):
                if command[0]==str(binary.resolve()):return 'Doremi Native Dev 0.1.0-dev\n'
                if command[0]==str(helper.resolve()):return 'Doremi Auth 0.1.0-dev\n'
                if command==['git','status','--porcelain']:return ''
                if command==['git','rev-parse','HEAD']:return 'a'*40+'\n'
                if command==['rustc','-vV']:return 'host: x86_64-unknown-linux-gnu\n'
                raise AssertionError(command)
            try:
                os.chdir(root)
                with patch.object(sys,'argv',['native-manifest.py',str(binary),str(manifest)]),patch('subprocess.check_output',side_effect=output):
                    runpy.run_path(str(ROOT/'scripts/native-manifest.py'))
            finally:os.chdir(previous)
            data=json.loads(manifest.read_text())
            self.assertEqual(data['payload']['file'],'doremi')
            self.assertEqual(data['authentication_helper']['file'],'doremi-auth')
            records=dict(line.split('  ',1)[::-1] for line in (artifact/'SHA256SUMS').read_text().splitlines())
            self.assertEqual(set(records),{'doremi','doremi-auth','licenses/notice.txt','manifest.json'})
            import hashlib
            for relative,digest in records.items():self.assertEqual(hashlib.sha256((artifact/relative).read_bytes()).hexdigest(),digest)

    def test_failure_categories_do_not_turn_errors_into_success(self):
        category = REPORT["failure_category"]
        self.assertIsNone(category(0, "warning"))
        for code, text, expected in [
            (1, "No space left on device", "storage"),
            (127, "command not found", "missing-tool"),
            (101, "test result: FAILED", "tests"),
            (101, "failed to download dependency", "dependency-or-network"),
            (101, "could not compile crate", "compiler-or-lint"),
            (1, "other", "command-failed"),
        ]:
            self.assertEqual(category(code, text), expected)

    def test_budget_removes_generated_outputs_only_and_respects_limit(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            target = root / "target"
            target.mkdir()
            (root / "source.rs").write_text("preserve")
            (target / "large").write_bytes(b"x" * 100)
            (target / "small").write_bytes(b"x" * 10)
            data = BUDGET["trim"](target, 20)
            self.assertEqual(data, {"target_bytes_before": 110, "target_bytes_after": 0, "removed_files": 2})
            self.assertEqual((root / "source.rs").read_text(), "preserve")
            self.assertEqual(list(target.iterdir()), [])

    def test_empty_budget_is_a_noop(self):
        with tempfile.TemporaryDirectory() as name:
            self.assertEqual(BUDGET["trim"](Path(name), 100)["removed_files"], 0)

    def test_command_failure_retains_status_timing_and_log(self):
        previous = Path.cwd()
        with tempfile.TemporaryDirectory() as name:
            try:
                os.chdir(name)
                code = REPORT["run"]("failed", [sys.executable, "-c", "print('failure detail'); raise SystemExit(3)"])
                self.assertEqual(code, 3)
                data = json.loads(Path("target/ci-reports/failed.json").read_text())
                self.assertEqual(data["exit_code"], 3)
                self.assertGreaterEqual(data["duration_seconds"], 0)
                self.assertIn("failure detail", Path("target/ci-reports/failed.log").read_text())
            finally:
                os.chdir(previous)

    def test_multiplatform_set_and_target_are_verified(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for kind, (targets, filename) in VERIFY["MULTIPLATFORM"].items():
                target = targets if isinstance(targets, str) else targets[0]
                directory = root / kind
                directory.mkdir()
                payload = directory / filename
                payload.write_bytes(kind.encode())
                data = {
                    "schema": 1, "product": "Doremi", "phase": VERIFY["BUILD"]["phase"](ROOT), "release": False,
                    "source_dirty": False, "source_commit": "abc", "version": "0.14.2",
                    "target": target, "kind": kind, "payload": VERIFY["BUILD"]["payload_identity"](payload),
                }
                (directory / "manifest.json").write_text(json.dumps(data))
            self.assertEqual(
                VERIFY["verify"](root, "abc", "0.14.2", VERIFY["MULTIPLATFORM"]),
                set(VERIFY["MULTIPLATFORM"]),
            )
            path = root / "legacy-desktop-macos/manifest.json"
            data = json.loads(path.read_text())
            data["target"] = "aarch64-linux-android"
            path.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                VERIFY["verify"](root, "abc", "0.14.2", VERIFY["MULTIPLATFORM"])

    def test_workflow_separates_jobs_and_cache_modes(self):
        text = (ROOT / ".github/workflows/ci.yml").read_text()
        for job in ("quick:", "full-tests:", "clippy:", "desktop-build:", "android-core:", "verified-artifacts:"):
            self.assertIn("  " + job, text)
        self.assertIn("Swatinem/rust-cache@v2", text)
        self.assertIn("linux-dev-codegen", text)
        self.assertIn("linux-dev-metadata", text)
        self.assertIn("cache-targets: false", text)
        self.assertIn("RUSTUP_HOME: ${{ github.workspace }}/target/rustup", text)
        self.assertNotIn("cargo build --release", text)
        self.assertNotIn("self-hosted", text)


if __name__ == "__main__":
    unittest.main()
