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
CAPTURES = ("qa-unicode", "qa-recreated", "qa-landscape", "qa-portrait",
            "qa-shared-queue-trace", "qa-input-background", "qa-audio-playing",
            "qa-audio-interrupted", "qa-audio-after-end")


def adb(*args):
    return subprocess.check_output(["adb", "-d", *args], timeout=120)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    verifier = runpy.run_path(str(ROOT / "scripts/android-qa.py"))
    apk = verifier["verify_payload"](args.artifact, args.commit)
    manifest = json.loads((args.artifact / "manifest.json").read_text())
    installed = adb("shell", "pm", "path", PACKAGE).decode().strip()
    assert installed.startswith("package:") and "\n" not in installed
    installed_path = installed.removeprefix("package:")
    assert installed_path.endswith("/base.apk")
    assert hashlib.sha256(adb("exec-out", "cat", installed_path)).digest() == hashlib.sha256(apk.read_bytes()).digest(), "Installed APK differs from verified CI artifact"
    output = ROOT / "target/android-qa" / args.commit[:12] / "instrumentation"
    output.mkdir(parents=True, exist_ok=True)
    adb("install", "-r", "-t", str(args.artifact / "app-debug-androidTest.apk"))
    log = adb("shell", "am", "instrument", "-w",
              f"{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation")
    (output / "instrumentation.log").write_bytes(log)
    assert log.decode().startswith("PASS: ") and "FAIL:" not in log.decode(), log.decode()
    states = []
    for name in CAPTURES:
        for suffix in ("png", "json"):
            data = adb("exec-out", "run-as", PACKAGE, "cat", f"files/{name}.{suffix}")
            if suffix == "png":
                assert data.startswith(b"\x89PNG\r\n\x1a\n")
            else:
                states.append(json.loads(data))
            (output / f"{name}.{suffix}").write_bytes(data)
    pids = {state["pid"] for state in states}
    assert len(pids) == 1, "Captures must belong to this instrumented process"
    resources = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa-audio-resources.json"))
    (output / "qa-audio-resources.json").write_text(json.dumps(resources, indent=2) + "\n")
    own_log = adb("logcat", "-d", f"--pid={pids.pop()}", "-t", "1000").decode(errors="replace")
    (output / "own-process.log").write_text(own_log)
    critical = [line for line in own_log.splitlines() if re.search(
        r"FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR|"
        r"shutdown.*timed out|forgot to call.*destroy", line, re.IGNORECASE)]
    assert not critical, "Relevant critical errors retained in own-process.log"
    summary = dict(source_commit=args.commit, native_source_commit=manifest.get("native_source_commit"),
                   result="pass", captures=len(CAPTURES), resources=resources,
                   relevant_errors=critical, acoustic_capture=False)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
