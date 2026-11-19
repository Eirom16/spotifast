#!/usr/bin/env python3
"""Capture allowlisted product timings and summarize JSONL without hiding failures."""
import argparse
import json
import math
from pathlib import Path
import re
import statistics

SCENARIOS = ("cold", "warm", "first-after-login", "next", "previously-played",
             "prefetched", "seek")
STAGES = ("requested_us", "queued_us", "worker_us", "resolve_us", "client_wait_us",
          "client_acquired_us", "extraction_us", "extracted_us", "verified_us",
          "http_us", "first_bytes_us", "first_pcm_us", "first_callback_us",
          "first_nonzero_callback_us", "device_start_us", "device_ready_us")
FIELDS = {"schema", "sample_kind", "source_commit", "platform", "scenario",
          "run", "request", "outcome", "times", "acoustic_capture"}
# Differences are computed per request before aggregating. Subtracting two
# percentiles would not measure the distribution of the intervening work.
# These are observed intervals, not exclusive CPU costs (device setup overlaps
# decoding). A missing boundary, including on a prefetch hit, stays unknown.
INTERVALS = {
    "intent_to_worker": ("requested_us", "worker_us"),
    "queue_wait": ("queued_us", "worker_us"),
    "client_wait": ("client_wait_us", "client_acquired_us"),
    "extraction": ("extraction_us", "extracted_us"),
    "first_http_block": ("http_us", "first_bytes_us"),
    "first_block_to_pcm": ("first_bytes_us", "first_pcm_us"),
    "device_setup_including_prebuffer": ("device_start_us", "device_ready_us"),
    "pcm_to_callback": ("first_pcm_us", "first_callback_us"),
    "callback_to_non_silent": ("first_callback_us", "first_nonzero_callback_us"),
    "intent_to_non_silent": ("requested_us", "first_nonzero_callback_us"),
}


def validate(row):
    if set(row) != FIELDS or row["schema"] != 1:
        raise ValueError("Invalid playback record schema")
    if row["sample_kind"] not in ("live-product", "synthetic"):
        raise ValueError("Missing evidence classification")
    if not re.fullmatch(r"[0-9a-f]{40}", row["source_commit"]):
        raise ValueError("Missing exact source commit")
    if row["platform"] not in ("linux", "windows", "android"):
        raise ValueError("Unknown platform")
    if row["scenario"] not in SCENARIOS or row["outcome"] not in ("ok", "error", "cancelled"):
        raise ValueError("Unknown scenario or outcome")
    if not re.fullmatch(r"[a-zA-Z0-9_-]{1,64}", row["run"]):
        raise ValueError("Invalid run label")
    if not re.fullmatch(r"RequestId \{ session: SessionId\(\d+\), generation: \d+ \}", row["request"]):
        raise ValueError("Invalid request identity")
    if row["acoustic_capture"] is not False or set(row["times"]) != set(STAGES):
        raise ValueError("Unsupported timing or acoustic claim")
    for value in row["times"].values():
        if value is not None and (type(value) is not int or value < 0):
            raise ValueError("Invalid monotonic timestamp")
    if row["outcome"] == "ok" and not row["times"]["first_nonzero_callback_us"]:
        raise ValueError("Success requires a measured non-silent callback")
    if row["times"]["requested_us"] != 0:
        raise ValueError("Expected the host intent monotonic origin")
    for start, end in INTERVALS.values():
        a, b = row["times"][start], row["times"][end]
        if a is not None and b is not None and b < a:
            raise ValueError("Inconsistent monotonic timing")
    return row


def capture(snapshot, commit, platform, scenario, run, outcome):
    trace = snapshot["online"]["latency"]
    audio = snapshot.get("audio_metrics") or {}
    if snapshot["current_provider"] != "youtube-music" or trace["origin"] != "host_queue_intent":
        raise ValueError("Expected a traced YouTube product request")
    if outcome == "ok" and (audio.get("request") != trace["request"] or not snapshot["audio"]):
        raise ValueError("Audio belongs to another request")
    if scenario == "prefetched" and not trace["prefetched"]:
        raise ValueError("Request did not consume prefetch")
    times = {name: trace.get(name) for name in STAGES}
    if audio.get("request") == trace["request"]:
        times.update({name: audio.get(name) for name in STAGES if name not in trace})
    return validate(dict(schema=1, sample_kind="live-product", source_commit=commit,
                         platform=platform, scenario=scenario, run=run, request=trace["request"],
                         outcome=outcome, times=times, acoustic_capture=False))


def distribution(values):
    values = sorted(values)
    return dict(n=len(values), min=min(values) if values else None,
                max=max(values) if values else None,
                mean=statistics.mean(values) if values else None,
                p50=values[math.ceil(len(values) * .50) - 1] if values else None,
                p95=values[math.ceil(len(values) * .95) - 1] if values else None)


def summarize(rows):
    groups, seen = {}, set()
    for row in rows:
        validate(row)
        key = (row["sample_kind"], row["source_commit"], row["platform"], row["scenario"])
        identity = (*key[:3], row["run"], row["request"])
        if identity in seen:
            raise ValueError("Duplicate request, not an independent sample")
        seen.add(identity)
        groups.setdefault(key, []).append(row)
    result = []
    for key, samples in sorted(groups.items()):
        successful = [row for row in samples if row["outcome"] == "ok"]
        stages = {}
        for name in STAGES:
            stages[name] = distribution(row["times"][name] for row in successful
                                        if row["times"][name] is not None)
        intervals = {
            name: distribution(row["times"][end] - row["times"][start]
                               for row in successful
                               if row["times"][start] is not None and row["times"][end] is not None)
            for name, (start, end) in INTERVALS.items()
        }
        result.append(dict(sample_kind=key[0], source_commit=key[1], platform=key[2], scenario=key[3],
                           n=len(samples), n_ok=len(successful),
                           n_cancelled=sum(row["outcome"] == "cancelled" for row in samples),
                           n_error=sum(row["outcome"] == "error" for row in samples),
                           failure_rate=1 - len(successful) / len(samples),
                           sufficient_samples=len(successful) >= 20, timestamps_us=stages,
                           intervals_us=intervals))
    return dict(schema=1, groups=result,
                warning="Callback delivery is not acoustic latency. n < 20 is exploratory, not a performance gate.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    snap = sub.add_parser("capture")
    snap.add_argument("snapshot", type=Path)
    snap.add_argument("--commit", required=True)
    snap.add_argument("--platform", choices=("linux", "windows", "android"), required=True)
    snap.add_argument("--scenario", choices=SCENARIOS, required=True)
    snap.add_argument("--run", required=True)
    snap.add_argument("--outcome", choices=("ok", "error", "cancelled"), required=True)
    summary = sub.add_parser("summarize")
    summary.add_argument("samples", type=Path)
    args = parser.parse_args()
    if args.command == "capture":
        result = capture(json.loads(args.snapshot.read_text()), args.commit, args.platform,
                         args.scenario, args.run, args.outcome)
    else:
        result = summarize([json.loads(line) for line in args.samples.read_text().splitlines() if line.strip()])
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
