# SPDX-License-Identifier: Apache-2.0
"""Model-free controls preventing a study result from alternative scorers or changed wrappers."""

import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import qwen_replay_conformance as checks


class ConformanceTest(unittest.TestCase):
    def test_cheap_or_mock_judge_refused_before_scoring(self):
        for judge in (checks.prototype.CheapJudge(), Mock(spec=checks.prototype.Judge)):
            with self.subTest(judge=type(judge)), self.assertRaisesRegex(ValueError, "concrete local"):
                checks.inspect_loaded(judge, Path.cwd())

    def test_wrong_model_directory_refused_before_loading(self):
        with patch.object(checks.prototype, "Judge") as load, self.assertRaisesRegex(ValueError, "checkpoint contents"):
            checks.load_verified_judge(Path(__file__).parent)
        load.assert_not_called()

    def test_weight_receipt_for_full_precision_or_superseded_scorer_refused(self):
        judge = object.__new__(checks.prototype.Judge)
        for weights in (None, {**checks.CENSUS, "layers": 251}, {**checks.CENSUS, "packed_bytes_estimate": 1}):
            judge.weights = weights
            with self.subTest(weights=weights), self.assertRaisesRegex(ValueError, "weight census"):
                checks.inspect_loaded(judge, Path.cwd())

    def test_changed_system_and_suffix_each_refused_before_loading(self):
        for key in ("PREFIX", "SUFFIX"):
            with (self.subTest(key=key), patch.object(checks, key, "changed"),
                  patch.object(checks.prototype, "Judge") as load, self.assertRaisesRegex(ValueError, "wrapper")):
                checks.load_verified_judge(Path.cwd())
            load.assert_not_called()

    def test_joined_control_rejects_drift_and_preserves_tie_order(self):
        paths, pairs = ["z", "a", "b"], [("q", p) for p in ("z", "a", "b")]
        scores, history = [0.1, 0.8, 0.8], [("z", 0.2), ("a", 0.6), ("b", 0.5)]
        check = checks.verify_control
        self.assertTrue(check(paths, pairs, scores, ["a", "b", "z"], pairs, history)["historical_rank_checked"])
        for order, old_pairs, old_scores in ((["b", "a", "z"], pairs, history),
                                              (["a", "b", "z"], pairs[::-1], history),
                                              (["a", "b", "z"], pairs, [("z", 1), ("a", 0), ("b", 0)])):
            with self.subTest(order=order, old_pairs=old_pairs, old_scores=old_scores), self.assertRaises(ValueError):
                check(paths, pairs, scores, order, old_pairs, old_scores)
