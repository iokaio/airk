# SPDX-License-Identifier: Apache-2.0
"""No DLL initialization: independent literal events and injected loader calls."""
import copy
import importlib.util
from pathlib import Path
import unittest
from types import SimpleNamespace

p = Path(__file__).resolve().parents[1] / 'cuda-loader-probe.py'
s = importlib.util.spec_from_file_location('cuda_loader_probe', p)
m = importlib.util.module_from_spec(s)
s.loader.exec_module(m)
SCHEMA = 'airk.private.cuda-loader-probe-event/1'
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'


def literal():
    rows = [{'name': 'nvcuda.dll', 'path': 'C:/Windows/System32/nvcuda.dll'},
            {'name': 'onnxruntime_providers_shared.dll', 'path': 'C:/literal/onnxruntime_providers_shared.dll'},
            {'name': 'onnxruntime_providers_cuda.dll', 'path': 'C:/literal/onnxruntime_providers_cuda.dll'}]
    descriptor = {'path': 'C:/literal/airk-cuda-runtime.json', 'bytes': 10, 'sha256': 'a' * 64}
    plan = {'human_sid': SID, 'descriptor': descriptor, 'load_roster': rows}
    events = [
        {'kind': 'start', 'pid': 42, 'user_sid': SID, 'default_owner_sid': SID, 'manifest': descriptor,
         'model_execution': False, 'cuinit_called': False, 'ort_api_called': False},
        {'kind': 'search_policy', 'default_flags': 2560, 'load_flags': 2304},
        {'kind': 'attempt', 'ordinal': 0, 'module': 'nvcuda.dll', 'path': 'C:/Windows/System32/nvcuda.dll'},
        {'kind': 'result', 'ordinal': 0, 'module': 'nvcuda.dll', 'loaded': True, 'handle': 100, 'error': 0},
        {'kind': 'attempt', 'ordinal': 1, 'module': 'onnxruntime_providers_shared.dll', 'path': 'C:/literal/onnxruntime_providers_shared.dll'},
        {'kind': 'result', 'ordinal': 1, 'module': 'onnxruntime_providers_shared.dll', 'loaded': True, 'handle': 200, 'error': 0},
        {'kind': 'provider_host', 'module': 'onnxruntime_providers_cuda.dll', 'pointer': 0, 'is_null': True},
        {'kind': 'attempt', 'ordinal': 2, 'module': 'onnxruntime_providers_cuda.dll', 'path': 'C:/literal/onnxruntime_providers_cuda.dll'},
        {'kind': 'result', 'ordinal': 2, 'module': 'onnxruntime_providers_cuda.dll', 'loaded': False, 'handle': 0, 'error': 1114},
        {'kind': 'terminal', 'outcome': 'load_failed', 'failed_module': 'onnxruntime_providers_cuda.dll', 'error': 1114,
         'attempted': 3, 'loaded': 2, 'provider_host_null': True},
    ]
    return plan, [dict(schema=SCHEMA, **e) for e in events]


class Events(unittest.TestCase):
    def setUp(self):
        self.plan, self.events = literal()

    def check(self):
        return m.validate_events(self.events, self.plan, 42)

    def test_literal_null_host_failure_is_diagnosis(self):
        result = self.check()
        self.assertEqual(result['outcome'], 'load_failed')
        self.assertEqual(result['error'], 1114)
        self.assertNotIn('qualification', result)

    def test_wrong_pid(self):
        self.events[0]['pid'] = 43
        with self.assertRaises(ValueError):
            self.check()

    def test_foreign_default_owner(self):
        self.events[0]['default_owner_sid'] = 'foreign'
        with self.assertRaises(ValueError):
            self.check()

    def test_false_scope_claim(self):
        for key in ['model_execution', 'cuinit_called', 'ort_api_called']:
            self.events[0][key] = True
            with self.assertRaises(ValueError):
                self.check()
            self.events[0][key] = False

    def test_missing_terminal(self):
        self.events.pop()
        with self.assertRaises(ValueError):
            self.check()

    def test_no_events_after_failure(self):
        self.events.append(copy.deepcopy(self.events[-1]))
        with self.assertRaises(ValueError):
            self.check()

    def test_missing_host_observation(self):
        del self.events[6]
        with self.assertRaises(ValueError):
            self.check()

    def test_host_null_flag_must_match_pointer(self):
        self.events[6]['is_null'] = False
        with self.assertRaises(ValueError):
            self.check()

    def test_host_pointer_is_not_bool(self):
        self.events[6]['pointer'] = False
        with self.assertRaises(ValueError):
            self.check()

    def test_changed_module_path(self):
        self.events[7]['path'] = 'C:/ambient/onnxruntime_providers_cuda.dll'
        with self.assertRaises(ValueError):
            self.check()

    def test_stale_error_on_success(self):
        self.events[3]['error'] = 1114
        with self.assertRaises(ValueError):
            self.check()

    def test_missing_error_on_failure(self):
        self.events[8]['error'] = 0
        with self.assertRaises(ValueError):
            self.check()

    def test_inconsistent_loaded_flag(self):
        self.events[8]['loaded'] = True
        with self.assertRaises(ValueError):
            self.check()

    def test_invented_loaded_count(self):
        self.events[-1]['loaded'] = 3
        with self.assertRaises(ValueError):
            self.check()

    def test_wrong_search_policy(self):
        self.events[1]['default_flags'] = 4096
        with self.assertRaises(ValueError):
            self.check()

    def test_reordered_modules(self):
        self.events[2], self.events[4] = self.events[4], self.events[2]
        with self.assertRaises(ValueError):
            self.check()

    def test_nonnull_host_is_observed_not_synthesized(self):
        self.events[6].update(pointer=4096, is_null=False)
        self.events[-1]['provider_host_null'] = False
        self.assertFalse(self.check()['provider_host_null'])

    def test_loaded_all_means_only_loader_observation(self):
        self.events[8].update(loaded=True, handle=300, error=0)
        self.events[-1].update(outcome='loaded_all', failed_module=None, error=0, loaded=3)
        self.assertEqual(self.check()['outcome'], 'loaded_all')


class StateMachine(unittest.TestCase):
    def test_emit_attempt_precedes_native_call_and_host_precedes_cuda(self):
        plan, _ = literal()
        timeline = []
        emitted = []
        def emit(value):
            timeline.append(value['kind'])
            emitted.append(value)
        def load(path):
            self.assertEqual(timeline[-1], 'attempt')
            timeline.append('native_load')
            return (0, 1114) if path.endswith('providers_cuda.dll') else (100, 0)
        def host(handle):
            self.assertEqual(handle, 100)
            self.assertEqual(timeline[-1], 'result')
            timeline.append('host_call')
            return 0
        m.exercise(plan['load_roster'], load, host, emit)
        self.assertEqual(timeline, ['attempt', 'native_load', 'result', 'attempt', 'native_load', 'result',
                                    'host_call', 'provider_host', 'attempt', 'native_load', 'result', 'terminal'])
        self.assertEqual(emitted[-1]['error'], 1114)

    def test_first_driver_failure_stops_without_host_call(self):
        plan, _ = literal()
        calls, events = [], []
        def load(path):
            calls.append(path)
            return 0, 126
        def host(_):
            self.fail('host must not be called after driver failure')
        m.exercise(plan['load_roster'], load, host, events.append)
        self.assertEqual(len(calls), 1)
        self.assertEqual(events[-1], {'kind': 'terminal', 'outcome': 'load_failed', 'failed_module': 'nvcuda.dll',
                                     'error': 126, 'attempted': 1, 'loaded': 0, 'provider_host_null': None})

    def test_missing_shared_provider_refuses_before_cuda(self):
        plan, _ = literal()
        with self.assertRaisesRegex(ValueError, 'shared provider'):
            m.exercise([plan['load_roster'][-1]], lambda _: (100, 0), lambda _: 0, lambda _: None)

    def test_native_loader_exception_remains_failure(self):
        plan, _ = literal()
        events = []
        def failed(_):
            raise OSError('literal API exception')
        with self.assertRaisesRegex(OSError, 'literal API exception'):
            m.exercise(plan['load_roster'], failed, lambda _: 0, events.append)
        self.assertEqual([e['kind'] for e in events], ['attempt'])

    def test_no_bounds_extension(self):
        plan = {'schema': m.SCHEMA, 'limits': dict(m.LIMITS, wall_seconds=121), 'human_sid': SID}
        c = type('C', (), {'SID': SID})
        with self.assertRaisesRegex(ValueError, 'bounds'):
            m.preflight(plan, c, None, True)


class FinalProof(unittest.TestCase):
    def test_prior_error_does_not_skip_final_hash(self):
        receipt = {'final_proof_errors': [{'message': 'original'}]}
        called = []
        def digest(path):
            called.append(path)
            return {'path': str(path), 'bytes': 20, 'sha256': 'a' * 64}
        m.retain_log(receipt, 'C:/literal/log', digest, lambda _: None)
        self.assertEqual(called, ['C:/literal/log'])
        self.assertEqual(receipt['final_proof_errors'], [{'message': 'original'}])

    def test_changed_final_bytes_are_retained_as_error(self):
        receipt = {'final_proof_errors': [], 'stdout': {'path': 'C:/literal/log', 'bytes': 20, 'sha256': 'b' * 64}}
        m.retain_log(receipt, 'C:/literal/log', lambda p: {'path': p, 'bytes': 20, 'sha256': 'a' * 64}, lambda _: None)
        self.assertEqual(len(receipt['final_proof_errors']), 1)

    def test_missing_sha_row_is_not_complete(self):
        receipt = {'final_proof_errors': []}
        m.retain_log(receipt, 'C:/literal/log', lambda _: None, lambda _: None)
        self.assertEqual(len(receipt['final_proof_errors']), 1)

    def test_foreign_owner_stops_consumption(self):
        receipt = {'final_proof_errors': []}
        def audit(_):
            raise ValueError('foreign owner')
        def digest(_):
            self.fail('unaudited bytes consumed')
        m.retain_log(receipt, 'C:/literal/log', digest, audit)
        self.assertEqual(receipt['final_proof_errors'], [{'message': 'foreign owner'}])


class OriginEvidence(unittest.TestCase):
    def setUp(self):
        image = {'path': 'C:/literal/native.exe', 'sha256': 'a' * 64}
        self.old = {'native_image': image, 'selector': 'literal::cuda_original_context_fp32_t17_component'}
        self.error = 'authenticated GPU module load failed: A dynamic link library (DLL) initialization routine failed. (os error 1114)'
        self.capture = {'schema': 'airk.private.cuda-component-result/1', 'complete': False, 'pid': 40388,
                        'execution_user': SID, 'native_binary_sha256': 'a' * 64, 'error': self.error, 'rows': []}
        self.stdout = 'running 1 test\n' + self.old['selector'] + '\n' + self.error + '\ntest result: FAILED. 0 passed; 1 failed;'
        self.receipt = {'schema': 'airk.private.owned-cuda-component/1', 'complete': False, 'failure': {'message': 'original'},
                        'whole_phase_expired': False, 'whole_phase_deadline_not_refreshed': True,
                        'post_input_drift': [], 'final_input_errors': [], 'final_proof_success': ['native_result', 'stdout'],
                        'final_proof_errors': [{'label': 'operations', 'type': 'ValueError', 'message': 'mandatory bounded final SHA proof absent'}],
                        'owned': {'exit_code': 101, 'timed_out': False, 'log_oversized': False,
                                  'owned_descendants_settled': True, 'owned_job_forced_termination': False,
                                  'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
                                  'parent_assigned_to_job': False, 'child_assigned_before_resume': True,
                                  'owned_leader_pid': 40388, 'owned_total_processes': 1, 'unexpected_third_process': False,
                                  'whole_process_deadline_not_refreshed': True,
                                  'retained_member_exits': [{'exit_observed': True, 'exit_code': 101}],
                                  'handle_close_proof': {'forced': False, 'settled': True, 'final_active': 0,
                                                         'members_before': [], 'final_member_pids': [],
                                                         'retained_exits': [{'pid': 40388, 'exit_observed': True, 'exit_code': 101}]},
                                  'suspended_assignment': {'pid': 40388, 'execution_user_sid': SID, 'default_owner_sid': SID,
                                                           'image': image, 'component_job': {'process_bytes': 17179869184,
                                                                                           'job_bytes': 25769803776,
                                                                                           'flags': 8968, 'active_limit': 2}}}}

    def check(self):
        m.verify_origin_facts(self.receipt, self.capture, self.stdout, self.old, SID)

    def test_literal_failed_origin_retains_expected_empty_journal_error(self):
        self.check()
        self.assertFalse(self.receipt['complete'])
        self.assertEqual(len(self.receipt['final_proof_errors']), 1)

    def test_forced_origin_is_not_natural101(self):
        self.receipt['owned']['owned_job_forced_termination'] = True
        with self.assertRaises(ValueError):
            self.check()

    def test_unknown_original_handle_exit(self):
        self.receipt['owned']['handle_close_proof']['retained_exits'][0]['exit_observed'] = False
        with self.assertRaises(ValueError):
            self.check()

    def test_wrong_original_handle_pid(self):
        self.receipt['owned']['handle_close_proof']['retained_exits'][0]['pid'] = 40389
        with self.assertRaises(ValueError):
            self.check()

    def test_wrong_loader_error(self):
        self.capture['error'] = 'os error 126'
        with self.assertRaises(ValueError):
            self.check()

    def test_origin_stdout_must_contain_actual_error(self):
        self.stdout = 'running 1 test\ntest result: FAILED. 0 passed; 1 failed;'
        with self.assertRaises(ValueError):
            self.check()

    def test_origin_cannot_include_model_scores(self):
        self.capture['rows'] = [{'score': 1}]
        with self.assertRaises(ValueError):
            self.check()

    def test_origin_must_not_erase_original_final_proof_error(self):
        self.receipt['final_proof_errors'] = []
        with self.assertRaises(ValueError):
            self.check()


class OriginContext(unittest.TestCase):
    def test_complete_context_is_exactly_original(self):
        old = {'descriptor': {'sha256': 'a' * 64}, 'runtime_root': 'C:/literal/runtime',
               'runtime_files': [{'sha256': 'b' * 64}], 'ownership_helper_root': 'C:/literal/helpers',
               'output_guard': {'sha256': 'c' * 64}, 'output_guard_lock': {'sha256': 'd' * 64}}
        m.origin_context(copy.deepcopy(old), old)
        for key in ['descriptor', 'runtime_root', 'runtime_files', 'ownership_helper_root', 'output_guard', 'output_guard_lock']:
            with self.subTest(key=key):
                changed = copy.deepcopy(old)
                changed[key] = 'another independently valid closure'
                with self.assertRaisesRegex(ValueError, 'original failed component'):
                    m.origin_context(changed, old)

    def test_missing_context_is_not_equal_by_default(self):
        with self.assertRaises(ValueError):
            m.origin_context({}, {})


class GuardDeadline(unittest.TestCase):
    def setUp(self):
        b = type('B', (), {'pinned': staticmethod(lambda _: None)})
        self.c = type('C', (), {'base': staticmethod(lambda: b), 'SID': SID, 'ANCHOR': Path('C:/literal')})
        self.plan = {'checked_guard': {'path': 'C:/literal/guard.ps1'}, 'powershell': {'path': 'C:/literal/pwsh.exe'}}

    def test_guard_timeout_uses_only_remaining_original_second(self):
        calls = []
        clock = iter([119, 119.5])
        m.bounded_guard(self.plan, self.c, deadline=120, clock=lambda: next(clock), run=lambda *a, **k: calls.append(k))
        self.assertEqual(calls[0]['timeout'], 1)

    def test_expired_guard_never_launches(self):
        def run(*a, **k):
            self.fail('guard launched after cutoff')
        with self.assertRaisesRegex(ValueError, 'expired before guard'):
            m.bounded_guard(self.plan, self.c, deadline=120, clock=lambda: 120, run=run)

    def test_guard_that_returns_after_original_cutoff_refuses(self):
        clock = iter([119, 120])
        with self.assertRaisesRegex(ValueError, 'expired during guard'):
            m.bounded_guard(self.plan, self.c, deadline=120, clock=lambda: next(clock), run=lambda *a, **k: None)

    def test_receipt_write_failure_does_not_replace_native_error(self):
        original = OSError('original native failure')
        receipt = {}
        def write():
            raise PermissionError('receipt write failed')
        m.write_receipt_after_clock({}, None, 'unused', receipt, original, writer=write, audit=lambda: None)
        self.assertEqual(str(original), 'original native failure')
        self.assertIn('receipt write failed', original.__notes__[0])
        self.assertEqual(receipt['receipt_retention_error']['type'], 'PermissionError')

    def test_postclock_owner_failure_without_original_raises(self):
        def audit():
            raise PermissionError('postwrite owner audit failed')
        with self.assertRaisesRegex(PermissionError, 'postwrite owner'):
            m.write_receipt_after_clock({}, None, 'unused', {}, None, writer=lambda: None, audit=audit)


class ProtectedPolicyDeadline(unittest.TestCase):
    def test_original_two_calls_use_remaining_cutoff_and_restore_binding(self):
        calls = []
        original = SimpleNamespace(run=lambda argv, **kw: calls.append((argv, kw)))
        policy = SimpleNamespace(subprocess=original)
        def observe(*args):
            policy.subprocess.run(['C:/Windows/System32/whoami.exe'], timeout=15)
            policy.subprocess.run(['pwsh', '-NoProfile'], timeout=15)
            return 'observed'
        policy.identity_and_parent = observe
        ticks = iter([117, 118, 119, 119.5])
        result = m.bounded_policy(policy, None, None, None, 120, 'C:/literal/pwsh.exe',
                                  clock=lambda: next(ticks), resolve=lambda _: 'C:/literal/pwsh.exe')
        self.assertEqual(result, 'observed')
        self.assertEqual([v[1]['timeout'] for v in calls], [3, 1])
        self.assertIs(policy.subprocess, original)

    def test_helper_error_restores_only_local_binding(self):
        def run(*args, **kwargs):
            raise OSError('observed policy failure')
        original = SimpleNamespace(run=run)
        policy = SimpleNamespace(subprocess=original)
        policy.identity_and_parent = lambda *args: policy.subprocess.run(['C:/Windows/System32/whoami.exe'], timeout=15)
        with self.assertRaisesRegex(OSError, 'observed policy failure'):
            m.bounded_policy(policy, None, None, None, 120, 'C:/literal/pwsh.exe', clock=lambda: 119)
        self.assertIs(policy.subprocess, original)
        self.assertIs(original.run, run)

    def test_expired_policy_does_not_launch(self):
        original = SimpleNamespace(run=lambda *a, **k: self.fail('expired policy launched'))
        policy = SimpleNamespace(subprocess=original)
        policy.identity_and_parent = lambda *a: policy.subprocess.run(['C:/Windows/System32/whoami.exe'], timeout=15)
        with self.assertRaisesRegex(ValueError, 'expired before'):
            m.bounded_policy(policy, None, None, None, 120, 'C:/literal/pwsh.exe', clock=lambda: 120)
        self.assertIs(policy.subprocess, original)

    def test_ambient_powershell_substitution_refuses(self):
        original = SimpleNamespace(run=lambda *a, **k: self.fail('foreign PowerShell launched'))
        policy = SimpleNamespace(subprocess=original)
        policy.identity_and_parent = lambda *a: policy.subprocess.run(['pwsh'], timeout=15)
        with self.assertRaisesRegex(ValueError, 'PowerShell resolution differs'):
            m.bounded_policy(policy, None, None, None, 120, 'C:/literal/pwsh.exe', clock=lambda: 119,
                             resolve=lambda _: 'C:/foreign/pwsh.exe')
        self.assertIs(policy.subprocess, original)


if __name__ == '__main__':
    unittest.main()
