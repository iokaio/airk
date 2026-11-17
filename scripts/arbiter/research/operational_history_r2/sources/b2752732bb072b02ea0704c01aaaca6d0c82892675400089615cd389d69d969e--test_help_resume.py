# SPDX-License-Identifier: Apache-2.0
"""Pure resume count/order/refusal controls; no browser, npm or native execution."""
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('help_resume',Path(__file__).with_name('resume_final_help.py'))
resume=importlib.util.module_from_spec(spec);spec.loader.exec_module(resume)

class ResumeControls(unittest.TestCase):
    def receipt(self,**updates):
        value={'exit_code':0,'unexpected_changes':[],'usage_input_unchanged':True,
            'compiled_out_changed':False,'skipped_tests':0,'expected_selected':21,'selected_tests':21}
        return dict(value,**updates)

    def test_same21_tests_success_only_when_all_passed_without_skips(self):
        self.assertFalse(resume.failed_step(self.receipt()))
        self.assertTrue(resume.failed_step(self.receipt(skipped_tests=1)))

    def test_zero_or_changed_selected_count_refuses(self):
        for count in (None,0,20,22):
            with self.subTest(count=count):self.assertTrue(resume.failed_step(self.receipt(selected_tests=count)))

    def test_original_failure_not_hidden_by_correct_count(self):
        self.assertTrue(resume.failed_step(self.receipt(exit_code=1)))

    def test_unexpected_source_change_refuses(self):
        self.assertTrue(resume.failed_step(self.receipt(unexpected_changes=['other.rs'])))

    def test_usage_and_compiled_out_are_immutable(self):
        self.assertTrue(resume.failed_step(self.receipt(usage_input_unchanged=False)))
        self.assertTrue(resume.failed_step(self.receipt(compiled_out_changed=True)))

    def test_static_tests_need_nonzero_selection(self):
        self.assertTrue(resume.failed_step(self.receipt(expected_selected='nonzero',selected_tests=0)))
        self.assertFalse(resume.failed_step(self.receipt(expected_selected='nonzero',selected_tests=5)))

    def test_admin_build_check_seal_precede_actual_generated_artifact_dom_test(self):
        command=['node-fixture','--test','actual-same-R1-test-list']
        steps=resume.stages(Path('receipt-fixture'),{'shared_test_command':command})
        self.assertEqual([step['name'] for step in steps],
            ['admin-build','admin-check','admin-seal','shared-help-tests','static-help-tests'])
        self.assertEqual(steps[3]['command'],command)
        self.assertEqual(steps[3]['expected_selected'],21)
        self.assertFalse(any('cargo' in part.lower() for step in steps for part in step['command']))
        self.assertFalse(any(part=='compile' or part=='ci' for step in steps for part in step['command']))

if __name__=='__main__':unittest.main()
