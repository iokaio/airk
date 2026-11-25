# SPDX-License-Identifier: Apache-2.0
"""Current consumers follow Rust-emitted policy; no inference or native launch."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    path = ROOT / 'scripts' / (name + '.py')
    specification = importlib.util.spec_from_file_location(name.replace('-', '_'), path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


POLICY, CLIENTS, ORACLE = load('int8-policy'), load('check-service-clients'), load('qualify-local-model')
NATIVE = json.loads((ROOT / 'fixtures/models/model-policy.json').read_bytes())['catalog']
SMOKE_OBSERVATIONS = json.loads((ROOT / 'fixtures/models/native-smoke-observations.json').read_bytes())


# The normal Windows matched pair sealed this exact native observation in
# airk-build.json (SHA-256 0c65bcbba8cf5df1f06995a552736f9a0d98c5b4d66749baf6f80855f16d5059).
CURRENT_SMOKE = {
    'dimensions': 384, 'engine': 'onnxruntime-cpu/1.30.0',
    'legacy_identity': '0191673c2073983f544cb136e5049f28a1759ed27d94b30a18fbec3c5856fe2b',
    'pipeline_digest': 'f743d218623749f265f4cab713ea8a9e7ffc3cb308fa4e1f616b5fdf83d25ce4',
    'profile': 'minilm-l6-v2-fp16-v1', 'repeat_equal': True, 'schema': 1, 'source': 'embedded',
}


class CurrentModelConsumers(unittest.TestCase):
    def test_approved_fp16_profile_keeps_complete_cpu_smoke_and_future_fields(self):
        report = {**CURRENT_SMOKE, 'future_observation': {'preserved': [1, 2, 3]}}
        self.assertIs(POLICY.verify_model_smoke(report), report)
        self.assertEqual(POLICY.approved_engine(report['profile']), 'onnxruntime-cpu/1.30.0')

    def test_old_foreign_missing_or_malformed_smoke_never_substitutes_policy(self):
        original = CURRENT_SMOKE
        mutations = [('engine', 'onnxruntime-cpu/1.22.1'),  # historical engine is not current approval
                     ('engine', 'onnxruntime-cpu/1.29.0'),
                     ('engine', 'onnxruntime-cuda/1.30.0'),
                     ('engine', 'onnxruntime/1.30.0/airk-minilm-fp16-shared'),
                     ('engine', 'onnxruntime/1.30.0/airk-minilm-fp16-shared/1'),
                     ('profile', 'minilm-l6-v2-u8-v1'), ('profile', 'foreign'),
                     ('schema', True), ('dimensions', True), ('dimensions', 0),
                     ('source', 'ambient'), ('repeat_equal', False),
                     ('legacy_identity', None), ('pipeline_digest', 'bad')]
        for field, value in mutations:
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                POLICY.verify_model_smoke({**original, field: value})
        for field in original:
            missing = {k: v for k, v in original.items() if k != field}
            with self.subTest(missing=field), self.assertRaises(ValueError):
                POLICY.verify_model_smoke(missing)

    def test_actual_client_consumer_uses_policy_without_changing_probe_or_timeout(self):
        binary = Mock()
        binary.with_name.return_value.exists.return_value = False
        fixture = SimpleNamespace(single_image=True, binary=binary,
                                  service_run=Mock(return_value=subprocess.CompletedProcess(
                                      [], 0, json.dumps(CURRENT_SMOKE), '')),
                                  assert_alive=Mock())
        self.assertEqual(CLIENTS.inference_worker_clean_exit(fixture), CURRENT_SMOKE)
        fixture.service_run.assert_called_once_with('--verify-model-smoke', check=False, timeout=120, selector=False)
        fixture.assert_alive.assert_called_once_with()
        fixture.assert_alive.reset_mock()
        fixture.service_run.return_value.stdout = json.dumps({**CURRENT_SMOKE, 'engine': 'unapproved-engine'})
        with self.assertRaises(AssertionError):
            CLIENTS.inference_worker_clean_exit(fixture)
        fixture.assert_alive.assert_not_called()

    def test_oracle_identity_matches_actual_r2_native_smoke_literal_and_key_order(self):
        # Retain this original test identity and actual historical R2 observation.
        # Current admission and oracle/key-order comparison are separate below.
        self.assertEqual(SMOKE_OBSERVATIONS['schema'], 'airk.retained-native-model-smoke-observations/1')
        retained = SMOKE_OBSERVATIONS['historical_r2']
        self.assertEqual(retained['build_receipt_sha256'], 'ed92c568dfc0b71a3093048005a077ab5b8b5349cb1406c7782c7a50fc903ba9')
        self.assertEqual(retained['observation'], {
            'dimensions': 384, 'engine': 'onnxruntime-cpu/1.30.0',
            'legacy_identity': '3e97f788b02fb5a578dc106628ecc7e3791b576dd60ed334401018893f9d0784',
            'pipeline_digest': '80aea601ebf0f8ec94d1df52bfb9a44df4bffbccc85dc4a22ec7522785efa065',
            'profile': 'minilm-l6-v2-u8-v2', 'repeat_equal': True, 'schema': 1, 'source': 'embedded'})
        with self.assertRaises(ValueError):
            POLICY.approved_profile(retained['observation']['profile'])

    def test_actual_r104_int8_smoke_remains_historical(self):
        # Exact Rust output from the successful R104 matched-pair smoke, not
        # Python-computed expectations or a historical profile relabeled current.
        retained = SMOKE_OBSERVATIONS['current_r104']
        self.assertEqual(retained['build_receipt_sha256'], 'c4f989d9e703fa44eeaff2b57588e93d4df4d8f4786fefd4e1ac764b741e4078')
        self.assertEqual(retained['observation'], {
            'dimensions': 384, 'engine': 'onnxruntime-cpu/1.30.0',
            'legacy_identity': 'fdd2d44dffa5715eecdb78546870009abf78f1abac0ee5cca9a6f6b2fea8b64c',
            'pipeline_digest': 'cb2166f2f648aa44cb4ee87f6b58b1e5c8007720d24fa4426234b65058e2b638',
            'profile': 'minilm-l6-v2-u8-v3', 'repeat_equal': True, 'schema': 1, 'source': 'embedded'})
        with self.assertRaises(ValueError):
            POLICY.verify_model_smoke(retained['observation'])

    def test_current_oracle_identity_matches_the_sealed_fp16_smoke_and_key_order(self):
        catalog = POLICY.catalog()
        profile = next(p for p in catalog['profiles'] if p['id'] == catalog['default_profile'])
        manifest = profile['manifest']
        self.assertEqual(profile['id'], CURRENT_SMOKE['profile'])
        expected = CURRENT_SMOKE['legacy_identity']
        self.assertEqual(ORACLE.expected_native_identity(profile['id'], manifest), expected)
        reordered = dict(reversed(list(manifest.items())))
        self.assertEqual(ORACLE.expected_native_identity(profile['id'], reordered), expected)
        wrong = copy.deepcopy(manifest)
        wrong['model_sha256'] = '00' * 32
        with self.assertRaises(ValueError):
            ORACLE.expected_native_identity(profile['id'], wrong)
        with self.assertRaises(ValueError):
            ORACLE.expected_native_identity('foreign', manifest)


if __name__ == '__main__':
    unittest.main()
