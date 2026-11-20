# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import runpy
import unittest


compare = runpy.run_path(str(Path(__file__).resolve().parents[1] /
                         "arbiter/analysis/public_source_admission/audit_techqa_menu_exposure.py"))["compare"]


class ExposureTests(unittest.TestCase):
    def setUp(self):
        self.meta = {key: dict(productName="p", productId=None, sourceDocumentId=key, canonicalUrl=key)
                     for key in ("g", "a", "b")}
        self.meta["b"]["canonicalUrl"] = "g"
        self.cases = {"q": dict(source_status="exact_publisher_span", ordered_document_ids=["a", "b"],
                                publisher_document_id="a"), "held": dict(source_status="withheld_publisher_span_mismatch")}
        self.documents = {"a": {"sha256": "x"}, "b": {"sha256": "y"}}

    def test_separates_menu_and_gold_preserving_unknown_fields(self):
        rows = compare(self.meta, {"g"}, self.cases, self.documents, {"y"}, str)
        self.assertEqual(set(rows), {"q"})
        self.assertEqual(rows["q"]["menu_matches"], {"literal_product_name": 2, "retained_file_bytes": 1,
                                                     "canonical_url": 1})
        self.assertEqual(rows["q"]["gold_matches"], ["literal_product_name"])
        self.assertEqual(rows["q"]["menu_missing_fields"], {"literal_product_id": 2})

    def test_refuses_reintroduced_train_id(self):
        with self.assertRaisesRegex(ValueError, "fence"):
            compare(self.meta, {"a"}, self.cases, self.documents, set(), str)


if __name__ == "__main__":
    unittest.main()
