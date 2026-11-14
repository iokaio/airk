# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import tempfile
import unittest

from run_goal_condition import guard_output_location,ROOT


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


if __name__=='__main__':unittest.main()
