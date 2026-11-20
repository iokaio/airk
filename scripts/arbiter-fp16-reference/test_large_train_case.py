# SPDX-License-Identifier: Apache-2.0
"""Regression controls for the separately authenticated Large TRAIN publisher and matching vectors."""
from contextlib import nullcontext
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest

HERE = Path(__file__).resolve().parent
RUN = runpy.run_path(str(HERE / "run_large_t32.py"))
TRAIN = runpy.run_path(str(HERE / "large_train_case.py"))


class OriginalCaseTests(unittest.TestCase):
    def test_publisher_requires_its_own_phase(self):
        args = SimpleNamespace(train_phase="publisher", recipe=RUN["DIRECT_RECIPE"], capture_hidden=False,
                               tokens=1588, publisher_execution=None, publisher_sha256=None)
        row = RUN["selected_row"]({}, args)
        self.assertEqual(row["input"]["input_digest"], TRAIN["INPUT"])
        self.assertEqual(len(row["input"]["option_mask"]), 7)
        self.assertIsNone(row["reference_original_fp32"])
        args.train_phase = "matching"
        with self.assertRaisesRegex(ValueError, "sealed publisher"):
            RUN["selected_row"]({}, args)

    def test_publisher_uses_sparse_scores_and_full_hidden(self):
        class Output:
            device = SimpleNamespace(type="cpu")
            dtype = "f32"
            shape = (1, 2)

            def __getitem__(self, _index):
                return SimpleNamespace(tolist=lambda: [3.0, 1.0])

        torch = SimpleNamespace(int64="i64", bool="bool", float32="f32", inference_mode=nullcontext,
                                tensor=lambda value, dtype: (value, dtype))
        row = dict(input_ids=[1], attention_mask=[1], position_ids=[0], decision_position=0,
                   candidate_positions=[32, 33], option_mask=[True, True])
        hidden = SimpleNamespace(device=Output.device, shape=(1, 1, 2560))
        self.assertEqual(TRAIN["score_publisher"](torch, lambda *x: (Output(), hidden), row), [3.0, 1.0])
        hidden.shape = (1, 1, 1024)
        with self.assertRaisesRegex(ValueError, "hidden output"):
            TRAIN["score_publisher"](torch, lambda *x: (Output(), hidden), row)

    def test_matching_requires_complete_original_ordered_cardinality(self):
        row = {"input":{"option_mask":[True]*7}, "pins":["native"], "reference_original_fp32":[1.0]*7}
        obs = {"train_phase":"matching", "case_pins":["native"], "forward_count":1,
               "matching_fp16":[1.0]*7, "original_fp32":[1.0]*7}
        args = SimpleNamespace(train_phase="matching")
        self.assertTrue(RUN["valid_scores"](obs, row, args))
        obs["matching_fp16"].pop()
        self.assertFalse(RUN["valid_scores"](obs, row, args))


if __name__ == "__main__":
    unittest.main()
