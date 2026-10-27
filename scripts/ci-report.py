#!/usr/bin/env python3
"""Run CI commands with durable timings/logs, or summarize a runner job."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

REPORTS = Path("target/ci-reports")


def failure_category(code, text):
    if code == 0:
        return None
    text = text.lower()
    for category, patterns in (
        ("storage", ("no space left", "disk quota", "insufficient runner disk")),
        ("missing-tool", ("no such file or directory", "command not found", "not recognized")),
        ("tests", ("test result: failed", "failed (errors=", "failed (failures=")),
        ("dependency-or-network", ("failed to download", "failed to fetch", "could not resolve")),
        ("compiler-or-lint", ("error[", "could not compile", "error:")),
    ):
        if any(pattern in text for pattern in patterns):
            return category
    return "command-failed"


def tree_bytes(path):
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file() and not p.is_symlink()) if path.exists() else 0


def run(name, command):
    REPORTS.mkdir(parents=True, exist_ok=True)
    started = datetime.now(timezone.utc).isoformat()
    before = shutil.disk_usage(".").free
    clock = time.monotonic()
    log_path = REPORTS / f"{name}.log"
    with log_path.open("w", encoding="utf-8") as log:
        try:
            if command[0] == "cargo" and os.environ.get("GITHUB_ACTIONS") == "true" and before < 4 * 2**30:
                raise OSError("Insufficient runner disk: refusing a Cargo job with less than 4 GiB free")
            with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) as process:
                for line in process.stdout:
                    print(line, end="", flush=True)
                    log.write(line)
                code = process.wait()
        except OSError as error:
            log.write(str(error) + "\n")
            print(error, file=sys.stderr)
            code = 127
    data = {
        "name": name, "command": command, "started_utc": started,
        "duration_seconds": round(time.monotonic() - clock, 3),
        "exit_code": code, "failure_category": failure_category(code, log_path.read_text()),
        "free_disk_before_bytes": before, "free_disk_after_bytes": shutil.disk_usage(".").free,
    }
    (REPORTS / f"{name}.json").write_text(json.dumps(data, indent=2) + "\n")
    return code


def finalize(cache_hit, cache_scope):
    REPORTS.mkdir(parents=True, exist_ok=True)
    steps = [json.loads(p.read_text()) for p in sorted(REPORTS.glob("*.json")) if p.name != "job.json"]
    steps = [s for s in steps if "exit_code" in s]
    data = {
        "job": os.environ.get("GITHUB_JOB", "local"),
        "source_commit": os.environ.get("GITHUB_SHA"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "cache_exact_hit": cache_hit or "not-restored",
        "cache_scope": cache_scope,
        "target_bytes": tree_bytes(Path(os.environ.get("CARGO_TARGET_DIR", "target/ci"))),
        "free_disk_bytes": shutil.disk_usage(".").free,
        "steps": steps,
    }
    (REPORTS / "job.json").write_text(json.dumps(data, indent=2) + "\n")
    rows = [
        "## CI job report", "",
        f"Cache scope: `{cache_scope}`. Exact hit: `{data['cache_exact_hit']}`.",
        "A non-exact restore may still reuse an older lockfile cache.",
        f"Target: {data['target_bytes'] / 2**30:.2f} GiB; free disk: {data['free_disk_bytes'] / 2**30:.2f} GiB.",
        "", "| Command | Seconds | Exit | Failure category |", "|---|---:|---:|---|",
    ]
    rows += [f"| {s['name']} | {s['duration_seconds']} | {s['exit_code']} | {s['failure_category'] or 'none'} |" for s in steps]
    text = "\n".join(rows) + "\n"
    (REPORTS / "job.md").write_text(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as output:
            output.write(text)
    print(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    step = sub.add_parser("run")
    step.add_argument("name")
    step.add_argument("command", nargs=argparse.REMAINDER)
    final = sub.add_parser("finalize")
    final.add_argument("--cache-hit", default="")
    final.add_argument("--cache-scope", default="none")
    args = parser.parse_args()
    if args.mode == "run":
        command = args.command[1:] if args.command[:1] == ["--"] else args.command
        if not command or not args.name.replace("-", "").replace("_", "").isalnum():
            parser.error("A safe step name and a command are required")
        return run(args.name, command)
    finalize(args.cache_hit, args.cache_scope)
    return 0


if __name__ == "__main__":
    sys.exit(main())
