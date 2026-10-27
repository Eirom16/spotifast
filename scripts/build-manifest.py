#!/usr/bin/env python3
"""Record validation artifact identity. Does not build, sign or publish releases."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def version(root):
    baseline = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    core = tomllib.loads((root / "crates/doremi-core/Cargo.toml").read_text())["package"]["version"]
    if core != baseline:
        raise ValueError("Core and baseline versions differ")
    return baseline


def payload_identity(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError("Payload must be a regular file, not a symlink")
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return {"file": path.name, "sha256": digest.hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--kind", required=True, choices=(
        "legacy-desktop", "legacy-desktop-macos", "legacy-desktop-windows", "core-rlib-not-apk",
    ))
    parser.add_argument("--payload", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT, text=True
    ).strip())
    manifest = {
        "schema": 1,
        "product": "Doremi",
        "phase": "F0",
        "version": version(ROOT),
        "source_commit": sha,
        "source_dirty": dirty,
        "target": args.target,
        "kind": args.kind,
        "release": False,
        "payload": payload_identity(args.payload),
    }
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
