# SPDX-License-Identifier: Apache-2.0
import runpy
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
interval = runpy.run_path(str(ROOT / "scripts/arbiter/analysis/public_source_admission/verify_techqa_delivered_evidence.py"))["returned_interval"]
covered = runpy.run_path(str(ROOT / "tools/retrieval-train/goal_rubric.py"))["covered_location"]


def hit(text, start=1, end=1):
    return {"text": text, "citation": {"start_line": start, "end_line": end,
            "location": {"Lines": {"start": start, "end": end}}}}


class DeliveredEvidence(unittest.TestCase):
    def test_repeated_text_in_same_citation_is_unresolved(self):
        self.assertIsNone(interval(b"aaaa", hit("aaa")))

    def test_truncated_text_does_not_cover_whole_citation(self):
        self.assertEqual(interval(b"abcdef", hit("abc")), (0, 3))

    def test_unicode_lines_use_original_byte_offsets(self):
        self.assertEqual(interval("á\n€!".encode(), hit("€", 2, 2)), (3, 6))

    def test_coverage_can_span_adjacent_delivered_fragments(self):
        def span(start, end):
            return {"file": "original", "byte_start": start, "byte_end": end}
        self.assertTrue(covered(span(2, 6), [span(0, 4), span(4, 9)]))
        self.assertFalse(covered(span(2, 6), [span(0, 4), span(5, 9)]))


if __name__ == "__main__":
    unittest.main()
