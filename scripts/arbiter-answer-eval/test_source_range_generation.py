# SPDX-License-Identifier: Apache-2.0
"""Regression coverage for bounded, need-specific source expansion."""
from types import SimpleNamespace
import unittest

import source_range_generation as generate


class SourceExpansionTests(unittest.TestCase):
    def context(self):
        lines = ["def unrelated():", "    pass", "", "def retry_rule():", "    return gateway_retry", "",
                 "def retry_test():", "    assert gateway_retry", "", "def another_test():", "    assert gateway_retry"]
        repo = SimpleNamespace(texts={"a.py":lines}, index={"files":{"a.py":{"defines":{
            "unrelated":[1], "retry_rule":[4], "retry_test":[7], "another_test":[10]}}}})
        return repo, [{"file":"a.py", "locations":["a.py:1-2"]}]

    def test_nominations_alternate_and_generated_range_contains_actual_new_seed(self):
        repo, pool = self.context()
        args = {"task":"Read source", "needs":["gateway retry", "assert gateway retry"]}
        nominees, audit = generate.nominate(repo, pool, args, 40)
        self.assertEqual([r.source.at for r in nominees[:2]], ["a.py:1-2"]*2)
        self.assertEqual([r.need for r in nominees[:4]], [1,2,1,2])
        self.assertTrue(all(r.source.first > 2 for r in nominees[2:]))
        self.assertEqual(len(audit["generated"]), 4)
        self.assertEqual(len({(r.need,r.source.at) for r in nominees}), len(nominees))
        for r in audit["generated"]:
            first, last = map(int, r["at"].split(":")[1].split("-"))
            self.assertLessEqual(first, r["seed"])
            self.assertGreaterEqual(last, r["seed"])

    def test_original_coverage_prevents_duplicate_proposals(self):
        repo, pool = self.context()
        pool[0]["locations"]=["a.py:1-11"]
        nominees, audit = generate.nominate(repo,pool,{"task":"gateway retry"},40)
        self.assertEqual([r.source.at for r in nominees], ["a.py:1-11"])
        self.assertEqual(audit["generated"], [])

    def test_seed_budget_covers_file_ends_and_boundary_window_contains_seed(self):
        indices = list(generate.seed_lines(18_000))
        self.assertEqual((len(indices),len(set(indices)),indices[0],indices[-1]), (4096,4096,1,18_000))
        for seed in (1,949,1001,1050,2000):
            first,last = generate.unit_window(seed,[1000,1050],2000)
            self.assertTrue(1<=first<=seed<=last<=2000 and last-first+1<=60)
        self.assertEqual(generate.unit_window(1005,[1000,1050],2000), (1000,1049))


if __name__ == "__main__":
    unittest.main()
