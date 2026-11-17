# SPDX-License-Identifier: Apache-2.0
"""Literal final-retention deadline controls; zero filesystem/native/model activity."""
import unittest
from types import SimpleNamespace
from launch_native import finish_observations

class RetainedStdout:
    def is_file(self):return True
    def stat(self):return SimpleNamespace(st_size=3)
    def __str__(self):return 'SYNTHETIC-ONLY-STDOUT'

class FinalObservation(unittest.TestCase):
    def receipt(self):return {'complete':True,'measurement_passed':True,'failure':None,'post_input_drift':[]}
    def test_final_stdout_hash_can_expire_original_phase(self):
        now=[99];calls=[];receipt=self.receipt()
        def retain(_):calls.append('hash');now[0]=101;return 'synthetic-sha'
        def clock():calls.append('clock');return now[0]
        finish_observations(receipt,0,100,RetainedStdout(),clock,retain)
        self.assertEqual(calls,['hash','clock'])
        self.assertEqual(receipt['elapsed_seconds'],101)
        self.assertIs(receipt['whole_phase_expired'],True)
        self.assertIs(receipt['complete'],False)
        self.assertIs(receipt['measurement_passed'],False)
        self.assertEqual(receipt['failure']['type'],'FinalObservationRefusal')
    def test_before_final_boundary_uses_same_single_clock_sample(self):
        receipt=self.receipt();calls=[]
        def clock():calls.append(99);return 99
        finish_observations(receipt,10,100,RetainedStdout(),clock,lambda _:'synthetic-sha')
        self.assertEqual(calls,[99]);self.assertEqual(receipt['elapsed_seconds'],89)
        self.assertIs(receipt['whole_phase_expired'],False)
        self.assertIs(receipt['complete'],True)

    def test_exact_whole_phase_expiry_refuses(self):
        receipt=self.receipt()
        finish_observations(receipt,10,100,RetainedStdout(),lambda:100,lambda _:'synthetic-sha')
        self.assertEqual(receipt['elapsed_seconds'],90)
        self.assertIs(receipt['whole_phase_expired'],True)
        self.assertIs(receipt['measurement_passed'],False)

if __name__=='__main__':unittest.main()
