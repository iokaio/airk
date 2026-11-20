# SPDX-License-Identifier: Apache-2.0
import itertools
import unittest
from pathlib import Path
from random import Random
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from paired_confidence import Pair, lower_bound, sensitivity
from simulate_paired_confidence import coverage, distribution


class PairedTests(unittest.TestCase):
    def test_missing_arm_is_retained_and_completions_are_sharp(self):
        roster = {"a":"x","b":"x","c":"y"}
        rows = [Pair("a","x",True,True),Pair("b","x",False,None)]
        report = sensitivity(roster,rows)
        self.assertEqual(report["cells_fp16_then_fp32"]["unknown/unknown"],1)
        self.assertEqual(report["requests"],3)
        possibilities = [(2-98*b+100*c-98*d)/300 for b,c,d in itertools.product((0,1),repeat=3)]
        self.assertEqual(report["sample_contrast_interval"],[min(possibilities),max(possibilities)])

    def test_both_arms_and_source_groups_have_frozen_identity(self):
        valid = Pair("a","x",True,False)
        for rows in ([valid,valid], [Pair("extra","x",False,True)], [Pair("a","y",True,True)],
                     [Pair("a","x",1,False)]):
            with self.assertRaises(ValueError):
                sensitivity({"a":"x"},rows)

    def test_each_request_keeps_one_vote(self):
        rows=[Pair(str(i),"large",False,True) for i in range(3)] + [Pair("one","small",True,False)]
        result=sensitivity({p.request:p.cluster for p in rows},rows)
        self.assertEqual(result["sample_contrast_interval"],[-.485,-.485])
        self.assertAlmostEqual(lower_bound(1,[1]*6),.0107044254773557)

    def test_exact_small_distribution_and_shared_shock(self):
        self.assertEqual(distribution([1,2],.5,False),{0:.25,1:.25,2:.25,3:.25})
        self.assertAlmostEqual(coverage([1]*6,-.98,1,49/99,True,10,Random(7))["exact_cluster_coverage"],50/99)
        equals = coverage([1]*6,0,0,0,False,10,Random(7))
        self.assertEqual(equals["exact_cluster_coverage"],1)


if __name__=="__main__":
    unittest.main()
