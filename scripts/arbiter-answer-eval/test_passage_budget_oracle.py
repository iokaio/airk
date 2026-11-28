# SPDX-License-Identifier: Apache-2.0
"""Cross-check the exact oracle against subset enumeration on small constrained examples."""

import itertools
import unittest
from collections import Counter
from unittest.mock import patch

import live_gather
from passage_budget_oracle import optimum


class BudgetOracleTests(unittest.TestCase):
    def test_dynamic_program_matches_exhaustive_enumeration(self):
        files = [
            [{"at": "a:1-2", "bytes": 8, "full": 3}, {"at": "a:5-7", "bytes": 4, "full": 1},
             {"at": "a:9-11", "bytes": 3, "full": 4}],
            [{"at": "b:1-2", "bytes": 3, "full": 2}, {"at": "b:4-5", "bytes": 8, "full": 24}],
            [{"at": "c:1-5", "bytes": 3, "full": 8}, {"at": "c:7-9", "bytes": 3, "full": 16}],
        ]
        rows = list(itertools.chain.from_iterable(files))
        for budget, file_limit, range_limit in itertools.product((0, 3, 10, 14, 18), (1, 2, 3), (1, 2)):
            with self.subTest(budget=budget, files=file_limit, ranges=range_limit):
                expected = (0, 0)
                for flags in itertools.product((False, True), repeat=len(rows)):
                    chosen = list(itertools.compress(rows, flags))
                    counts = Counter(row["at"].split(":")[0] for row in chosen)
                    size = sum(row["bytes"] for row in chosen)
                    if size > budget or len(counts) > file_limit or max(counts.values(), default=0) > range_limit:
                        continue
                    mask = 0
                    for row in chosen:
                        mask |= row["full"]
                    expected = max(expected, (mask.bit_count(), -size))
                with patch.multiple(live_gather, FOCUS_FILES=file_limit, RANGES_PER_FILE=range_limit,
                                    FOCUS_PASSAGE_BYTES=budget):
                    achieved, size, _ = optimum(files, "full")
                self.assertEqual((achieved.bit_count(), -size), expected)


if __name__ == "__main__":
    unittest.main()
