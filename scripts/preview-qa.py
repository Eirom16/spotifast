#!/usr/bin/env python3
"""Real X11/GL window captures and mouse/keyboard smoke tests. Run under Xvfb."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True, timeout=15).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    # winit 0.30 chooses Wayland when these exist; WINIT_UNIX_BACKEND
    # alone does not force X11. Do not send QA input to the user's desktop.
    os.environ.pop("WAYLAND_DISPLAY", None)
    os.environ.pop("WAYLAND_SOCKET", None)
    binary, out = args.binary.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    profile = out / "profile"
    results = []

    def capture(name, page, click=None, keys=False, width=1240):
        path = out / f"{name}.png"
        started = time.monotonic()
        command = [str(binary), "--data-dir", str(profile), "--page", page,
                   "--capture", str(path), "--width", str(width)]
        with (out / f"{name}.log").open("w") as log:
            process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
            try:
                window = xdo("search", "--sync", "--onlyvisible", "--pid", process.pid)
                # One top-level Preview window, not a compositor screenshot.
                window = window.splitlines()[0]
                xdo("windowfocus", "--sync", window)
                # Input after the first native frame, before the 3 s capture.
                xdo("sleep", "0.7")
                if click:
                    x, y = click
                    xdo("mousemove", "--window", window, round(x), round(y), "click", 1)
                if keys:
                    xdo("key", "--window", window, "ctrl+f")
                    xdo("key", "--window", window, "ctrl+a")
                    xdo("type", "--window", window, "--clearmodifiers", "Rosewood")
                    xdo("key", "--window", window, "Return")
                code = process.wait(timeout=40)
                assert code == 0, f"{name}: exit {code}; inspect {name}.log"
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        assert path.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
        state = json.loads(path.with_suffix(".json").read_text())
        assert state["source"] == "actual-egui-OpenGL-framebuffer"
        # Native titlebar/framebuffer sizing differs by display backend.
        assert state["width"] == width and 520 <= state["height"] <= 800
        results.append({"case": name, "seconds": time.monotonic() - started, "state": state})
        return state

    settings = capture("settings-dark-purple", "settings")
    assert settings["preferences"] == {"accent": "Purple", "theme": "dark"}
    green = capture("settings-dark-green", "settings", click=settings["controls"]["Green"])
    assert green["preferences"]["accent"] == "Green"
    restored = capture("settings-green-restored", "settings")
    assert restored["preferences"]["accent"] == "Green"
    light = capture("settings-light-green", "settings", click=restored["controls"]["Light"])
    assert light["preferences"]["theme"] == "light"
    purple = capture("settings-light-purple", "settings", click=light["controls"]["Purple"])
    assert purple["preferences"] == {"accent": "Purple", "theme": "light"}
    capture("home-light", "home")
    capture("settings-dark", "settings", click=purple["controls"]["Dark"])
    capture("home-dark", "home")
    search = capture("search-keyboard", "home", keys=True)
    assert search["page"] == "search" and search["query"] == "Rosewood" and search["tracks"] > 0
    for page, name in [("search", "search-fixtures"), ("albums", "library"),
                       ("playlist:pl1", "playlist"), ("artist:art0", "artist"),
                       ("album:alb0", "album"), ("queue", "queue")]:
        capture(name, page)
    capture("settings-narrow", "settings", width=900)
    preferences = json.loads((profile / "config/preview.json").read_text())
    assert preferences == {"accent": "Purple", "theme": "dark"}
    forbidden = {"settings.json", "session.json", "credentials", "proxy_password", "instance"}
    assert not any(p.name in forbidden for p in profile.rglob("*"))
    (out / "qa-report.json").write_text(json.dumps({
        "source_commit": os.environ.get("GITHUB_SHA"), "renderer": "OpenGL (Xvfb, software Mesa)",
        "results": results, "passed": True,
        "limits": "Not an audible test, a native Wayland test or an Android APK."
    }, indent=2))
    print(f"{len(results)} real window captures; mouse accent/theme, restart persistence and keyboard search passed.")


if __name__ == "__main__":
    main()
