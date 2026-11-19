# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the byte-level TechQA validation metadata reader."""

import io
from pathlib import Path
import runpy
import unittest


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "scripts/arbiter/analysis/public_source_admission/audit_techqa_validation_metadata.py"
AUDIT = runpy.run_path(str(SOURCE))


class TechqaValidationMetadataControls(unittest.TestCase):
    def read(self, raw, parse):
        reader = AUDIT["MetadataReader"](io.BytesIO(raw), lambda: None)
        parsed = parse(reader)
        self.assertEqual(reader.finish(len(raw))["bytes"], len(raw))
        return parsed

    def test_skips_question_answer_and_nested_text_without_decoding(self):
        raw = (b'[{"QUESTION_ID":"q1","DOC_IDS":["doc-A"],"QUESTION_TEXT":"\xff\\" } ]",'
               b'"ANSWER":"\xfe","DETAIL":{"unknown":["\xfd",{"body":"\xfc"}]},'
               b'"ANSWERABLE":"Y","DOCUMENT":"doc-A","START_OFFSET":"1","END_OFFSET":4}]')
        parsed = self.read(raw, lambda reader: AUDIT["parse_roster"](reader, reference=True))
        self.assertEqual(parsed, {"q1": {"DOC_IDS": ("doc-A",), "ANSWERABLE": "Y",
                                        "DOCUMENT": "doc-A", "START_OFFSET": "1", "END_OFFSET": 4}})

    def test_only_corpus_document_keys_are_decoded(self):
        self.assertEqual(self.read(b'{"doc-A":{"text":"\xff"},"doc-B":{"text":["\xfe"]}}', AUDIT["parse_corpus"]),
                         {"doc-A", "doc-B"})

    def test_aggregate_distinguishes_exact_orders_missing_docs_and_valid_offsets(self):
        q = {"ok": {"DOC_IDS": ("A", "B")}, "swap": {"DOC_IDS": ("A", "B")},
             "missing": {"DOC_IDS": ("C",)}, "extra_q": {"DOC_IDS": ("D",)}}
        r = {"ok": {"DOC_IDS": ("A", "B"), "ANSWERABLE": "Y", "DOCUMENT": "A",
                    "START_OFFSET": "0", "END_OFFSET": 3},
             "swap": {"DOC_IDS": ("B", "A"), "ANSWERABLE": "N"},
             "missing": {"DOC_IDS": ("C",), "ANSWERABLE": "Y", "DOCUMENT": "C",
                         "START_OFFSET": False, "END_OFFSET": 3},
             "extra_r": {"DOC_IDS": ("B",), "ANSWERABLE": "-"}}
        result = AUDIT["paired_counts"](q, r, {"A", "B", "D"}, {"A", "C"}, {"B"})
        self.assertEqual((result["matched_question_ids"], result["matched_menu_rosters"],
                          result["matched_menu_orders"], result["matched_complete_menus"],
                          result["answerable_matched_complete_menus_with_offsets"]), (3, 3, 2, 1, 1))
        self.assertEqual((result["question_ids_without_reference"], result["reference_ids_without_question"],
                          result["menu_documents_missing_in_corpus"], result["menu_documents_overlapping_dev_menu_ids"],
                          result["menu_documents_overlapping_train_gold_ids"]), (1, 1, 1, 2, 1))

    def test_selected_unknown_types_duplicate_ids_and_corrupt_skips_refuse(self):
        for raw in (b'[{"QUESTION_ID":"q","QUESTION_ID":"r","DOC_IDS":["a"]}]',
                    b'[{"QUESTION_ID":"q","DOC_IDS":[{"TEXT":"never decode"}]}]',
                    b'[{"QUESTION_ID":"q","DOC_IDS":["a"],"ANSWER":"unterminated'):
            with self.subTest(size=len(raw)), self.assertRaises(ValueError):
                self.read(raw, lambda reader: AUDIT["parse_roster"](reader, reference=True))


if __name__ == "__main__":
    unittest.main()
