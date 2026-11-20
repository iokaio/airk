# SPDX-License-Identifier: Apache-2.0
"""Run with checked Python -I -B -S."""
from pathlib import Path
import runpy
import unittest

audit = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission/verify_aws_native_source_reads.py"))["reader"]


class NativeSourceReadTests(unittest.TestCase):
    def setUp(self):
        self.raw = "é\r\nhi\nend".encode()
        self.span = {"byte_start": 5, "byte_end_exclusive": 6, "line_start": 2, "line_end_inclusive": 2,
                     "span_sha256": audit["digest"](b"i")}
        doc = {"path": "nested/file.md", "sha256": audit["digest"](self.raw)}
        self.case = {"source_span": self.span, "document": doc}
        self.citation = {"path": doc["path"], "content_hash": doc["sha256"], "start_line": 2,
                         "end_line": 2, "location": {"Lines": {"start": 2, "end": 2}}}
        self.response = {"truncated": False, "hits": [{"truncated": False, "text": "hi", "citation": self.citation}]}

    def test_source_bytes_and_citation_cover_published_span(self):
        self.assertEqual(audit["verified_lines"](self.case, self.raw, self.response)["span_sha256"], self.span["span_sha256"])

    def test_changed_location_refused_even_with_matching_prose(self):
        self.citation["start_line"] = 1
        with self.assertRaisesRegex(ValueError, "citation"):
            audit["verified_lines"](self.case, self.raw, self.response)

    def test_partial_or_changed_line_refused(self):
        self.response["hits"][0]["text"] = "i"
        with self.assertRaisesRegex(ValueError, "line bytes"):
            audit["verified_lines"](self.case, self.raw, self.response)


if __name__ == "__main__":
    unittest.main()
