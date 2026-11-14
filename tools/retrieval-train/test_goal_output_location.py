# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import tempfile
import unittest

from run_goal_condition import guard_output_location,original_task_metadata,ROOT


class GoalOutputLocationTests(unittest.TestCase):
    def test_entire_fixture_and_separate_bound_source_refused_before_creation(self):
        with tempfile.TemporaryDirectory(dir=ROOT/'artifacts') as directory:
            root=Path(directory);fixture=root/'fixture';source=root/'other-source'
            fixture.mkdir();source.mkdir();rows=[{'source_root':str(source)}]
            for output in (fixture/'not-created',source/'not-created'):
                with self.assertRaisesRegex(ValueError,'outside all frozen'):guard_output_location(output,fixture,rows)
                self.assertFalse(output.exists())
            separate=root/'evaluation'
            self.assertEqual(guard_output_location(separate,fixture,rows),separate.resolve())
            self.assertFalse(separate.exists())

    @unittest.skipUnless(__import__('os').name=='nt','Windows actual extended-path alias')
    def test_windows_actual_extended_fixture_alias_refused(self):
        with tempfile.TemporaryDirectory(dir=ROOT/'artifacts') as directory:
            fixture=Path(directory)/'fixture';fixture.mkdir()
            alias=Path('\\\\?\\'+str(fixture.resolve()))
            output=alias/'not-created'
            with self.assertRaisesRegex(ValueError,'outside all frozen'):guard_output_location(output,fixture,[])
            self.assertFalse(output.exists())


class GoalConditionMetadataTests(unittest.TestCase):
    def test_development_does_not_invent_a_final_bootstrap_component(self):
        task={'dataset':'QASPER','original_question_categories':{'answer_kinds':['short_answer']},'disagreement':False}
        value=original_task_metadata(task,'development')
        self.assertIsNone(value['source_component'])
        self.assertEqual(value['original_question_categories'],task['original_question_categories'])
        self.assertFalse(value['original_human_disagreement'])

    def test_final_still_requires_its_frozen_dependency_component(self):
        task={'dataset':'QASPER','original_question_categories':{},'disagreement':False}
        with self.assertRaises(KeyError):original_task_metadata(task,'final')
        task['bootstrap_source_component']='original-source-component'
        self.assertEqual(original_task_metadata(task,'final')['source_component'],'original-source-component')


if __name__=='__main__':unittest.main()
