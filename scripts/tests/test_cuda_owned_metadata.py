# SPDX-License-Identifier: Apache-2.0
"""Inert checks for the maintained NN0 stage contracts and frozen reader."""
import copy
import hashlib
import os
from pathlib import Path
import runpy
import time
import types
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
M = types.SimpleNamespace(**runpy.run_path(str(SCRIPTS / 'cuda-native-execution/metadata.py')))
COMPONENT = types.SimpleNamespace(**runpy.run_path(str(SCRIPTS / 'cuda-component-execution.py')))
P = 'inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::'
H = '1' * 64
J = '2' * 64
SCHEMA = 'airk.private.cuda-component-probe-request/1'
Q = 'AIRK_CUDA_QUALIFICATION_INPUT'
R = str(M.ANCHOR / 'synthetic-stage')
D = str(M.ANCHOR / 'synthetic-runtime')


def pin(path, sha=H):
    return {'path': str(path), 'bytes': 17, 'sha256': sha}


def wire(pin):
    return {key: pin[key] for key in ('path', 'sha256')}


def profile_fixture():
    target = Path(R) / 'profile'
    names = ('profile.json', 'native-input.json', 'probe-request.json', 'commands.json',
             'metadata-receipt.json', 'cal.json')
    paths = [target / name for name in names]
    rows = [pin(path) for path in paths]
    request = pin(Path(R) / 'prior-prepare.json', J)
    image = pin(Path(R) / 'native.exe')
    plan = {'kind': 'metadata', 'output_root': R, 'native_image': image,
            'selector': P + 'emit_cuda_fp32_probe_metadata',
            'output_pins_expected': [str(path) for path in paths[:-1]],
            'environment': {Q: request['path'], Q + '_SHA256': J,
                            'AIRK_CUDA_METADATA_DIRECTORY': str(target)}}
    prepared = {'schema': SCHEMA, 'kind': 'prepare',
                'cpu_input': wire(pin(Path(R) / 'cpu-input.json')),
                'runtime_manifest': wire(pin(Path(D) / 'airk-cuda-runtime.json')),
                'bounds': {'schema': 'airk.arbiter.gpu-probe-reservation/1', 'deadline_ms': 120_000,
                           'host_limit_bytes': 16 << 30, 'device_limit_bytes': 24 << 30},
                'cold_bytes': 16 << 30, 'warm_bytes': 8 << 30, 'headroom_bytes': 2 << 30,
                'job_limit_bytes': 24 << 30}
    commands = [{'program': image['path'],
                 'args': ['--exact', P + f'cuda_original_context_fp32_t{count}_component',
                          '--ignored', '--nocapture'],
                 'env': {Q: rows[2]['path'], Q + '_SHA256': H}}
                for count in (17, 128)]
    values = dict(zip(paths[:-1], [
        {'model': 'arbiter-context-qwen3-17b', 'manifest': H,
         'assets': [{'role': 'calibration', 'name': 'cal.json', 'bytes': 17, 'sha256': H}]},
        {'profile': wire(rows[0]), 'files': {'cal.json': rows[5]['path']}},
        {**{key: value for key, value in prepared.items() if key != 'cpu_input'},
         'kind': 'probe', 'gpu_input': wire(rows[1])},
        commands,
        {'schema': 'airk.private.cuda-probe-metadata/1', 'elapsed_us': 6,
         'request': wire(request), 'cpu_input': prepared['cpu_input'], 'probe_request': wire(rows[2]),
         'model': 'arbiter-context-qwen3-17b', 'profile': H,
         'model_requests_dispatched': False, 'forward_count_observed': None, 'qualified': False},
    ]))
    return plan, prepared, paths, rows, values


class OwnedMetadataTests(unittest.TestCase):
    def test_stage_selection_and_closed_environment(self):
        plan, _, paths, _, _ = profile_fixture()
        self.assertEqual(M.stage_paths(plan)[2], paths[:-1])
        bad = copy.deepcopy(plan)
        bad['environment']['AIRK_CUDA_RUNTIME_MANIFEST_SHA256'] = H
        with self.assertRaisesRegex(ValueError, 'stage-specific environment'):
            M.stage_paths(bad)
        bad = dict(plan, selector=P + 'cuda_original_context_fp32_t17_component')
        with self.assertRaisesRegex(ValueError, 'selector differs'):
            M.stage_paths(bad)

    def test_assembly_requires_descriptor_beside_exact_runtime(self):
        manifest = Path(D) / 'airk-cuda-runtime.json'
        request_pin = pin(Path(R) / 'assembly.json')
        plan = {'kind': 'assembly', 'output_root': R,
                'selector': P + 'emit_cuda_runtime_manifest_from_held_native_closure',
                'environment': {Q: request_pin['path'], Q + '_SHA256': H,
                                'AIRK_CUDA_QUALIFICATION_OUTPUT': str(manifest)},
                'output_pins_expected': [str(manifest), str(manifest.with_suffix('.receipt.json'))]}
        request = {'schema': SCHEMA, 'kind': 'assembly', 'assembly': {'directory': D}}
        self.assertEqual(M.stage_paths(plan, request)[2], [manifest, manifest.with_suffix('.receipt.json')])
        request['assembly']['directory'] = str(Path(D).parent)
        with self.assertRaisesRegex(ValueError, 'directory differs'):
            M.stage_paths(plan, request)

    def test_request_emitters_are_exact_opaque_rust_inputs(self):
        result_path = Path(R) / 'request.json'
        raw_proof = wire(pin(Path(R) / 'build-proof.json'))
        provider_a, provider_b = ('onnxruntime_providers_shared.dll', 'onnxruntime_providers_cuda.dll')
        assembly = {'directory': D, 'producer_name': 'fresh-compile.json', 'producer_sha256': H,
                    'checkpointed_proof': raw_proof, 'module_names': [provider_a, provider_b],
                    'load_order': [provider_a, provider_b], 'system_imports': ['kernel32.dll', 'ntdll.dll']}
        plan = {'kind': 'assembly-request', 'output_root': R,
                'selector': P + 'emit_cuda_component_probe_request',
                'output_pins_expected': [str(result_path)],
                'environment': {
                    'AIRK_CUDA_REQUEST_KIND': 'assembly', 'AIRK_CUDA_RUNTIME_ROOT': D,
                    'AIRK_CUDA_PRODUCER_NAME': 'fresh-compile.json', 'AIRK_CUDA_PRODUCER_SHA256': H,
                    'AIRK_CUDA_CHECKPOINTED_PROOF': raw_proof['path'],
                    'AIRK_CUDA_CHECKPOINTED_PROOF_SHA256': raw_proof['sha256'],
                    'AIRK_CUDA_MODULE_NAMES': f'{provider_a};{provider_b}',
                    'AIRK_CUDA_LOAD_ORDER': f'{provider_a};{provider_b}',
                    'AIRK_CUDA_SYSTEM_IMPORTS': 'ntdll.dll;kernel32.dll', 'AIRK_CUDA_DEADLINE_MS': '120000',
                    'AIRK_CUDA_REQUEST_OUTPUT': str(result_path)}}
        self.assertEqual(M.stage_paths(plan)[2], [result_path])
        expected = {'schema': SCHEMA, 'kind': 'assembly', 'assembly': assembly, 'deadline_ms': 120000}
        M.output_values('assembly-request', [result_path], [], plan, {}, {}, lambda path: expected)
        expected['assembly'] = dict(assembly, producer_sha256=J)
        with self.assertRaisesRegex(ValueError, 'emitted request differs'):
            M.output_values('assembly-request', [result_path], [], plan, {}, {}, lambda path: expected)
        _, prepared, _, _, _ = profile_fixture()
        plan['kind'] = 'request'
        plan['environment'] = {
            'AIRK_CUDA_REQUEST_KIND': 'prepare', 'AIRK_CUDA_DEADLINE_MS': '120000',
            'AIRK_CUDA_REQUEST_OUTPUT': str(result_path),
            'AIRK_SOFTWARE_V2_NATIVE_INPUT': prepared['cpu_input']['path'],
            'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256': prepared['cpu_input']['sha256'],
            'AIRK_CUDA_RUNTIME_MANIFEST': prepared['runtime_manifest']['path'],
            'AIRK_CUDA_RUNTIME_MANIFEST_SHA256': prepared['runtime_manifest']['sha256'],
            'AIRK_CUDA_HOST_LIMIT_BYTES': str(16 << 30), 'AIRK_CUDA_DEVICE_LIMIT_BYTES': str(24 << 30),
            'AIRK_CUDA_COLD_BYTES': str(16 << 30), 'AIRK_CUDA_WARM_BYTES': str(8 << 30),
            'AIRK_CUDA_HEADROOM_BYTES': str(2 << 30), 'AIRK_CUDA_JOB_LIMIT_BYTES': str(24 << 30)}
        self.assertEqual(M.stage_paths(plan)[2], [result_path])
        M.output_values('request', [result_path], [], plan, {}, {}, lambda path: prepared)
        prepared['bounds']['deadline_ms'] -= 1
        with self.assertRaisesRegex(ValueError, 'emitted request differs'):
            M.output_values('request', [result_path], [], plan, {}, {}, lambda path: prepared)

    def test_native_assembly_emission_binds_original_pid_and_runtime_bytes(self):
        manifest = Path(D) / 'airk-cuda-runtime.json'
        paths = [manifest, manifest.with_suffix('.receipt.json')]
        rows = list(map(pin, paths))
        source = wire(pin(Path(R) / 'assembly.json'))
        producer = 'fresh-compile.json'
        modules = ['onnxruntime_providers_shared.dll', 'onnxruntime_providers_cuda.dll']
        assembly = {'directory': D, 'producer_name': producer, 'producer_sha256': H,
                    'module_names': modules, 'load_order': modules, 'system_imports': ['kernel32.dll']}
        runtime = {name: pin(Path(D) / name) for name in (producer, *modules)}
        plan = {'environment': {Q: source['path'], Q + '_SHA256': H},
                'input_seals': {row['path']: H for row in runtime.values()}}
        inputs = {Q: {'assembly': assembly}, 'runtime_pins': runtime}
        values = {manifest: {
            'schema': 'airk.native.cuda-runtime-manifest/1', 'ort_version': '1.30.0',
            'producer_sha256': H, 'producer_name': producer, 'producer_bytes': 17,
            'static_core_build': H, 'fits': [], 'load_order': modules, 'system_imports': ['kernel32.dll'],
            'files': [{'name': name, 'bytes': 17, 'sha256': H} for name in modules]},
            paths[1]: {'schema': 'airk.private.cuda-runtime-assembly-emission/1', 'input': source,
                       'manifest': wire(rows[0]), 'execution_user': M.SID, 'pid': 91, 'elapsed_us': 6,
                       'model_requests_dispatched': False, 'runtime_admission_dispatched': False,
                       'forward_count_observed': None, 'approved': False, 'fits_empty': True}}

        def read(path, limit=None):
            return values[path]

        M.output_values('assembly', paths, rows, plan, inputs, {'owned_leader_pid': 91}, read)
        with self.assertRaisesRegex(ValueError, 'emission identity'):
            M.output_values('assembly', paths, rows, plan, inputs, {'owned_leader_pid': 92}, read)
        values[manifest]['files'][0]['sha256'] = J
        with self.assertRaisesRegex(ValueError, 'differs from held files'):
            M.output_values('assembly', paths, rows, plan, inputs, {'owned_leader_pid': 91}, read)

    def test_native_profile_validates_provenance_and_six_outputs(self):
        plan, prepare, paths, rows, values = profile_fixture()
        inputs = {Q: prepare, 'AIRK_SOFTWARE_V2_NATIVE_INPUT': {'profile': wire(rows[0]), 'files': {}},
                  'cpu_model': 'arbiter-context-qwen3-17b'}
        M.output_values('metadata', paths, rows, plan, inputs, {}, values.__getitem__)
        for key, mutation in [('forward_count_observed', 0), ('qualified', True),
                              ('cpu_input', {'path': prepare['cpu_input']['path'], 'sha256': J})]:
            bad = copy.deepcopy(values)
            bad[paths[4]][key] = mutation
            with self.subTest(key=key), self.assertRaises(ValueError):
                M.output_values('metadata', paths, rows, plan, inputs, {}, bad.__getitem__)
        values[paths[3]][0]['args'][1] = P + 'cuda_selected_focus_fp32_512_component'
        with self.assertRaisesRegex(ValueError, 'command metadata differs'):
            M.output_values('metadata', paths, rows, plan, inputs, {}, values.__getitem__)

    def test_frozen_metadata_consumer_accepts_compatible_six_output_receipt(self):
        stage, _, paths, rows, native = profile_fixture()
        # Keep the consumer's real transition and pin-roster checks. Only disk
        # byte reads and retained Job observations are supplied as inert fixtures.
        sources = {key: H for key in COMPONENT.SOURCE_KEYS}
        build_pin, plan_pin, current_build = (pin(Path(R) / name)
                                             for name in ('build.json', 'plan.json', 'next-build.json'))
        source = sorted(sources)[0]
        source_path = str(COMPONENT.ROOT / source)
        current_sources = dict(sources, **{source: J})
        receipt_pin = pin(Path(R) / 'execution.json')
        old = dict(stage, required_native_sources=sources, input_seals={source_path: H},
                   native_build_receipt=build_pin)
        receipt = {'schema': 'airk.private.owned-cuda-profile-nn0/1', 'plan': plan_pin,
                   'complete': True, 'failure': None, 'whole_phase_expired': False,
                   'post_input_drift': [], 'elapsed_seconds': 10, 'kind': 'metadata',
                   'owned': {'fixture': 'settled'}, 'activation': False, 'qualification': False,
                   'execution_user_sid': M.SID, 'input_seals': {source_path: H, plan_pin['path']: H}, 'outputs': rows}
        data = {receipt_pin['path']: receipt, plan_pin['path']: old,
                build_pin['path']: {'groups': {'native': {'files': sources}}},
                current_build['path']: {'groups': {'native': {'files': current_sources}}},
                str(paths[4]): native[paths[4]]}
        allowed_pins = {row['path']: row for row in (receipt_pin, plan_pin, build_pin, current_build, *rows)}

        def checked(row):
            self.assertEqual(allowed_pins[row['path']], row)
            return row['path']

        base = types.SimpleNamespace(read=lambda path, limit=None: data[path], pinned=checked,
                                     core_pin_matches=lambda actual, expected: M.equivalent(expected, actual),
                                     source_image=lambda value: self.assertIs(value, old),
                                     regular=lambda path: path,
                                     digest=lambda path: {source_path: J, plan_pin['path']: H}[path])
        parent = types.SimpleNamespace(settled=lambda owned: owned == {'fixture': 'settled'})
        current = {'metadata_receipt': receipt_pin, 'native_build_receipt': current_build,
                   'required_native_sources': current_sources, 'native_source_introductions': [],
                   'native_source_transition': [{'path': source_path, 'old_sha256': H, 'new_sha256': J}],
                   'request': rows[2], 'native_input': rows[1], 'profile': rows[0]}
        self.assertEqual(COMPONENT.metadata(current, base, parent), (old, rows))
        self.assertEqual(receipt['input_seals'][source_path], H)
        receipt['whole_phase_expired'] = True
        with self.assertRaisesRegex(ValueError, 'successful metadata custody'):
            COMPONENT.metadata(current, base, parent)

    def test_deadline_or_source_drift_blocks_completion(self):
        self.assertIsNone(M.check(10, clock=lambda: 9))
        with self.assertRaises(TimeoutError):
            M.check(10, clock=lambda: 10)
        path = pin(Path(R) / 'original.json')['path']
        namespace = M.verify_seals.__globals__
        observed = {}
        with patch.dict(namespace, {'file_observation': lambda key, deadline: (pin(key, J), None)}):
            self.assertEqual(M.verify_seals({path: H}, 10, observed), [path])
        self.assertEqual(observed, {path: pin(path, J)})

    @unittest.skipUnless(os.environ.get('AIRK_METADATA_TEST_FILE'), 'restricted synthetic fixture not supplied')
    def test_actual_bounded_file_reader_under_owned_synthetic_root(self):
        path = Path(os.environ['AIRK_METADATA_TEST_FILE'])
        deadline = time.monotonic() + 2
        row, value = M.read_json(path, deadline, 18)
        self.assertEqual(row['sha256'], hashlib.sha256(b'{"synthetic":true}').hexdigest())
        self.assertEqual(value, {'synthetic': True})
        with self.assertRaisesRegex(ValueError, 'byte bound'):
            M.read_json(path, deadline, 8)
        with self.assertRaises(TimeoutError):
            M.read_json(path, time.monotonic(), 18)


if __name__ == '__main__':
    unittest.main()
