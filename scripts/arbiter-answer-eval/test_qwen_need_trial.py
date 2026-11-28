# SPDX-License-Identifier: Apache-2.0
"""Focused plumbing regressions for the local per-need experiment."""

from types import SimpleNamespace
import unittest

from qwen_need_trial import complete_card, interleave, ModelCache


class EvidenceCards(unittest.TestCase):
    def test_truncated_excerpt_has_an_exact_interval(self):
        repo = SimpleNamespace(texts={"a": ["x" * 600, "def valid_name():", "    return 11", "y" * 600]},
                               index={"files": {"a": {"defines": {"valid_name": [2], "outside": [4], "wrong": [3]}}}})
        doc, scored, returned = complete_card(repo, "a", ["a:1-4"])
        self.assertEqual(("a:2-3", "a:1-4"), (scored, returned))
        self.assertIn("Declaration: valid_name\n", doc)
        self.assertTrue(doc.endswith("def valid_name():\n    return 11"))
        self.assertNotIn("outside", doc)
        self.assertNotIn("wrong", doc)

    def test_no_complete_source_produces_no_citation(self):
        repo = SimpleNamespace(texts={"a": ["x" * 600, " ", ""]}, index={"files": {"a": {}}})
        self.assertEqual((None, None), complete_card(repo, "a", ["a:1-3"])[1:])

    def test_need_interleave_skips_duplicates_without_losing_turns(self):
        self.assertEqual(["a", "b", "d", "c", "e"],
                         interleave([["a", "c", "e"], ["a", "b", "e"], ["a", "b", "d"]]))

    def test_reused_pairs_keep_actual_score_and_logical_clipping(self):
        class Judge:
            room, clipped = 1, 0
            tokenizer = SimpleNamespace(encode=lambda *args, **kwargs: [1, 2])
            def score(self, instruction, pairs):
                self.clipped += len(pairs)
                return [0.7] * len(pairs)
        cache = ModelCache(Judge())
        p = ("question", "citation")
        values, count, clipped, _ = cache.score("i", [p, p])
        self.assertEqual(([0.7, 0.7], 1, 1), (values, count, clipped))
        values, count, clipped, _ = cache.score("i", [p])
        self.assertEqual(([0.7], 0, 0), (values, count, clipped))
        self.assertTrue(cache.truncated["i", p])


if __name__ == "__main__":
    unittest.main()
