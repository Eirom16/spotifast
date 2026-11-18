#!/usr/bin/env python3
"""Offline checks for the ADB journey geometry predicate.

The journey must keep injecting input only while its coordinate mapping is
trustworthy: a scale or width change aborts, an IME height change is recorded
and tolerated. Runs without a device by replacing adb with canned states.
"""
import importlib.util
import json
from pathlib import Path
import types

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("device_qa", ROOT / "scripts/android-playback-device-qa.py")
device_qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(device_qa)

GEOMETRY = {"qa_surface_origin": [0, 45], "viewport": [384, 658.65625], "pixels_per_point": 1.875}
BASE = {"viewport": [384, 658.65625], "pixels_per_point": 1.875}


def journey(states):
    """A Journey whose state() reads canned states instead of adb."""
    args = types.SimpleNamespace(profile="offline", commit="offline", output=Path("/dev/null"))
    tester = device_qa.Journey(args, GEOMETRY)
    feed = iter(states)
    device_qa.adb = lambda *arguments, timeout=45: json.dumps(next(feed)).encode()
    return tester


def expect_abort(states, message):
    try:
        journey(states).state()
    except AssertionError as error:
        assert message in str(error), f"{error!r} does not mention {message!r}"
    else:
        raise AssertionError(f"expected abort: {message}")


def test_unchanged_surface_passes():
    tester = journey([dict(BASE)])
    assert tester.state()["viewport"] == GEOMETRY["viewport"]
    assert "geometry_events" not in tester.report


def test_ime_height_change_is_recorded_and_tolerated():
    ime = dict(BASE, viewport=[384, 320.5])
    tester = journey([dict(BASE), ime, ime, dict(BASE)])
    for _ in range(4):
        tester.state()
    events = tester.report["geometry_events"]
    assert events == [{"viewport": [384, 320.5], "monotonic_s": events[0]["monotonic_s"]},
                      {"viewport": [384, 658.65625], "monotonic_s": events[1]["monotonic_s"]}], events


def test_scale_change_aborts():
    expect_abort([dict(BASE, pixels_per_point=2.0)], "scale changed")


def test_width_change_aborts():
    expect_abort([dict(BASE, viewport=[512, 658.65625])], "width changed")


def test_missing_geometry_keys_are_rejected():
    """Malformed states must abort the journey, not pass it."""
    try:
        journey([{"viewport": [384, 658.65625]}]).state()
    except KeyError as error:
        assert "pixels_per_point" in str(error), error
    else:
        raise AssertionError("expected KeyError without pixels_per_point")
    try:
        journey([{"pixels_per_point": 1.875}]).state()
    except KeyError:
        pass
    else:
        raise AssertionError("expected KeyError without viewport")


if __name__ == "__main__":
    for name, case in sorted(globals().items()):
        if name.startswith("test_"):
            case()
            print(name, "ok")
