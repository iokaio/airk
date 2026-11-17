# SPDX-License-Identifier: Apache-2.0
"""Pure conversion custody controls; no fixture writes or third-party imports."""
import copy
import importlib.util
from pathlib import Path
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('fp16_execution', ROOT / 'scripts/arbiter-fp16-execution.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def row(name='literal', size=1, sha='a' * 64):
    return {'path': str(ROOT / name), 'bytes': size, 'sha256': sha}


def fixture():
    plan = {'schema': M.SCHEMA, 'human_sid': M.SID, 'limits': dict(M.LIMITS), 'model': 'compact',
            'expected_weights_bytes': 3441397760, 'expected_attention_count': 28,
            'repository_root': str(ROOT), 'output_root': str(ROOT / 'artifacts/absent-literal-output'),
            'cache_prefix': str(ROOT / 'absent-literal-cache'),
            'prerequisite_evidence': {'first_fp32': row('fp32-proof'), 'native_g5': row('g5-proof')},
            'helpers': {name: row(name, sha=sha) for name, sha in M.HELPERS.items()}}
    for key in ('python', 'powershell', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest'):
        plan[key] = row(key)
    plan['wrapper'] = row('scripts/arbiter-fp16-execution.py')
    plan['input_files'] = [plan[key] for key in ('python', 'powershell', 'wrapper', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest')] + list(plan['helpers'].values()) + list(plan['prerequisite_evidence'].values())
    return plan


class PlanControls(unittest.TestCase):
    def validate(self, plan):
        with patch.object(M.Path, 'exists', return_value=False), patch.object(M.Path, 'is_dir', return_value=True):
            return M.validate_plan(plan)

    def test_exact_original_conversion_limits(self):
        self.assertEqual(M.LIMITS, {'wall_seconds': 600, 'host_committed_bytes': 8589934592, 'log_bytes': 67108864, 'cleanup_seconds': 5})
        self.validate(fixture())

    def test_bounds_cannot_be_extended(self):
        for key in M.LIMITS:
            plan = fixture(); plan['limits'][key] += 1
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(plan)

    def test_identity_or_schema_cannot_be_reinterpreted(self):
        for key in ('human_sid', 'schema', 'model'):
            plan = fixture(); plan[key] = 'other'
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(plan)

    def test_prerequisite_pins_are_required_not_generated(self):
        plan = fixture(); del plan['prerequisite_evidence']['first_fp32']
        with self.assertRaises(ValueError): self.validate(plan)

    def test_missing_helper_or_changed_helper_refuses(self):
        plan = fixture(); del plan['helpers']['reference_worker.py']
        with self.assertRaises(ValueError): self.validate(plan)
        plan = fixture(); plan['helpers']['reference_worker.py']['sha256'] = 'b' * 64
        with self.assertRaises(ValueError): self.validate(plan)

    def test_omitted_or_duplicate_input_refuses(self):
        plan = fixture(); plan['input_files'].pop()
        with self.assertRaises(ValueError): self.validate(plan)
        plan = fixture(); plan['input_files'].append(plan['input_files'][0])
        with self.assertRaises(ValueError): self.validate(plan)

    def test_wrong_attention_roster_refuses(self):
        plan = fixture(); plan['expected_attention_count'] = 36
        with self.assertRaises(ValueError): self.validate(plan)

    def test_sibling_output_escape_refuses(self):
        plan = fixture(); plan['output_root'] = str(ROOT.parent / 'airk-other/artifacts/output')
        with self.assertRaises(ValueError): self.validate(plan)

    def test_lexical_dotdot_output_escape_refuses(self):
        plan = fixture(); plan['output_root'] = str(ROOT / 'artifacts/../scripts/new-output')
        with self.assertRaises(ValueError): self.validate(plan)

    def test_existing_output_refuses(self):
        with patch.object(M.Path, 'exists', return_value=True), self.assertRaises(ValueError):
            M.validate_plan(fixture())

    def test_complete_runtime_roster_required(self):
        plan = fixture(); manifest = {'roots': [str(ROOT)], 'files': {}, 'import_paths': []}
        with patch.object(M, 'validate_plan', return_value={}), patch.object(M, 'pinned'), patch.object(M, 'read', return_value=manifest), patch.object(M, 'file_roster', return_value={str(ROOT / 'extra.pyd')}), self.assertRaisesRegex(ValueError, 'roster'):
            M.authenticate(plan)

    def test_runtime_member_must_be_in_input_seal(self):
        plan = fixture(); item = row('module.py'); manifest = {'roots': [str(ROOT)], 'files': {item['path']: item}, 'import_paths': []}
        with patch.object(M, 'validate_plan', return_value={}), patch.object(M, 'pinned'), patch.object(M, 'read', return_value=manifest), patch.object(M, 'file_roster', return_value={item['path']}), self.assertRaisesRegex(ValueError, 'omitted'):
            M.authenticate(plan)

    def test_loaded_module_outside_seal_refuses(self):
        module = types.SimpleNamespace(__file__=__file__)
        with patch.dict(M.sys.modules, {'literal': module}, clear=True), self.assertRaisesRegex(ValueError, 'outside sealed'):
            M.loaded_runtime({})


class FinalProofControls(unittest.TestCase):
    def callbacks(self, events):
        return {name: lambda name=name: (events.append(name), row(name))[1] for name in M.PROOFS}

    def test_last_clock_follows_all_six_callbacks(self):
        events = []; receipt = {'complete': True, 'failure': None}
        M.finish(receipt, 0, 600, self.callbacks(events), clock=lambda: (events.append('clock'), 599)[1], utc_clock=lambda: (events.append('utc'), M.datetime(2026, 11, 17, tzinfo=M.timezone.utc))[1])
        self.assertTrue(receipt['complete'])
        self.assertEqual(events, ['graph', 'weights', 'manifest', 'stdout', 'worker', 'inputs', 'utc', 'clock'])

    def test_slow_final_inputs_cannot_pass(self):
        events = []; receipt = {'complete': True, 'failure': None}
        M.finish(receipt, 0, 600, self.callbacks(events), clock=lambda: 600)
        self.assertFalse(receipt['complete']); self.assertTrue(receipt['whole_phase_expired'])

    def test_missing_callback_does_not_skip_remaining_proofs(self):
        events = []; callbacks = self.callbacks(events); del callbacks['manifest']
        receipt = M.finish({'complete': True, 'failure': None}, 0, 600, callbacks, clock=lambda: 1)
        self.assertFalse(receipt['complete']); self.assertIn('inputs', events)
        self.assertEqual(receipt['final_errors'][0]['proof'], 'manifest')

    def test_original_failure_survives_successful_callbacks(self):
        original = {'type': 'ChildFailure'}
        receipt = M.finish({'complete': True, 'failure': original}, 0, 600, self.callbacks([]), clock=lambda: 1)
        self.assertFalse(receipt['complete']); self.assertIs(receipt['failure'], original)

    def test_malformed_callback_rows_refuse(self):
        for bad in (None, {}, row(sha='z' * 64), row(size=-1)):
            callbacks = self.callbacks([]); callbacks['graph'] = lambda: bad
            with self.subTest(bad=bad):
                receipt = M.finish({'complete': True, 'failure': None}, 0, 600, callbacks, clock=lambda: 1)
                self.assertFalse(receipt['complete'])

    def test_hash_exception_keeps_all_later_proofs(self):
        events = []; callbacks = self.callbacks(events)
        def failed(): raise OSError('retained hash denied')
        callbacks['weights'] = failed
        receipt = M.finish({'complete': True, 'failure': None}, 0, 600, callbacks, clock=lambda: 1)
        self.assertFalse(receipt['complete']); self.assertEqual(events[-1], 'inputs')
        self.assertEqual(receipt['final_errors'][0]['type'], 'OSError')

    def test_post_validation_drift_refuses(self):
        receipt = {'complete': True, 'failure': None, 'observed_proofs': {'graph': row('graph', sha='b' * 64)}}
        M.finish(receipt, 0, 600, self.callbacks([]), clock=lambda: 1)
        self.assertFalse(receipt['complete'])

    def test_nonfinite_or_backwards_clock_refuses(self):
        for now in (-1, float('nan'), float('inf')):
            receipt = M.finish({'complete': True, 'failure': None}, 0, 600, self.callbacks([]), clock=lambda: now)
            self.assertFalse(receipt['complete'])


class GuardDeadlineControls(unittest.TestCase):
    def test_guard_uses_only_remaining_original_deadline(self):
        calls = []
        def runner(command, **kwargs):
            calls.append(kwargs['timeout'])
            return types.SimpleNamespace(stdout='{"identity":"literal"}')
        with patch.object(M, 'pinned'):
            value = M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: 599.75, runner=runner)
        self.assertEqual(calls, [.25]); self.assertEqual(value, {'identity': 'literal'})

    def test_guard_timeout_preserves_original_exception(self):
        failure = M.subprocess.TimeoutExpired('literal guard', .25)
        def runner(command, **kwargs):
            self.assertEqual(kwargs['timeout'], .25)
            raise failure
        with patch.object(M, 'pinned'):
            with self.assertRaises(M.subprocess.TimeoutExpired) as caught:
                M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: 599.75, runner=runner)
        self.assertIs(caught.exception, failure)

    def test_expired_guard_is_not_launched(self):
        def runner(*args, **kwargs): self.fail('expired guard launched')
        with patch.object(M, 'pinned'), self.assertRaises(TimeoutError):
            M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: 600, runner=runner)

    def test_guard_postprocessing_cannot_pass_after_original_deadline(self):
        values = iter([599, 600])
        with patch.object(M, 'pinned'), self.assertRaises(TimeoutError):
            M.guard(fixture(), ROOT / 'literal', deadline=600, clock=lambda: next(values), runner=lambda *a, **kw: types.SimpleNamespace(stdout='{}'))

class OutputControls(unittest.TestCase):
    def test_worker_cannot_spawn_or_connect(self):
        for event in ('subprocess.Popen', 'os.system', 'os.posix_spawn', 'socket.connect'):
            with self.subTest(event=event), self.assertRaises(ValueError): M.output_access(event, (), ROOT)

    def test_dotdot_write_escape_refuses(self):
        target = ROOT / 'artifacts/absent-output'
        with self.assertRaisesRegex(ValueError, 'outside'):
            M.output_access('open', (str(target / '../escape'), 'w', M.os.O_CREAT | M.os.O_WRONLY), target, owner=lambda p: None)

    def test_new_write_parent_is_checked(self):
        target = ROOT / 'artifacts/absent-output'; seen = []
        M.output_access('open', (str(target / 'new'), 'w', M.os.O_CREAT | M.os.O_WRONLY), target, owner=seen.append)
        self.assertEqual(seen, [target])

    def test_foreign_output_parent_refuses(self):
        def foreign(path): raise ValueError('foreign owner')
        with self.assertRaisesRegex(ValueError, 'foreign'):
            M.output_access('os.mkdir', (str(ROOT / 'artifacts/new'),), ROOT / 'artifacts', owner=foreign)

    def test_manifest_binds_actual_graph_size_and_roster(self):
        plan = fixture(); graph = row('model.onnx'); weights = row('weights.bin', size=3441397760)
        manifest = {'qualification': False, 'native_execution': False, 'graph_sha256': graph['sha256'], 'attention_nodes': [{'name': f'/airk_g5/attention/{i}/efficient', 'operator': 'GroupQueryAttention', 'domain': 'com.microsoft'} for i in range(28)]}
        M.validate_manifest(manifest, plan, graph, weights)
        for field in ('qualification', 'native_execution', 'graph_sha256', 'attention_nodes'):
            bad = copy.deepcopy(manifest); bad[field] = True if field in ('qualification', 'native_execution') else 'changed'
            with self.subTest(field=field), self.assertRaises(ValueError): M.validate_manifest(bad, plan, graph, weights)
        with self.assertRaises(ValueError): M.validate_manifest(manifest, plan, graph, row('weights.bin', size=1))


if __name__ == '__main__':
    unittest.main()
