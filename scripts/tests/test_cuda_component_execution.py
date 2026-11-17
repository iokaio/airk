# SPDX-License-Identifier: Apache-2.0
"""Literal refusal controls; no process launch, fixtures, cache or GPU work."""
import copy
import ctypes
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

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

if __name__ == '__main__':
    unittest.main()
