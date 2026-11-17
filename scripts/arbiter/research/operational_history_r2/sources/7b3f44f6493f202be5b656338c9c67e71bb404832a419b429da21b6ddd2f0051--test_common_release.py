# SPDX-License-Identifier: Apache-2.0
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest

BASE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BASE))
import fixture_bound_support_r1 as bounds
import prepare_common_cal_release_r1 as common

def save(path, value):
    path.write_text(json.dumps(value) + '\n', encoding='utf-8')
    return bounds.pin(path)

class OriginalLifetimeTests(unittest.TestCase):
    def evidence(self, root):
        packet = {'schema': 'airk.private-selected-cal-fixture-launch/1',
                  'fixture_seconds': 10800, 'owner_seconds': 11400, 'work': str(root / 'metadata')}
        packet_ref = save(root / 'packet.json', packet)
        decision_ref = save(root / 'decision.json', {'approved_fixture_packet_sha256': packet_ref['sha256']})
        output = root / 'owned'
        output.mkdir()
        reference = bounds.write_started(output, packet_ref, decision_ref, packet, 1000.125,
                                         BASE / 'run_fixture_started_r1.py')
        return packet, packet_ref, decision_ref, output, reference

    def test_actual_start_is_exclusive_and_ceiling_is_not_refreshed(self):
        with tempfile.TemporaryDirectory() as directory:
            packet, packet_ref, decision, output, started = self.evidence(Path(directory))
            value = bounds.original_bound(started, packet_ref)
            self.assertEqual(value['original_start_unix_ms'], 1_000_125)
            self.assertEqual(value['absolute_deadline_unix_ms'], 11_800_125)
            self.assertEqual(value['original_start_field'], 'started_unix')
            with self.assertRaises(FileExistsError):
                bounds.write_started(output, packet_ref, decision, packet, 9999., BASE / 'run_fixture_started_r1.py')
            self.assertEqual(bounds.original_bound(started, packet_ref), value)

    def test_extended_deadline_and_replaced_clock_field_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, packet, _, _, started = self.evidence(root)
            good = bounds.original_bound(started, packet)
            for key, value in [('absolute_deadline_unix_ms', good['absolute_deadline_unix_ms'] + 1),
                               ('original_start_field', 'finished_unix')]:
                changed = copy.deepcopy(good)
                changed[key] = value
                ref = save(root / ('bad-' + key + '.json'), changed)
                with self.assertRaises(ValueError):
                    bounds.validate_bound(ref, packet)

    def test_changed_started_bytes_or_foreign_packet_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packet, packet_ref, _, _, started = self.evidence(root)
            foreign = dict(packet, work=str(root / 'foreign'))
            with self.assertRaises(ValueError):
                bounds.original_bound(started, save(root / 'foreign.json', foreign))
            actual = json.loads(Path(started['path']).read_bytes())
            actual['started_unix'] = 2000.
            save(Path(started['path']), actual)
            with self.assertRaises(ValueError):
                bounds.original_bound(started, packet_ref)

    def test_nonfinite_start_and_unapproved_packet_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, packet_ref, _, _, started = self.evidence(root)
            actual = json.loads(Path(started['path']).read_bytes())
            actual['started_unix'] = float('nan')
            with self.assertRaises(ValueError):
                bounds.original_bound(save(root / 'nan.json', actual), packet_ref)
            actual['started_unix'] = 1000.
            actual['decision'] = save(root / 'wrong-decision.json', {'approved_fixture_packet_sha256': '0' * 64})
            with self.assertRaises(ValueError):
                bounds.original_bound(save(root / 'wrong-approval.json', actual), packet_ref)

class CommonReleaseTests(unittest.TestCase):
    def test_retained_native_metadata_preserves_all_science_and_has_one_original_bound(self):
        # Historical real native metadata is a metadata fixture only: no process/model is invoked.
        release = json.loads((BASE / 'CAL-capture-release.root-approved-purpose-corrected-r143.json').read_bytes())
        release['approved'] = None
        packet_ref = release['actual_binding_evidence']['fixture_packet']
        packet = bounds.verified(packet_ref)
        decision = bounds.pin(BASE / 'root-fixture-decision-next-corrected-r142.json')
        protocol = bounds.pin(BASE.parents[1] / 'models/selected-native-goal-driver-r1/chunked-cal-r1/proof/protocol.proposed.json')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            started = bounds.write_started(root, packet_ref, decision, packet, time.time(), BASE / 'run_fixture_started_r1.py')
            bound = save(root / 'bound.json', bounds.original_bound(started, packet_ref))
            result = common.attach(release, protocol, bound)
            self.assertIsNone(result['approved'])
            self.assertEqual(result['Focus_configuration'], 'ON')
            self.assertEqual(result['original_fixture_bound'], bound)
            self.assertEqual(result['chunk_protocol_sha256'], 'da5eb4de2642deb952cdbc608b302d99137965ed43969bfafd5dcc07c187ac99')
            for name in ('Focus_profile', 'Context_bootstrap_profile', 'MiniLM_pipeline', 'task_policy',
                         'frozen_configuration', 'source_manifest_sha256', 'corpus_sha256',
                         'maximum_questions', 'maximum_probes', 'Context_task_forward', 'paths'):
                self.assertEqual(result[name], release[name])
            for old_path, digest in release['input_pins'].items():
                self.assertEqual(result['input_pins'][old_path], digest)
            wrong = dict(protocol, sha256='0' * 64)
            with self.assertRaises(ValueError):
                common.attach(release, wrong, bound)
            changed = copy.deepcopy(release)
            changed['frozen_configuration']['focus']['enabled'] = False
            with self.assertRaises(ValueError):
                common.attach(changed, protocol, bound)

if __name__ == '__main__':
    unittest.main()
