#!/usr/bin/env python3
"""Summarize actual GitHub jobs (including post-cache steps), not estimates."""

from datetime import datetime
import json
import os
from pathlib import Path
import sys


def seconds(start, end):
    if not start or not end:
        return None
    return (datetime.fromisoformat(end.replace("Z", "+00:00")) -
            datetime.fromisoformat(start.replace("Z", "+00:00"))).total_seconds()


def main(directory):
    jobs = json.loads((directory / "github-jobs.json").read_text())["jobs"]
    cache = json.loads((directory / "cache-usage-after.json").read_text())
    rows = [
        "# Actual CI run", "",
        "| Job | Conclusion | Seconds |", "|---|---|---:|",
    ]
    for job in jobs:
        rows.append(f"| {job['name']} | {job.get('conclusion') or job['status']} | {seconds(job.get('started_at'), job.get('completed_at'))} |")
        failed = [s["name"] for s in job.get("steps", []) if s.get("conclusion") in ("failure", "cancelled", "timed_out")]
        if failed:
            rows.append(f"\nFailure steps: {', '.join(failed)}. See job logs; no cause inferred from a skipped job.\n")
    rows += [
        "", f"Repository cache usage: {cache['active_caches_size_in_bytes'] / 2**30:.2f} GiB "
        f"({cache['active_caches_count']} entries).",
        "The cache restore/save step durations and conclusions are retained in github-jobs.json.",
        "Per-command logs, failure categories, disk usage and exact cache hits are in each job report.",
        "The aggregation job itself may still be in progress in this API snapshot.", "",
    ]
    text = "\n".join(rows)
    (directory / "summary.md").write_text(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as out:
            out.write(text)
    print(text)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
