# SPDX-License-Identifier: Apache-2.0
"""Tests the ordered menu projection without any publisher data."""
from pathlib import Path
import runpy
import unittest

source = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission/stage_techqa_full_menus.py"
plan = runpy.run_path(str(source))["plan"]


class FullMenuTests(unittest.TestCase):
    def setUp(self):
        ids = [f"doc{i}" for i in range(51)]
        self.doc_ids = ids
        self.first = list(reversed(ids[:50]))
        self.second = list(reversed(ids[1:]))
        self.freeze = {"cases": {
            "Q02": {"source_status": "exact_publisher_span", "ordered_document_ids": self.second},
            "Q00": {"source_status": "withheld_publisher_span_mismatch", "ordered_document_ids": ["withheld"]},
            "Q01": {"source_status": "exact_publisher_span", "ordered_document_ids": self.first},
        }, "documents": dict.fromkeys(ids + ["withheld"])}

    def test_publisher_order_and_single_canonical_copy(self):
        menus, pilot, files = plan(self.freeze)
        self.assertEqual((pilot, len(files)), ("Q01", 51))
        self.assertEqual(menus, {"Q01": self.first, "Q02": self.second})
        self.assertEqual(set(files.values()), {f"pilot-menu/doc{i}.txt" for i in range(50)} | {"remaining-documents/doc50.txt"})
        self.assertNotIn("withheld", files)

    def test_duplicate_or_missing_source_refuses_even_if_case_count_matches(self):
        self.freeze["cases"]["Q02"]["ordered_document_ids"] = ["doc1"] * 50
        with self.assertRaisesRegex(ValueError, "menu"):
            plan(self.freeze)
        self.freeze["cases"]["Q02"]["ordered_document_ids"] = self.second
        del self.freeze["documents"]["doc50"]
        with self.assertRaisesRegex(ValueError, "document"):
            plan(self.freeze)

    def test_windows_alias_collision_refuses(self):
        self.freeze["cases"]["Q02"]["ordered_document_ids"][0] = "DOC1"
        self.freeze["documents"]["DOC1"] = {}
        with self.assertRaisesRegex(ValueError, "collision"):
            plan(self.freeze)


class NativePilotMenuTests(unittest.TestCase):
    def setUp(self):
        self.pilot = runpy.run_path(str(source.with_name("verify_techqa_native_menu.py")))
        ids = [f"d{i}" for i in range(50)]
        self.manifest = {
            "pilot_case": "DEV_Q002", "pilot_source_root": "pilot-menu", "ordered_menus": {"DEV_Q002": ids},
            "documents": {i: {"path": f"pilot-menu/{i}.txt", "bytes": 3, "sha256": "a" * 64} for i in ids},
        }

    def test_native_inventory_order_does_not_reorder_publisher_menu(self):
        tree, cases, _ = self.pilot["menu_inputs"](self.manifest)
        self.assertEqual(tree, "pilot-menu")
        def observe(label, tool, arguments):
            if tool == "airk_list_files":
                return {"paths": [f"d{i}.txt" for i in reversed(range(50))], "truncated": False}
            self.assertEqual(tool, "airk_file_info")
            return {"path": arguments["path"], "source_bytes": 3, "content_hash": "a" * 64,
                    "chunks": 1, "parse_status": "ok", "extraction_method": "utf8"}
        row = self.pilot["observe_menu"](self.pilot["COHORT"], cases, None, observe)[0]
        self.assertEqual(row["ordered_documents"][0]["publisher_id"], "d0")
        self.assertEqual(row["native_inventory_order"][0], "d49.txt")
        self.assertFalse(row["answer_offer_captured"])

    def test_partial_native_page_refused_before_per_document_calls(self):
        _, cases, _ = self.pilot["menu_inputs"](self.manifest)
        with self.assertRaisesRegex(ValueError, "inventory"):
            self.pilot["observe_menu"](self.pilot["COHORT"], cases, None,
                                       lambda *args: {"paths": [], "cursor": "next", "truncated": True})

    def test_foreign_view_path_refused_before_native_setup(self):
        self.manifest["documents"]["d0"]["path"] = "remaining-documents/d0.txt"
        with self.assertRaisesRegex(ValueError, "outside"):
            self.pilot["menu_inputs"](self.manifest)


if __name__ == "__main__":
    unittest.main()
