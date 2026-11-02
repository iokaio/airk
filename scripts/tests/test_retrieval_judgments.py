# SPDX-License-Identifier: Apache-2.0
"""Meaningful relevance guards: rank, duplicate chunks, scope and failures."""
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("retrieval_judgments", SCRIPTS / "retrieval_judgments.py")
JUDGMENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(JUDGMENTS)


class RetrievalJudgmentTests(unittest.TestCase):
    def test_frozen_corpus_has_all_four_families_and_separate_held_out_queries(self):
        corpus, digest = JUDGMENTS.load(SCRIPTS / "fixtures/retrieval-v190.json")
        self.assertEqual(len(digest), 64)
        self.assertTrue({"code", "manifest", "prose", "table"} <= {q["family"] for q in corpus["queries"]})
        self.assertEqual({q["split"] for q in corpus["queries"]}, {"development", "held_out"})

    def test_duplicate_chunks_consume_rank_without_boosting_recall_or_gain(self):
        result = JUDGMENTS.score(["a", "a", "b"], {"a": 3, "b": 2})
        expected = (7 + 3 / math.log2(4)) / (7 + 3 / math.log2(3))
        self.assertAlmostEqual(result["ndcg_at_10"], expected)
        self.assertEqual(result["recall_at_10"], 1)
        self.assertEqual(result["relevant_files_returned"], 2)
        self.assertLess(result["ndcg_at_10"], JUDGMENTS.score(["a", "b"], {"a": 3, "b": 2})["ndcg_at_10"])

    def test_rank_cutoff_does_not_recover_an_eleventh_chunk_by_deduplication(self):
        result = JUDGMENTS.score(["unrelated"] * 10 + ["wanted"], {"wanted": 3})
        self.assertEqual(result["ndcg_at_10"], 0)
        self.assertEqual(result["recall_at_10"], 0)

    def test_negative_queries_and_failures_do_not_inflate_positive_macro_scores(self):
        records = [dict(JUDGMENTS.score(["a"], {"a": 3}), mode="lexical", split="held_out", status="ok"),
                   dict(JUDGMENTS.score([], {}), mode="lexical", split="held_out", status="ok"),
                   {"mode": "lexical", "split": "held_out", "status": "error"}]
        result = JUDGMENTS.summarize(records)["lexical/held_out"]
        self.assertEqual((result["attempts"], result["failures"], result["scored_queries"]), (3, 1, 1))
        self.assertEqual(result["ndcg_at_10"], 1)
        self.assertEqual(result["negative_empty_rate"], 1)
        self.assertFalse(JUDGMENTS.score(["unrelated"], {})["negative_empty"])

    def test_fixture_rejects_escape_and_out_of_scope_judgments(self):
        corpus, _ = JUDGMENTS.load(SCRIPTS / "fixtures/retrieval-v190.json")
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "corpus.json"
            corpus["sources"]["../escape"] = "source"
            path.write_text(json.dumps(corpus), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "source path"):
                JUDGMENTS.load(path)
            del corpus["sources"]["../escape"]
            corpus["queries"][0]["grades"]["docs/refunds.md"] = 3
            path.write_text(json.dumps(corpus), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "out-of-scope"):
                JUDGMENTS.load(path)

    def test_controlled_edit_cannot_name_a_file_outside_the_frozen_inventory(self):
        corpus, _ = JUDGMENTS.load(SCRIPTS / "fixtures/retrieval-v190.json")
        corpus["index_control"]["path"] = "../not-a-fixture"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "corpus.json"
            path.write_text(json.dumps(corpus), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "owned index control"):
                JUDGMENTS.load(path)


if __name__ == "__main__":
    unittest.main()
