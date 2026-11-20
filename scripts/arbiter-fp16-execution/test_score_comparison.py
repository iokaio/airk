# SPDX-License-Identifier: Apache-2.0
"""Small independent numerical gate cases shared by both Windows scientific owners."""
from pathlib import Path
import runpy
import unittest

HERE = Path(__file__).resolve().parent
compare = runpy.run_path(str(HERE / "score_comparison.py"))["numerical"]


class ScoreComparisonTests(unittest.TestCase):
    def test_exact_tie_is_stable(self):
        scores = [1.0, 1.0]
        result = compare(scores, scores, scores)
        self.assertTrue(result["release_numerical_case_pass"])
        self.assertEqual(result["matching"], {"max_raw_abs": 0.0, "max_mass_abs": 0.0, "winner_agrees": True})

    def test_reversed_near_tie_fails_despite_small_raw_error(self):
        result = compare([1.0, 1.001], [1.001, 1.0], [1.001, 1.0])
        self.assertLess(result["matching"]["max_raw_abs"], 0.007)
        self.assertFalse(result["matching"]["winner_agrees"])
        self.assertFalse(result["release_numerical_case_pass"])

    def test_distinct_raw_gates_and_original_mass(self):
        result = compare([1.01, 0.01], [1.0, 0.0], [1.0, 0.0])
        self.assertFalse(result["release_numerical_case_pass"])
        self.assertTrue(result["development_numerical_case_pass"])
        self.assertFalse(compare([1.0, 0.0], [1.0, 0.0], [-1.0, 2.0])["release_numerical_case_pass"])

    def test_missing_nonfinite_and_changed_cardinality(self):
        for bad in ([], [float("nan")], [1.0, 0.5]):
            with self.assertRaises(ValueError):
                compare([1.0], bad, [1.0])


if __name__ == "__main__":
    unittest.main()
