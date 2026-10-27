#!/usr/bin/env python3
"""Offline regression checks for the F0 structure and artifact identity."""

import hashlib
import json
from pathlib import Path
import runpy
import sys
import tempfile
import tomllib
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
BUILD = runpy.run_path(str(ROOT / "scripts/build-manifest.py"))
AUDIT = runpy.run_path(str(ROOT / "scripts/audit-dependencies.py"))
VERIFY = runpy.run_path(str(ROOT / "scripts/verify-artifacts.py"))


class F0Tests(unittest.TestCase):
    def test_workspace_preserves_the_legacy_entrypoints(self):
        data = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(data["workspace"]["members"], [".", "crates/doremi-core"])
        self.assertEqual(data["workspace"]["default-members"], data["workspace"]["members"])
        self.assertEqual(data["package"]["name"], "spotifast")
        self.assertEqual(data["lib"]["path"], "src/lib.rs")
        self.assertEqual(data["bin"], [{"name": "spotifast", "path": "src/spotifast.rs"}])

    def test_core_has_no_dependency_or_build_script(self):
        directory = ROOT / "crates/doremi-core"
        data = tomllib.loads((directory / "Cargo.toml").read_text())
        self.assertEqual(set(data), {"package"})
        self.assertFalse((directory / "build.rs").exists())
        self.assertFalse(data["package"]["publish"])

    def test_versions_and_toolchains_match(self):
        legacy = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
        core = tomllib.loads((ROOT / "crates/doremi-core/Cargo.toml").read_text())["package"]
        self.assertEqual(BUILD["version"](ROOT), legacy["version"])
        for field in ("edition", "rust-version", "license"):
            self.assertEqual(core[field], legacy[field])
        self.assertEqual(legacy["repository"], "https://github.com/crmne/spotifast")
        self.assertEqual(core["repository"], legacy["metadata"]["doremi"]["repository"])
        pin = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        self.assertEqual(pin, legacy["rust-version"] + ".0")

    def test_original_mit_is_unchanged(self):
        expected = "MIT License\n\nCopyright (c) 2026 Carmine Paolino\n"
        license_text = (ROOT / "LICENSE").read_text()
        self.assertTrue(license_text.startswith(expected))
        self.assertIn("The above copyright notice and this permission notice", license_text)
        for relative in ("assets/fonts/Inter-LICENSE.txt", "assets/fonts/NotoEmoji-LICENSE.txt",
                         "assets/icons/LICENSE.txt"):
            self.assertTrue((ROOT / relative).is_file())

    def test_inventory_is_current(self):
        self.assertEqual((ROOT / AUDIT["INVENTORY"]).read_text(), AUDIT["render"](ROOT))

    def test_only_validation_workflow_is_active(self):
        active = sorted(p.name for p in (ROOT / ".github/workflows").glob("*.y*ml"))
        self.assertEqual(active, ["ci.yml"])
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        for forbidden in ("contents: write", "pull_request_target:", "secrets.", "gh release",
                          "pages: write", "publish = true"):
            self.assertNotIn(forbidden, workflow)
        self.assertIn("aarch64-linux-android", workflow)
        self.assertIn("needs: [quick, full-tests, clippy, desktop-build, android-core]", workflow)

    def test_payload_hash_and_missing_file(self):
        with tempfile.TemporaryDirectory() as name:
            path = Path(name) / "core.rlib"
            path.write_bytes(b"artifact")
            self.assertEqual(BUILD["payload_identity"](path), {
                "file": "core.rlib", "sha256": hashlib.sha256(b"artifact").hexdigest(),
            })
            with self.assertRaises(ValueError):
                BUILD["payload_identity"](path.with_name("missing"))
            link = path.with_name("link")
            link.symlink_to(path)
            with self.assertRaises(ValueError):
                BUILD["payload_identity"](link)

    def test_version_drift_is_rejected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / "crates/doremi-core").mkdir(parents=True)
            (root / "Cargo.toml").write_text('[package]\nversion = "1.0.0"\n')
            (root / "crates/doremi-core/Cargo.toml").write_text('[package]\nversion = "2.0.0"\n')
            with self.assertRaisesRegex(ValueError, "versions differ"):
                BUILD["version"](root)

    def test_pair_requires_matching_commit_version_hash_and_clean_sources(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for kind, (target, filename) in VERIFY["EXPECTED"].items():
                directory = root / kind
                directory.mkdir()
                payload = directory / filename
                payload.write_bytes(kind.encode())
                data = {
                    "schema": 1, "product": "Doremi", "phase": "F0", "release": False,
                    "source_dirty": False, "source_commit": "abc", "version": "0.14.2",
                    "target": target, "kind": kind, "payload": BUILD["payload_identity"](payload),
                }
                (directory / "manifest.json").write_text(json.dumps(data))
            self.assertEqual(VERIFY["verify"](root, "abc", "0.14.2"), set(VERIFY["EXPECTED"]))
            for commit, version in (("other", "0.14.2"), ("abc", "0.14.3")):
                with self.assertRaises(ValueError):
                    VERIFY["verify"](root, commit, version)
            manifest = root / "legacy-desktop/manifest.json"
            original = manifest.read_text()
            data = json.loads(original)
            data["source_dirty"] = True
            manifest.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                VERIFY["verify"](root, "abc", "0.14.2")
            manifest.write_text(original)
            (root / "legacy-desktop/spotifast").write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                VERIFY["verify"](root, "abc", "0.14.2")
            manifest.unlink()
            with self.assertRaises(ValueError):
                VERIFY["verify"](root, "abc", "0.14.2")


if __name__ == "__main__":
    unittest.main()
