# SPDX-License-Identifier: Apache-2.0
"""Control whole-guide partition membership and complete, bounded bundle assignment without a service."""
from pathlib import Path
import runpy
import unittest

owner = runpy.run_path(str(Path(__file__).with_name("aws_guide_partitions.py")))


class Partitions(unittest.TestCase):
    def test_prefixes_are_disjoint_and_literal(self):
        paths = {"one/x", "one-extra/y", "one/sub/x", "two/x"}
        parts = owner["partition"](paths)
        self.assertEqual(parts["one/"], {"one/x", "one/sub/x"})
        self.assertEqual(parts["one-extra/"], {"one-extra/y"})
        self.assertEqual(set.union(*parts.values()), paths)
        for invalid in ("root-file", "/orphan", "empty/", "../escape"):
            with self.assertRaises(ValueError):
                owner["partition"]({invalid})

    def test_complete_guides_appear_once_without_a_split(self):
        guides = {p: set(range(n)) for p, n in (("large/", 5), ("medium/", 3), ("mini/", 2))}
        plan = owner["bundles"](guides, 2, 4)
        self.assertEqual(len(plan), 2)
        self.assertCountEqual([g for part in plan for g in part], guides)
        for part in plan:
            self.assertLessEqual(sum((len(guides[g]) + 1) // 2 for g in part), 4)
        with self.assertRaises(ValueError):
            owner["bundles"](guides, 1, 4)


if __name__ == "__main__":
    unittest.main()
