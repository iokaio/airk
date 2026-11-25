# SPDX-License-Identifier: Apache-2.0
"""Synthetic L7 scoring checks; no weights, files, or training."""

from pathlib import Path
import sys
import unittest
from unittest.mock import Mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import score


def fixture():
    rows = [
        {"id": f"{repo}-{split}-{label}", "pair_id": f"{repo}-{split}", "set": split,
         "repo": repo, "kind": "test", "label": label, "instruction": "publisher"}
        for repo in ("bookbuilder", "munarium")
        for split in ("heldout_id", "heldout_path", "train_sample")
        for label in (0, 1)
    ]
    rows += [
        {"id": f"general-{kind}", "pair_id": f"general-{kind}", "set": "replay_general_heldout",
         "repo": "replay", "kind": kind, "label": 1, "instruction": "publisher"}
        for kind in ("qasper", "techqa")
    ]
    scores = {"base": [], "candidate": []}
    for row in rows:
        margin = 1.0 if row["label"] == 1 else -1.0
        changed = -margin if row["repo"] == "munarium" and row["set"] in score.HELDOUT else margin
        if row["repo"] == "replay":
            changed = -0.5 if row["kind"] == "qasper" else 1.5
        scores["base"].append([row["id"], 0.0, margin])
        scores["candidate"].append([row["id"], 0.0, changed])
    return rows, {"schema": "airk.private.focus-answer-eval-worker-result/1", "mode": "run",
                  "complete": True, "gradient_steps": 3, "optimizer_steps_planned": 3, "scores": scores}


class L7ScoreTests(unittest.TestCase):
    def test_matched_heldout_gaps_and_positive_only_general(self):
        rows, worker = fixture()
        out = score.report_l7(rows, worker)
        self.assertEqual(out["heldout_candidate_minus_base"]["heldout_id/bookbuilder"]["mrr"], 0.0)
        self.assertEqual(out["heldout_candidate_minus_base"]["heldout_path/munarium"]["mrr"], -0.5)
        self.assertEqual(out["memorization_signal"]["munarium:train_minus_heldout_id"]["auroc"], 1.0)
        self.assertNotIn("replay_general_heldout/all", out["arms"]["base"]["metrics"])
        replay = out["general_replay"]
        self.assertEqual((replay["positive_rows"], replay["negative_rows"]), (2, 0))
        candidate = replay["arms"]["candidate"]["all"]
        self.assertIsNone(candidate["auroc"])
        self.assertIsNone(candidate["hit_at_1"])
        self.assertIsNone(candidate["mrr"])
        self.assertEqual(candidate["mean_positive_margin"], 0.5)
        self.assertEqual(candidate["positive_above_zero"], 0.5)
        legacy = score.report(rows, worker)
        self.assertNotIn("mode", legacy)
        self.assertNotIn("munarium:train_minus_heldout_id", legacy["arms"]["base"]["gaps"])

    def test_refuses_partial_worker_and_invalid_score_rosters(self):
        rows, worker = fixture()
        with self.assertRaisesRegex(ValueError, "complete owned"):
            score.report_l7(rows, {**worker, "mode": "probe"})
        source = worker["scores"]["candidate"]
        for roster in (source[:-1], [*source, source[0]], [source[1], source[0], *source[2:]],
                       [[source[0][0], 0.0, float("inf")], *source[1:]]):
            with self.subTest(roster_length=len(roster)), self.assertRaisesRegex(ValueError, "candidate: exact ordered"):
                score.report_l7(rows, {**worker, "scores": {**worker["scores"], "candidate": roster}})

    def test_refuses_missing_heldout_and_nonpositive_general_replay(self):
        rows, result = fixture()
        with self.assertRaisesRegex(ValueError, "nonpositive general replay"):
            score.report_l7([*rows[:-1], {**rows[-1], "label": 0}], result)
        with self.assertRaisesRegex(ValueError, "held-out splits"):
            score.report_l7([row for row in rows if not (
                row["repo"] == "munarium" and row["set"] == "heldout_path"
            )], result)

    def test_input_pin_rejects_changed_bytes(self):
        with self.assertRaisesRegex(ValueError, "SHA-256 differs"):
            score.read_pinned(Mock(name="fixture", read_bytes=Mock(return_value=b"changed")), "0" * 64)
