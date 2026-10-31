#!/usr/bin/env python3
"""Real XTest/own-window QA of the two audited Commons tones. No microphone."""
import argparse
import ctypes
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--commit", required=True)
parser.add_argument("--local-only", action="store_true", help="Exercise long local WAV without provider requests")
args = parser.parse_args()
binary = args.binary.resolve()
out = args.output.resolve()
assert out.is_relative_to(ROOT / "target")
manifest = json.loads((binary.parent / "manifest.json").read_text())
assert manifest["source_commit"] == args.commit and not manifest["source_dirty"]
assert manifest["product"] == "Doremi" and manifest["target"] == "x86_64-unknown-linux-gnu" and not manifest["release"]
for line in (binary.parent / "SHA256SUMS").read_text().splitlines():
    digest, name = line.split("  ", 1)
    path = (binary.parent / name).resolve()
    assert path.is_relative_to(binary.parent) and hashlib.sha256(path.read_bytes()).hexdigest() == digest
assert manifest["payload"]["sha256"] == hashlib.sha256(binary.read_bytes()).hexdigest()
binary.chmod(binary.stat().st_mode | 0o100)
out.mkdir(parents=True, exist_ok=True)
profile = out / "profile"
assert not profile.exists(), "Use a new evidence directory; preserve earlier results"
profile.mkdir()
(profile / "preferences-v1.json").write_text(json.dumps(dict(schema=1, volume=0.08, library=["dawn"], green=False, light=False)))
env = os.environ.copy()
env.pop("WAYLAND_DISPLAY", None)
env.pop("WAYLAND_SOCKET", None)
env["DOREMI_DATA_DIR"] = str(profile)
process = None
log = None
window = None


def xdo(*arguments):
    return subprocess.check_output(["xdotool", *map(str, arguments)], text=True, timeout=10).strip()


def state():
    data = json.loads((profile / "qa-state.json").read_text())
    assert data["pid"] == process.pid, "Stale process diagnostics"
    return data


def wait(predicate, seconds=14):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        assert process.poll() is None, "Doremi exited"
        try:
            data = state()
            if predicate(data):
                return data
        except (FileNotFoundError, ValueError):
            pass
        time.sleep(0.02)
    raise AssertionError("Expected UI/source state was not reached")


def start():
    global process, window, log
    log = (out / ("runtime-1.log" if process is None else "runtime-2.log")).open("w")
    process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
    window = xdo("search", "--sync", "--onlyvisible", "--pid", process.pid).splitlines()[0]
    xdo("windowfocus", "--sync", window)
    wait(lambda data: bool(data["controls"]))


def tap(name):
    data = wait(lambda data: any(control["name"] == name for control in data["controls"]))
    rect = next(control["rect"] for control in data["controls"] if control["name"] == name)
    left, top, right, bottom = rect
    assert 0 <= left < right <= data["viewport"][0] and 0 <= top < bottom <= data["viewport"][1], name
    assert int(xdo("getwindowfocus")) == int(window), "Own Doremi window must have focus"
    scale = data["pixels_per_point"]
    xdo("mousemove", "--window", window, round((left + right) * scale / 2), round((top + bottom) * scale / 2), "click", 1)
    time.sleep(0.06)


def capture(name):
    assert int(xdo("getwindowfocus")) == int(window)
    subprocess.run(["import", "-window", window, str(out / (name + ".png"))], check=True, timeout=8)
    (out / (name + ".json")).write_text(json.dumps(state(), indent=2))


def playing(data, local):
    return data["current"] == local and data["current_provider"] == "wikimedia-commons" and data["playback"] == "Playing" and data["audio_metrics"] and data["audio_metrics"]["frames"] > 10000


def close():
    at = time.monotonic()
    # Xvfb has no WM to relay _NET_CLOSE_WINDOW. Deliver the standard client
    # close request directly; XDestroyWindow is not a normal application close.
    class Data(ctypes.Union):
        _fields_ = [("bytes", ctypes.c_char * 20), ("shorts", ctypes.c_short * 10), ("longs", ctypes.c_long * 5)]
    class Client(ctypes.Structure):
        _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int), ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong), ("format", ctypes.c_int), ("data", Data)]
    class Event(ctypes.Union):
        _fields_ = [("client", Client), ("padding", ctypes.c_long * 24)]
    library = ctypes.CDLL(ctypes.util.find_library("X11"))
    library.XOpenDisplay.argtypes = [ctypes.c_char_p]
    library.XOpenDisplay.restype = ctypes.c_void_p
    library.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    library.XInternAtom.restype = ctypes.c_ulong
    library.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(Event)]
    library.XFlush.argtypes = [ctypes.c_void_p]
    library.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = library.XOpenDisplay(os.environ["DISPLAY"].encode())
    assert display
    try:
        event = Event()
        event.client.type = 33  # ClientMessage
        event.client.send_event = 1
        event.client.display = display
        event.client.window = int(window)
        event.client.message_type = library.XInternAtom(display, b"WM_PROTOCOLS", 0)
        event.client.format = 32
        event.client.data.longs[0] = library.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        assert library.XSendEvent(display, int(window), 0, 0, ctypes.byref(event)) != 0
        library.XFlush(display)
    finally:
        library.XCloseDisplay(display)
    assert process.wait(timeout=8) == 0, "Window close must shut down normally"
    log.close()
    return (time.monotonic() - at) * 1000


def cpu_ticks():
    raw = Path(f"/proc/{process.pid}/stat").read_text()
    fields = raw[raw.rfind(")") + 2:].split()
    return int(fields[11]) + int(fields[12])


report = dict(source_commit=args.commit, platform="Linux X11 / native default audio", software_rendering=env.get("LIBGL_ALWAYS_SOFTWARE") == "1", acoustic_capture=False, render_rate_measured=False)
def slider(name, fraction):
    data=state();rect=next(control["rect"] for control in data["controls"] if control["name"]==name)
    radius=(rect[3]-rect[1])/2.5*0.75
    x=(rect[0]+radius+(100-2*radius)*fraction)*data["pixels_per_point"]
    y=(rect[1]+rect[3])/2*data["pixels_per_point"]
    assert int(xdo("getwindowfocus"))==int(window)
    xdo("mousemove","--window",window,round(x),round(y),"mousedown",1)
    time.sleep(.08);xdo("mouseup",1)


def local_wav_ui():
    start();tap("long-audio")
    wait(lambda s:s["current"]=="tone-scale-long-v1" and s["playback"]=="Playing" and (s["audio_metrics"] or {}).get("nonzero_samples",0)>0)
    wait(lambda s:s["position_ms"]>23000,32)
    sample=state();metrics=sample["audio_metrics"]
    assert metrics["streaming"] and metrics["pcm_bytes"]==65536 and metrics["buffer_capacity_frames"]==8192
    assert metrics["buffer_underruns"]==0 and metrics["nonzero_samples"]>0 and sample["volume"]>0
    assert not sample["online"]["worker_initialized"]
    capture("long-playing-dark")
    before_ticks=cpu_ticks();at=time.monotonic();time.sleep(1.1);elapsed=time.monotonic()-at
    used=(cpu_ticks()-before_ticks)/os.sysconf("SC_CLK_TCK")
    memory={line.split(":")[0]:int(line.split()[1]) for line in Path(f"/proc/{process.pid}/status").read_text().splitlines() if line.startswith(("VmRSS:","VmHWM:"))}
    report["resource_sample"]=dict(elapsed_ms=elapsed*1000,process_cpu_ms=used*1000,percent_of_one_cpu=used/elapsed*100,rss_kib=memory["VmRSS"],peak_rss_kib=memory["VmHWM"],audio_metrics=state()["audio_metrics"])
    tap("pause");wait(lambda s:s["playback"]=="Paused");time.sleep(.2)
    paused=state()["position_ms"];time.sleep(.4);assert state()["position_ms"]==paused
    tap("settings");tap("light");tap("home");tap("player");capture("long-playing-light")
    slider("seek",.75);wait(lambda s:s["playback"]=="Paused" and 85000<s["position_ms"]<95000)
    tap("play");wait(lambda s:s["playback"]=="Playing" and s["audio_metrics"]["nonzero_samples"]>0);capture("long-after-seek")
    tap("next");wait(lambda s:playing(s,"tone-chord"));tap("previous");wait(lambda s:playing(s,"tone-scale-long-v1"))
    slider("seek",.99);wait(lambda s:playing(s,"tone-chord"),8);capture("long-after-end")
    tap("stop");wait(lambda s:s["playback"]=="Paused" and s["audio_metrics"] is None)
    report["close_ms"]=close();report["result"]="pass"
    report["checks"]=["regular WAV file / bounded 8192-frame channel",">23 seconds delivered without full PCM allocation","positive output samples","no provider initialization","pause / seek / next / previous / EOF / cancel","dark/light UI","decoder shutdown"]
    (out/"report.json").write_text(json.dumps(report,indent=2)+"\n");print(out)


if args.local_only:
    try:
        local_wav_ui()
    except BaseException as error:
        (out/"failure.json").write_text(json.dumps(dict(report,result="failed",error=str(error),last_state=state() if process and process.poll() is None else None),indent=2)+"\n")
        if process and process.poll() is None:process.terminate();process.wait(timeout=8)
        raise
    raise SystemExit(0)


try:
    start()
    tap("search")
    wait(lambda data: data["page"] == "Search")
    tap("source-commons")
    tap("query")
    xdo("type", "--clearmodifiers", "--delay", 15, "sine")
    wait(lambda data: data["online"]["search_status"] == "Pending")
    tap("cancel-search")
    time.sleep(1)
    assert state()["query"] == "" and state()["online"]["search_status"] == "Idle"
    assert not any(control["name"].startswith("select:219") for control in state()["controls"])
    capture("cancelled-search")
    tap("query")
    xdo("type", "--clearmodifiers", "--delay", 15, "sine")
    wait(lambda data: data["online"]["search_status"] == "Ready" and any(control["name"] == "select:21923199" for control in data["controls"]))
    assert any(control["name"] == "save:21923157" for control in state()["controls"])
    capture("results-dark")
    tap("settings"); tap("light"); tap("search")
    wait(lambda data: data["light"] and data["page"] == "Search")
    capture("results-light")
    xdo("windowsize", window, 384, 614)
    wait(lambda data: data["viewport"] == [384.0, 614.0])
    picker = [control["rect"] for control in state()["controls"] if control["name"].startswith("source-")]
    assert len(picker) == 2 and picker[0][1] == picker[1][1]
    assert all(rect[2] - rect[0] >= 48 and rect[3] - rect[1] >= 48 and rect[2] <= 384 for rect in picker)
    assert abs(picker[1][0] - picker[0][2] - 8) < 0.1
    report["narrow_picker_rects"] = picker
    capture("results-narrow")
    xdo("windowsize", window, 440, 800)
    wait(lambda data: data["viewport"] == [440.0, 800.0])
    tap("settings"); tap("dark"); tap("search")
    tap("select:21923157")
    before = wait(lambda data: data["online"]["phase"] == "download", 5)
    at = time.monotonic(); tap("stop")
    wait(lambda data: data["playback"] == "Paused" and not data["online"]["loading"] and data["audio_metrics"] is None)
    report["cancel_observation"] = dict(phase_before=before["online"]["phase"], cancel_to_state_ms=(time.monotonic() - at) * 1000, network_abort_latency_measured=False)
    time.sleep(1)
    assert state()["playback"] == "Paused" and state()["audio_metrics"] is None
    capture("cancelled-load")
    tap("play")
    wait(lambda data: playing(data, "21923157"))
    capture("playing-first")
    before_ticks = cpu_ticks(); at = time.monotonic(); time.sleep(1.1)
    elapsed = time.monotonic() - at
    used = (cpu_ticks() - before_ticks) / os.sysconf("SC_CLK_TCK")
    memory = {line.split(":")[0]: int(line.split()[1]) for line in Path(f"/proc/{process.pid}/status").read_text().splitlines() if line.startswith(("VmRSS:", "VmHWM:"))}
    report["resource_sample"] = dict(elapsed_ms=elapsed * 1000, process_cpu_ms=used * 1000, percent_of_one_cpu=used / elapsed * 100, rss_kib=memory["VmRSS"], peak_rss_kib=memory["VmHWM"], audio_metrics=state()["audio_metrics"], method="own process CPU ticks / sysconf(SC_CLK_TCK)")
    tap("pause"); wait(lambda data: data["playback"] == "Paused")
    time.sleep(0.2); paused = state()["position_ms"]; time.sleep(0.4)
    assert state()["position_ms"] == paused
    tap("play"); wait(lambda data: data["playback"] == "Playing")
    tap("next"); wait(lambda data: playing(data, "21923199"), 4); capture("playing-next")
    tap("previous"); wait(lambda data: playing(data, "21923157"), 4)
    # Previous creates forward-history replay, which correctly precedes manual
    # additions. Start a fresh context for the independent manual-queue trace.
    prior_occurrence = state()["current_occurrence"]
    tap("search"); tap("select:21923157")
    wait(lambda data: playing(data, "21923157") and data["current_occurrence"] != prior_occurrence, 4)
    tap("search"); tap("add:21923157"); tap("add:21923157")
    manual = wait(lambda data: len(data["manual_occurrences"]) == 2)["manual_occurrences"]
    assert manual[0] != manual[1]
    assert state()["upcoming"] == ["21923157", "21923157", "21923199"]
    tap("queue"); capture("duplicate-queue")
    tap("home"); tap("player")
    for index, occurrence in enumerate(manual):
        tap("next")
        wait(lambda data: playing(data, "21923157") and data["current_occurrence"] == occurrence and len(data["manual"]) == 1 - index, 4)
    tap("next"); wait(lambda data: playing(data, "21923199") and not data["manual"], 4)
    tap("stop"); checkpoint = wait(lambda data: data["playback"] == "Paused" and data["audio_metrics"] is None)
    tap("search"); tap("save:21923157"); tap("save:21923157")
    expected_saved = [{"provider":"demo","id":"dawn"},{"provider":"wikimedia-commons","id":"21923157"}]
    wait(lambda data: data["saved_media"] == expected_saved)
    assert state()["current_occurrence"] == checkpoint["current_occurrence"]
    tap("library"); capture("library-dark")
    tap("settings"); tap("light"); tap("library"); capture("library-light")
    tap("forget:21923157")
    wait(lambda data: data["saved_media"] == expected_saved[:1])
    tap("search"); tap("save:21923157")
    wait(lambda data: data["saved_media"] == expected_saved)
    tap("settings"); tap("dark")
    time.sleep(0.3)
    assert json.loads((profile / "preferences-v1.json").read_text())["library"] == ["dawn"]
    assert len(json.loads((profile / "library-v1.json").read_text())["items"]) == 2
    report["library_saved_ids"] = expected_saved
    report["first_close_ms"] = close()
    assert "https://" not in (profile / "queue-v1.json").read_text()
    start()
    restored = wait(lambda data: data["current"] == checkpoint["current"] and data["playback"] == "Paused")
    assert restored["current_occurrence"] == checkpoint["current_occurrence"] and restored["position_ms"] == checkpoint["position_ms"]
    assert not restored["online"]["worker_initialized"] and restored["audio_metrics"] is None
    assert restored["saved_media"] == expected_saved
    tap("player"); tap("play"); wait(lambda data: playing(data, "21923199")); capture("restored-online")
    tap("pause"); tap("library"); capture("restored-library")
    assert state()["online"]["search_source"] == "demo" and state()["online"]["search_status"] == "Idle"
    tap("select:21923157"); wait(lambda data: playing(data,"21923157")); capture("restored-library-audio")
    # A cached source can resolve/download between two UI observations. Use a
    # fresh metadata operation whose Pending state is published before IO.
    tap("search"); tap("source-commons"); tap("query")
    xdo("type", "--clearmodifiers", "--delay", 15, "440 sine")
    closing = wait(lambda data: data["online"]["search_status"] == "Pending", 5)
    report["close_during_source_phase"] = "search-pending"
    report["pending_query_at_close"] = closing["query"]
    report["second_close_ms"] = close()
    for path in out.glob("runtime-*.log"):
        assert not re.search(r"panicked at|FATAL|forgot to call.*destroy|shutdown.*timed out", path.read_text(), re.IGNORECASE), path
    report["result"] = "pass"
    report["checks"] = ["real search / cancel without late results", "dark/light/narrow picker", "cancel during download without late playback", "native PCM play/pause/next/previous", "two distinct duplicate occurrences", "context continuation", "restore paused without automatic network", "resume by queued neutral ID", "qualified favorites / duplicate save / remove / restore / direct playback", "close during IO without orphan process"]
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(out)
except BaseException as error:
    failure = dict(report, result="failed", error=str(error))
    try:
        failure["last_state"] = state()
    except (OSError, ValueError, AssertionError):
        pass
    (out / "failure.json").write_text(json.dumps(failure, indent=2) + "\n")
    raise
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill(); process.wait()
    if log is not None and not log.closed:
        log.close()
