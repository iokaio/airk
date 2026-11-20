# SPDX-License-Identifier: Apache-2.0
"""Check TechQA publisher document pins and scalar-to-byte span conversion."""
import hashlib
from pathlib import Path
import runpy
import unittest


SOURCE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
freeze_document = runpy.run_path(str(SOURCE / "audit_techqa_dev_source_spans.py"))["freeze_document"]


class MenuFreezeTests(unittest.TestCase):
    def test_multibyte_and_repeated_answer_use_exact_publisher_offset(self):
        state = {"cases": {"verified": {}, "wrong": {}}, "documents": {}}
        body = "é CAT 🐈 CAT"
        freeze_document(state, "doc", body, [(8, 11, "CAT", "verified"), (2, 5, "cat", "wrong")])
        self.assertEqual(state["documents"]["doc"], {"bytes": 15, "sha256": hashlib.sha256(body.encode()).hexdigest()})
        case = state["cases"]["verified"]
        self.assertEqual((case["byte_start"], case["byte_end_exclusive"]), (12, 15))
        self.assertEqual(case["span_sha256"], hashlib.sha256(b"CAT").hexdigest())
        self.assertEqual(state["cases"]["wrong"], {"source_status": "withheld_publisher_span_mismatch"})

    def test_menu_only_document_creates_no_case(self):
        state = {"cases": {}, "documents": {}}
        freeze_document(state, "other", "distractor", [])
        self.assertEqual(state["cases"], {})
        self.assertEqual(state["documents"]["other"]["bytes"], 10)


if __name__ == "__main__":
    unittest.main()
