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


def smoke(profile):
    return {'schema': 1, 'source': 'embedded', 'profile': profile['id'],
            'engine': profile['runtime'].rsplit('/', 1)[0],
            'dimensions': profile['manifest']['dimensions'], 'repeat_equal': True,
            'pipeline_digest': 'ab' * 32, 'legacy_identity': 'cd' * 32}


class CurrentModelConsumers(unittest.TestCase):
    def test_both_native_profiles_keep_complete_smoke_and_future_fields(self):
        for profile in NATIVE['profiles']:
            with self.subTest(profile=profile['id']):
                report = {**smoke(profile), 'future_observation': {'preserved': [1, 2, 3]}}
                self.assertIs(POLICY.verify_model_smoke(report), report)
                self.assertEqual(POLICY.approved_engine(profile['id']), profile['runtime'].rsplit('/', 1)[0])

    def test_old_foreign_missing_or_malformed_smoke_never_substitutes_policy(self):
        original = smoke(NATIVE['profiles'][0])
        mutations = [('engine', 'onnxruntime-cpu/1.22.1'),  # historical engine is not current approval
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
        with tempfile.TemporaryDirectory() as directory:
            report = smoke(NATIVE['profiles'][0])
            fixture = SimpleNamespace(single_image=True, binary=Path(directory) / 'airk.exe',
                                      service_run=Mock(return_value=subprocess.CompletedProcess([], 0, json.dumps(report), '')),
                                      assert_alive=Mock())
            self.assertEqual(CLIENTS.inference_worker_clean_exit(fixture), report)
            fixture.service_run.assert_called_once_with('--verify-model-smoke', check=False, timeout=120, selector=False)
            fixture.assert_alive.assert_called_once_with()
            fixture.assert_alive.reset_mock()
            fixture.service_run.return_value.stdout = json.dumps({**report, 'engine': 'unapproved-engine'})
            with self.assertRaises(AssertionError):
                CLIENTS.inference_worker_clean_exit(fixture)
            fixture.assert_alive.assert_not_called()

    def test_oracle_identity_matches_actual_r2_native_smoke_literal_and_key_order(self):
        # Native R2 airk-build.json emitted this identity for the approved default
        # u8-v2 profile. This is a retained native observation, not Python output.
        catalog = POLICY.catalog()
        profile = next(p for p in catalog['profiles'] if p['id'] == catalog['default_profile'])
        manifest = profile['manifest']
        expected = '3e97f788b02fb5a578dc106628ecc7e3791b576dd60ed334401018893f9d0784'
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
