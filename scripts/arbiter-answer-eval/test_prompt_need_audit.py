# SPDX-License-Identifier: Apache-2.0
import unittest
from pathlib import Path
from unittest.mock import patch

import prompt_need_audit as audit


class PromptNeedAuditTests(unittest.TestCase):
    def setUp(self):
        self.note = {"id": "N1", "path": "code.txt", "lines": [1, 7], "mark": "essential",
                     "alternatives": [{"path": "alt.txt", "lines": [10, 11]}]}
        self.data = {"passages": [{"at": "code.txt:1-7", "sha256": audit.digest(b"a\n" * 7), "needs": [1]}],
                     "passage_bytes": 14, "budget": 14000}

    @patch("prompt_need_audit.source_lines", return_value=["a"] * 8)
    def test_attributed_score_and_returned_context_are_separate(self, _):
        self.data["scored_support"] = [{"need": 1, "scored_at": "code.txt:1-2", "returned_at": "code.txt:1-7"}]
        self.data["reply_needs"] = [{"status": "candidate_evidence"}, {"status": "no_candidate_evidence"}]
        score = audit.score_variant(self.data, Path("."), {"N1": self.note}, [["N1"], ["N1"]])
        self.assertEqual(score["needs"][0]["attributed_whole_refs"], ["N1"])
        self.assertEqual(score["needs"][0]["scored_excerpt_whole_refs"], [])
        self.assertTrue(score["needs"][1]["no_candidate_despite_delivered_whole_ref"])

    def test_pairing_rejects_changed_same_input_or_owner(self):
        args = {"task": "task", "needs": ["find it"]}
        control = {"scores": [{"file": "x", "need": None, "score": 0.2, "rendered_sha256": "A"}],
                   "notes": {}, "ranks": {}, "passage_bytes": 4, "passages": [], "pairs": 1, "clipped": 0}
        direct = dict(control, scores=[dict(control["scores"][0], need=1, rendered_sha256="B")],
                      passages=[{"at": "x:2-3", "needs": [1]}],
                      scored_support=[{"need": 1, "scored_at": "x:2-2", "returned_at": "x:2-3"}],
                      reply_needs=[{"need": 1, "text": "find it", "status": "candidate_evidence", "where": [{"at": "x:2-3"}]}])
        entry = {"task": "t", "arguments": args, "pool": [{"file": "x", "returned_at": "x:2-3", "scored_at": "x:2-2"}],
                 "variants": {"control": control, "direct": direct, "deterministic": control}}
        old = {"scores": [["x", 0.2]], "clipped": 0, "control": {"notes": {}, "ranks": {}, "bytes": 4}}
        old["qwen"] = old["control"]
        audit.verify_pairing(entry, {"t": args}, old)
        direct["scores"][0].update(rendered_sha256="A", score=0.3)
        with self.assertRaisesRegex(ValueError, "Same rendered input"):
            audit.verify_pairing(entry, {"t": args}, old)
        direct["scores"][0]["score"] = 0.2
        direct["passages"][0]["needs"] = []
        with self.assertRaisesRegex(ValueError, "attribution"):
            audit.verify_pairing(entry, {"t": args}, old)

    def test_model_prompts_reconstruct_without_a_reference_label(self):
        file = Path(__file__).resolve()
        relative = file.name
        line = file.read_text(encoding="utf8").splitlines()[0]
        file_card = f"{relative}\n\nLines 1-1 (excerpt):\n{line}"
        short_card = f"Path: {relative}\nLines: 1-1\nSource:\n{line}"
        at = f"{relative}:1-1"
        entry = {"file": relative, "locations": [at], "scored_at": at, "returned_at": at,
                 "source_sha256": audit.digest(file.read_bytes()), "original_card_sha256": audit.digest(file_card.encode()),
                 "direct_card_sha256": audit.digest(short_card.encode())}
        args = {"task": "task", "needs": ["need"], "anchors": ["anchor"]}
        prompt = audit.PREFIX + audit.pair_text("Given a coding agent's task, determine whether this repository file contains evidence needed to complete it.",
                                                "task\nneed\nanchor", file_card) + audit.SUFFIX
        scores = [{"need": None, "file": relative, "rendered_sha256": audit.digest(prompt.encode())}]
        row = {"task": "x", "pool": [entry], "arguments": args, "variants": {"control": {"scores": scores}}}
        audit.verify_model_inputs(row, file.parent)
        scores[0]["rendered_sha256"] = audit.digest((prompt + "gold label").encode())
        with self.assertRaisesRegex(ValueError, "source-and-caller-only"):
            audit.verify_model_inputs(row, file.parent)

    def test_pool_upper_bound_separates_missing_files_from_missing_lines(self):
        notes = {"N1": self.note, "N2": dict(self.note, path="other.py", alternatives=[]),
                 "N3": dict(self.note, lines=[4, 8], alternatives=[])}
        rows = audit.pool_ceiling([{"file": "code.txt", "locations": ["code.txt:1-3", "code.txt:4-7"]}],
                                  notes, [["N1"], ["N2"], ["N3"], []])
        self.assertEqual([r["pooled_whole_refs"] for r in rows], [["N1"], [], [], []])
        self.assertEqual([r["pooled_file_refs"] for r in rows], [["N1"], [], ["N3"], []])
        self.assertFalse(rows[-1]["scorable"])

    def test_separate_partial_and_whole_including_alternatives_and_gap(self):
        def level(*ranges):
            return audit.evidence_level(self.note, [(*audit.parse_location(at), []) for at in ranges])
        self.assertEqual(level("code.txt:1-3", "code.txt:4-7"), "whole")
        self.assertEqual(level("code.txt:1-5", "code.txt:7-7"), "rule3")
        self.assertEqual(level("alt.txt:9-11"), "whole")
        self.assertEqual(level("different/code.txt:1-7"), "missing")
        self.assertEqual(level("code.txt:7-7"), "overlap")

    @patch("prompt_need_audit.source_lines", return_value=["a"] * 8)
    def test_only_returned_bytes_receive_credit(self, _):
        notes = {"N1": self.note, "N2": {"absence": True, "mark": "essential"}}
        groups = [["N1"], ["N2"], []]
        result = audit.score_variant(self.data, Path("."), notes, groups)
        self.assertEqual(audit.totals(result)["scorable_needs"], 1)
        unchanged = audit.score_variant(dict(self.data, omitted=["else:1-8"], where=["else:1-8"]), Path("."), notes, groups)
        self.assertEqual(result, unchanged)
        trial = audit.score_variant(dict(self.data, passages=[], passage_bytes=0), Path("."), notes, groups)
        self.assertEqual(audit.differences(result, trial)["whole_refs"]["lost"], [1])
        self.data["passages"][0]["sha256"] = audit.digest(b"a\n" * 4)
        with self.assertRaisesRegex(ValueError, "differs"):
            audit.score_variant(self.data, Path("."), notes, groups)

    @patch("prompt_need_audit.source_lines", return_value=["a"] * 8)
    def test_bounds_budget_and_duplicate_claims(self, _):
        for key, value in [("at", "code.txt:1-9"), ("needs", [0]), ("needs", [1, 1]), ("at", "../outside:1-7")]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                audit.verified_passages(dict(self.data, passages=[dict(self.data["passages"][0], **{key: value})]), Path("."), 1)
        for update in [{"budget": 13}, {"passage_bytes": 13}, {"passages": self.data["passages"] * 2}]:
            with self.assertRaises(ValueError):
                audit.verified_passages(dict(self.data, **update), Path("."), 1)

    @patch("prompt_need_audit.source_lines", return_value=["a"] * 8)
    def test_unmatched_claim_has_no_truth_value_without_labels(self, _):
        notes = {"N1": dict(self.note, path="other", alternatives=[])}
        result = audit.score_variant(self.data, Path("."), notes, [["N1"]])
        self.assertEqual(result["needs"][0]["attributed_without_reference_overlap"], ["code.txt:1-7"])
        self.assertFalse(result["needs"][0]["whole_refs"])


if __name__ == "__main__":
    unittest.main()
