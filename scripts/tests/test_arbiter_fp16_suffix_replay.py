# SPDX-License-Identifier: Apache-2.0
"""Literal checks for the single native cut in the CPU suffix diagnostic."""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest

PATH = Path(__file__).resolve().parents[1] / "arbiter-fp16-execution/replay_compact_t1024_suffix.py"
score_suffix = runpy.run_path(str(PATH))["score_suffix"]


class SuffixReplayTests(unittest.TestCase):
    def run_cut(self, count, captured):
        tensor = SimpleNamespace(shape=(1, 16, 1024, 128), dtype="half", device="cpu")
        ordinary = lambda *_: "ordinary"
        wider = SimpleNamespace(wider_flash_attention=ordinary)

        def forward(*_):
            return [wider.wider_flash_attention(None, tensor, None, None, None, None, None) for _ in range(count)]

        wider.wider_projection_scores = forward
        try:
            return score_suffix(wider, None, None, None, captured, None)
        finally:
            self.assertIs(wider.wider_flash_attention, ordinary)

    def test_only_first_of_28_calls_substituted(self):
        captured = SimpleNamespace(shape=(1, 16, 1024, 128), dtype="half", device="cpu")
        self.assertEqual(self.run_cut(28, captured), [captured] + ["ordinary"]*27)

    def test_incomplete_forward_refused(self):
        captured = SimpleNamespace(shape=(1, 16, 1024, 128), dtype="half", device="cpu")
        with self.assertRaisesRegex(ValueError, "27 ordinary"):
            self.run_cut(27, captured)

    def test_wrong_layout_refused(self):
        captured = SimpleNamespace(shape=(1, 1024, 16, 128), dtype="half", device="cpu")
        with self.assertRaisesRegex(ValueError, "layout"):
            self.run_cut(28, captured)


if __name__ == "__main__":
    unittest.main()
