# SPDX-License-Identifier: Apache-2.0
"""Run with python -I -B -S scripts/tests/test_aws_repeated_answer_locations.py."""

import hashlib
from pathlib import Path
import runpy
import unittest


SOURCE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
PROJECTOR = runpy.run_path(str(SOURCE / "audit_aws_documentation_source_projection.py"))
AUDIT = runpy.run_path(str(SOURCE / "audit_aws_repeated_answer_locations.py"))


class RepeatedAnswerLocations(unittest.TestCase):
    def test_pins_match_maintained_source(self):
        self.assertEqual(hashlib.sha256((SOURCE / "audit_aws_documentation_source_projection.py").read_bytes())
                         .hexdigest(), AUDIT["PROJECTOR_SHA"])

    def test_byte_offsets_overlapping_and_word_boundaries(self):
        hits = AUDIT["occurrences"]("é aaa aa\n", "How much?", "aa", PROJECTOR)
        self.assertEqual([(3, 5), (4, 6), (7, 9)], [(h["byte_start"], h["byte_end_exclusive"]) for h in hits])
        self.assertEqual([False, False, True], [h["word_bounded"] for h in hits])
        self.assertFalse(any(h["all_question_terms_on_answer_lines"] for h in hits))

    def test_evidence_context_and_hints_keep_every_occurrence(self):
        body = "first alpha\r\n\r\nWhich disk limit?\r\ndisk limit alpha\r\n\r\n~~~\r\nalpha\r\n~~~"
        hits = AUDIT["occurrences"](body, "Which disk limit?", "alpha", PROJECTOR)
        self.assertEqual([1, 4, 7], [h["line_start"] for h in hits])
        self.assertEqual([False, True, False], [h["literal_question_in_paragraph"] for h in hits])
        self.assertEqual([False, True, False], [h["all_question_terms_on_answer_lines"] for h in hits])
        self.assertEqual([False, False, True], [h["overlaps_markdown_fenced_code"] for h in hits])
        context = b"Which disk limit?\r\ndisk limit alpha\r\n"
        row = hits[1]
        self.assertEqual(context, body.encode()[row["paragraph_byte_start"]:row["paragraph_byte_end_exclusive"]])
        self.assertEqual(hashlib.sha256(context).hexdigest(), row["paragraph_sha256"])

    def test_multiline_offsets_after_blank_line(self):
        hits = AUDIT["occurrences"]("\nfoo\nbar\n\nfoo\nbar", "What?", "foo\nbar", PROJECTOR)
        self.assertEqual([(2, 3), (5, 6)], [(h["line_start"], h["line_end_inclusive"]) for h in hits])
        self.assertEqual([(1, 9), (10, 17)],
                         [(h["paragraph_byte_start"], h["paragraph_byte_end_exclusive"]) for h in hits])


if __name__ == "__main__":
    unittest.main()
