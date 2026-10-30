#!/usr/bin/env python3
"""Restart only Doremi Dev and test its persisted favorite in a fresh process."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "dev.eirom.doremi.dev"


def adb(*args):
    return subprocess.check_output(["adb", "-d", *args], timeout=120)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    verifier = runpy.run_path(str(ROOT / "scripts/android-qa.py"))
    apk = verifier["verify_payload"](args.artifact, args.commit)
    installed = adb("shell", "pm", "path", PACKAGE).decode().strip()
    assert installed.startswith("package:") and "\n" not in installed
    assert hashlib.sha256(adb("exec-out", "cat", installed.removeprefix("package:"))).digest() == hashlib.sha256(apk.read_bytes()).digest()
    output = ROOT / "target/android-qa" / args.commit[:12] / "library-restore"
    output.mkdir(parents=True, exist_ok=False)
    adb("install", "-r", "-t", str(args.artifact / "app-debug-androidTest.apk"))
    before = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa-state.json"))
    adb("shell", "am", "force-stop", PACKAGE)
    log = adb("shell", "am", "instrument", "-w", "-e", "mode", "library-restore",
              f"{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation")
    (output / "instrumentation.log").write_bytes(log)
    latest = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/qa-state.json"))
    (output / "latest-state.json").write_text(json.dumps(latest, indent=2) + "\n")
    own_log = adb("logcat", "-d", f"--pid={int(latest['pid'])}", "-t", "1000").decode(errors="replace")
    (output / "own-process.log").write_text(own_log)
    assert log.decode().startswith("PASS: ") and "FAIL:" not in log.decode(), log.decode()
    states = []
    for name in ("qa-library-restored", "qa-library-restored-audio"):
        for suffix in ("png", "json"):
            data = adb("exec-out", "run-as", PACKAGE, "cat", f"files/{name}.{suffix}")
            if suffix == "png":
                assert data.startswith(b"\x89PNG\r\n\x1a\n")
            else:
                states.append(json.loads(data))
            (output / f"{name}.{suffix}").write_bytes(data)
    restored, playing = states
    assert restored["pid"] == playing["pid"] == latest["pid"] != before["pid"]
    assert not restored["online"]["worker_initialized"] and restored["audio_metrics"] is None
    assert restored["saved_media"] == before["saved_media"]
    assert playing["current_provider"] == "wikimedia-commons" and playing["current"] == "21923157"
    assert playing["online"]["search_source"] == "demo" and playing["online"]["search_status"] == "Idle"
    assert 0.02 < playing["volume"] < 0.15 and playing["audio_metrics"]["frames"] > 0
    critical = [line for line in own_log.splitlines() if re.search(
        r"FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR|"
        r"shutdown.*timed out|forgot to call.*destroy", line, re.IGNORECASE)]
    assert not critical, "Relevant critical errors retained in own-process.log"
    summary = dict(source_commit=args.commit, result="pass", captures=2,
                   fresh_process=True, automatic_network=False, playback_without_search=True,
                   volume=playing["volume"], audio_metrics=playing["audio_metrics"],
                   relevant_errors=critical, acoustic_capture=False)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
