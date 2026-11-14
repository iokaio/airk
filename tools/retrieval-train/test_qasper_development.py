# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import json
import unittest

import qasper_development as subject


class DevelopmentDataTests(unittest.TestCase):
    def setUp(self):
        self.paper = {"title": "A test work", "full_text": [{"section_name": "Results",
            "paragraphs": ["Measured result.", 'A quotation: "someone else".']}], "qas": [
            {"question_id": "q1", "question": "What was measured?", "answers": [
                {"annotation_id": "a1", "answer": {"evidence": ["Measured result."], "unanswerable": False}},
                {"annotation_id": "a2", "answer": {"evidence": [], "unanswerable": True}}]},
            {"question_id": "q2", "question": "What was quoted?", "answers": [
                {"annotation_id": "a3", "answer": {"evidence": ['A quotation: "someone else".'], "unanswerable": False}},
                {"annotation_id": "a4", "answer": {"evidence": ["Missing figure"], "unanswerable": False}},
                {"annotation_id": "a5", "answer": {"evidence": ["Measured result."], "unanswerable": False}}]}]}
        self.decision = {"title": "A test work", "paper_id": "fixture", "publisher_split": "validation",
            "source_text_sha256": hashlib.sha256(json.dumps(self.paper["full_text"], sort_keys=True,
                ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()}

    def test_all_alternatives_and_disagreement_survive(self):
        result = subject.prepare(self.paper, self.decision)
        self.assertEqual([[a["human_annotation_id"] for a in q["human_alternatives"]] for q in result["questions"]],
                         [["a1", "a2"], ["a3", "a4", "a5"]])
        self.assertTrue(result["questions"][0]["disagreement"])
        self.assertFalse(result["questions"][1]["disagreement"])
        self.assertEqual(result["publisher_split"], "validation")

    def test_excluded_or_missing_evidence_never_becomes_gold(self):
        result = subject.prepare(self.paper, self.decision)
        self.assertEqual([p["text"] for p in result["passages"]], ["Measured result."])
        alternatives = result["questions"][1]["human_alternatives"]
        for row in alternatives[:2]:
            self.assertEqual(row["alignment"], "unsupported_or_excluded_evidence_not_admitted")
            self.assertEqual(row["evidence"][0]["allowed_locations"], [])
        self.assertEqual(alternatives[2]["evidence"][0]["allowed_locations"], ["fixture:0:0"])

    def test_changed_source_refused(self):
        changed = copy.deepcopy(self.paper)
        changed["full_text"][0]["paragraphs"][0] = "Different source."
        with self.assertRaisesRegex(ValueError, "source work/hash differs"):
            subject.prepare(changed, self.decision)


if __name__ == "__main__":
    unittest.main()
