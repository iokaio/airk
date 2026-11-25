# SPDX-License-Identifier: Apache-2.0
"""Synthetic grading-receipt and historical CSV tests for pass comparisons."""

import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import compare_passes  # noqa: E402
import grade  # noqa: E402


def legacy_row(turns: int) -> dict:
    return {"repo": "munarium", "slot": "C1", "configuration": "focus", "device": "gpu",
            "correct_finish": True, "turns_to_finish": turns}


def write_receipts(directory: Path, first_turn: int, second_turn: int) -> None:
    rows = []
    for sample, turn in enumerate((first_turn, second_turn), start=1):
        path = directory / f"sample-{sample}"
        path.mkdir(parents=True)
        common.write_json(path / "session.json", {"status": "complete"})
        common.write_json(path / "coverage.json", {
            "essential": ["note"], "essential_uncovered": [], "essential_covered": 1, "essential_total": 1,
            "all_essential_turn": turn, "first_call_yield": float(turn == 1), "counted_turns": turn,
            "total_bytes": 100, "total_elapsed_ms": 10,
        })
        judges = []
        for number in (1, 2):
            verdict = path / f"judge-{number}.json"
            common.write_json(verdict, {"status": "valid",
                                       "verdict": {"notes": [{"id": "note", "verdict": "correct"}]}})
            judges.append(str(verdict))
        rows.append({**legacy_row(turn), "kind": "coding", "key": f"C1/focus/{sample}", "sample": sample,
                     "session_dir": str(path), "coverage": str(path / "coverage.json"), "judges": judges})
    common.write_json(directory / "pass-index.json", {"series": directory.name, "results": rows})
    common.write_text(directory / "results.csv", grade.csv_text([legacy_row(first_turn), legacy_row(second_turn)]))


class PassComparisonTests(unittest.TestCase):
    def test_sample_one_wins_over_later_rows_and_uses_canonical_adjudication(self):
        with tempfile.TemporaryDirectory() as scratch:
            a, b = Path(scratch) / "a", Path(scratch) / "b"
            write_receipts(a, first_turn=4, second_turn=1)
            write_receipts(b, first_turn=2, second_turn=6)
            common.write_json(b / "adjudications.json", {
                "C1/focus/1": {"grade": 2, "correct_finish": True},
            })

            comparison = compare_passes.summarize(compare_passes.rows(a), compare_passes.rows(b))["focus-gpu"]

            self.assertEqual(comparison["rows"], 1)
            self.assertEqual(comparison["finishes_a"], 1)
            self.assertEqual(comparison["finishes_b"], 1)
            self.assertEqual(comparison["turns_saved_on_shared_finishes"], 2)
            self.assertEqual(comparison["mean_grade_b"], 2)

    def test_unambiguous_legacy_csv_remains_supported(self):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch)
            common.write_text(path / "results.csv", grade.csv_text([legacy_row(3)]))
            rows = compare_passes.rows(path)

            self.assertIs(rows[("focus-gpu", "munarium-C1")]["correct_finish"], True)
            self.assertEqual(compare_passes.summarize(rows, rows)["focus-gpu"]["finishes_a"], 1)

    def test_ambiguous_legacy_csv_requires_grading_receipts(self):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch)
            common.write_text(path / "results.csv", grade.csv_text([legacy_row(3), legacy_row(1)]))

            with self.assertRaisesRegex(common.HarnessError, r"duplicate comparison row.*pass-index\.json"):
                compare_passes.rows(path)
