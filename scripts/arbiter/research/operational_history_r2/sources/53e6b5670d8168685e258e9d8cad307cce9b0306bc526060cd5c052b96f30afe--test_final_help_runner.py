# SPDX-License-Identifier: Apache-2.0
"""Pure refusal/recovery controls; no real command, Cargo, model or UI writes."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('prepared_help_runner', Path(__file__).with_name('run_final_help.py'))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)

class GenerationSafety(unittest.TestCase):
    def exercise(self, *, log='running 1 test\n', code=0, after=None, usage_changed=False, seal_wrong=False, gap=False):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        out = root/'artifacts/receipts'
        static = root/'static.json'; static.write_text('{"environment":{}}')
        node = root/'node.exe'; node.write_bytes(b'fictional node; never executed')
        usage = root/'fixtures/split/ui_pilot/usage_native.json'
        usage.parent.mkdir(parents=True); usage.write_bytes(b'original usage identity')
        initial = {'fixture.json': 'original'}
        observed = [initial, initial, after if after is not None else initial]
        if gap:
            observed.append({'fixture.json': 'changed between commands'})
        calls = []
        def inputs():
            return observed.pop(0) if observed else (after if after is not None else initial)
        def process(command, **options):
            calls.append(options['env'])
            options['stdout'].write(log)
            if usage_changed:
                usage.write_bytes(b'changed usage identity')
            return subprocess.CompletedProcess(command, code)
        stages = [dict(name='emitter',command=['never-executed'],write_variable='AIRK_WRITE_HELP_CONTENT',
                       allowed_changes=['fixture\\.json'],test=True,expected_selected=1),
                  dict(name='readback',command=['never-executed'],write_variable=None,
                       allowed_changes=[],test=True,expected_selected=1)]
        argv = ['runner', '--execute-reviewed', '--expected-source-sha256',
                'wrong' if seal_wrong else runner.seal({'source_files': initial,
                    'bundled_model_directory': str((root/'artifacts/advanced-split/bundled-model-v2').resolve())}), '--out', str(out)]
        with patch.object(runner, 'ROOT', root), patch.object(runner, 'STATIC', static), \
                patch.object(runner, 'STATIC_SHA', runner.digest(static)), patch.object(runner, 'NODE', node), \
                patch.object(runner, 'NODE_SHA', runner.digest(node)), \
                patch.object(runner, 'versions', return_value='1.9.0'), patch.object(runner, 'inputs', side_effect=inputs), \
                patch.object(runner, 'stages', return_value=stages), patch.object(runner, 'inventory', return_value={}), \
                patch.object(runner.subprocess, 'run', side_effect=process), \
                patch.dict(runner.os.environ, {'AIRK_WRITE_UI_PILOT_USAGE':'1',
                    'NPM_CONFIG_CACHE':'C:/human-cache-must-not-be-used',
                    'NPM_CONFIG_LOGS_DIR':'C:/human-logs-must-not-be-used',
                    'NPM_CONFIG_UPDATE_NOTIFIER':'true'}), patch.object(sys, 'argv', argv), patch('builtins.print'):
            if seal_wrong:
                with self.assertRaisesRegex(ValueError, 'source seal differs'):
                    runner.main()
                return out, calls
            result = runner.main()
        return result, out, calls

    def test_stale_source_refuses_before_commands_or_output_creation(self):
        out, calls = self.exercise(seal_wrong=True)
        self.assertEqual(calls, [])
        self.assertFalse(out.exists())

    def test_zero_selected_retains_failure_and_never_runs_next_stage(self):
        result, out, calls = self.exercise(log='running 0 tests\n')
        self.assertEqual(result, 3)
        self.assertEqual(len(calls), 1)
        self.assertEqual(json.loads((out/'emitter.json').read_bytes())['selected_tests'], 0)
        self.assertFalse((out/'result.json').exists())

    def test_emitter_requires_exactly_one_selected(self):
        result, out, calls = self.exercise(log='running 2 tests\n')
        self.assertEqual(result, 3)
        self.assertEqual(len(calls), 1)

    def test_native_failure_is_not_retried_and_keeps_original_exit_and_log(self):
        result, out, calls = self.exercise(code=17)
        self.assertEqual(result, 17)
        self.assertEqual(len(calls), 1)
        self.assertEqual(json.loads((out/'emitter.json').read_bytes())['exit_code'], 17)
        self.assertEqual((out/'emitter.log').read_text(), 'running 1 test\n')

    def test_unexpected_source_change_fails_closed(self):
        result, out, calls = self.exercise(after={'fixture.json':'original','other.rs':'changed'})
        self.assertEqual(result, 3)
        self.assertEqual(json.loads((out/'emitter.json').read_bytes())['unexpected_changes'], ['other.rs'])
        self.assertEqual(len(calls), 1)

    def test_usage_identity_cannot_change_even_during_generation(self):
        result, out, calls = self.exercise(usage_changed=True)
        self.assertEqual(result, 3)
        self.assertFalse(json.loads((out/'emitter.json').read_bytes())['usage_input_unchanged'])
        self.assertEqual(len(calls), 1)

    def test_write_variables_are_child_local_and_unknown_writer_is_removed(self):
        result, out, calls = self.exercise()
        self.assertEqual(result, 0)
        self.assertEqual({k:v for k,v in calls[0].items() if k.startswith('AIRK_WRITE_')}, {'AIRK_WRITE_HELP_CONTENT':'1'})
        self.assertEqual({k:v for k,v in calls[1].items() if k.startswith('AIRK_WRITE_')}, {})
        for child in calls:
            self.assertEqual(child['NPM_CONFIG_CACHE'], str(out.parent/'advanced-split/npm-cache'))
            self.assertEqual(child['NPM_CONFIG_LOGS_DIR'], str(out.parent/'advanced-split/npm-cache/_logs'))
            self.assertEqual(child['NPM_CONFIG_UPDATE_NOTIFIER'], 'false')
        self.assertTrue((out/'result.json').is_file())

    def test_only_declared_generation_output_can_change(self):
        result, out, calls = self.exercise(after={'fixture.json':'generated'})
        self.assertEqual(result, 0)
        receipt=json.loads((out/'emitter.json').read_bytes())
        self.assertEqual(receipt['changed_inputs'], ['fixture.json'])
        self.assertEqual(receipt['unexpected_changes'], [])

    def test_source_change_between_commands_refuses_before_next_process(self):
        result, out, calls = self.exercise(gap=True)
        self.assertEqual(result, 3)
        self.assertEqual(len(calls), 1)
        refusal=json.loads((out/'readback.json').read_bytes())
        self.assertEqual(refusal['result'], 'refused_source_changed_between_steps')
        self.assertFalse(refusal['command_executed'])

if __name__ == '__main__':
    unittest.main()
