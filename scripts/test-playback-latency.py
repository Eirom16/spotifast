#!/usr/bin/env python3
"""Offline statistical contracts. Numbers here are synthetic, never real playback."""
import copy
from pathlib import Path
import runpy
import unittest

MODULE = runpy.run_path(str(Path(__file__).with_name("playback-latency-summary.py")))


class PlaybackLatencyTests(unittest.TestCase):
    def row(self, number, outcome="ok"):
        times = dict.fromkeys(MODULE["STAGES"])
        times["requested_us"] = 0
        times["first_nonzero_callback_us"] = number if outcome == "ok" else None
        return dict(schema=1, sample_kind="synthetic", source_commit="a" * 40,
                    platform="linux", scenario="cold", run="unit",
                    request=f"RequestId {{ session: SessionId(1), generation: {number} }}",
                    outcome=outcome, times=times, acoustic_capture=False)

    def test_percentiles_bounds_denominators_and_insufficient_samples(self):
        rows = [self.row(n) for n in range(1, 21)] + [self.row(21, "error"), self.row(22, "cancelled")]
        group = MODULE["summarize"](rows)["groups"][0]
        self.assertEqual((group["n"], group["n_ok"], group["n_cancelled"]), (22, 20, 1))
        self.assertAlmostEqual(group["failure_rate"], 2 / 22)
        self.assertTrue(group["sufficient_samples"])
        self.assertEqual(group["timestamps_us"]["first_nonzero_callback_us"],
                         dict(n=20, min=1, max=20, p50=10, p95=19))
        self.assertFalse(MODULE["summarize"](rows[:2])["groups"][0]["sufficient_samples"])
        self.assertIsNone(MODULE["summarize"]([rows[-1]])["groups"][0]["timestamps_us"]["first_pcm_us"]["p50"])

    def test_duplicates_secrets_missing_pcm_and_clock_errors_are_rejected(self):
        with self.assertRaises(ValueError):
            MODULE["summarize"]([self.row(1), self.row(1)])
        for change in ("secret", "pcm", "clock", "negative"):
            row = self.row(1)
            if change == "secret":
                row["url"] = "PRIVATE_FIXTURE"
            elif change == "pcm":
                row["times"]["first_nonzero_callback_us"] = None
            elif change == "clock":
                row["times"].update(http_us=10, first_bytes_us=1)
            else:
                row["times"]["first_pcm_us"] = -1
            with self.assertRaises(ValueError):
                MODULE["validate"](row)

    def test_real_and_synthetic_platforms_and_commits_are_never_pooled(self):
        row = self.row(1)
        real = copy.deepcopy(row)
        real["sample_kind"] = "live-product"
        self.assertEqual(len(MODULE["summarize"]([row, real])["groups"]), 2)

    def test_capture_fences_requests_and_emits_only_allowlisted_timing_data(self):
        row = self.row(1)
        snapshot = dict(
            current_provider="youtube-music", audio=True, query="PRIVATE_FIXTURE",
            online=dict(latency=dict(request=row["request"], origin="host_queue_intent",
                                     prefetched=False, requested_us=0)),
            audio_metrics=dict(request=row["request"], first_nonzero_callback_us=100),
        )
        captured = MODULE["capture"](snapshot, "a" * 40, "linux", "cold", "unit", "ok")
        self.assertNotIn("PRIVATE_FIXTURE", str(captured))
        snapshot["audio_metrics"]["request"] = self.row(2)["request"]
        with self.assertRaises(ValueError):
            MODULE["capture"](snapshot, "a" * 40, "linux", "cold", "unit", "ok")
        with self.assertRaises(ValueError):
            MODULE["capture"](snapshot, "a" * 40, "linux", "prefetched", "unit", "error")


if __name__ == "__main__":
    unittest.main()
