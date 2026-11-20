# SPDX-License-Identifier: Apache-2.0
import copy
import unittest

from inputs import CONTROL, CONTROL_SHA, ref, row_key, selected
from run import verify_result


class ScientificReferences(unittest.TestCase):
    def setUp(self):
        self.rows,self.mode = selected(ref(CONTROL,CONTROL_SHA),0,1)
        row = self.rows[0]
        self.result = {"mode":self.mode,"role":"protected_matching","candidate_forwards":0,
                       "matching_recipe_sha256":"7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394",
                       "rows":[row|{"input_row_sha256":row_key(row),"scores":[0.0]*7,"seconds":110}]}

    def test_real_original_census_order_and_separate_science_deadline(self):
        self.assertEqual(len(self.rows[0]["compiled"]["input_ids"]),1408)
        self.assertEqual(self.rows[0]["remaining_ms"],100639)
        verify_result(self.result,self.rows,self.mode,"protected_matching")

    def test_refuses_incomplete_or_reordered_card_vectors(self):
        self.result["rows"][0]["scores"].pop()
        with self.assertRaises(ValueError):
            verify_result(self.result,self.rows,self.mode,"protected_matching")
        altered = copy.deepcopy(self.rows)
        altered[0]["roster"]["cards"].reverse()
        with self.assertRaises(ValueError):
            verify_result(self.result,altered,self.mode,"protected_matching")

    def test_original_and_matching_roles_do_not_substitute(self):
        with self.assertRaises(ValueError):
            verify_result(self.result,self.rows,self.mode,"publisher_fp32")

    def test_historical_control_cannot_supply_another_ordinal(self):
        with self.assertRaises(ValueError):
            selected(ref(CONTROL,CONTROL_SHA),1,1)


if __name__ == "__main__":
    unittest.main()
