# SPDX-License-Identifier: Apache-2.0
"""Synthetic wire contracts for the editor fixture verifier, not native qualification."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]

def framed(domain, fields):
    value = hashlib.sha256(b'AIRK-DIGEST-1\0')
    for field in [domain.encode(), *fields]:
        value.update(len(field).to_bytes(8, 'big'))
        value.update(field)
    return value.hexdigest()


def fixture():
    candidate, predecessor = 'a' * 64, 'b' * 64
    namespace, operation = '1' * 32, '2' * 32
    plan = {'id': operation, 'namespace': namespace, 'control': str(ROOT / 'control'), 'installer_identity': candidate,
            'indexes': [{'registration': {'binding': '9' * 64, 'root': str(ROOT / 'source'), 'data_root': str(ROOT / 'data')}}]}
    catalog = {'schema': 12, 'authority_contract': 1, 'policy_contract': 0, 'activated': False, 'accepted_new_writes': False,
               'namespace': namespace, 'upgrade_id': operation, 'revision': 2}
    observation = {'schema': 1, 'nonce': '3' * 32, 'installer_identity': candidate, 'catalog': catalog,
                   'basis': {'kind': 'legacy_upgrade', 'upgrade_id': operation, 'plan_digest': 'c' * 64,
                             'seal_digest': 'd' * 64, 'native_host': 'e' * 64}}
    diagnostic = {'final_classified_diagnostic_digest': 'f' * 64}
    provenance = {'native_entrypoints': {'read': 'service_requests_list', 'write': 'service_requests_forget'},
                  'read': diagnostic, 'write': diagnostic}
    raw = json.dumps(provenance, separators=(',', ':')).encode()
    refusal = {'payload_identity': predecessor, 'native_provenance': provenance,
               **{direction + '_refusal_digest': framed('airk/catalog-native-refusal-provenance/1',
                                                       [bytes.fromhex(predecessor), raw, direction.encode()])
                  for direction in ('read', 'write')}}
    authority = {'phase': 'settled', 'readiness': {'candidate': observation, 'required_predecessors': [predecessor], 'predecessor_refusals': [refusal]}}
    base = {'installer_identity': candidate, 'catalog': {**catalog, 'authority_contract': 0, 'revision': 1},
            'pending_source_imports': 0, 'pending_context_imports': 0, 'indexes': 1}
    receipt = {'schema': 1, 'stage': 'settled', 'plan': plan, 'plan_digest': 'c' * 64, 'authority_v1_host': 'e' * 64,
               'authority_v1': authority, 'readiness': {'candidate': base, 'predecessor_refusals': [refusal]}}
    activation = {'schema': 1, 'outcome': 'selected', 'install': str(ROOT / 'install'), 'candidate': '4' * 64,
                  'preceding': '5' * 64, 'observed_unix_ms': 1}
    health = {'status': 'health', 'observation': {'runtime_sha256': '4' * 64, 'executable_sha256': '6' * 64,
              'instance': '7' * 32, 'policy': {'accepting': True}, 'catalog_clock': {'namespace': namespace, 'revision': 2}}}
    inspection = {'registry_namespace': namespace}
    return {'install_dir': str(ROOT / 'install'), 'control_dir': str(ROOT / 'control'),
            'source_root': str(ROOT / 'source'), 'data_root': str(ROOT / 'data'),
            'legacy_config': {'args': ['--service-project', '9' * 64]},
            'candidate': {'install_manifest_sha256': '4' * 64, 'engine_sha256': '6' * 64},
            'predecessor': {'install_manifest_sha256': '5' * 64},
            'native': {'upgrade_receipt': receipt, 'authority_v1_readiness': authority, 'activation_receipt': activation,
                       'post_health': health, 'inspect': inspection}}, {'candidateIdentity': candidate, 'predecessorIdentity': predecessor}


class NativeEditorLegacyContracts(unittest.TestCase):
    def validate(self, value, expected, retained=True):
        value = copy.deepcopy(value)
        native = value['native']
        native['parsed_evidence'] = [{'value': native[key]} for key in ('upgrade_receipt', 'activation_receipt', 'post_health', 'inspect')] if retained else []
        script = "const fs=require('node:fs'),v=JSON.parse(fs.readFileSync(0,'utf8')); console.log(JSON.stringify(require('./scripts/native-editor-legacy.cjs').qualifyLegacyNative(v.value,v.expected)));"
        return subprocess.run(['node', '-e', script], cwd=ROOT, input=json.dumps({'value': value, 'expected': expected}),
                              text=True, encoding='utf-8', capture_output=True, timeout=10)

    def test_complete_bound_contract_and_reordered_json(self):
        value, expected = fixture()
        self.assertEqual(self.validate(value, expected).returncode, 0)
        reordered = json.loads(json.dumps(value, sort_keys=True))
        self.assertEqual(self.validate(reordered, expected).returncode, 0)

    def test_pair_identity_matches_native_framing_and_refuses_extra_fields(self):
        pair = {'schema': 1, 'version': '1.8.8', 'os': 'windows', 'arch': 'x86_64', 'airk_sha256': 'a' * 64, 'embed_sha256': 'b' * 64}
        script = "console.log(require('./scripts/native-editor-legacy.cjs').pairIdentity(JSON.parse(require('node:fs').readFileSync(0,'utf8'))))"
        result = subprocess.run(['node', '-e', script], cwd=ROOT, input=json.dumps(pair), text=True, capture_output=True, timeout=10)
        self.assertEqual(result.stdout.strip(), framed('airk-maintenance-engine/1.4', [json.dumps(pair, separators=(',', ':')).encode()]))
        pair['unvalidated'] = True
        self.assertNotEqual(subprocess.run(['node', '-e', script], cwd=ROOT, input=json.dumps(pair), text=True, capture_output=True, timeout=10).returncode, 0)

    def test_manual_connection_must_match_the_upgraded_index(self):
        value, expected = fixture()
        value['legacy_config']['args'][1] = '0' * 64
        self.assertNotEqual(self.validate(value, expected).returncode, 0)

    def test_missing_complete_observations_refuses(self):
        value, expected = fixture()
        self.assertNotEqual(self.validate(value, expected, retained=False).returncode, 0)

    def test_crossed_native_artifact_and_transition_refuse(self):
        for field, replacement in [('candidateIdentity', '9' * 64), ('predecessorIdentity', '8' * 64)]:
            value, expected = fixture()
            expected[field] = replacement
            self.assertNotEqual(self.validate(value, expected).returncode, 0)
        value, expected = fixture()
        value['native']['activation_receipt']['preceding'] = '9' * 64
        self.assertNotEqual(self.validate(value, expected).returncode, 0)

    def test_incomplete_readiness_and_crossed_live_health_refuse(self):
        for key in ('pending_source_imports', 'pending_context_imports'):
            value, expected = fixture()
            value['native']['upgrade_receipt']['readiness']['candidate'][key] = 1
            self.assertNotEqual(self.validate(value, expected).returncode, 0)
        for field, replacement in [('namespace', '8' * 32), ('revision', 3)]:
            value, expected = fixture()
            value['native']['post_health']['observation']['catalog_clock'][field] = replacement
            self.assertNotEqual(self.validate(value, expected).returncode, 0)

    def test_mutated_refusal_and_empty_predecessor_coverage_refuse(self):
        value, expected = fixture()
        value['native']['upgrade_receipt']['authority_v1']['readiness']['required_predecessors'] = []
        self.assertNotEqual(self.validate(value, expected).returncode, 0)
        value, expected = fixture()
        value['native']['upgrade_receipt']['readiness']['predecessor_refusals'][0]['read_refusal_digest'] = '0' * 64
        self.assertNotEqual(self.validate(value, expected).returncode, 0)


if __name__ == '__main__':
    unittest.main()
