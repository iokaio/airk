# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import runpy
import unittest


bind = runpy.run_path(str(Path(__file__).resolve().parents[1] /
                      "arbiter/analysis/public_source_admission/prepare_techqa_publisher_judgments.py"))["bind_case"]


class PublisherEvidence(unittest.TestCase):
    def setUp(self):
        ids = [f"doc{i}" for i in range(50)]
        self.docs = {d: {"bytes": 100, "sha256": d} for d in ids}
        self.span = {"byte_start": 4, "byte_end_exclusive": 12, "span_sha256": "span", "line_start": 2, "line_end_inclusive": 4}
        self.publisher = dict(ordered_document_ids=ids, publisher_document_id="doc34", source_status="exact_publisher_span",
                              publisher_start=3, publisher_end=9, **self.span)
        self.normal = {"case_id": "q", "ordered_members": [{"publisher_id": d, "path": d + ".txt", "visible": True,
                       "publisher_position_zero_based": i, "original_sha256": d, "native_sha256": d,
                       "original_bytes": 100, "native_bytes": 100} for i, d in enumerate(ids)]}
        self.gold = {"selector": {"question_id": "q"}, "publisher_document_id": "doc34",
                     "document": {"path":"doc34.txt", "sha256":"doc34", "bytes":100}, "source_span": self.span}
        self.read = {"selector": {"question_id": "q"}, "citation": {"path": "doc34.txt", "content_hash": "doc34",
                     "start_line": 2, "end_line": 4}, "span_sha256": "span", "native_text_sha256": "range"}

    def observed(self):
        return bind("q", self.publisher, self.docs, self.normal, self.gold, self.read)

    def test_preserve_publisher_scalar_and_native_byte_locations(self):
        result = self.observed()
        self.assertEqual((result["publisher_scalar_start"], result["publisher_scalar_end_exclusive"]), (3,9))
        self.assertEqual((result["byte_start"], result["byte_end_exclusive"], result["span_sha256"]), (4,12,"span"))

    def test_full_menu_member_mismatch_refuses(self):
        self.normal["ordered_members"][0]["native_sha256"] = "other revision"
        with self.assertRaisesRegex(ValueError, "menu member"):
            self.observed()

    def test_original_endpoint_and_independent_range_must_agree(self):
        self.publisher["byte_end_exclusive"] = 13
        with self.assertRaisesRegex(ValueError, "publisher span"):
            self.observed()
        self.publisher["byte_end_exclusive"] = 12
        self.read["citation"]["start_line"] = 1
        with self.assertRaisesRegex(ValueError, "publisher span"):
            self.observed()


if __name__ == "__main__":
    unittest.main()
