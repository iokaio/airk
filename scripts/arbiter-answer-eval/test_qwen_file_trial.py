# SPDX-License-Identifier: Apache-2.0
"""File scoring controls using small in-memory file cards and model scores."""

import unittest
from types import SimpleNamespace

from qwen_file_trial import document, reorder


class QwenFileTrial(unittest.TestCase):
    def test_ties_keep_original_order_and_invalid_scores_fail(self):
        self.assertEqual(reorder(["b", "a", "c"], [0.1, 0.1, 0.8]), ["c", "b", "a"])
        for scores in ([0.5], [0.1, float("nan")], [-1, 0], [0, 1.1]):
            with self.subTest(scores=scores), self.assertRaises(ValueError):
                reorder(["a", "b"], scores)

    def test_file_card_uses_each_selected_range_and_its_path(self):
        repo = SimpleNamespace(texts={"x.py": ["top", "x" * 1000, "unselected", "bottom"]})
        card = document(repo, "x.py", ["x.py:2-2", "x.py:4-4"])
        self.assertTrue(card.startswith("x.py\n"))
        self.assertIn("x" * 480, card)
        self.assertNotIn("x" * 481, card)
        self.assertNotIn("unselected", card)
        self.assertTrue(card.endswith("bottom"))


if __name__ == "__main__":
    unittest.main()
