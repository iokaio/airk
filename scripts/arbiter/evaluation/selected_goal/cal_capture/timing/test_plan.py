# SPDX-License-Identifier: Apache-2.0
import copy
import unittest

from affinity import FULL, ORDER, choose
from run import PLAN_SHA, R5, linked, pin, verify_saved_binding


class TimingPlan(unittest.TestCase):
    def test_four_distinct_cores_one_l3_outside_both_siblings_of_other_workers(self):
        cores=[3 << (i*2) for i in range(32)]
        caches=[65535 << (i*16) for i in range(4)]
        loads=[0.7]*64
        for i in range(16,32):
            loads[i]=0.1
        result=choose(cores,caches,loads,3<<20)
        mask=int(result["local_mask_hex"],16)
        self.assertEqual(mask.bit_count(),4)
        self.assertEqual(mask & (3<<20),0)
        self.assertEqual(mask & caches[1],mask)
        self.assertEqual(len({next(c for c in cores if c&(1<<i)) for i in result["logical_cpus"]}),4)
        with self.assertRaises(ValueError):
            choose(cores,caches,loads,FULL)

    def test_crossover_reverses_within_subject_and_balances_each_period(self):
        self.assertEqual(ORDER[0],("full","local4","local4","full"))
        self.assertEqual(ORDER[1],("local4","full","full","local4"))
        self.assertTrue(all(a!=b for a,b in zip(*ORDER,strict=True)))

    def test_real_original_publication_and_member_evidence(self):
        original=linked(linked(pin(R5/"lifetime-plan.json",PLAN_SHA))["installation"]["bindings"])
        verify_saved_binding(original,original)
        changed=copy.deepcopy(original)
        changed["bindings"][0]["publication"]["token"]="different"
        with self.assertRaises(ValueError):
            verify_saved_binding(original,changed)
        changed=copy.deepcopy(original)
        changed["bindings"][0]["status_result"]["structuredContent"]["scope_observation"]["members"].clear()
        with self.assertRaises(ValueError):
            verify_saved_binding(original,changed)
