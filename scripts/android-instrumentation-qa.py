#!/usr/bin/env python3
"""Install verified Doremi test APK and retain only its own lifecycle evidence.

The main APK must already be installed. Never clear data or collect other apps.
An adb exit status alone is insufficient: require the instrumented PASS marker.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "dev.eirom.doremi.dev"
CAPTURES = ("qa-startup", "qa-unicode", "qa-recreated", "qa-landscape", "qa-portrait",
            "qa-shared-queue-trace", "qa-input-background", "qa-audio-playing",
            "qa-audio-interrupted", "qa-audio-after-end", "qa-online-results-dark",
            "qa-online-results-light", "qa-online-cancelled", "qa-online-playing",
            "qa-online-recreated", "qa-online-background", "qa-library-saved", "qa-library-result", "qa-long-playing", "qa-long-after-seek", "qa-long-after-end", "qa-long-recreated")


def adb(*args):
    return subprocess.check_output(["adb", "-d", *args], timeout=240)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--attempt", default="", help="New evidence suffix after a diagnosed terminal failure")
    args = parser.parse_args()
    assert re.fullmatch(r"[a-z0-9-]{0,32}", args.attempt)
    verifier = runpy.run_path(str(ROOT / "scripts/android-qa.py"))
    apk = verifier["verify_payload"](args.artifact, args.commit)
    manifest = json.loads((args.artifact / "manifest.json").read_text())
    installed = adb("shell", "pm", "path", PACKAGE).decode().strip()
    assert installed.startswith("package:") and "\n" not in installed
    installed_path = installed.removeprefix("package:")
    assert installed_path.endswith("/base.apk")
    assert hashlib.sha256(adb("exec-out", "cat", installed_path)).digest() == hashlib.sha256(apk.read_bytes()).digest(), "Installed APK differs from verified CI artifact"
    output = ROOT / "target/android-qa" / args.commit[:12] / ("instrumentation" + ("-" + args.attempt if args.attempt else ""))
    output.mkdir(parents=True, exist_ok=False)
    adb("install", "-r", "-t", str(args.artifact / "app-debug-androidTest.apk"))
    with verifier["qa_session"]("instrumentation"):
        adb("shell", "am", "force-stop", PACKAGE)
        log = adb("shell", "am", "instrument", "-w",
                  f"{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation")
        (output / "instrumentation.log").write_bytes(log)
        # Retain failure diagnostics before evaluating PASS. Read only Doremi's
        # own versioned QA state and process-filtered log, never the global log.
        latest = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-state.json"))
        (output / "latest-state.json").write_text(json.dumps(latest, indent=2) + "\n")
        own_log = adb("logcat", "-d", f"--pid={int(latest['pid'])}", "-t", "1000").decode(errors="replace")
        (output / "own-process.log").write_text(own_log)
        if not log.decode().startswith("PASS: ") or "FAIL:" in log.decode():
            # Capture was taken inside our still-running Activity before finish.
            # Reject stale diagnostics from an earlier instrumented process.
            try:
                failure = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-failure.json"))
                if failure["pid"] == latest["pid"]:
                    (output / "qa-failure.json").write_text(json.dumps(failure, indent=2) + "\n")
                    (output / "qa-failure.png").write_bytes(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-failure.png"))
                    pointer = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-input-delivery.json"))
                    if pointer.get("pid") == latest["pid"]:
                        (output / "qa-input-delivery.json").write_text(json.dumps(pointer, indent=2) + "\n")
            except (subprocess.CalledProcessError, ValueError, KeyError):
                pass
        assert log.decode().startswith("PASS: ") and "FAIL:" not in log.decode(), log.decode()
        states = []
        for name in CAPTURES:
            for suffix in ("png", "json"):
                data = adb("exec-out", "run-as", PACKAGE, "cat", f"files/qa/instrumentation/{name}.{suffix}")
                if suffix == "png":
                    assert data.startswith(b"\x89PNG\r\n\x1a\n")
                else:
                    states.append(json.loads(data))
                (output / f"{name}.{suffix}").write_bytes(data)
        pids = {state["pid"] for state in states}
        assert len(pids) == 1, "Captures must belong to this instrumented process"
        resources = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-audio-resources.json"))
        (output / "qa-audio-resources.json").write_text(json.dumps(resources, indent=2) + "\n")
        own_log = adb("logcat", "-d", f"--pid={pids.pop()}", "-t", "1000").decode(errors="replace")
        (output / "own-process.log").write_text(own_log)
        critical = [line for line in own_log.splitlines() if re.search(
            r"FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR|"
            r"shutdown.*timed out|forgot to call.*destroy", line, re.IGNORECASE)]
        assert not critical, "Relevant critical errors retained in own-process.log"
        online_resources = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-online-resources.json"))
        (output / "qa-online-resources.json").write_text(json.dumps(online_resources, indent=2) + "\n")
        for sample in (resources, online_resources):
            assert 0.02 < sample["volume"] < 0.15, "Output probe must have positive, low application gain"
            assert sample["playback"] == "Playing" and sample["audio_metrics"]["frames"] > 0
        assert online_resources["current_provider"] == "wikimedia-commons"
        library_checks = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-library-checks.json"))
        (output / "qa-library-checks.json").write_text(json.dumps(library_checks, indent=2) + "\n")
        assert library_checks["duplicate_save_unique"] and library_checks["legacy_archive_preserved"]
        long_resources = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa/instrumentation/qa-long-resources.json"))
        (output / "qa-long-resources.json").write_text(json.dumps(long_resources, indent=2) + "\n")
        assert long_resources["audio_metrics"]["streaming"] and long_resources["audio_metrics"]["pcm_bytes"] == 65536
        assert long_resources["audio_metrics"]["buffer_underruns"] == 0 and long_resources["audio_metrics"]["nonzero_samples"] > 0
        device = {key: adb("shell", "getprop", key).decode().strip() for key in
                  ("ro.product.manufacturer", "ro.product.model", "ro.build.version.sdk", "ro.product.cpu.abilist")}
        summary = dict(source_commit=args.commit, native_source_commit=manifest.get("native_source_commit"),
                       physical_device=True, device=device,
                       result="pass", captures=len(CAPTURES), resources=resources, online_resources=online_resources, long_resources=long_resources,
                       relevant_errors=critical, library_checks=library_checks, acoustic_capture=False)
        (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(output)


if __name__ == "__main__":
    main()
