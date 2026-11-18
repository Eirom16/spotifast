#!/usr/bin/env python3
"""Offline checks for the ADB journey normal-profile preservation rule.

Real user data must survive a journey byte-identically (five files) and the
queue checkpoint must keep identical user content: only its documented
volatile fields (playback progress and ID allocation) may move. Runs without
a device; the device fetches are separate functions.
"""
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("device_qa", ROOT / "scripts/android-playback-device-qa.py")
device_qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(device_qa)

HASHES = "".join(f"{'a' * 64}  files/{name}\n" for name in device_qa.NORMAL_FILES)
CHANGED = HASHES.replace("a" * 64, "b" * 64, 1)

CONTEXT = [
    {"duration_ms": None, "id": "GOqU7LwyDmA", "occurrence": 799,
     "provider": "youtube-music", "title": "Clase Aparte"},
    {"duration_ms": None, "id": "JQWqqDAjgkI", "occurrence": 800,
     "provider": "youtube-music", "title": "Saquen Bocina"},
]


def checkpoint(**overrides):
    value = {"context": CONTEXT, "current": {"context_index": 1, "entry": CONTEXT[1]},
             "cursor": 1, "forward": [], "history": [], "manual": [], "order": [0, 1],
             "repeat": "Off", "shuffled": False,
             "position_ms": 17620, "next_entry": 805, "request_watermark": 800}
    value.update(overrides)
    return value


def compare(before_queue, after_queue, before_hashes=HASHES, after_hashes=HASHES):
    return device_qa.profile_preserved((before_hashes, before_queue), (after_hashes, after_queue))


def test_identical_profile_is_preserved():
    preserved, reason = compare(checkpoint(), checkpoint())
    assert preserved and reason == "", reason


def test_volatile_checkpoint_fields_may_move():
    after = checkpoint(position_ms=0, next_entry=806, request_watermark=801)
    preserved, reason = compare(checkpoint(), after)
    assert preserved and reason == "", reason
    assert device_qa.queue_delta(checkpoint(), after) == {
        "position_ms": [17620, 0], "next_entry": [805, 806], "request_watermark": [800, 801]}


def test_queue_content_change_is_refused():
    lost = CONTEXT[1]
    after = checkpoint(context=[CONTEXT[0]])
    preserved, reason = compare(checkpoint(), after)
    assert not preserved and reason == device_qa.QUEUE_FILE + " content changed", reason
    assert lost in checkpoint()["context"]


def test_queue_bookkeeping_field_rename_is_refused():
    """A missing volatile field means schema drift: refuse, don't stay silent."""
    after = checkpoint()
    del after["position_ms"]
    preserved, reason = compare(checkpoint(), after)
    assert not preserved and reason == device_qa.QUEUE_FILE + " lost checkpoint fields", reason


def test_byte_data_change_is_refused():
    preserved, reason = compare(checkpoint(), checkpoint(), after_hashes=CHANGED)
    assert not preserved and reason == "library-v1.json changed", reason


def test_missing_queue_on_both_sides_is_preserved():
    preserved, reason = compare(None, None)
    assert preserved and reason == "", reason


def test_missing_queue_on_one_side_is_refused():
    preserved, reason = compare(checkpoint(), None)
    assert not preserved, "losing the queue file must fail preservation"


def test_digest_map_is_order_independent():
    lines = HASHES.splitlines(keepends=True)
    assert device_qa.digest_map(lines[1] + lines[0]) == device_qa.digest_map(lines[0] + lines[1])


if __name__ == "__main__":
    for name, case in sorted(globals().items()):
        if name.startswith("test_"):
            case()
            print(name, "ok")
