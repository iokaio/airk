# SPDX-License-Identifier: Apache-2.0
"""Verify the two-lifetime predeclaration and its independently replayable capacity bound."""
import unittest
from unittest.mock import patch

import two_lifetimes as two


class TwoLifetimes(unittest.TestCase):
    def test_actual_preflight_has_original_questions_and_complete_native_source_capacity(self):
        plan = two.make_plan(123456,None,None)
        self.assertEqual(plan["phase_groups"],[list(range(7)),list(range(7,13))])
        self.assertEqual([sum(len(plan["phase_ids"][i]) for i in group) for group in plan["phase_groups"]],[56,44])
        self.assertEqual(plan["capacity_basis"]["group_upper"],[960,920])
        self.assertEqual(len(plan["capacity_basis"]["phase_closures"]),11)
        self.assertEqual(len(plan["sources"]),17)
        self.assertEqual(plan["start_ms"]+10800000,plan["deadline_ms"]+plan["setup_elapsed_ms"])

    def test_missing_or_misattributed_admissions_refuse(self):
        observed = [128,132,140,140,148,144,128,140,144,152,148]
        self.assertEqual(two.capacity_bound(observed),([960,584],[960,920]))
        with self.assertRaises(ValueError):
            two.capacity_bound(observed[:-1])
        different = list(observed)
        different[6] += 1
        different[7] -= 1
        with self.assertRaises(ValueError):
            two.capacity_bound(different)

    def test_wrong_group_closure_rejected_even_if_numbers_match(self):
        ref = {"sha256":"a"}
        row = {"schema":"airk.private.cal100-group-lifetime/1","group":0,"prior":ref,"plan":ref,
               "publication":"same","phases":list(range(6))}
        with patch.object(two,"linked",return_value=row):
            with self.assertRaisesRegex(ValueError,"grouped closure"):
                two.verify_group(ref,ref,ref,0,"same")


if __name__ == "__main__":
    unittest.main()
