#!/usr/bin/env python3
"""Light daily loop for low-memory machines; full app validation belongs in CI."""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = [
    [sys.executable, "scripts/test-f0.py"],
    [sys.executable, "scripts/test-ci.py"],
    [sys.executable, "scripts/test-provider-harness.py", "--unit-only"],
    [sys.executable, "scripts/audit-dependencies.py", "--check"],
    [sys.executable, ".github/scripts/check-dependency-sources.py"],
    [sys.executable, ".github/scripts/check-review-evidence.py"],
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "--locked", "-p", "doremi-core"],
]

if __name__ == "__main__":
    for command in COMMANDS:
        result = subprocess.run(command, cwd=ROOT)
        if result.returncode:
            sys.exit(result.returncode)
    print("Light local checks passed. This does not replace full CI or native testing.")
