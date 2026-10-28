#!/usr/bin/env python3
"""Lightweight Preview asset/provenance and isolated packaging regressions."""
import hashlib
from pathlib import Path
import subprocess
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PreviewTests(unittest.TestCase):
    def test_original_images_are_preserved_byte_for_byte_with_their_license(self):
        expected = {
            "Doremi.png": "b133848e0dbe8f19c144fa8fc1a0bcddf0973c25f4593c34c0aad9db016f47e4",
            "Doremi_Withoth_Background.png": "f0b46d2984d4b71c3205399249b7240d8da5633d443e681a8bd7e3c63b6b0eff",
            "icon.ico": "9ffe6198dd9fe8f0666a115475af9907583ae09665afd7568840b57895054653",
        }
        for name, digest in expected.items():
            self.assertEqual(hashlib.sha256((ROOT / "assets/doremi" / name).read_bytes()).hexdigest(), digest)
        self.assertIn("GNU GENERAL PUBLIC LICENSE", (ROOT / "assets/doremi/LICENSE").read_text())
        self.assertIn("1e44a21f883810a8337b31180ada9bf0fb0d4daf",
                      (ROOT / "assets/doremi/PROVENANCE.md").read_text())

    def test_separate_preview_does_not_change_the_legacy_default_command_or_version(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(manifest["package"]["default-run"], "spotifast")
        self.assertEqual(manifest["package"]["version"], "0.14.2")
        self.assertIn({"name": "doremi-preview", "path": "src/doremi-preview.rs",
                       "required-features": ["demo"]}, manifest["bin"])

    def test_preview_does_not_add_a_logo_panel_to_the_original_content(self):
        source = (ROOT / "src/doremi-preview.rs").read_text()
        body = source.split("    fn ui(", 1)[1].split("    fn persist_egui_memory", 1)[0]
        self.assertIn("self.app.frame_ui(ui);", body)
        for extra in ("egui::Panel", "egui::Image", "ui.label", "horizontal_wrapped"):
            self.assertNotIn(extra, body)
        self.assertIn(".with_icon(egui::IconData", source)
        self.assertIn('include_bytes!("../assets/doremi/Doremi.png")', source)

    def test_portable_launcher_is_valid_shell_without_installation(self):
        launcher = ROOT / "packaging/preview/run.sh"
        subprocess.run(["sh", "-n", str(launcher)], check=True)
        self.assertIn("doremi-preview-data", launcher.read_text())
        self.assertNotIn("spotifast", launcher.read_text().lower())

    def test_preview_job_reuses_the_test_binary_and_does_not_pollute_legacy_payload_set(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        preview = workflow.split("\n  preview-linux:\n", 1)[1].split("\n  verified-artifacts:", 1)[0]
        self.assertIn("needs: full-tests", preview)
        self.assertNotIn("cargo build", preview)
        self.assertNotIn("Swatinem/rust-cache", preview)
        self.assertIn("scripts/preview-qa.py", preview)
        self.assertIn("doremi-preview-0.1.0-linux-x86_64-", preview)
        self.assertNotIn("name: payload-", preview)


if __name__ == "__main__":
    unittest.main()
