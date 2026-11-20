# SPDX-License-Identifier: Apache-2.0
import itertools
from pathlib import Path
import runpy
import unittest


MODULE = runpy.run_path(str(Path(__file__).resolve().parents[1] /
                            "arbiter/analysis/public_source_admission/audit_active_confirmation_groups.py"))


class SourceGroups(unittest.TestCase):
    def test_bridge_order_and_excluded_bridge(self):
        isolated = ("c", ["z"])
        bridge = ("join", ["x", "y"])
        cases = [("a", ["x"]), ("b", ["y"]), isolated, bridge]
        for permutation in itertools.permutations(cases):
            self.assertEqual(MODULE["groups"](permutation), [["a", "b", "join"], ["c"]])
        self.assertEqual(MODULE["groups"](cases[:-1]), [["a"], ["b"], ["c"]])

    def test_duplicate_case_and_missing_source_refused(self):
        for rows in [[("q", ["x"]), ("q", ["y"])], [("q", [])], [("q", [""])]]:
            with self.assertRaises(ValueError):
                MODULE["groups"](rows)

    def test_complete_case_requires_every_span_and_matches_exact_ordinal(self):
        rows = [
            {"instance_id": case, "ordinal": n, "repo": "r", "status": status}
            for case, n, status in [("a", 1, "exact_declared_lines"), ("a", 2, "out_of_bounds"),
                                    ("b", 1, "declared_lines_differ"), ("c", 1, "pinned_blob_unresolved")]
        ]
        fixed = {"rows": [{"original": rows[1], "status": "strict_derived_match"},
                          {"original": rows[2], "status": "raw_body_absent"}]}
        all_cases, complete = MODULE["contextbench_cases"]({"rows": rows}, fixed)
        self.assertEqual(set(all_cases), {"a", "b", "c"})
        self.assertEqual(complete, {"a": {"r"}})


if __name__ == "__main__":
    unittest.main()
