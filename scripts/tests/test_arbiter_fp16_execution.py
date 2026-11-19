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
    for key in ('python', 'powershell', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest', 'source_plan', 'runtime_baseline', 'prerequisites_manifest'):
        plan[key] = row(key)
    plan['wrapper'] = row('scripts/arbiter-fp16-execution.py')
    plan['input_files'] = [plan[key] for key in ('python', 'powershell', 'wrapper', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest', 'source_plan', 'runtime_baseline', 'prerequisites_manifest')] + list(plan['helpers'].values()) + list(plan['prerequisite_evidence'].values())
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


class IndependentReferenceImportControls(unittest.TestCase):
    def test_only_the_exact_existing_audited_cache_directory_may_be_checked(self):
        path = ROOT / 'scripts/arbiter-fp16-execution/verify_compact.py'
        spec = importlib.util.spec_from_file_location('fp16_reference_worker_cache', path)
        reference = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reference)
        reference.read_only('os.mkdir', (str(ROOT), 0o700, -1), ROOT)
        for proposed, fd in ((ROOT / 'absent-cache-child', -1), (ROOT, 1)):
            with self.subTest(proposed=proposed, fd=fd), self.assertRaises(ValueError):
                reference.read_only('os.mkdir', (str(proposed), 0o700, fd), ROOT)
        with self.assertRaises(ValueError):
            reference.read_only('open', (str(ROOT / 'cache-data'), 'w', reference.os.O_WRONLY), ROOT)

    def test_generated_names_cannot_invent_a_file_or_hide_real_modules(self):
        path = ROOT / 'scripts/arbiter-fp16-execution/verify_compact.py'
        spec = importlib.util.spec_from_file_location('fp16_reference_worker', path)
        reference = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reference)

        class GeneratedNamespace(types.ModuleType):
            def __getattr__(self, key):
                return '_ops.py' if key == '__file__' else None

        synthetic = GeneratedNamespace('torch.ops')
        real = types.SimpleNamespace(__file__=str(path))
        real_pin = row('scripts/arbiter-fp16-execution/verify_compact.py')
        result = reference.loaded_runtime(
            {str(path): real_pin}, lambda file, check: real_pin, lambda: None, [synthetic, real])
        self.assertEqual(result, {str(path): real_pin['sha256']})
        with self.assertRaisesRegex(ValueError, 'outside sealed closure'):
            reference.loaded_runtime({}, lambda file, check: real_pin, lambda: None, [real])


class CompactT32ComparisonControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"
        spec = importlib.util.spec_from_file_location("compact_t32_probe", path)
        cls.probe = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.probe)

    def comparison_row(self):
        return {
            "matching_fp16_raw_error": .001,
            "matching_fp16_mass_error": .05,
            "matching_fp16_argmax_matches": True,
            "original_fp32_raw_error": .012271881,
            "original_fp32_mass_error": .05,
            "original_fp32_argmax_matches": False,
        }

    def test_original_score_drift_and_winner_changes_are_diagnostic(self):
        self.probe.validate_t32_comparisons(self.comparison_row())

    def test_matching_parity_original_mass_and_diagnostic_types_remain_required(self):
        for name, value in {
            "matching_fp16_raw_error": .0011,
            "matching_fp16_mass_error": .051,
            "matching_fp16_argmax_matches": False,
            "original_fp32_raw_error": float("inf"),
            "original_fp32_mass_error": .051,
            "original_fp32_argmax_matches": 0,
        }.items():
            invalid = self.comparison_row()
            invalid[name] = value
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.probe.validate_t32_comparisons(invalid)


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


class PreparationControls(unittest.TestCase):
    def fp32(self):
        return {'schema': 'airk.private.owned-compact-bounded-hybrid-profile/2', 'complete': True,
                'failure': None, 'execution_user_sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
                'whole_phase_deadline_not_refreshed': True, 'whole_phase_expired': False,
                'qualification': False, 'product_admission': False, 'elapsed_seconds': 119.5,
                'graph_custody': {'opened_handles': 15, 'closed_handles': 15, 'close_errors': []},
                'final_guard_errors': {}, 'final_proof_errors': [],
                'final_proof_success': ['native_result', 'operations', 'stdout', 'runtime_profile']}


    def readiness(self, kind='tests'):
        return {'schema': 'airk-readiness-command/1', 'status': 'completed', 'passed': True,
                'exit_code': 0, 'runner_exit_code': 0, 'interrupted': False,
                'inputs_unchanged': True, 'runner_unchanged': True, 'runtime_unchanged': True,
                'command_inputs_unchanged': True, 'selected_tests': 25,
                'command': ['cargo', 'nextest', 'run', '--profile', 'ci', '--locked', '-p', 'airk-retrieval', '--all-features']
                if kind == 'tests' else ['python', 'scripts/dev-check.py', '-p', 'airk-retrieval', '--all-features']}

    def test_original_source_identity_cannot_be_replaced(self):
        with self.assertRaisesRegex(ValueError, 'source plan identity'):
            M.source_facts(row(), 'compact')

    def test_focus_has_no_qualified_fp32_prerequisite(self):
        with self.assertRaisesRegex(ValueError, 'only Compact and Large'):
            M.source_facts(row(), 'focus')

    def test_wrong_model_receipt_refuses_before_open(self):
        evidence = {'model': 'large',
                    'first_fp32': row('compact-proof', sha='1abbd515e534f4056e3c48380fffd290654e0b92dea67088ae291512f02cf2a2'),
                    'resource_fp32': row('large-timing', sha='8ff0d3ed5152ea533a46ab1dc258b96b536aa7df2948b49400f275aabda8fc7b')}
        with self.assertRaisesRegex(ValueError, 'exact selected model'):
            M.fp32_facts(evidence, {}, lambda proof: self.fail('wrong-model proof opened'), None, {})

    def test_original_source_graph_and_closure_are_reopened(self):
        original = {'schema': 'airk.private.g5-original3-export-run-plan/1', 'models': [
            {'model': 'compact', 'original_graph': row('graph'), 'source_closure': row('closure'), 'source_closure_authority': row('authority')}]}
        source = row('source', sha='cdf57c6c86b490cf2b6656fa8c48fbb4ff3b6ccb4d5c5be9705a71a2866b735c')
        observed = []
        with patch.object(M, 'read', return_value=original), patch.object(M, 'pinned', side_effect=lambda value, check: observed.append(value['path'])):
            M.source_facts(source, 'compact')
        self.assertEqual(observed, [source['path'], str(ROOT / 'graph'), str(ROOT / 'closure'), str(ROOT / 'authority')])

    def test_missing_or_changed_source_member_refuses(self):
        original = {'schema': 'airk.private.g5-original3-export-run-plan/1', 'models': [
            {'model': 'compact', 'original_graph': row('graph'), 'source_closure': row('closure'), 'source_closure_authority': row('authority')}]}
        source = row('source', sha='cdf57c6c86b490cf2b6656fa8c48fbb4ff3b6ccb4d5c5be9705a71a2866b735c')
        for name in ('graph', 'closure', 'authority'):
            def verify(value, check):
                if Path(value['path']).name == name: raise ValueError('changed source')
            with self.subTest(name=name), patch.object(M, 'read', return_value=original), patch.object(M, 'pinned', side_effect=verify), self.assertRaisesRegex(ValueError, 'changed source'):
                M.source_facts(source, 'compact')

    def test_runtime_baseline_cannot_be_replaced(self):
        with self.assertRaisesRegex(ValueError, 'baseline identity'):
            M.runtime_facts(row(), [])

    def test_runtime_baseline_plus_added_files_are_explicit(self):
        baseline = row('baseline', sha='23f9fbfa770d18ee2549dbd6c1b553e204ab8e387ba51b92b620dd24c10189cb')
        one, added, outside = row('one'), row('added'), row('outside')
        pins = {x['path']: x for x in (one, added, outside)}
        value = {'schema': 'airk.private.exact-python-runtime-files/1', 'files': {one['path']: one['sha256'], outside['path']: outside['sha256']}}
        with patch.object(M, 'pinned'), patch.object(M, 'read', return_value=value), patch.object(M, 'file_roster', return_value={one['path'], added['path']}), patch.object(M, 'pin', side_effect=lambda path, check: pins[str(path)]):
            manifest, rest = M.runtime_facts(baseline, [ROOT])
        self.assertEqual(manifest['files'], {one['path']: one, added['path']: added})
        self.assertEqual(rest, {outside['path']: outside})

    def test_changed_or_missing_runtime_baseline_refuses(self):
        baseline = row('baseline', sha='23f9fbfa770d18ee2549dbd6c1b553e204ab8e387ba51b92b620dd24c10189cb')
        value = {'schema': 'airk.private.exact-python-runtime-files/1', 'files': {str(ROOT / 'missing'): 'b' * 64}}
        with patch.object(M, 'pinned'), patch.object(M, 'read', return_value=value), patch.object(M, 'file_roster', return_value=set()), patch.object(M, 'pin', return_value=row('missing')), self.assertRaisesRegex(ValueError, 'changed/missing'):
            M.runtime_facts(baseline, [ROOT])
        with patch.object(M, 'pinned'), patch.object(M, 'read', return_value=value), patch.object(M, 'file_roster', return_value=set()), patch.object(M, 'pin', side_effect=FileNotFoundError), self.assertRaises(FileNotFoundError):
            M.runtime_facts(baseline, [ROOT])

    def test_runtime_interpreter_pin_may_also_be_retained_process_evidence(self):
        shared = row('python.exe')
        pins = {shared['path']: shared}
        self.assertEqual(M.merge_input_pins({}, pins, pins), pins)
        changed = dict(shared, sha256='b' * 64)
        with self.assertRaisesRegex(ValueError, 'conflicting input pin'):
            M.merge_input_pins(pins, {shared['path']: changed})

    def test_actual_scoring_success_shape(self):
        M.successful_fp32(self.fp32(), M.FP32['compact'])

    def test_fp32_failed_late_missing_or_retagged_refuses(self):
        for key, value in [('complete', False), ('failure', {}), ('whole_phase_expired', True), ('elapsed_seconds', 120),
                           ('qualification', True), ('product_admission', True), ('execution_user_sid', 'other'),
                           ('final_proof_success', ['native_result']), ('final_proof_errors', [{}]),
                           ('final_guard_errors', {'output': 'changed'}),
                           ('graph_custody', {'opened_handles': 15, 'closed_handles': 14, 'close_errors': []})]:
            receipt = self.fp32(); receipt[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): M.successful_fp32(receipt, M.FP32['compact'])
        for key in self.fp32():
            receipt = self.fp32(); del receipt[key]
            with self.subTest(missing=key), self.assertRaises(ValueError): M.successful_fp32(receipt, M.FP32['compact'])

    def test_large_scoring_requires_the_equivalent_predecessor_basis(self):
        receipt = {'schema': 'airk.private.owned-large-cuda-batch-view-scoring/1', 'complete': True,
                   'failure': None, 'execution_user_sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
                   'qualification': False, 'elapsed_seconds': 84, 'reference_basis': 'equivalent_predecessor',
                   'model_requests_dispatched': True}
        M.successful_fp32(receipt, M.FP32['large'])
        for wrong in ('same_artifact', None):
            with self.subTest(wrong=wrong), self.assertRaisesRegex(ValueError, 'reference basis'):
                M.successful_fp32(dict(receipt, reference_basis=wrong), M.FP32['large'])

    def test_actual_readiness_shapes(self):
        M.successful_readiness(self.readiness(), 'tests')
        M.successful_readiness(self.readiness('clippy'), 'clippy')

    def test_readiness_failures_and_missing_predicates_refuse(self):
        for key, value in [('passed', False), ('exit_code', 1), ('runner_exit_code', 1), ('interrupted', True),
                           ('inputs_unchanged', False), ('runner_unchanged', False), ('runtime_unchanged', False),
                           ('command_inputs_unchanged', False), ('selected_tests', 0), ('command', ['cargo', 'check'])]:
            receipt = self.readiness(); receipt[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): M.successful_readiness(receipt, 'tests')

    def test_fp32_cannot_substitute_for_g5(self):
        for kind in ('clippy', 'tests'):
            with self.assertRaises(ValueError): M.successful_readiness(self.fp32(), kind)

    def test_junit_missing_required_tests_refuses(self):
        with self.assertRaisesRegex(ValueError, 'identities absent'):
            M.validate_junit(b'<testsuites><testcase classname="airk-retrieval" name="unrelated"/></testsuites>', 1)

    def test_junit_skipped_failed_and_duplicate_refuse(self):
        for tag in ('skipped', 'failure', 'error'):
            raw = ('<testsuites><testcase name="literal"><' + tag + '/></testcase></testsuites>').encode()
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, 'failure/skipped/error'):
                M.validate_junit(raw, 1)
        raw = b'<testsuites><testcase name="literal"/><testcase name="literal"/></testsuites>'
        with self.assertRaisesRegex(ValueError, 'unique identities'): M.validate_junit(raw, 2)

    def test_junit_entity_and_count_refuse(self):
        with self.assertRaisesRegex(ValueError, 'literal JUnit'):
            M.validate_junit(b'<!DOCTYPE testsuites><testsuites/>', 1)
        with self.assertRaisesRegex(ValueError, 'selected count'):
            M.validate_junit(b'<testsuites/>', 1)

    def test_g5_supplement_is_mandatory_and_independent_of_fp32(self):
        self.assertEqual(len(M.G5_TESTS), 25)
        self.assertIn('arbiter::runtime::cpu_source_transition::tests::g5_supplement_binds_exact_current_source_without_relabelling_old_row', M.G5_TESTS)
        self.assertIn('arbiter::runtime::cpu_source_transition::tests::cuda_bridge_and_driver_closure_preserve_closed_cpu_runtime_source', M.G5_TESTS)
        self.assertIn('crates/airk-retrieval/src/arbiter/runtime/cuda/windows.rs', M.G5_SOURCES)
        self.assertIn('crates/airk-retrieval/src/arbiter/runtime/cuda/fp16.rs', M.G5_SOURCES)
        self.assertIn('crates/airk-retrieval/src/arbiter/external_data/g5.rs', M.G5_SOURCES)

class G5EvidenceControls(unittest.TestCase):
    def test_actual_onnx_source_owner_is_crate_root(self):
        self.assertIn('crates/airk-retrieval/src/onnx.rs', M.G5_SOURCES)
        self.assertNotIn('crates/airk-retrieval/src/arbiter/onnx.rs', M.G5_SOURCES)
        self.assertTrue((ROOT / 'crates/airk-retrieval/src/onnx.rs').is_file())
    def test_g5_source_is_reopened_and_missing_group_entry_refuses(self):
        with patch.object(M, 'G5_SOURCES', ('literal.rs',)), patch.object(M, 'pin', return_value=row('literal.rs')):
            self.assertEqual(M.g5_source_facts({'files': {'literal.rs': 'a' * 64}}, ROOT), {str(ROOT / 'literal.rs'): row('literal.rs')})
            with self.assertRaisesRegex(ValueError, 'absent/changed'):
                M.g5_source_facts({'files': {}}, ROOT)
            with self.assertRaisesRegex(ValueError, 'absent/changed'):
                M.g5_source_facts({'files': {'literal.rs': 'b' * 64}}, ROOT)

    def test_missing_live_g5_source_refuses(self):
        with patch.object(M, 'G5_SOURCES', ('literal.rs',)), patch.object(M, 'pin', side_effect=FileNotFoundError), self.assertRaises(FileNotFoundError):
            M.g5_source_facts({'files': {'literal.rs': 'a' * 64}}, ROOT)

    def test_literal_g5_and_transition_roster_passes(self):
        # Independent expected identities from the reviewed G5/CPU contracts.
        fp16 = ['inherited_exact_settings_pass_and_late_setter_or_drift_refuses',
                'fp32_policy_remains_distinct_from_each_model_fp16_policy',
                'context_boundaries_and_exact_original_mask_positions_refuse_unsupported_input',
                'focus_all_live_and_extreme_left_pad_pass_without_renumbering',
                'half_initializer_bit_loading_preserves_half_values_without_rounding',
                'actual_final_placement_needs_exact_complete_attention_roster',
                'qualified_product_math_does_not_force_attention_debug_logging']
        external = ['three_literal_selected_rosters_admit_actual_half_boundaries',
                    'wrong_scale_grouping_causal_and_bias_refuse_before_session',
                    'wrong_half_cast_and_missing_layer_refuse', 'focus_bias_and_wrong_head_count_refuse',
                    'all_f32_and_overlap_are_not_half_weight_closure',
                    'large_half_metadata_is_additive_and_never_allocates_tensor_bytes',
                    'unknown_quantized_half_closure_refuses',
                    'focus_unwritten_padded_queries_require_explicit_sanitization']
        cpu = ['closed_profile_refuses_neural_metadata_fit_and_source_drift',
               'cuda_bridge_and_driver_closure_preserve_closed_cpu_runtime_source',
               'current_runtime_predicate_preserves_the_exact_legacy_cpu_literal',
               'exact_legacy_descriptor_and_public_sources_remain_byte_stable',
               'native_version_scope_is_pre_190_exact_release_only',
               'source_row_refuses_wrong_current_core_or_task_policy',
               'publisher_census_row_refuses_old_task_and_preserves_legacy_payload',
               'transition_never_admits_context_or_relabels_a_current_profile',
               'witness_rechecks_the_exact_descriptor_and_keeps_new_input_provenance',
               'g5_supplement_binds_exact_current_source_without_relabelling_old_row']
        names = ['arbiter::runtime::cuda::fp16::tests::' + x for x in fp16]
        names += ['arbiter::external_data::g5::tests::' + x for x in external]
        names += ['arbiter::runtime::cpu_source_transition::tests::' + x for x in cpu]
        def document(values):
            return ('<testsuites>' + ''.join('<testcase classname="airk-retrieval" name="' + name + '"/>' for name in values) + '</testsuites>').encode()
        M.validate_junit(document(names), 25)
        for omitted in names:
            remaining = [name for name in names if name != omitted]
            with self.subTest(omitted=omitted), self.assertRaisesRegex(ValueError, 'identities absent'):
                M.validate_junit(document(remaining), 24)

class FP32ProcessProofControls(unittest.TestCase):
    def fixture(self, model='compact'):
        large = model == 'large'
        host, device = (48 << 30, 44 << 30) if large else (32 << 30, 24 << 30)
        arena, warm, headroom = (40 << 30, 8 << 30, 4 << 30) if large else (22 << 30, 16 << 30, 2 << 30)
        image = row('native-image')
        sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
        proof = {'pid': 123, 'image': {'path': str(ROOT / 'native-image'), 'sha256': 'a' * 64},
                 'execution_user_sid': sid, 'default_owner_sid': sid}
        if not large:
            proof['component_job'] = {'process_bytes': 34359738368, 'job_bytes': 34359738368,
                                      'flags': 8968, 'active_limit': 2}
        receipt = {'owned': {'owned_leader_pid': 123, 'suspended_assignment': proof,
                            'peak_host_committed_bytes': host - 1}}
        native = {'pid': 123, 'execution_user': sid, 'default_owner': sid, 'native_binary_sha256': 'a' * 64,
                  'actual_job': {'process_memory_limit_bytes': host, 'job_memory_limit_bytes': host, 'flags': 8968},
                  'native_host_reservation_bytes': host, 'peak_process_rss_bytes': host - 1,
                  'applied_capacity': {'arena_limit_bytes': arena, 'warm_extra_bytes': warm, 'headroom_bytes': headroom},
                  'bounds': {'schema': 'airk.arbiter.gpu-probe-reservation/1', 'deadline_ms': 120000,
                             'device_limit_bytes': device, 'host_limit_bytes': host},
                  'profile': '7e4481caaf7034a70779702edad73a0eb08321418d1b785a4b7f4f891522bdac' if large else
                             '840bd478675cbf03f2477c170401af75027d55de82b6ab69933f971a1104531d',
                  'complete': True, 'error': None, 'native_control_cleanup_error': None,
                  'session_teardown_observed': True, 'native_host_charge_release_observed': True,
                  'native_control_cleanup_observed': True, 'qualified': False, 'product_admission': False,
                  'process_cuda_poisoned': False, 'native_control_retained_for_unknown_cuda': False}
        return receipt, native, image, M.FP32[model], lambda owned: True

    def test_both_actual_suspended_os_proof_shapes(self):
        for model in ('compact', 'large'):
            with self.subTest(model=model):
                M.validate_fp32_process(*self.fixture(model))

    def test_missing_or_changed_suspended_proof_refuses(self):
        for key in ('pid', 'image', 'execution_user_sid', 'default_owner_sid', 'component_job'):
            for action in ('missing', 'changed'):
                args = self.fixture()
                proof = args[0]['owned']['suspended_assignment']
                if action == 'missing':
                    del proof[key]
                else:
                    proof[key] = 'other'
                with self.subTest(key=key, action=action), self.assertRaises(ValueError):
                    M.validate_fp32_process(*args)

    def test_original_process_job_device_caps_and_release_are_required(self):
        for model, host in (('compact', 32 << 30), ('large', 48 << 30)):
            for key, value in (('native_host_reservation_bytes', host + 1),
                               ('peak_process_rss_bytes', host + 1),
                               ('applied_capacity', {}), ('native_host_charge_release_observed', False),
                               ('actual_job', {'process_memory_limit_bytes': host, 'job_memory_limit_bytes': host + 1}),
                               ('native_control_retained_for_unknown_cuda', True)):
                args = self.fixture(model)
                args[1][key] = value
                with self.subTest(model=model, key=key), self.assertRaises(ValueError):
                    M.validate_fp32_process(*args)

    def test_actual_pid_and_parent_settlement_are_required(self):
        for value in (124, True):
            args = self.fixture()
            args[0]['owned']['owned_leader_pid'] = value
            with self.assertRaises(ValueError):
                M.validate_fp32_process(*args)
        args = self.fixture('large')
        args[0]['owned']['peak_host_committed_bytes'] = (48 << 30) + 1
        with self.assertRaises(ValueError):
            M.validate_fp32_process(*args)
        with self.assertRaises(ValueError):
            M.validate_fp32_process(*self.fixture('large')[:4], lambda owned: False)



if __name__ == '__main__':
    unittest.main()
