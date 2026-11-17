# SPDX-License-Identifier: Apache-2.0
"""Literal refusal controls; no process launch, fixtures, cache or GPU work."""
import copy
import ctypes
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch, mock_open
from types import SimpleNamespace
import json

SOURCE = Path(__file__).resolve().parents[1] / 'cuda-component-execution.py'
spec = importlib.util.spec_from_file_location('cuda_component_execution', SOURCE)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def pin(name, sha='a'):
    return {'path': 'C:/literal/' + name, 'bytes': 10, 'sha256': sha * 64}


def literal():
    plan = {'request': pin('request'), 'native_input': pin('input'), 'descriptor': pin('runtime'),
            'native_image': pin('image', 'b')}
    native = {'numeric_fixture': pin('fixture'), 'source_manifest': pin('source')}
    profile = {'manifest': 'c' * 64, 'runtime': 'onnxruntime-cuda/1.30.0',
               'device_execution': {'host_fit': {'minimum_physical_cores': 4}}}
    fixture = {'reference_producer': pin('producer'), 'rows': [{'name': 'all-live-0', 'input': {'input_digest': 'd' * 64}}]}
    node = {'subgraph_index': 0, 'name': 'n', 'domain': '', 'op': 'MatMul', 'ep': 'CUDAExecutionProvider'}
    value = {'schema': 'airk.private.cuda-component-result/1', 'complete': True, 'qualified': False,
             'product_admission': False, 'measured_peak_device_bytes': None, 'request': plan['request'],
             'native_input': plan['native_input'], 'runtime_manifest': plan['descriptor'],
             'fixture': native['numeric_fixture'], 'source_manifest': native['source_manifest'],
             'reference_producer': fixture['reference_producer'], 'profile': profile['manifest'],
             'model': 'arbiter-context-qwen3-17b', 'pid': 42, 'execution_user': m.SID,
             'native_binary_sha256': 'b' * 64,
             'bounds': {'schema': 'airk.arbiter.gpu-probe-reservation/1', 'host_limit_bytes': 17179869184,
                        'device_limit_bytes': 25769803776, 'deadline_ms': 120000},
             'native_host_reservation_bytes': 17179869184,
             'actual_job': {'process_memory_limit_bytes': 17179869184, 'job_memory_limit_bytes': 25769803776, 'flags': 8968},
             'native_control_retained_for_unknown_cuda': False, 'native_control_cleanup_observed': True,
             'native_host_charge_release_observed': True, 'process_cuda_poisoned': False,
             'native_control_cleanup_error': None, 'final_host_peak_error': None,
             'peak_process_rss_bytes': 1000000, 'elapsed_us_to_final_report_start': 1100, 'load_us': 400,
             'final_placement': {'scope': 'onnxruntime-final-session-state-ep-placement', 'complete': True,
                                 'shim_status': 0, 'ort_error_code': 0, 'reported_nodes': 1,
                                 'reported_string_bytes': 28, 'nodes': [node]},
             'adapter_before': {'uuid': list(range(16)), 'ordinal': 0, 'compute_major': 8, 'compute_minor': 6,
                                'driver_api_version': 12080, 'driver_image_sha256': 'a' * 64,
                                'physical_total_bytes': 51539607552, 'total_bytes': 50000000000, 'free_bytes': 49000000000, 'epoch': 0},
             'adapter_after': {'uuid': list(range(16)), 'ordinal': 0, 'compute_major': 8, 'compute_minor': 6,
                               'driver_api_version': 12080, 'driver_image_sha256': 'a' * 64,
                               'physical_total_bytes': 51539607552, 'total_bytes': 50000000000, 'free_bytes': 48000000000, 'epoch': 0},
             'execution_identity': ['onnxruntime-cuda/1.30.0/device-execution/' + 'e' * 64, 16],
             'original_expires_monotonic_ns': 5000,
             'rows': [{'name': 'all-live-0', 'tokens': 17, 'input_digest': 'd' * 64, 'raw_scores': [1., 2., 3.],
                       'argmax_matches': True, 'same_artifact_raw_error': .0001, 'same_artifact_mass_error': .0001,
                       'original_fp32_raw_error': .0001, 'original_fp32_mass_error': .0001,
                       'elapsed_us': 100, 'native_call_elapsed_us': 90}]}
    identity = {'input_sha256': 'a' * 64, 'profile': 'c' * 64, 'process_id': 42, 'native_binary_sha256': 'b' * 64}
    events = []
    for sequence, kind, ordinal, phase, start, observed in [
            (1, 'load', 0, 'started', 100, 100), (2, 'load', 0, 'returned', 100, 200),
            (3, 'numeric', 1, 'started', 300, 300), (4, 'numeric', 1, 'returned', 300, 400)]:
        events.append({'schema': 'airk.arbiter.native-operation/1', 'sequence': sequence, 'identity': identity,
                       'kind': kind, 'ordinal': ordinal, 'phase': phase, 'clock': 'windows_performance_counter',
                       'operation_input_digest': None if kind == 'load' else 'd' * 64,
                       'started_monotonic_ns': start, 'observed_monotonic_ns': observed,
                       'expires_monotonic_ns': 5000, 'native_call_elapsed_us': None if phase == 'started' else 1,
                       'native_success': None if phase == 'started' else True})
    return plan, native, profile, fixture, value, events


class Capture(unittest.TestCase):
    def setUp(self):
        self.plan, self.native, self.profile, self.fixture, self.value, self.events = literal()

    def validate(self):
        m.validate_native(self.value, self.plan, 42, self.native, self.profile, self.fixture)

    def reject(self, key, value):
        self.value[key] = value
        with self.assertRaises(ValueError):
            self.validate()

    def test_literal_complete_capture(self):
        self.validate()

    def test_wrong_pid(self):
        self.reject('pid', 43)

    def test_bool_pid(self):
        self.reject('pid', True)

    def test_wrong_native_image(self):
        self.reject('native_binary_sha256', 'f' * 64)

    def test_wrong_sid(self):
        self.reject('execution_user', 'foreign')

    def test_wrong_request(self):
        self.reject('request', pin('different'))

    def test_wrong_profile(self):
        self.reject('profile', 'f' * 64)

    def test_wrong_model(self):
        self.reject('model', 'arbiter-context-qwen3-4b')

    def test_missing_result(self):
        self.reject('complete', False)

    def test_no_product_or_qualification_claim(self):
        for key in ['qualified', 'product_admission']:
            with self.subTest(key=key):
                old = self.value[key]
                self.reject(key, True)
                self.value[key] = old

    def test_no_invented_device_peak(self):
        self.reject('measured_peak_device_bytes', 100)

    def test_missing_host_release(self):
        self.reject('native_host_charge_release_observed', False)

    def test_unknown_control_custody(self):
        self.reject('native_control_retained_for_unknown_cuda', True)

    def test_missing_cleanup_key(self):
        del self.value['native_control_cleanup_error']
        with self.assertRaises(ValueError):
            self.validate()

    def test_poisoned_teardown(self):
        self.reject('process_cuda_poisoned', True)

    def test_peak_exceeds_original(self):
        self.reject('peak_process_rss_bytes', 17179869185)

    def test_peak_unknown(self):
        self.reject('peak_process_rss_bytes', None)

    def test_late_native_retention(self):
        self.reject('final_report_retention_exceeded_original_deadline', True)

    def test_native_error_is_not_pass(self):
        self.reject('error', 'failed')

    def test_wrong_job_limit(self):
        self.value['actual_job']['job_memory_limit_bytes'] = 17179869184
        with self.assertRaises(ValueError):
            self.validate()

    def test_no_extra_row(self):
        self.value['rows'] *= 2
        with self.assertRaises(ValueError):
            self.validate()

    def test_no128_substitution(self):
        self.value['rows'][0]['tokens'] = 128
        with self.assertRaises(ValueError):
            self.validate()

    def test_no_cropped_score(self):
        self.value['rows'][0]['raw_scores'].pop()
        with self.assertRaises(ValueError):
            self.validate()

    def test_nan_score_refused(self):
        self.value['rows'][0]['raw_scores'][0] = float('nan')
        with self.assertRaises(ValueError):
            self.validate()

    def test_all_original_numeric_limits(self):
        for key, value in [('same_artifact_raw_error', .00101), ('original_fp32_raw_error', .00101),
                           ('same_artifact_mass_error', .0501), ('original_fp32_mass_error', .0501)]:
            with self.subTest(key=key):
                old = self.value['rows'][0][key]
                self.value['rows'][0][key] = value
                with self.assertRaises(ValueError):
                    self.validate()
                self.value['rows'][0][key] = old

    def test_argmax_refusal(self):
        self.value['rows'][0]['argmax_matches'] = False
        with self.assertRaises(ValueError):
            self.validate()

    def test_cpu_placement_refused(self):
        self.value['final_placement']['nodes'][0]['ep'] = 'CPUExecutionProvider'
        with self.assertRaises(ValueError):
            self.validate()

    def test_incomplete_placement_refused(self):
        self.value['final_placement']['complete'] = False
        with self.assertRaises(ValueError):
            self.validate()

    def test_graph_count_and_bytes(self):
        for key in ['reported_nodes', 'reported_string_bytes']:
            old = self.value['final_placement'][key]
            self.value['final_placement'][key] = 0
            with self.assertRaises(ValueError):
                self.validate()
            self.value['final_placement'][key] = old

    def test_execution_identity_is_native_tuple(self):
        self.reject('execution_identity', 'e' * 64)

    def test_changed_adapter_identity(self):
        self.value['adapter_after']['uuid'][0] = 255
        with self.assertRaises(ValueError):
            self.validate()

    def test_empty_adapter_observation(self):
        self.reject('adapter_after', {})

    def test_impossible_adapter_capacity(self):
        self.value['adapter_before']['free_bytes'] = 999999999999
        with self.assertRaises(ValueError):
            self.validate()

    def test_literal_operation_journal(self):
        m.validate_operations(self.events, self.value)

    def test_unreturned_operation(self):
        with self.assertRaises(ValueError):
            m.validate_operations(self.events[:-1], self.value)

    def test_extra_operation(self):
        with self.assertRaises(ValueError):
            m.validate_operations(self.events + [self.events[-1]], self.value)

    def test_reordered_operations(self):
        with self.assertRaises(ValueError):
            m.validate_operations(self.events[::-1], self.value)

    def test_refreshed_expiry(self):
        self.events[-1]['expires_monotonic_ns'] += 1
        with self.assertRaises(ValueError):
            m.validate_operations(self.events, self.value)

    def test_failed_native_return(self):
        self.events[-1]['native_success'] = False
        with self.assertRaises(ValueError):
            m.validate_operations(self.events, self.value)

    def test_return_after_expiry(self):
        self.events[-1]['observed_monotonic_ns'] = 5000
        with self.assertRaises(ValueError):
            m.validate_operations(self.events, self.value)

    def test_wrong_journal_input(self):
        self.events[-1]['operation_input_digest'] = 'f' * 64
        with self.assertRaises(ValueError):
            m.validate_operations(self.events, self.value)

    def test_wrong_journal_pid(self):
        self.events[-1]['identity'] = dict(self.events[-1]['identity'], process_id=43)
        with self.assertRaises(ValueError):
            m.validate_operations(self.events, self.value)


class FinalProofs(unittest.TestCase):
    def setUp(self):
        self.paths = {key: 'C:/literal/' + key for key in ['native_result', 'operations', 'stdout']}
        self.receipt = {'complete': True, 'failure': None, 'started_utc': '2026-11-17T17:00:00+00:00',
                        'original_deadline': '2026-11-17T17:02:00+00:00', 'original_deadline_monotonic': 120}
        self.called = []

    def hash(self, path):
        self.called.append(path)
        return {'path': path, 'bytes': 20, 'sha256': 'a' * 64}

    def finish(self, callback=None, clock=lambda: 10):
        m.finish(self.receipt, self.paths, 0, 120, callback or self.hash, clock)

    def test_all_original_final_hashes(self):
        self.finish()
        self.assertTrue(self.receipt['complete'])
        self.assertEqual(len(self.called), 3)

    def test_required_utc_and_original_deadline_fields(self):
        frozen = {key: self.receipt[key] for key in ['started_utc', 'original_deadline', 'original_deadline_monotonic']}
        utc = m.datetime.datetime(2026, 11, 17, 17, 0, 10, tzinfo=m.datetime.timezone.utc)
        m.finish(self.receipt, self.paths, 0, 120, self.hash, clock=lambda: 10, utc_now=lambda: utc)
        self.assertEqual(self.receipt['finished_utc'], '2026-11-17T17:00:10+00:00')
        self.assertEqual({key: self.receipt[key] for key in frozen}, frozen)
        self.assertEqual(self.receipt['elapsed_seconds'], 10)

    def test_utc_finish_is_sampled_after_hashes_before_final_monotonic(self):
        order = []
        def utc():
            self.assertEqual(len(self.called), 3)
            order.append('utc')
            return m.datetime.datetime(2026, 11, 17, tzinfo=m.datetime.timezone.utc)
        def clock():
            self.assertEqual(order, ['utc'])
            self.assertIn('finished_utc', self.receipt)
            order.append('monotonic')
            return 120
        m.finish(self.receipt, self.paths, 0, 120, self.hash, clock=clock, utc_now=utc)
        self.assertEqual(order, ['utc', 'monotonic'])
        self.assertFalse(self.receipt['complete'])
        self.assertTrue(self.receipt['whole_phase_expired'])

    def test_missing_first_proof_still_hashes_all(self):
        def hash_file(path):
            row = self.hash(path)
            if path.endswith('native_result'):
                raise FileNotFoundError(path)
            return row
        self.finish(hash_file)
        self.assertEqual(len(self.called), 3)
        self.assertFalse(self.receipt['complete'])

    def test_absent_sha_row_refuses(self):
        self.finish(lambda _: None)
        self.assertFalse(self.receipt['complete'])
        self.assertEqual(len(self.receipt['final_proof_errors']), 3)

    def test_changed_observed_output_refuses(self):
        self.receipt['native_result'] = {'path': self.paths['native_result'], 'bytes': 19, 'sha256': 'b' * 64}
        self.finish()
        self.assertFalse(self.receipt['complete'])

    def test_final_clock_after_all_hashes(self):
        def clock():
            self.assertEqual(len(self.called), 3)
            return 120
        self.finish(clock=clock)
        self.assertFalse(self.receipt['complete'])
        self.assertTrue(self.receipt['whole_phase_expired'])

    def test_original_failure_survives(self):
        original = {'type': 'NativeFailure', 'message': 'original'}
        self.receipt['failure'] = original
        self.finish(lambda _: None, clock=lambda: 121)
        self.assertEqual(self.receipt['failure'], original)
        self.assertFalse(self.receipt['complete'])

    def test_log_cap_is_not_widened(self):
        self.finish(lambda path: {'path': path, 'bytes': 67108865, 'sha256': 'a' * 64})
        self.assertFalse(self.receipt['complete'])


class RefusalPolicy(unittest.TestCase):
    def test_context128_plan_cannot_launch(self):
        with self.assertRaisesRegex(ValueError, 'exact Context17'):
            m.preflight({'schema': m.SCHEMA, 'human_sid': m.SID, 'selector': 'context128', 'limits': m.LIMITS}, None, before=True)

    def test_increased_wall_cap_cannot_launch(self):
        with self.assertRaisesRegex(ValueError, 'original bounds'):
            m.preflight({'schema': m.SCHEMA, 'human_sid': m.SID, 'selector': m.SELECTOR,
                         'limits': dict(m.LIMITS, wall_seconds=121)}, None, before=True)

    def test_finite_rejects_bool_nan_infinity(self):
        for value in [True, False, float('nan'), float('inf'), -1, None]:
            self.assertFalse(m.finite(value))

    def test_token_owner_actual_read_only(self):
        if __import__('os').name != 'nt':
            self.skipTest('Windows token observation')
        self.assertEqual(m.token_owner(ctypes.c_void_p(-1)), m.SID)


class JobReadback(unittest.TestCase):
    class API:
        def __init__(self, process=17179869184, job=17179869184, flags=8968, alter=False):
            self.process, self.job, self.flags = process, job, flags
            self.alter = alter
            self.writes = 0

        def QueryInformationJobObject(self, job, kind, pointer, size, returned):
            value = pointer._obj
            value.process, value.job = self.process, self.job
            value.basic.flags, value.basic.active = self.flags, 2
            return 1

        def SetInformationJobObject(self, job, kind, pointer, size):
            value = pointer._obj
            self.writes += 1
            self.process, self.job = value.process, value.job + int(self.alter)
            self.flags = value.basic.flags
            return 1

    def test_original_process_and_frozen_job_readback(self):
        api = self.API()
        result = m.configure_job(api, 42)
        self.assertEqual(result, {'process_bytes': 17179869184, 'job_bytes': 25769803776,
                                  'flags': 8968, 'active_limit': 2})
        self.assertEqual(api.writes, 1)

    def test_wrong_original_process_refuses_before_write(self):
        api = self.API(process=8589934592)
        with self.assertRaises(ValueError):
            m.configure_job(api, 42)
        self.assertEqual(api.writes, 0)

    def test_wrong_original_flags_refuse_before_write(self):
        api = self.API(flags=768)
        with self.assertRaises(ValueError):
            m.configure_job(api, 42)
        self.assertEqual(api.writes, 0)

    def test_changed_readback_refuses(self):
        api = self.API(alter=True)
        with self.assertRaises(ValueError):
            m.configure_job(api, 42)
        self.assertEqual(api.writes, 1)

class NativeSourceEpoch(unittest.TestCase):
    def setUp(self):
        self.mandatory = [
            'crates/airk-retrieval/src/arbiter/runtime.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cuda.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cuda/CONTRACT.md',
            'crates/airk-retrieval/src/arbiter/runtime/cpu_source_transition.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cpu_source_transition/tests.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cpu_source_transition/CONTRACT.md',
            'crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/tests.rs',
            'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification.rs',
            'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/CONTRACT.md',
            'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/operation.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cuda/windows.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cuda/placement.rs',
            'crates/airk-host/src/gpu_runtime.rs',
        ]
        self.extra = 'crates/airk-retrieval/src/arbiter/runtime/cuda/modules.rs'
        self.old_files = {key: 'a' * 64 for key in self.mandatory + [self.extra]}
        self.new_files = dict(self.old_files)
        self.new_files[self.extra] = 'b' * 64
        self.source_path = str(m.ROOT / self.extra)
        self.other_path = str(m.ROOT / 'artifacts/literal-metadata.json')
        self.old = {'required_native_sources': {self.extra: 'a' * 64},
                    'input_seals': {self.source_path: 'a' * 64, self.other_path: 'c' * 64}}
        self.current = {key: self.new_files[key] for key in self.mandatory + [self.extra]}
        self.rows = [{'path': self.source_path, 'old_sha256': 'a' * 64, 'new_sha256': 'b' * 64}]
        self.plan = {'required_native_sources': dict(self.current), 'native_source_transition': copy.deepcopy(self.rows), 'native_source_introductions': []}

    def test_every_old_key_rebound_and_literal_set_preserved(self):
        before = copy.deepcopy(self.old)
        result = m.rebind_native_sources(self.old['required_native_sources'], self.old_files, self.new_files)
        self.assertEqual(result, self.current)
        self.assertEqual(set(result), set(self.mandatory + [self.extra]))
        self.assertEqual(result[self.extra], 'b' * 64)
        self.assertEqual(self.old, before)

    def test_exact_transition_preserves_unrelated_old_seal(self):
        rows = m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)
        self.assertEqual(rows, self.rows)
        actual = {self.source_path: 'b' * 64, self.other_path: 'c' * 64}
        m.verify_historical_inputs(self.old['input_seals'], rows, actual.__getitem__)

    def test_missing_preserved_key_in_new_build_refuses(self):
        del self.new_files[self.extra]
        with self.assertRaisesRegex(ValueError, 'missing from an authenticated build'):
            m.rebind_native_sources(self.old['required_native_sources'], self.old_files, self.new_files)

    def test_missing_mandatory_key_in_old_build_refuses(self):
        del self.old_files[self.mandatory[2]]
        with self.assertRaises(ValueError):
            m.rebind_native_sources(self.old['required_native_sources'], self.old_files, self.new_files)

    def test_falsely_claimed_historical_required_sha_refuses(self):
        self.old['required_native_sources'][self.extra] = 'd' * 64
        with self.assertRaisesRegex(ValueError, 'original build'):
            m.rebind_native_sources(self.old['required_native_sources'], self.old_files, self.new_files)

    def test_falsely_claimed_historical_input_sha_refuses(self):
        self.old['input_seals'][self.source_path] = 'd' * 64
        with self.assertRaisesRegex(ValueError, 'historical build'):
            m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)

    def test_dropped_preserved_key_in_plan_refuses(self):
        del self.plan['required_native_sources'][self.extra]
        with self.assertRaisesRegex(ValueError, 'complete old'):
            m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)

    def test_unrelated_extra_key_in_plan_refuses(self):
        self.plan['required_native_sources']['crates/unrelated.rs'] = 'a' * 64
        with self.assertRaises(ValueError):
            m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)

    def test_wrong_new_build_hash_in_plan_refuses(self):
        self.plan['required_native_sources'][self.extra] = 'f' * 64
        with self.assertRaisesRegex(ValueError, 'new build'):
            m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)

    def test_omitted_transition_row_refuses(self):
        self.plan['native_source_transition'] = []
        with self.assertRaisesRegex(ValueError, 'transition differs'):
            m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)

    def test_unrelated_old_input_drift_refuses(self):
        actual = {self.source_path: 'b' * 64, self.other_path: 'd' * 64}
        with self.assertRaisesRegex(ValueError, 'metadata input changed'):
            m.verify_historical_inputs(self.old['input_seals'], self.rows, actual.__getitem__)

    def test_old_source_bytes_cannot_stand_in_for_new_image_source(self):
        actual = {self.source_path: 'a' * 64, self.other_path: 'c' * 64}
        with self.assertRaises(ValueError):
            m.verify_historical_inputs(self.old['input_seals'], self.rows, actual.__getitem__)

    def test_unrelated_path_cannot_enter_transition_rows(self):
        self.plan['native_source_transition'].append({'path': self.other_path, 'old_sha256': 'c' * 64, 'new_sha256': 'd' * 64})
        with self.assertRaisesRegex(ValueError, 'transition differs'):
            m.verify_native_transition(self.old, self.old_files, self.new_files, self.plan)

    def test_bad_digest_and_noncanonical_native_path_refuse(self):
        self.new_files[self.extra] = 'not-a-sha'
        with self.assertRaises(ValueError):
            m.rebind_native_sources(self.old['required_native_sources'], self.old_files, self.new_files)
        bad = 'crates/../outside.rs'
        self.old_files[bad] = self.new_files[bad] = 'a' * 64
        with self.assertRaises(ValueError):
            m.rebind_native_sources({bad: 'a' * 64}, self.old_files, self.new_files)



class BatchedGuard(unittest.TestCase):
    def setUp(self):
        self.plan = {'checked_guard': pin('guard.ps1'), 'powershell': pin('pwsh.exe')}
        self.paths = [m.ROOT / 'first.json', m.ROOT / 'second.json']
        self.base = SimpleNamespace(pinned=lambda value: value)
        self.mutate = lambda value: None
        self.reply = None

    def process(self, argv, **kwargs):
        payload = json.loads(kwargs['input'])
        value = {'schema': 'airk.human-output-audit/1', 'elapsed_seconds': .2, 'expired': False,
                 'results': [{'index': i, 'passed': True, 'error': None} for i in range(len(payload['entries']))]}
        self.mutate(value)
        self.reply = value
        return SimpleNamespace(stdout=json.dumps(value))

    def invoke(self, **kwargs):
        with patch.object(m, 'base', return_value=self.base), patch.object(m.subprocess, 'run', side_effect=self.process) as run:
            result = m.guard_many(self.plan, self.paths, True, **kwargs)
        return result, run

    def test_one_process_for_exact_path_roster_and_remaining_budget(self):
        with patch.object(m.time, 'monotonic', return_value=50):
            result, run = self.invoke(deadline=53)
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args.kwargs['timeout'], 3)
        payload = json.loads(run.call_args.kwargs['input'])
        self.assertEqual(payload['budget'], 3)
        self.assertEqual(payload['entries'], [{'path': str(p), 'root': str(m.ROOT), 'created': True} for p in self.paths])
        self.assertEqual(result, {str(p): None for p in self.paths})

    def test_original_expiry_refuses_before_spawn(self):
        with patch.object(m.time, 'monotonic', return_value=53), patch.object(m, 'base', return_value=self.base), patch.object(m.subprocess, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'deadline expired'):
                m.guard_many(self.plan, self.paths, deadline=53)
        run.assert_not_called()

    def test_parent_clock_expiry_after_reply_refuses(self):
        with patch.object(m.time, 'monotonic', side_effect=[50, 53]):
            with self.assertRaisesRegex(ValueError, 'deadline expired'):
                self.invoke(deadline=53)

    def test_batch_expiry_does_not_accept_earlier_passed_rows(self):
        self.mutate = lambda value: value.update(expired=True)
        result, _ = self.invoke(collect_errors=True)
        self.assertTrue(all('deadline expired' in error for error in result.values()))

    def test_expiry_retains_actual_path_refusal_as_well(self):
        def mutate(value):
            value['expired'] = True
            value['results'][0].update(passed=False, error='foreign owner')
        self.mutate = mutate
        result, _ = self.invoke(collect_errors=True)
        self.assertIn('deadline expired', result[str(self.paths[0])])
        self.assertIn('foreign owner', result[str(self.paths[0])])
        self.assertEqual(result[str(self.paths[1])], 'original output audit deadline expired')

    def test_missing_path_result_refuses(self):
        self.mutate = lambda value: value['results'].pop()
        with self.assertRaisesRegex(ValueError, 'reply differs'):
            self.invoke()

    def test_reordered_path_result_refuses(self):
        self.mutate = lambda value: value['results'].reverse()
        with self.assertRaisesRegex(ValueError, 'path result differs'):
            self.invoke()

    def test_boolean_index_cannot_replace_exact_integer(self):
        self.mutate = lambda value: value['results'][0].update(index=False)
        with self.assertRaisesRegex(ValueError, 'path result differs'):
            self.invoke()

    def test_path_refusal_preserved_while_later_success_retained(self):
        self.mutate = lambda value: value['results'][0].update(passed=False, error='foreign owner')
        result, _ = self.invoke(collect_errors=True)
        self.assertEqual(result, {str(self.paths[0]): 'foreign owner', str(self.paths[1]): None})
        with self.assertRaisesRegex(ValueError, 'foreign owner'):
            self.invoke()

    def test_duplicate_or_empty_roster_refuses(self):
        for paths in [[], [self.paths[0], self.paths[0]]]:
            with self.subTest(paths=paths), self.assertRaises(ValueError):
                m.guard_many(self.plan, paths)

    def test_subprocess_timeout_propagates(self):
        with patch.object(m, 'base', return_value=self.base), patch.object(m.subprocess, 'run', side_effect=m.subprocess.TimeoutExpired('pwsh', 1)):
            with self.assertRaises(m.subprocess.TimeoutExpired):
                m.guard_many(self.plan, self.paths)

    def test_retention_budget_stays_ten_without_refreshing_execution_clock(self):
        with patch.object(m.time, 'monotonic', return_value=1000):
            self.assertEqual(m.audit_timeout(None), 10)
            with self.assertRaises(ValueError):
                m.audit_timeout(999)
        with patch.object(m, 'guard') as guard, patch.object(m.Path, 'open', mock_open()), patch.object(m.os, 'fsync'):
            m.write_new(self.plan, self.paths[0], {'complete': False, 'whole_phase_expired': True})
        self.assertEqual(guard.call_count, 2)
        self.assertEqual(guard.call_args_list[0].kwargs, {})
        self.assertEqual(guard.call_args_list[1].args, (self.plan, self.paths[0], True))

    def test_final_proofs_all_attempted_after_one_audit_failure(self):
        paths = dict(zip(['native_result', 'operations', 'stdout'], [m.ROOT / name for name in ['a', 'b', 'c']]))
        audits = {str(p): None for p in paths.values()}
        audits[str(paths['native_result'])] = 'foreign owner'
        hashed = []
        def hash_file(path):
            hashed.append(path)
            return {'path': str(path), 'bytes': 20, 'sha256': 'a' * 64}
        with patch.object(m, 'guard_many', return_value=audits):
            errors, callback = m.final_output_hasher(self.plan, paths, 120, hash_file)
        receipt = {'complete': True, 'failure': None}
        m.finish(receipt, paths, 0, 120, callback, clock=lambda: 10)
        self.assertEqual(errors, {str(paths['native_result']): 'foreign owner'})
        self.assertEqual(hashed, [paths['operations'], paths['stdout']])
        self.assertEqual(receipt['final_proof_success'], ['operations', 'stdout'])
        self.assertEqual([e['label'] for e in receipt['final_proof_errors']], ['native_result'])
        self.assertFalse(receipt['complete'])

    def test_final_batch_failure_retains_error_for_every_proof(self):
        paths = {name: m.ROOT / name for name in ['native_result', 'operations', 'stdout']}
        with patch.object(m, 'guard_many', side_effect=TimeoutError('original cutoff')):
            errors, callback = m.final_output_hasher(self.plan, paths, 120, lambda path: self.fail('unaudited file consumed'))
        receipt = {'complete': True, 'failure': None}
        m.finish(receipt, paths, 0, 120, callback, clock=lambda: 120)
        self.assertEqual(len(errors), 3)
        self.assertEqual([e['label'] for e in receipt['final_proof_errors']], ['native_result', 'operations', 'stdout'])
        self.assertTrue(receipt['whole_phase_expired'])


class RuntimeDiagnostic(unittest.TestCase):
    def setUp(self):
        self.plan,self.native,self.profile,self.fixture,v,_=literal()
        self.plan.update(schema='airk.private.cuda-runtime-diagnostic-plan/1',selector='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::cuda_runtime_only_admission_diagnostic')
        for key in ['complete','model','fixture','reference_producer','measured_peak_device_bytes','load_us','rows','adapter_before','adapter_after','final_placement','execution_identity']:
            v.pop(key)
        v.update(schema='airk.private.cuda-runtime-admission-diagnostic/1',diagnostic_complete=True,diagnostic_error=None,
                 runtime_admitted=True,outcome='admitted',error=None,error_chain=[],model_requests_dispatched=False,
                 adapters_observed=False,device_context_created=False,scorer_created=False,session_created=False,
                 operations_journal_created=False,forward_count_observed=None,default_owner='S-1-5-21-3678370870-3858642417-826968105-1002',resource_snapshot={'host_bytes':17179869184})
        v['operation_scope']='False operation flags describe explicit diagnostic caller operations only; production admission includes cuInit and makes no claim that driver internals allocate nothing or create no internal contexts'
        self.value=v

    def validate(self):
        return m.validate_diagnostic(self.value,self.plan,42,self.native,self.profile)

    def refuse(self):
        self.value.update(runtime_admitted=False,outcome='refused',error='admission refused',error_chain=['admission refused'])

    def test_clean_admission(self):
        self.assertTrue(self.validate())

    def test_clean_refusal(self):
        self.refuse();self.assertFalse(self.validate())

    def test_clean_refusal_cannot_be_poisoned(self):
        self.refuse(); self.value['process_cuda_poisoned']=True
        with self.assertRaises(ValueError): self.validate()

    def test_missing_caller_scope_refused(self):
        del self.value['operation_scope']
        with self.assertRaises(ValueError): self.validate()

    def test_retained_refusal_preserves_facts(self):
        self.refuse()
        self.value.update(native_control_retained_for_unknown_cuda=True,process_cuda_poisoned=True,native_control_cleanup_observed=False,native_host_charge_release_observed=False)
        before=copy.deepcopy(self.value)
        self.assertFalse(self.validate());self.assertEqual(before,self.value)

    def test_scope_custody_deadline_refusals(self):
        original=copy.deepcopy(self.value)
        cases=[('schema','airk.private.cuda-component-result/1'),('diagnostic_complete',False),('diagnostic_error','drift'),
               ('pid',True),('pid',43),('default_owner','foreign'),('execution_user','foreign'),('profile','bad'),
               ('native_binary_sha256','f'*64),('resource_snapshot',{}),('qualified',True),('product_admission',True),
               ('model_requests_dispatched',True),('adapters_observed',True),('device_context_created',True),
               ('scorer_created',True),('session_created',True),('operations_journal_created',True),('forward_count_observed',0),
               ('original_expires_monotonic_ns',0),('elapsed_us_to_final_report_start',120000000),('peak_process_rss_bytes',17179869185),
               ('native_host_reservation_bytes',1),('final_host_peak_error','unknown'),('final_report_retention_exceeded_original_deadline',True),
               ('native_control_cleanup_error','unknown'),('native_control_cleanup_observed',False),('native_host_charge_release_observed',False),
               ('process_cuda_poisoned',True),('native_control_retained_for_unknown_cuda',True),('error','unexpected'),('error_chain',['unexpected']),('outcome','refused'),('rows',[])]
        for key,bad in cases:
            with self.subTest(key=key,bad=bad):
                self.value=copy.deepcopy(original);self.value[key]=bad
                with self.assertRaises(ValueError):self.validate()

    def test_refusal_requires_chain(self):
        self.refuse()
        for bad in [None,[],[''],[None]]:
            with self.subTest(bad=bad):
                self.value['error_chain']=bad
                with self.assertRaises(ValueError):self.validate()

    def test_census_complete_and_unmodified(self):
        self.refuse()
        census={'schema':'airk.private.cuda-module-refusal/1','stage':'after-cuInit','complete':True,'omitted_count':0,'unapproved_count':1,'reported_count':1,'mismatches':[{'actual':{'path':'C:/driver.dll'}}]}
        error='new/unapproved mapped images: '+json.dumps(census)
        self.value.update(error=error,error_chain=[error])
        self.assertFalse(self.validate());self.assertEqual(self.value['error_chain'],[error])
        for key,bad in [('complete',False),('omitted_count',1),('reported_count',0),('unapproved_count',2)]:
            with self.subTest(key=key):
                changed=dict(census);changed[key]=bad
                self.value['error_chain']=['new/unapproved mapped images: '+json.dumps(changed)]
                with self.assertRaises(ValueError):self.validate()

    def test_both_census_markers_refuse_incomplete_or_ambiguous_counts(self):
        self.refuse()
        base={'schema':'airk.private.cuda-module-refusal/1','stage':'initial_baseline','complete':True,
              'omitted_count':0,'unapproved_count':1,'reported_count':1,'mismatches':[{'actual':{'path':'C:/driver.dll'}}]}
        for marker in ['new/unapproved mapped images: ', 'unapproved existing GPU/core module: ']:
            self.value['error_chain']=[marker+json.dumps(base)+'  ']
            self.assertFalse(self.validate())
            for key,bad in [('complete',False),('omitted_count',1),('omitted_count',False),
                            ('reported_count',True),('unapproved_count',True),('reported_count',0),
                            ('schema','wrong'),('stage',''),('mismatches',[])]:
                with self.subTest(marker=marker,key=key,bad=bad):
                    census=dict(base);census[key]=bad
                    self.value['error_chain']=[marker+json.dumps(census)]
                    with self.assertRaises(ValueError):self.validate()
            for suffix in [' trailing', '{}', ' null']:
                with self.subTest(marker=marker,suffix=suffix):
                    self.value['error_chain']=[marker+json.dumps(base)+suffix]
                    with self.assertRaises(ValueError):self.validate()

    def test_exact_mode_pair(self):
        self.assertTrue(m.execution_mode(self.plan))
        for key,bad in [('schema',m.SCHEMA),('selector',m.SELECTOR)]:
            changed=dict(self.plan);changed[key]=bad
            with self.assertRaises(ValueError):m.execution_mode(changed)

    def test_component_refuses_diagnostic(self):
        with self.assertRaises(ValueError):m.validate_native(self.value,self.plan,42,self.native,self.profile,self.fixture)

    def test_two_proofs_and_original_expiry(self):
        paths={'native_result':Path('result'),'stdout':Path('stdout')};observed=[]
        def hasher(path):
            observed.append(path);return {'path':str(path),'bytes':1,'sha256':'a'*64}
        receipt={'complete':True,'failure':None}
        m.finish(receipt,paths,0,120,hasher,clock=lambda:10,proofs=m.DIAGNOSTIC_PROOFS)
        self.assertTrue(receipt['complete']);self.assertEqual(observed,[Path('result'),Path('stdout')])
        receipt={'complete':True,'failure':None}
        m.finish(receipt,paths,0,120,hasher,clock=lambda:120,proofs=m.DIAGNOSTIC_PROOFS)
        self.assertFalse(receipt['complete']);self.assertTrue(receipt['whole_phase_expired'])


class PlacementDiagnostic(unittest.TestCase):
    def setUp(self):
        RuntimeDiagnostic.setUp(self)
        self.plan.update(schema='airk.private.cuda-placement-diagnostic-plan/1',selector='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::cuda_load_only_placement_diagnostic')
        for key in ['runtime_admitted','qualified','adapters_observed','device_context_created','scorer_created','operations_journal_created']:
            self.value.pop(key)
        self.value.update(schema='airk.private.cuda-placement-diagnostic/1',outcome='placement_observed',
            operation_scope='Disposable session creation and final placement observation only; no Scorer, forward, hybrid policy or qualification is returned',
            session_load_dispatched=True,session_created=True,strict_production_unchanged=True,scorer_returned=False,
            forward_dispatched=False,qualification=False,cpu_fallback_disabled=False,
            teardown={'pre_sync_observed':True,'session_drop_attempted':True,'session_drop_returned':True,'post_sync_observed':True,'pending_claims':0,'poisoned':False,'errors':[]},
            placement={'scope':'onnxruntime-final-session-state-ep-placement','complete':True,'shim_status':0,'ort_error_code':0,
                       'reported_nodes':1,'reported_string_bytes':26,'nodes':[{'subgraph_index':0,'name':'n','domain':'','op':'Shape','ep':'CPUExecutionProvider'}]},
            logs={'seen':2,'selected':1,'retained':1,'omitted':0,'bytes':6,'truncated':False,'rows':[{'location':'a','message':'cpu'}]})

    def validate(self):
        m.validate_placement_diagnostic(self.value,self.plan,42,self.native,self.profile)


class PlacementControls(PlacementDiagnostic):
    def test_full_cpu_placement_is_diagnostic_only(self):
        self.validate()
        with self.assertRaises(ValueError):m.validate_native(self.value,self.plan,42,self.native,self.profile,self.fixture)
        with self.assertRaises(ValueError):m.validate_diagnostic(self.value,self.plan,42,self.native,self.profile)

    def test_mode_pair_is_separate(self):
        self.assertEqual(m.execution_mode(self.plan),'placement')
        self.plan['selector']=m.SELECTOR
        with self.assertRaises(ValueError):self.validate()

    def test_scope_and_common_custody_refusals(self):
        original=copy.deepcopy(self.value)
        for key,bad in [('schema','airk.private.cuda-runtime-admission-diagnostic/1'),('diagnostic_complete',False),('diagnostic_error','post-validation failed'),
             ('session_created',False),('session_load_dispatched',False),('outcome','load_failed'),('scorer_returned',True),
             ('forward_dispatched',True),('model_requests_dispatched',True),('qualification',True),('product_admission',True),
             ('cpu_fallback_disabled',True),('strict_production_unchanged',False),('forward_count_observed',0),('error','failed'),
             ('error_chain',['failed']),('native_control_cleanup_observed',False),('native_host_charge_release_observed',False),
             ('native_control_retained_for_unknown_cuda',True),('process_cuda_poisoned',True),('default_owner','foreign'),
             ('pid',43),('pid',True),('peak_process_rss_bytes',17179869185),('elapsed_us_to_final_report_start',120000000),
             ('native_host_reservation_bytes',0),('resource_snapshot',{}),('rows',[]),('placement',None)]:
            with self.subTest(key=key):
                self.value=copy.deepcopy(original);self.value[key]=bad
                with self.assertRaises(ValueError):self.validate()

    def test_teardown_refusals(self):
        for key,bad in [('pre_sync_observed',False),('session_drop_attempted',False),('session_drop_returned',False),('errors',['drop failure']),('post_sync_observed',False),
                        ('pending_claims',None),('pending_claims',True),('pending_claims',1),('poisoned',True)]:
            with self.subTest(key=key,bad=bad):
                previous=copy.deepcopy(self.value['teardown']);self.value['teardown'][key]=bad
                with self.assertRaises(ValueError):self.validate()
                self.value['teardown']=previous

    def test_partial_placement_and_counts(self):
        for key,bad in [('complete',False),('shim_status',1),('shim_status',False),('ort_error_code',1),
                        ('reported_nodes',True),('reported_nodes',2),('reported_string_bytes',25),('nodes',[])]:
            with self.subTest(key=key):
                previous=copy.deepcopy(self.value['placement']);self.value['placement'][key]=bad
                with self.assertRaises(ValueError):self.validate()
                self.value['placement']=previous

    def test_log_omission_counts_and_caps(self):
        for key,bad in [('truncated',True),('omitted',1),('seen',0),('selected',2),('retained',True),('bytes',5),
                        ('rows',[{'location':'x'*4096,'message':''}]),('rows',[{'location':None,'message':''}])]:
            with self.subTest(key=key):
                previous=copy.deepcopy(self.value['logs']);self.value['logs'][key]=bad
                if key=='rows': self.value['logs']['bytes']=4098
                with self.assertRaises(ValueError):self.validate()
                self.value['logs']=previous


class PlacementBoundaries(PlacementDiagnostic):
    def test_utf8_node_limit(self):
        node=self.value['placement']['nodes'][0]
        node['name']='é'*2048
        self.value['placement']['reported_string_bytes']=4121
        self.validate()
        node['name']+='a';self.value['placement']['reported_string_bytes']+=1
        with self.assertRaises(ValueError):self.validate()

    def test_log_exact_row_and_total_limits(self):
        row={'location':'','message':'x'*4094}
        self.value['logs'].update(seen=16,selected=16,retained=16,bytes=65536,rows=[row.copy() for _ in range(16)])
        self.validate()
        self.value['logs'].update(seen=17,selected=17,retained=17,bytes=69632,rows=[row.copy() for _ in range(17)])
        with self.assertRaises(ValueError):self.validate()

    def test_empty_unselected_logs_are_truthful(self):
        self.value['logs'].update(seen=5,selected=0,retained=0,bytes=0,rows=[])
        self.validate()

    def test_no_boolean_or_overflow_log_counters(self):
        for key in ['seen','selected','retained','omitted','bytes']:
            for bad in [False,1<<64,-1]:
                with self.subTest(key=key,bad=bad):
                    previous=self.value['logs'][key];self.value['logs'][key]=bad
                    with self.assertRaises(ValueError):self.validate()
                    self.value['logs'][key]=previous

    def test_all_subgraphs_and_provider_observations_preserved(self):
        self.value['placement']['nodes'].append({'subgraph_index':3,'name':'','domain':'','op':'If','ep':'CUDAExecutionProvider'})
        self.value['placement'].update(reported_nodes=2,reported_string_bytes=49)
        before=copy.deepcopy(self.value['placement']);self.validate()
        self.assertEqual(before,self.value['placement'])


class NativeSourceIntroductions(unittest.TestCase):
    def setUp(self):
        NativeSourceEpoch.setUp(self)
        self.new_keys=[
            'crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic.rs',
            'crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/tests.rs',
        ]
        for key in self.new_keys:del self.old_files[key]
        self.introductions=[{'path':str(m.ROOT/key),'new_sha256':'a'*64} for key in self.new_keys]
        self.plan['native_source_introductions']=copy.deepcopy(self.introductions)

    def test_exact_new_owner_keys_record_no_historical_sha(self):
        self.assertEqual(m.verify_native_transition(self.old,self.old_files,self.new_files,self.plan),self.rows)
        self.assertEqual(m.native_source_introductions(self.old_files,self.current),self.introductions)
        self.assertTrue(all(set(row)=={'path','new_sha256'} for row in self.introductions))

    def test_missing_new_build_key_refuses(self):
        for key in self.new_keys:
            changed=dict(self.new_files);del changed[key]
            with self.assertRaises(ValueError):m.rebind_native_sources(self.old['required_native_sources'],self.old_files,changed)

    def test_new_owner_stale_hash_or_omitted_from_plan_refuses(self):
        for bad in ['f'*64,None]:
            changed=copy.deepcopy(self.plan)
            if bad is None:del changed['required_native_sources'][self.new_keys[0]]
            else:changed['required_native_sources'][self.new_keys[0]]=bad
            with self.assertRaises(ValueError):m.verify_native_transition(self.old,self.old_files,self.new_files,changed)

    def test_missing_extra_stale_or_fabricated_introduction_refuses(self):
        for rows in [[],self.introductions+[{'path':str(m.ROOT/'crates/unrelated.rs'),'new_sha256':'a'*64}],
                     [dict(self.introductions[0],new_sha256='f'*64),self.introductions[1]],
                     [dict(self.introductions[0],old_sha256='a'*64),self.introductions[1]]]:
            with self.subTest(rows=rows):
                changed=copy.deepcopy(self.plan);changed['native_source_introductions']=rows
                with self.assertRaises(ValueError):m.verify_native_transition(self.old,self.old_files,self.new_files,changed)

    def test_old_seal_cannot_claim_new_owner_historical_identity(self):
        self.old['input_seals'][str(m.ROOT/self.new_keys[0])]='a'*64
        with self.assertRaises(ValueError):m.verify_native_transition(self.old,self.old_files,self.new_files,self.plan)

    def test_historical_required_owner_cannot_be_reclassified_new(self):
        self.old['required_native_sources'][self.new_keys[0]]='a'*64
        with self.assertRaises(ValueError):m.rebind_native_sources(self.old['required_native_sources'],self.old_files,self.new_files)

    def test_other_missing_old_owner_refuses(self):
        key='crates/airk-retrieval/src/arbiter/runtime.rs';del self.old_files[key]
        with self.assertRaises(ValueError):m.rebind_native_sources(self.old['required_native_sources'],self.old_files,self.new_files)


if __name__ == '__main__':
    unittest.main()
