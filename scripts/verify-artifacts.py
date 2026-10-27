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


def verify(directory, commit, version):
    manifests = sorted(directory.glob("*/manifest.json"))
    if len(manifests) != 2:
        raise ValueError("Expected exactly two artifact manifests")
    kinds = set()
    for path in manifests:
        data = json.loads(path.read_text())
        kind = data["kind"]
        if kind not in EXPECTED or kind in kinds:
            raise ValueError("Missing, duplicate or unexpected artifact kind")
        kinds.add(kind)
        target, filename = EXPECTED[kind]
        if (data["schema"], data["product"], data["phase"], data["release"],
                data["source_dirty"], data["source_commit"], data["version"], data["target"]) != (
                1, "Doremi", "F0", False, False, commit, version, target):
            raise ValueError("Artifact identity does not match the verified source")
        if data["payload"] != BUILD["payload_identity"](path.parent / filename):
            raise ValueError("Artifact hash or filename differs")
    return kinds


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    verify(args.directory, commit, BUILD["version"](ROOT))
    print("Both validation artifacts match this commit, version and their SHA-256.")


if __name__ == "__main__":
    main()
