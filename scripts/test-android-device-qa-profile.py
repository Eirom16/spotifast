#!/usr/bin/env python3
"""Offline checks for the ADB journey normal-profile preservation rule.

Real user data must survive a journey byte-identically (four data files); the
queue checkpoint must keep identical user content, only its documented
volatile fields may move; the sealed YouTube session may be re-sealed with a
fresh IV but must keep its envelope shape and plaintext size. Runs without a
device; the device fetches are separate functions.
"""
import importlib.util
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("device_qa", ROOT / "scripts/android-playback-device-qa.py")
device_qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(device_qa)

HASHES = "".join(f"{'a' * 64}  files/{name}\n" for name in device_qa.DIGEST_FILES)
CHANGED = HASHES.replace("a" * 64, "b" * 64, 1)
SESSION = {"size": 128, "magic": device_qa.SESSION_MAGIC}


def resealed_session_hashes():
    line = "a" * 64 + "  files/" + device_qa.SESSION_FILE
    return HASHES.replace(line, "c" * 64 + "  files/" + device_qa.SESSION_FILE)

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


def compare(before_queue, after_queue, before_hashes=HASHES, after_hashes=HASHES,
            before_session=SESSION, after_session=SESSION):
    return device_qa.profile_preserved((before_hashes, before_queue, before_session),
                                       (after_hashes, after_queue, after_session))


def test_identical_profile_is_preserved():
    preserved, reason = compare(checkpoint(), checkpoint())
    assert preserved and reason == "", reason


def test_isolated_only_never_reads_or_relaunches_normal_profile_even_after_failure():
    with patch.object(device_qa, "normal_hashes", side_effect=AssertionError("normal read")), \
            patch.object(device_qa, "queue_checkpoint", side_effect=AssertionError("queue read")), \
            patch.object(device_qa, "session_envelope", side_effect=AssertionError("session read")), \
            patch.object(device_qa, "adb") as mocked:
        assert device_qa.normal_snapshot(True) is None
        try:
            with device_qa.playback_session({}, "offline", True):
                raise RuntimeError("synthetic failure")
        except RuntimeError:
            pass
        assert [call.args for call in mocked.call_args_list] == [
            ("shell", "am", "force-stop", device_qa.PACKAGE),
            ("shell", "am", "force-stop", device_qa.PACKAGE)]


def test_session_reseal_is_preserved():
    preserved, reason = compare(checkpoint(), checkpoint(), after_hashes=resealed_session_hashes())
    assert preserved and reason == "", reason


def test_session_size_change_is_refused():
    after = dict(SESSION, size=SESSION["size"] + 1)
    preserved, reason = compare(checkpoint(), checkpoint(), after_session=after)
    assert not preserved and reason == device_qa.SESSION_FILE + " size changed", reason


def test_missing_session_is_refused():
    preserved, reason = compare(checkpoint(), checkpoint(), after_session=None)
    assert not preserved and reason == device_qa.SESSION_FILE + " disappeared", reason


def test_session_magic_is_refused():
    after = dict(SESSION, magic="00 00 00 00")
    preserved, reason = compare(checkpoint(), checkpoint(), after_session=after)
    assert not preserved and reason == device_qa.SESSION_FILE + " is not a sealed envelope", reason


def test_missing_session_on_both_sides_is_preserved():
    preserved, reason = compare(checkpoint(), checkpoint(), before_session=None, after_session=None)
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
