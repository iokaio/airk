# SPDX-License-Identifier: Apache-2.0
"""Synthetic parser/observer controls only; no actual service/model execution."""
import unittest
from unittest.mock import patch
from run_public_focus import FOCUS,applied_focus,profile_row,remaining
class PublicPreparationTests(unittest.TestCase):
    def test_native_profile_identity_is_exact_and_unambiguous(self):
        self.assertEqual(profile_row({'focus':{'profiles':[{'profile':FOCUS,'availability':'missing_assets'}]}})['availability'],'missing_assets')
        for rows in ([],[{'profile':'f'*64}],[{'profile':FOCUS},{'profile':FOCUS}]):
            with self.assertRaises(ValueError):profile_row({'focus':{'profiles':rows}})
    def test_ready_or_notice_is_not_actual_scoring_application(self):
        self.assertFalse(applied_focus({'focus':{'kind':'not_applied','availability':'ready'}}))
        self.assertFalse(applied_focus({'focus':{'kind':'receipt','receipt':{'applied':False}}}))
        self.assertTrue(applied_focus({'focus':{'kind':'receipt','receipt':{'applied':True}}}))
    def test_observation_uses_original_cutoff_without_renewal(self):
        with patch('run_public_focus.time.monotonic',return_value=50):self.assertEqual(remaining(70),20)
        with patch('run_public_focus.time.monotonic',return_value=70),self.assertRaises(TimeoutError):remaining(70)
if __name__=='__main__':unittest.main()
