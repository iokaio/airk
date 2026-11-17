# SPDX-License-Identifier: Apache-2.0
"""Literal native Job accounting controls; no actual OS child or AIRK starts."""
import unittest
from os_hook_controls_r2 import settled_actual_job
class Census(unittest.TestCase):
    def test_every_actual_member_must_be_empty_without_inventing_process_cap(self):
        self.assertTrue(settled_actual_job(1,0,[]))
        self.assertTrue(settled_actual_job(2,0,[]))
        self.assertTrue(settled_actual_job(4,0,[]))
        self.assertFalse(settled_actual_job(2,1,[]))
        self.assertFalse(settled_actual_job(2,0,[77]))
        self.assertFalse(settled_actual_job(0,0,[]))
        self.assertFalse(settled_actual_job(True,0,[]))
if __name__=='__main__':unittest.main()
