#!/usr/bin/env python3
"""Bound generated target data before rust-cache saves it, only on hosted CI."""

import argparse
import json
import os
from pathlib import Path
import shutil
import sys


def trim(target, limit):
    # No sources, payloads, reports, Cargo home or remote caches are removed.
    files = [p for p in target.rglob("*") if p.is_file() and not p.is_symlink()]
    before = sum(p.stat().st_size for p in files)
    if before > limit:
        # Never remove individual native OUT_DIR libraries while leaving their
        # build-script fingerprints intact. An oversized target cache is dropped
        # as one coherent unit; rust-cache can still save downloads.
        shutil.rmtree(target)
        target.mkdir()
    return {
        "target_bytes_before": before,
        "target_bytes_after": 0 if before > limit else before,
        "removed_files": len(files) if before > limit else 0,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mib", type=int, required=True)
    args = parser.parse_args()
    root = Path.cwd().resolve()
    target = root / "target/ci"
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        parser.error("Refusing to trim outside an ephemeral GitHub-hosted runner")
    if target.is_symlink() or target.resolve() != root / "target/ci" or args.mib < 1:
        parser.error("Unsafe cache target or budget")
    data = trim(target, args.mib * 2**20)
    output = root / "target/ci-reports/cache-budget.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(data, indent=2) + "\n")
    print(json.dumps(data))
    return 0


if __name__ == "__main__":
    sys.exit(main())
