# SPDX-License-Identifier: Apache-2.0
import copy
import sys
from pathlib import Path
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import r6_epoch as epoch


class Epoch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original=epoch.read(epoch.reference(epoch.R5,epoch.R5_SHA))
        cls.ids=[key for phase in cls.original["phase_ids"] for key in phase]

    def test_correct_partition_and_bound(self):
        epoch.original_clock(self.original,self.original,self.ids)

    def test_short_population_extended_clock_or_changed_anchor(self):
        for issue in ("less","clock","anchor","different-group","debit"):
            with self.subTest(issue=issue):
                plan=copy.deepcopy(self.original)
                if issue=="less":plan["phase_ids"][-1].pop()
                if issue=="clock":plan["deadline_ms"]+=900000
                if issue=="anchor":plan["installation"]["ready"]["sha256"]="0"*64
                if issue=="different-group":plan["phase_groups"][0].pop()
                if issue=="debit":plan["setup_elapsed_ms"]-=1
                with self.assertRaises(ValueError):
                    epoch.original_clock(plan,self.original,self.ids)

    def test_rejects_r5_output_before_opening_any_question(self):
        with self.assertRaisesRegex(ValueError,"dedicated"):
            epoch.preflight(epoch.R5.parent,{},{},"0"*64)

    def test_foreign_service_does_not_inherit_original_audit(self):
        with self.assertRaisesRegex(ValueError,"unbound service"):
            epoch.audit_for_epoch(self.original["input_audit"],"0"*64)


if __name__=="__main__":
    unittest.main()
