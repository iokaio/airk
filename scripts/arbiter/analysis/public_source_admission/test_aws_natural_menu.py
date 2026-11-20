# SPDX-License-Identifier: Apache-2.0
"""Check multi-page publication-bound visibility, including missing source members."""
from pathlib import Path
import runpy
import unittest

capture = runpy.run_path(str(Path(__file__).with_name("verify_aws_natural_menu.py")))


class NativeMenu(unittest.TestCase):
    def capture(self, second_page=None):
        messages = []
        pages = [
            {"files": 3, "scope_observation": "scope"},
            {"paths": ["a", "b"], "scope_observation": "scope", "truncated": True, "cursor": "token"},
            {"paths": ["c"], "scope_observation": "scope", "truncated": False} | (second_page or {}),
        ]

        def call(label, operation, args):
            messages.append((operation, args.copy()))
            if operation == "airk_file_info":
                return {"scope_observation": "scope", "path": "c", "content_hash": "pinned-c"}
            return pages.pop(0)

        case = {"selector": {"menu": "documents/"}, "members": {p: {"sha256": "pinned-" + p} for p in ["a", "b", "c", "ignored"]},
                "cases": [{"selector": "one", "source_path": "documents/c"}]}
        rows = capture["observe"](capture["COHORT"], [case], None, call)
        self.assertEqual(messages[2][1]["cursor"], "token")
        self.assertEqual([op for op, _ in messages], ["airk_status", "airk_list_files", "airk_list_files", "airk_file_info"])
        return rows[0]

    def test_full_inventory(self):
        row = self.capture()
        self.assertEqual(row["native_visible_paths"], ["a", "b", "c"])
        self.assertEqual(row["source_absent_from_native_inventory"], ["ignored"])
        self.assertEqual(row["question_source_visibility"], [{"selector": "one", "known_positive_visible": True}])

    def test_incomplete_changed_or_foreign_page(self):
        for replacement in [{"paths": ["b"]}, {"paths": ["outside"]}, {"scope_observation": "new"},
                            {"cursor": "orphan"}, {"paths": []}, {"truncated": True, "cursor": "token"}]:
            with self.subTest(replacement=replacement), self.assertRaises(ValueError):
                self.capture(replacement)


if __name__ == "__main__":
    unittest.main()
