#!/usr/bin/env python3
"""Exercise the normal product UI in an isolated X11 profile, without provider IO."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--size", type=int, nargs=2, default=[384, 760])
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    profile = output / "profile"
    env = os.environ.copy()
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DOREMI_INTERNAL_QA"):
        env.pop(key, None)
    env["DOREMI_DATA_DIR"] = str(profile)
    (output / "qa-script.py").write_bytes(Path(__file__).read_bytes())
    with args.binary.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    log = (output / "runtime.log").open("w")
    process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=subprocess.STDOUT)
    window = None

    def xdo(*command):
        return subprocess.check_output(["xdotool", *map(str, command)], text=True, timeout=15).strip()

    def state():
        return json.loads((profile / "qa-state.json").read_text())

    def wait(predicate):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            assert process.poll() is None, "Product window exited unexpectedly"
            try:
                data = state()
                if predicate(data):
                    return data
            except (FileNotFoundError, ValueError):
                pass
            time.sleep(.1)
        raise AssertionError("Expected product state not observed")

    def tap(name):
        data = wait(lambda s: any(c["name"] == name for c in s["controls"]))
        rect = next(c["rect"] for c in data["controls"] if c["name"] == name)
        left, top, right, bottom = rect
        assert 0 <= left < right <= args.size[0] and 0 <= top < bottom <= args.size[1], (name, rect)
        assert int(xdo("getwindowfocus")) == int(window), "Own window lost focus; no input injected"
        scale = data["pixels_per_point"]
        xdo("mousemove", "--window", window, round((left + right) * scale / 2), round((top + bottom) * scale / 2), "click", 1)
        time.sleep(.25)

    def capture(name):
        subprocess.run(["import", "-window", window, str(output / f"{name}.png")], check=True)
        (output / f"{name}.json").write_text(json.dumps(state(), ensure_ascii=False, indent=2))

    forbidden = {"demo-examples", "local-audio", "long-audio", "session-cookie",
                 "youtube-cookie-signin", "youtube-import-session", "cancel-search"}
    try:
        window = xdo("search", "--sync", "--onlyvisible", "--pid", process.pid).splitlines()[0]
        xdo("windowfocus", "--sync", window)
        xdo("windowsize", "--sync", window, *args.size)
        wait(lambda s: all(abs(a - b) < 1 for a, b in zip(s["viewport"], args.size)))
        for theme in ("dark", "light"):
            tap("settings")
            wait(lambda s: s["page"] == "Settings")
            tap(theme)
            wait(lambda s: s["light"] == (theme == "light"))
            for name, page in (("home", "Home"), ("search", "Search"), ("library", "Library"),
                               ("queue", "Queue"), ("settings", "Settings")):
                tap(name)
                data = wait(lambda s: s["page"] == page)
                controls = {c["name"] for c in data["controls"]}
                assert not controls.intersection(forbidden), (page, controls.intersection(forbidden))
                assert not any(c.startswith("source-") for c in controls), page
                if page == "Search":
                    assert data["music_catalog"]["provider"] == "youtube-music"
                    filters = [c for c in data["controls"] if c["name"].startswith("catalog-kind:")]
                    assert len(filters) == 4
                    for control in filters:
                        left, top, right, bottom = control["rect"]
                        assert 0 <= left < right <= args.size[0] and 0 <= top < bottom <= args.size[1]
                    for kind in ("Artist", "Release", "Playlist", "Recording"):
                        tap(f"catalog-kind:{kind}")
                        wait(lambda s: s["music_catalog"]["kind"] == kind and not s["music_catalog"]["waiting"])
                assert data["current"] is None and data["audio_metrics"] is None
                capture(f"{name}-{theme}")
        (output / "report.json").write_text(json.dumps({
            "result": "pass", "binary_sha256": digest, "platform": "Linux X11",
            "viewport": args.size, "mode": "product", "themes": ["dark", "light"],
            "network": False, "audio": False,
            "checks": ["no source selectors", "no QA samples", "no session-cookie editor",
                       "no idle cancel action", "four functional content filters", "no fake playback"],
        }, indent=2) + "\n")
        print(output)
    except Exception:
        if window and process.poll() is None:
            capture("failure")
        raise
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        log.close()


if __name__ == "__main__":
    main()
