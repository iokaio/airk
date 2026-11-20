# SPDX-License-Identifier: Apache-2.0
"""Verify exact raw-byte fallback and immutable original geometry."""

from pathlib import Path
import runpy
import unittest


SOURCE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
match = runpy.run_path(str(SOURCE / "audit_contextbench_derived_spans.py"))["unique_whole_lines"]


class DerivedSpans(unittest.TestCase):
    def test_exact_utf8_byte_and_line_coordinates_include_trailing_lf(self):
        raw = "é\r\nA\nB\nC".encode()
        result = match(raw, "A\nB\n")
        self.assertEqual(result["status"], "strict_derived_match")
        self.assertEqual(result["derived"], {"byte_start": 4, "byte_end_exclusive": 8,
                                            "start_line": 2, "end_line": 3})

    def test_no_trim_or_line_ending_rewrite(self):
        self.assertEqual(match(b" x\r\n", "x")["status"], "raw_body_inside_line")
        self.assertEqual(match(b"a\r\nb", "a\nb")["status"], "raw_body_absent")
        self.assertEqual(match(b"line\rrest", "line")["status"], "raw_body_inside_line")

    def test_even_embedded_or_overlapping_duplicates_refuse(self):
        self.assertEqual(match(b"abc\nabcde", "abc")["status"], "raw_body_nonunique")
        self.assertEqual(match(b"aaaa", "aaa")["occurrences"], 2)

    def test_numbered_format_never_becomes_unlabeled_content(self):
        self.assertEqual(match(b"1\tx\n2\ty", "1\tx\n2\ty")["status"], "numbered_formatter_not_eligible")

    def test_eof_and_crlf_boundaries(self):
        self.assertEqual(match(b"a\r\nb", "a")["derived"]["end_line"], 1)
        self.assertEqual(match(b"a\r\nb", "b")["derived"]["byte_end_exclusive"], 4)


if __name__ == "__main__":
    unittest.main()
