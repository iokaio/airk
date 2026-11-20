# SPDX-License-Identifier: Apache-2.0
"""Checks against an independently recorded Git tree from the campaign baseline."""
import unittest
from contextbench_full_tree_inventory import verify_tree


class InventoryTests(unittest.TestCase):
    def setUp(self):
        # git ls-tree 253bcb8f:scripts/arbiter-fp16-reference
        self.tree = "cff6dd36ae9c930419fd953b1cb948dc692b78a7"
        self.rows = [[name, "100644", oid, 0] for name, oid in (
            ("CONTRACT.md", "e037c9f56654ed44df25b081fb6f8a8159d29dac"),
            ("graph.py", "675756c03de9c4fbb008c4191f6022f40fb861b2"),
            ("test_graph.py", "044c246c33c74ac74a8dd76e3fe23b15d0f83fcd"),
            ("test_reference.py", "432b6615fef47d0854db357327a541cf0ff5cf4b"))]

    def test_tree_matches_even_if_api_order_differs(self):
        self.assertEqual(verify_tree(list(reversed(self.rows)), self.tree), {"100644": 4})

    def test_missing_or_renamed_leaf_fails(self):
        with self.assertRaisesRegex(ValueError, "altered Git tree"):
            verify_tree(self.rows[1:], self.tree)
        self.rows[0][0] = "nested/file"
        with self.assertRaisesRegex(ValueError, "parent is absent"):
            verify_tree(self.rows, self.tree)


if __name__ == "__main__":
    unittest.main()
