#!/usr/bin/env python3
"""Small admissible live experiment; no credentials, app profiles or playback.

Fresh processes are measured twice in interleaved order. The second pass is a
repeat, not a promise of warm DNS/TLS/CDN caches. Decoder is identical for both.
"""
import argparse
import array
import hashlib
import json
import math
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def checked_artifact(directory, commit):
    directory = directory.resolve()
    manifest = json.loads((directory / "manifest.json").read_text())
    assert manifest["source_commit"] == commit
    assert manifest["target"] == "x86_64-unknown-linux-gnu"
    assert manifest["experiment"] is True
    for line in (directory / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        path = (directory / name).resolve()
        assert path.is_relative_to(directory) and path.is_file()
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    binary = directory / "doremi-provider-live-probe"
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == manifest["binary_sha256"]
    binary.chmod(binary.stat().st_mode | 0o100)
    assert subprocess.check_output([binary, "--version"], text=True).strip() == manifest["version"]
    return binary


def measured(command, output):
    """wait4 accounts CPU of the process and children it waited for.

    Linux ru_maxrss is the largest individual process, NOT simultaneous group RAM.
    """
    start = time.monotonic()
    with output.open("wb") as stream:
        process = subprocess.Popen(command, cwd=ROOT, stdout=stream,
                                   stderr=subprocess.DEVNULL, start_new_session=True)
        while True:
            pid, status, usage = os.wait4(process.pid, os.WNOHANG)
            if pid:
                process.returncode = os.waitstatus_to_exitcode(status)
                break
            if time.monotonic() - start > 30:
                os.killpg(process.pid, signal.SIGKILL)
                _, status, usage = os.wait4(process.pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
                raise TimeoutError("Provider group terminated after 30 s")
            time.sleep(0.01)
    wall = time.monotonic() - start
    result = json.loads(output.read_text())
    result["process_measurement"] = dict(wall_ms=wall * 1000,
        user_cpu_ms=usage.ru_utime * 1000, system_cpu_ms=usage.ru_stime * 1000,
        largest_process_rss_kib=usage.ru_maxrss, exit_code=process.returncode,
        group_peak_rss_measured=False)
    return result


def decode(payload):
    """First decoded bytes of the verified local file; no audio output device."""
    start = time.monotonic()
    command = ["ffmpeg", "-nostdin", "-v", "error", "-i", str(payload),
               "-t", "10", "-map", "0:a:0", "-ac", "1", "-ar", "48000",
               "-f", "f32le", "pipe:1"]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    data = bytearray()
    first_ms = None
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            while True:
                if time.monotonic() - start > 5:
                    raise TimeoutError("Common decoder exceeded 5 s")
                if not selector.select(timeout=0.1):
                    continue
                block = os.read(process.stdout.fileno(), 4096)
                if not block:
                    break
                if first_ms is None:
                    first_ms = (time.monotonic() - start) * 1000
                data.extend(block)
                if len(data) > 48000 * 10 * 4:
                    raise ValueError("Common decoder exceeded PCM bound")
        assert process.wait(timeout=1) == 0
        assert first_ms is not None and len(data) % 4 == 0
        samples = array.array("f", data)
        assert samples and all(math.isfinite(value) for value in samples)
        assert max(abs(value) for value in samples) > 0.001
        return dict(decoder="ffmpeg common local-file stage", first_pcm_decode_ms=first_ms,
                    pcm_bytes=len(data), sample_rate=48000, channels=1,
                    decoded_duration_ms=len(samples) / 48,
                    decoded_at_most_seconds=10, audio_played=False,
                    audible_audio_ms=None)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        process.stdout.close()


def cancelled(command, phase):
    """Cancel the isolated experimental group after an observed phase marker."""
    process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.DEVNULL,
                               stderr=subprocess.PIPE, start_new_session=True)
    start = time.monotonic()
    observed = False
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(process.stderr, selectors.EVENT_READ)
            while time.monotonic() - start < 12 and process.poll() is None:
                if selector.select(timeout=0.1):
                    if process.stderr.readline().strip() == f"phase={phase}".encode():
                        observed = True
                        break
        assert observed, "Cancellation phase was not reached"
        # Give the resolver time to enter IO/start its extractor. The result
        # records whether a child was actually observed; never infer one.
        time.sleep(0.1)
        def members():
            listing = subprocess.check_output(["ps", "-eo", "pid,pgid,stat"], text=True)
            return [line.split() for line in listing.splitlines()[1:]
                    if line.split()[1] == str(process.pid)]
        before = members()
        assert process.poll() is None, "Operation finished before cancellation"
        at = time.monotonic()
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=2)
        while time.monotonic() - at < 2:
            remaining = members()
            if not remaining:
                break
            time.sleep(0.02)
        live = [row for row in remaining if not row[2].startswith("Z")]
        assert not live, "Live process remains after cancellation"
        return dict(scope="isolated CLI process group", phase=phase,
                    child_observed=any(int(row[0]) != process.pid for row in before),
                    cancel_to_observation_ms=(time.monotonic() - at) * 1000,
                    exit_code=process.returncode, live_processes_remaining=len(live),
                    zombies_remaining=len(remaining) - len(live),
                    app_worker_cancellation_tested=False)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        process.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("extractor", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    binary = checked_artifact(args.artifact, args.commit)
    output = args.output.resolve()
    assert output.is_relative_to(ROOT / "target"), "Evidence must stay under target/"
    output.mkdir(parents=True, exist_ok=True)
    corpus = json.loads((args.artifact / "corpus.json").read_text())
    assert len(corpus) == 2
    rows = []
    report = dict(schema=1, source_commit=args.commit, experiment=True, samples=rows,
                  audio_played=False, cold_warm_cache_benchmark=False, cancellation=[])
    for repeat in range(2):
        for recording in corpus:
            for resolver in ("native", "yt-dlp"):
                command = [str(binary)]
                if resolver == "yt-dlp":
                    command += ["--yt-dlp", str(args.extractor.resolve())]
                command += ["sine", str(recording["page_id"])]
                raw = output / f"{repeat}-{resolver}-{recording['page_id']}.json"
                result = measured(command, raw)
                result.update(repeat=repeat, process_state="fresh",
                              remote_cache_state="uncontrolled", requested_resolver=resolver)
                if result["process_measurement"]["exit_code"] == 0:
                    assert result["sha1"] == recording["sha1"]
                    assert result["download_bytes"] == recording["bytes"]
                    payload = (ROOT / result["payload"]).resolve()
                    assert payload.is_relative_to(ROOT / "target/provider-live")
                    assert hashlib.sha1(payload.read_bytes()).hexdigest() == recording["sha1"]
                    result["common_decode"] = decode(payload)
                rows.append(result)
                (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
                print(resolver, recording["page_id"], result.get("status", "ok"), flush=True)
                time.sleep(1)
    for resolver in ("native", "yt-dlp"):
        command = [str(binary)]
        if resolver == "yt-dlp":
            command += ["--yt-dlp", str(args.extractor.resolve())]
        command += ["sine", "21923157"]
        result = cancelled(command, "resolve")
        result["resolver"] = resolver
        report["cancellation"].append(result)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        time.sleep(1)


if __name__ == "__main__":
    main()
