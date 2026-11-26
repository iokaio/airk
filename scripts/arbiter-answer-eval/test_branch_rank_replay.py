# SPDX-License-Identifier: Apache-2.0
"""Controls for exact lexical/fusion replay inputs."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import branch_rank_replay as branches
import common


class BranchReplay(unittest.TestCase):
    def test_focused_ranges_are_utf8_bytes_and_expose_embedding_fallback(self):
        query = "éx then alpha"
        pieces, embedded = branches.focused_queries(query, {"byte_ranges": [[0, 3], [9, 14]], "embedded": 0})
        self.assertEqual(pieces, ["éx", "alpha"])
        self.assertEqual(embedded, 0)
        with self.assertRaisesRegex(common.HarnessError, "invalid native focused"):
            branches.focused_queries(query, {"byte_ranges": [[0, 3], [9, 14]], "embedded": 1})
        with self.assertRaises(UnicodeDecodeError):
            branches.focused_queries(query, {"byte_ranges": [[1, 3]], "embedded": 1})


if __name__ == "__main__":
    unittest.main()
