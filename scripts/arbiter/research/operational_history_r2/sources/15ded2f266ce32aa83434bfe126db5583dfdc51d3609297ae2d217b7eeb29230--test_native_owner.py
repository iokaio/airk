# SPDX-License-Identifier: Apache-2.0
import unittest
from run_native_focus import remaining_seconds,natural

class NativeOwnerControls(unittest.TestCase):
    def test_original_phase_deadline_decreases_through_pre_and_post_input_work(self):
        deadline=1600
        self.assertEqual(remaining_seconds(deadline,1000),600)
        self.assertEqual(remaining_seconds(deadline,1090),510)
        self.assertEqual(remaining_seconds(deadline,1599.5),.5)
        with self.assertRaises(TimeoutError):remaining_seconds(deadline,1600)
        with self.assertRaises(TimeoutError):remaining_seconds(deadline,1601)
    def test_only_one_natural_observed_exit_and_empty_job_can_complete(self):
        good=dict(exit_code=0,timed_out=False,log_oversized=False,owned_descendants_settled=True,
                  owned_job_forced_termination=False,unexpected_third_process=False,owned_total_processes=1,
                  owned_job_final_member_pids=[],owned_job_final_active_processes=0,
                  retained_member_exits=[dict(exit_observed=True,exit_code=0)])
        self.assertTrue(natural(good))
        for field,value in [('timed_out',True),('owned_job_forced_termination',True),('owned_total_processes',2),('owned_job_final_active_processes',1),('owned_job_final_member_pids',[999])]:
            bad={**good,field:value}
            with self.subTest(field=field):self.assertFalse(natural(bad))
        self.assertFalse(natural({**good,'retained_member_exits':[dict(exit_observed=False,exit_code=0)]}))
if __name__=='__main__':unittest.main()
