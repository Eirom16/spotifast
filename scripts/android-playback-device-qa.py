#!/usr/bin/env python3
"""Opt-in playback on a verified installed APK, using only a fresh private profile.

This is an independent ADB journey, not a replacement PASS for instrumentation.
Selection-to-observation includes ADB and snapshot polling; it is not acoustic
latency or the new correlated YouTube request clock.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import re
import runpy
import shlex
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "dev.eirom.doremi.dev"
# Byte-stable user data across a journey. The queue checkpoint is compared
# by user content instead: the app re-persists it at every normal launch.
NORMAL_FILES = ("library-v1.json", "playlists-v1.json", "history-v1.json",
                "preferences-v1.json")
# The session is an AES-GCM envelope (MAGIC 'DYM',1 + 12-byte IV + sealed
# cookie). A successful restore re-seals the same cookie with a fresh IV
# (crates/doremi-app/src/youtube_session.rs), so its bytes change by design:
# require a valid envelope of unchanged size and record the re-seal.
SESSION_FILE = "youtube-session-v1.bin"
SESSION_MAGIC = "44 59 4d 01"
DIGEST_FILES = NORMAL_FILES + (SESSION_FILE,)
QUEUE_FILE = "queue-v1.json"
# Documented volatile checkpoint fields: playback progress and ID allocation
# (crates/doremi-core/src/queue/checkpoint.rs). Everything else is user data.
QUEUE_VOLATILE = ("position_ms", "next_entry", "request_watermark")


@contextmanager
def playback_session(verifier, profile, isolated_only):
    """An isolated-only journey never relaunches the ordinary account."""
    if not isolated_only:
        with verifier["qa_session"](profile):
            yield
        return
    adb("shell", "am", "force-stop", PACKAGE)
    try:
        yield
    finally:
        adb("shell", "am", "force-stop", PACKAGE)


def normal_snapshot(isolated_only):
    # Do not even inspect the normal queue or sealed session in this mode.
    if isolated_only:
        return None
    return normal_hashes(), queue_checkpoint(), session_envelope()


def adb(*arguments, timeout=45):
    return subprocess.check_output(["adb", "-d", *map(str, arguments)], timeout=timeout)


def normal_hashes():
    command = "for f in " + " ".join("files/" + name for name in DIGEST_FILES)
    command += '; do if [ -f "$f" ]; then sha256sum "$f"; fi; done'
    return adb("shell", "run-as " + PACKAGE + " sh -c " + shlex.quote(command)).decode()


def queue_checkpoint():
    return json.loads(adb("shell", "run-as", PACKAGE, "cat", "files/" + QUEUE_FILE))


def session_envelope():
    """Size and leading magic of the sealed session, without decrypting it."""
    command = ("f=files/" + SESSION_FILE +
               '; if [ -f "$f" ]; then wc -c < "$f"; head -c 4 "$f" | od -An -tx1; fi')
    lines = [line.strip() for line in
             adb("shell", "run-as " + PACKAGE + " sh -c " + shlex.quote(command)).decode().splitlines()
             if line.strip()]
    if not lines:
        return None
    return {"size": int(lines[0]), "magic": " ".join(lines[1].split())}


def digest_map(text):
    result = {}
    for line in text.splitlines():
        digest, _, name = line.partition("  ")
        if name:
            result[name] = digest
    return result


def queue_projection(checkpoint):
    if checkpoint is None:
        return None
    return {key: value for key, value in checkpoint.items() if key not in QUEUE_VOLATILE}


def queue_delta(before, after):
    if before is None or after is None:
        return None
    return {key: [before.get(key), after.get(key)]
            for key in QUEUE_VOLATILE if before.get(key) != after.get(key)}


def profile_preserved(before, after):
    """Whether real user data survived the journey unchanged.

    before/after are (sha256sum text, queue checkpoint or None, session
    envelope or None). The four NORMAL_FILES must be byte-identical; the queue
    must keep identical user content, while its documented volatile fields may
    move; the sealed session may be re-sealed but must keep its envelope shape
    and size (same plaintext, fresh IV).
    """
    map_before, map_after = digest_map(before[0]), digest_map(after[0])
    for name in NORMAL_FILES:
        path = "files/" + name
        if map_before.get(path) != map_after.get(path):
            return False, name + " changed"
    if queue_projection(before[1]) != queue_projection(after[1]):
        return False, QUEUE_FILE + " content changed"
    if before[1] is not None and after[1] is not None:
        for checkpoint in (before[1], after[1]):
            if not set(QUEUE_VOLATILE) <= set(checkpoint):
                return False, QUEUE_FILE + " lost checkpoint fields"
    session_before, session_after = before[2], after[2]
    if session_before is None or session_after is None:
        if session_before != session_after:
            return False, SESSION_FILE + " disappeared"
        return True, ""
    if (session_before["magic"] != SESSION_MAGIC
            or session_after["magic"] != SESSION_MAGIC):
        return False, SESSION_FILE + " is not a sealed envelope"
    if session_before["size"] < 32 or session_after["size"] < 32:
        return False, SESSION_FILE + " is truncated"
    if session_before["size"] != session_after["size"]:
        return False, SESSION_FILE + " size changed"
    return True, ""


class Journey:
    def __init__(self, args, geometry):
        self.args = args
        self.geometry = geometry
        self.geometry_seen = geometry["viewport"]
        self.private = f"files/qa/{args.profile}/"
        self.report = dict(source_commit=args.commit, physical_device=True,
                           profile=args.profile, acoustic_capture=False,
                           timing_scope="ADB command to first observed non-silent snapshot, not click-to-callback",
                           checks=[], resource_samples=[])

    def state(self):
        value = json.loads(adb("shell", "run-as", PACKAGE, "cat", self.private + "qa-state.json"))
        # Scale or width changes invalidate the coordinate mapping: no input.
        # A height change is the IME overlay; taps stay safe because point()
        # validates each rect against the live viewport and control() settles
        # rects across two reads, so record it and continue.
        if value["pixels_per_point"] != self.geometry["pixels_per_point"]:
            raise AssertionError("Own QA surface scale changed; no input injected")
        if value["viewport"][0] != self.geometry["viewport"][0]:
            raise AssertionError("Own QA surface width changed; no input injected")
        if value["viewport"] != self.geometry_seen:
            self.geometry_seen = value["viewport"]
            self.report.setdefault("geometry_events", []).append(
                dict(viewport=value["viewport"], monotonic_s=time.monotonic()))
        return value

    def wait(self, predicate, seconds=35):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            data = self.state()
            if predicate(data):
                return data
            time.sleep(.15)
        raise AssertionError("Expected playback state not observed")

    def save(self, label, state=None):
        data = self.state() if state is None else state
        (self.args.output / (label + ".json")).write_text(json.dumps(data, indent=2) + "\n")
        return data

    def focus(self):
        windows = adb("shell", "dumpsys", "window", "windows").decode()
        assert any("mCurrentFocus=" in line and PACKAGE + "/" in line for line in windows.splitlines()), "No input outside Doremi"

    def point(self, rect, fraction=.5):
        a, b, c, d = rect
        state = self.state()
        assert 0 <= a < c <= state["viewport"][0] and 0 <= b < d <= state["viewport"][1]
        scale = state["pixels_per_point"]
        origin = self.geometry["qa_surface_origin"]
        return round((a + (c - a) * fraction) * scale + origin[0]), round((b + d) * scale / 2 + origin[1])

    def control(self, name):
        for attempt in range(30):
            data = self.state()
            rect = next((item["rect"] for item in data["controls"] if item["name"] == name), None)
            if rect is None:
                if name in ("volume", "seek", "previous", "stop") and data["page"] != "Player":
                    self.tap("player")
                    continue
                if name in ("local-audio", "long-audio") and data["page"] == "Home":
                    self.tap("demo-examples")
                    continue
                if attempt < 3:
                    time.sleep(.25)
                    continue
                raise AssertionError("Missing own control: " + name)
            page = data["page_rect"]
            fixed = name in ("home", "search", "library", "queue", "settings", "query")
            fixed |= data["page"] != "Player" and name in ("play", "pause", "next", "player")
            if fixed or page[1] <= rect[1] < rect[3] <= page[3]:
                time.sleep(.18)
                fresh = self.state()
                settled = next((item["rect"] for item in fresh["controls"] if item["name"] == name), None)
                if settled == rect:
                    return rect
                continue
            self.focus()
            scale = data["pixels_per_point"]
            x = round((page[0] + page[2]) * scale / 2 + self.geometry["qa_surface_origin"][0])
            origin = self.geometry["qa_surface_origin"][1]
            above = rect[1] < page[1]
            start = page[1] + (page[3] - page[1]) * (.25 if above else .8)
            end = page[1] + (page[3] - page[1]) * (.8 if above else .25)
            adb("shell", "input", "swipe", x, round(start * scale + origin), x, round(end * scale + origin), 250)
            time.sleep(.25)
        raise AssertionError("Own control did not settle in its visible page: " + name)

    def tap(self, name, fraction=.5):
        rect = self.control(name)
        self.focus()
        x, y = self.point(rect, fraction)
        adb("shell", "input", "touchscreen", "swipe", x, y, x, y, 120)
        time.sleep(.25)

    def paused(self):
        self.tap("pause")
        self.wait(lambda s: s["playback"] == "Paused")
        time.sleep(.4)
        before = self.state()
        time.sleep(.7)
        after = self.state()
        assert before["position_ms"] == after["position_ms"], "Paused clock advanced"
        assert before["audio_metrics"]["frames"] == after["audio_metrics"]["frames"], "Paused PCM advanced"
        self.report["checks"].append(dict(test="pause", result="pass", position_ms=after["position_ms"]))

    def playing(self, local, seconds=40):
        return self.wait(lambda s: s["current"] == local and s["playback"] == "Playing"
                         and not s["online"]["loading"] and (s["audio_metrics"] or {}).get("nonzero_samples", 0) > 1000,
                         seconds)

    def resources(self, label):
        data = self.state()
        pid = int(data["pid"])
        raw = adb("shell", "dumpsys", "meminfo", pid).decode()
        (self.args.output / (label + "-meminfo.txt")).write_text(raw)
        match = re.search(r"TOTAL\s+(\d+)\s+(\d+)\s+(\d+)", raw)
        stat = adb("shell", "run-as", PACKAGE, "cat", f"/proc/{pid}/stat").decode()
        fields = stat[stat.rindex(")") + 2:].split()
        sample = dict(label=label, monotonic_s=time.monotonic(), pid=pid,
                      process_cpu_ticks=int(fields[11]) + int(fields[12]),
                      total_pss_kib=int(match[1]) if match else None,
                      metrics=data["audio_metrics"])
        self.report["resource_samples"].append(sample)
        return sample

    def continued(self, local, label):
        first = self.playing(local)
        self.save(label + "-first-pcm", first)
        self.resources(label + "-start")
        observed = time.monotonic()
        timeline = []
        self.report.setdefault("buffer_observations", {})[label] = timeline

        def complete(s):
            metrics = s.get("audio_metrics") or {}
            if len(timeline) < 256:
                timeline.append(dict(
                    # Host polling time since this observation window began,
                    # never interpreted as a device request/underrun timestamp.
                    observed_ms=round((time.monotonic() - observed) * 1000, 3),
                    playback=s["playback"],
                    metrics={key: metrics.get(key) for key in (
                        "request", "frames", "callbacks", "buffering", "buffer_underruns",
                        "reported_xruns", "nonzero_samples", "output_rate")},
                    network={key: (s["online"].get("network") or {}).get(key)
                             for key in ("received_bytes", "requests")}))
            return (s["current"] == local and s["playback"] == "Playing"
                    and metrics.get("output_rate", 0) > 0
                    and metrics.get("frames", 0) >= metrics["output_rate"] * 15)

        end = self.wait(complete, 30)
        assert end["audio_metrics"]["nonzero_samples"] > first["audio_metrics"]["nonzero_samples"]
        self.save(label + "-15s", end)
        self.resources(label + "-15s")
        self.report["checks"].append(dict(test=label, result="pass", metrics=end["audio_metrics"]))

    def search(self, query):
        assert re.fullmatch(r"[A-Za-z0-9 -]{1,128}", query)
        self.tap("search")
        self.tap("source-commons")
        if any(c["name"] == "cancel-search" for c in self.state()["controls"]):
            self.tap("cancel-search")
        self.tap("query")
        adb("shell", "input", "text", query.replace(" ", "%s"))
        adb("shell", "input", "keyevent", 4)
        self.wait(lambda s: s["query"] == query and s["online"]["search_status"] == "Ready", 35)

    def run(self):
        self.wait(lambda s: bool(s["controls"]))
        self.tap("home")
        self.tap("demo-examples")
        self.tap("select:dawn")
        self.tap("pause")
        self.tap("volume", .08)
        self.wait(lambda s: .02 < s["volume"] < .15)
        self.tap("home")
        self.tap("long-audio")
        self.continued("tone-scale-long-v1", "original-native")
        self.paused()
        self.tap("stop")
        self.report["original_smoke"] = "pass"

        for index, (query, local) in enumerate((("Chopin", "26896924"), ("Mendelssohn", "17687586"))):
            self.search(query)
            self.wait(lambda s: any(c["name"] == "select:" + local for c in s["controls"]))
            started = time.monotonic()
            self.tap("select:" + local)
            first = self.playing(local)
            assert first["current_provider"] == "wikimedia-commons" and first["audio_metrics"]["streaming"]
            self.report["checks"].append(dict(test="selection-to-observation", result="pass", media_id=local,
                                              observed_ms=(time.monotonic() - started) * 1000,
                                              engine_first_pcm_us=first["audio_metrics"].get("first_pcm_us"),
                                              engine_first_callback_us=first["audio_metrics"].get("first_callback_us")))
            self.continued(local, "commons-" + local)
            self.paused()
            old = self.state()
            self.tap("seek", .35)
            sought = self.wait(lambda s: s["playback"] == "Paused" and s["position_ms"] > 10_000
                               and not s["online"]["loading"] and s["audio_metrics"] is not None)
            assert sought["position_ms"] > old["position_ms"]
            self.save("commons-" + local + "-seek", sought)
            self.tap("play")
            self.playing(local)
            self.report["checks"].append(dict(test="seek-resume", media_id=local, result="pass", position_ms=sought["position_ms"]))
            if index == 1:
                self.lifecycle(local)
            self.tap("stop")
            self.wait(lambda s: s["audio_metrics"] is None and not s["online"]["loading"])
        self.report["result"] = "pass"

    def lifecycle(self, local):
        before = self.state()
        assert before["playback"] == "Playing", "Background test needs active playback"
        adb("shell", "input", "keyevent", 3)
        # The app stops drawing in the background, so qa-state.json freezes at
        # the last frame; its pause decision is only observable after refocus.
        time.sleep(1.5)
        adb("shell", "am", "start", "-W", "-n", PACKAGE + "/dev.eirom.doremi.MainActivity",
            "--es", "dev.eirom.doremi.qa.profile", self.args.profile)
        resumed = self.wait(lambda s: s["playback"] == "Paused" and s["current"] == local)
        assert resumed["pid"] == before["pid"], "Background pause restarted the process"
        assert resumed["position_ms"] >= before["position_ms"], "Background pause restarted the track"
        self.save("foreground-paused", resumed)
        self.tap("play")
        self.playing(local)
        adb("shell", "am", "force-stop", PACKAGE)
        adb("shell", "am", "start", "-W", "-n", PACKAGE + "/dev.eirom.doremi.MainActivity",
            "--es", "dev.eirom.doremi.qa.profile", self.args.profile)
        restored = self.wait(lambda s: s["pid"] != before["pid"] and s["current"] == local
                             and s["playback"] == "Paused" and s["audio_metrics"] is None)
        assert not restored["online"]["worker_initialized"], "Restoration started network automatically"
        self.save("fresh-process-paused", restored)
        self.tap("play")
        self.playing(local)
        self.report["checks"].append(dict(test="background-and-process-reopen", result="pass",
                                          old_pid=before["pid"], new_pid=restored["pid"]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--geometry", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--isolated-only", action="store_true",
                        help="Never read or relaunch the normal profile; its invariants stay NOT_VERIFIED")
    args = parser.parse_args()
    assert re.fullmatch(r"[a-z0-9][a-z0-9-]{0,47}", args.profile)
    args.output = args.output.resolve()
    assert not args.output.is_relative_to(ROOT), "Evidence must stay outside the worktree"
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "qa-script.py").write_bytes(Path(__file__).read_bytes())
    verifier = runpy.run_path(str(ROOT / "scripts/android-qa.py"))
    apk = verifier["verify_payload"](args.artifact, args.commit)
    installed = adb("shell", "pm", "path", PACKAGE).decode().strip().removeprefix("package:")
    assert installed.endswith("/base.apk") and "\n" not in installed
    digest = hashlib.file_digest(apk.open("rb"), "sha256").hexdigest()
    assert adb("shell", "sha256sum", installed).decode().split()[0] == digest
    private = f"files/qa/{args.profile}/"
    exists = subprocess.run(["adb", "-d", "shell", "run-as", PACKAGE, "sh", "-c",
                             "test -e " + private], capture_output=True, timeout=30)
    assert exists.returncode == 1, "Use a fresh private profile"
    geometry = json.loads(args.geometry.read_text())
    assert len(geometry["qa_surface_origin"]) == 2
    journey = Journey(args, geometry)
    journey.report.update(apk_sha256=digest, device={
        key: adb("shell", "getprop", key).decode().strip()
        for key in ("ro.product.model", "ro.build.version.sdk", "ro.product.cpu.abilist")})
    before = normal_snapshot(args.isolated_only)
    if before is not None:
        (args.output / "normal-before.sha256").write_text(before[0])
    try:
        with playback_session(verifier, args.profile, args.isolated_only):
            adb("shell", "am", "start", "-W", "-n", PACKAGE + "/dev.eirom.doremi.MainActivity",
                "--es", "dev.eirom.doremi.qa.profile", args.profile)
            time.sleep(1)
            try:
                journey.run()
            except BaseException:
                journey.save("failure-state")
                raise
    except BaseException as error:
        journey.report.update(result="failed", error=str(error))
        raise
    finally:
        preserved, reason = None, ""
        after = normal_snapshot(args.isolated_only)
        if before is not None:
            preserved, reason = profile_preserved(before, after)
            journey.report["queue_checkpoint_delta"] = queue_delta(before[1], after[1])
            journey.report["session_resealed"] = (
                before[2] is not None and after[2] is not None
                and digest_map(before[0]).get("files/" + SESSION_FILE)
                != digest_map(after[0]).get("files/" + SESSION_FILE))
            (args.output / "normal-after.sha256").write_text(after[0])
        journey.report["normal_profile_preserved"] = preserved
        journey.report["normal_profile_check"] = "NOT_VERIFIED" if args.isolated_only else "executed"
        (args.output / "report.json").write_text(json.dumps(journey.report, indent=2) + "\n")
        assert preserved is not False, "Normal-profile data changed: " + reason
    print(args.output)


if __name__ == "__main__":
    main()
