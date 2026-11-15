#!/usr/bin/env python3
"""Verify the compiled product UI on Android using private data and own-surface captures."""
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
    return subprocess.check_output(["adb", "-d", *args], timeout=300)


def normal_library_hash():
    result = subprocess.run(["adb", "-d", "shell", "run-as", PACKAGE, "cat", "files/library-v1.json"],
                            capture_output=True, timeout=30)
    if result.returncode == 0:
        return hashlib.sha256(result.stdout).hexdigest()
    assert b"No such file or directory" in result.stderr, result.stderr.decode(errors="replace")
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert re.fullmatch("[a-z0-9][a-z0-9-]{0,47}", args.profile)
    output = args.output.resolve()
    assert not output.is_relative_to(ROOT), "Keep review evidence outside the repository"
    output.mkdir(parents=True, exist_ok=False)
    (output / "qa-script.py").write_bytes(Path(__file__).read_bytes())
    verifier = runpy.run_path(str(ROOT / "scripts/android-qa.py"))
    apk = verifier["verify_payload"](args.artifact, args.commit)
    assert adb("get-state").decode().strip() == "device"
    normal_before = normal_library_hash()
    private = f"files/qa/{args.profile}/"
    exists = subprocess.run(["adb", "-d", "shell", "run-as", PACKAGE, "test", "-e", private],
                            capture_output=True, timeout=30)
    assert exists.returncode == 1, "Use a fresh QA profile, never overwrite existing data"
    adb("install", "-r", str(apk))
    installed = adb("shell", "pm", "path", PACKAGE).decode().strip()
    assert installed.startswith("package:") and "\n" not in installed
    assert hashlib.sha256(adb("exec-out", "cat", installed.removeprefix("package:"))).digest() == hashlib.sha256(apk.read_bytes()).digest()
    adb("install", "-r", "-t", str(args.artifact / "app-debug-androidTest.apk"))
    captures = ["qa-startup", "qa-product-ime"] + [
        f"qa-product-{page}-{theme}-{orientation}"
        for orientation in ("portrait", "landscape")
        for theme in ("dark", "light")
        for page in ("home", "search", "library", "queue", "settings")
    ]
    with verifier["qa_session"](args.profile):
        log = adb("shell", "am", "instrument", "-w", "-e", "mode", "product-ui",
                  "-e", "profile", args.profile,
                  f"{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation")
        (output / "instrumentation.log").write_bytes(log)
        latest = json.loads(adb("shell", "run-as", PACKAGE, "cat", private + "qa-state.json"))
        (output / "latest-state.json").write_text(json.dumps(latest, indent=2) + "\n")
        own_log = adb("logcat", "-d", f"--pid={latest['pid']}", "-t", "1000").decode(errors="replace")
        (output / "own-process.log").write_text(own_log)
        passed = log.decode().startswith("PASS: ") and "FAIL:" not in log.decode()
        for name in captures + ["qa-failure"]:
            snapshot = subprocess.run(["adb", "-d", "shell", "run-as", PACKAGE, "cat", private + name + ".json"],
                                      capture_output=True, timeout=30)
            if snapshot.returncode:
                assert not passed or name == "qa-failure", f"Missing required capture: {name}"
                continue
            assert json.loads(snapshot.stdout)["pid"] == latest["pid"], "Stale process capture"
            image = adb("exec-out", "run-as", PACKAGE, "cat", private + name + ".png")
            assert image.startswith(b"\x89PNG\r\n\x1a\n")
            (output / (name + ".json")).write_bytes(snapshot.stdout)
            (output / (name + ".png")).write_bytes(image)
        assert passed, log.decode()
        critical = [line for line in own_log.splitlines()
                    if re.search(r"FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR", line)]
        assert not critical, "Critical errors retained in own-process.log"
    normal_after = normal_library_hash()
    assert normal_after == normal_before, "Normal library changed during private-profile QA"
    (output / "report.json").write_text(json.dumps({
        "result": "pass", "source_commit": args.commit, "physical_device": True,
        "device": {key: adb("shell", "getprop", key).decode().strip()
                   for key in ("ro.product.model", "ro.build.version.sdk", "ro.product.cpu.abilist")},
        "mode": "product", "private_profile": args.profile, "captures": len(captures),
        "normal_library_sha256": normal_after,
        "checks": ["no internal controls", "four musical filters", "portrait and landscape",
                   "light and dark", "physical IME resize", "no fake playback"],
        "audio": False, "search_requests": False,
    }, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
