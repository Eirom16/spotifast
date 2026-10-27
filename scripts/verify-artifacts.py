#!/usr/bin/env python3
"""Validate the Linux/Android-core pair. Neither payload is a Doremi release."""

import argparse
import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BUILD = runpy.run_path(str(ROOT / "scripts/build-manifest.py"))
EXPECTED = {
    "legacy-desktop": ("x86_64-unknown-linux-gnu", "spotifast"),
    "core-rlib-not-apk": ("aarch64-linux-android", "libdoremi_core.rlib"),
}
MULTIPLATFORM = {
    **EXPECTED,
    "legacy-desktop-macos": (("aarch64-apple-darwin", "x86_64-apple-darwin"), "spotifast"),
    "legacy-desktop-windows": ("x86_64-pc-windows-msvc", "spotifast.exe"),
}


def verify(directory, commit, version, expected=EXPECTED):
    manifests = sorted(directory.glob("*/manifest.json"))
    if len(manifests) != len(expected):
        raise ValueError(f"Expected exactly {len(expected)} artifact manifests")
    kinds = set()
    for path in manifests:
        data = json.loads(path.read_text())
        kind = data["kind"]
        if kind not in expected or kind in kinds:
            raise ValueError("Missing, duplicate or unexpected artifact kind")
        kinds.add(kind)
        targets, filename = expected[kind]
        targets = (targets,) if isinstance(targets, str) else targets
        if data["target"] not in targets:
            raise ValueError("Unexpected platform target")
        if (data["schema"], data["product"], data["phase"], data["release"],
                data["source_dirty"], data["source_commit"], data["version"], data["target"]) != (
                1, "Doremi", BUILD["phase"](ROOT), False, False, commit, version, data["target"]):
            raise ValueError("Artifact identity does not match the verified source")
        if data["payload"] != BUILD["payload_identity"](path.parent / filename):
            raise ValueError("Artifact hash or filename differs")
    return kinds


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--multiplatform", action="store_true")
    args = parser.parse_args()
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    verify(args.directory, commit, BUILD["version"](ROOT), MULTIPLATFORM if args.multiplatform else EXPECTED)
    print("All validation artifacts match this commit, version and their SHA-256.")


if __name__ == "__main__":
    main()
