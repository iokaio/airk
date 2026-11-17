# SPDX-License-Identifier: Apache-2.0
"""Synthetic assembler refusal controls; never actual custody/model evidence."""
import copy
import contextlib
import io
import json
from pathlib import Path
from types import SimpleNamespace
import uuid
import unittest
import assemble_references as assembler


def execution(kind):
    return {'schema': 'airk.arbiter.independent-numeric-reference-execution/1', 'complete': True,
        'failure': None, 'post_input_drift': [], 'whole_phase_deadline_not_refreshed': True,
        'whole_phase_expired': False, 'no_automatic_retry': True, 'other_processes_touched': False,
        'native_airk_nn_invocations': 0, 'model': 'opaque-native-model', 'compiler': 'opaque-native-compiler',
        'resource_policy_digest': 'opaque-native-policy', 'reference_kind': kind, 'torch_forward_invocations': 1,
        'rows': [{'name': 'synthetic', 'input': {'candidate_positions': [32, 33], 'unknown_native': {'hash': 'a' * 64}}, 'scores': [1.25, 2.5]}],
        'owned': {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
            'owned_descendants_settled': True, 'owned_job_forced_termination': False,
            'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [], 'parent_assigned_to_job': False,
            'child_assigned_before_resume': True, 'owned_total_processes': 1, 'unexpected_third_process': False,
            'whole_process_deadline_not_refreshed': True, 'retained_member_exits': [{'exit_observed': True, 'exit_code': 0}],
            'handle_close_proof': {'forced': False, 'settled': True, 'final_active': 0, 'members_before': [], 'final_member_pids': []}}}


class Assembly(unittest.TestCase):
    def setUp(self):
        self.same, self.original = execution('same_artifact'), execution('original_fp32')
        self.template = {'model': 'opaque-native-model', 'compiler': 'opaque-native-compiler', 'resource_policy': 'opaque-native-policy',
            'rows': [{'name': 'synthetic', 'input': copy.deepcopy(self.same['rows'][0]['input']),
                      'reference_same_artifact': None, 'reference_original_fp32': None, 'unknown_future_case': {'retained': True}}]}

    def test_copy_fill_preserves_native_input_and_unknown_fields(self):
        before = copy.deepcopy(self.template)
        rows = assembler.joined_rows(self.template, self.same, self.original)
        self.assertEqual(self.template, before)
        self.assertEqual(rows[0]['input'], before['rows'][0]['input'])
        self.assertEqual(rows[0]['unknown_future_case'], {'retained': True})
        self.assertEqual(rows[0]['reference_original_fp32'], [1.25, 2.5])

    def test_full_assembly_exports_the_native_core_fixture_pin_shape(self):
        # Synthetic real-file assembly exercises the exported boundary, not an
        # isolated shape helper. It is never selected-model/custody evidence.
        scratch = Path(__file__).resolve().parents[4] / 'artifacts/independent-numeric-test-state' / ('assembly-' + uuid.uuid4().hex)
        scratch.mkdir()
        def write(name, value):
            path = scratch / name
            path.write_text(json.dumps(value), encoding='utf-8')
            return path
        def core_pin(path):
            return {'path': str(path), 'sha256': assembler.worker.digest(path)}
        def asset_pin(path):
            return dict(core_pin(path), bytes=path.stat().st_size)
        asset = write('synthetic-source.json', {'synthetic_source_only': True})
        source_file = asset_pin(asset)
        candidate_assets = [{'role': 'model', 'name': 'opaque-asset', 'unknown_asset_metadata': 'preserved'}]
        original_assets = [{'role': 'model', 'name': 'opaque-original', 'unknown_original_metadata': 'preserved'}]
        profile_path = write('profile.json', {'model': self.template['model'], 'compiler': self.template['compiler'], 'assets': candidate_assets})
        source_manifest = write('source-manifest.json', {'synthetic_source_only': True})
        request = {'profile': core_pin(profile_path), 'source_manifest': core_pin(source_manifest), 'policy_digest': self.template['resource_policy']}
        request_before = copy.deepcopy(request)
        input_path = write('native-input.json', request)
        template_path = write('native-template.json', self.template)
        source_input = write('original-source-input.json', {'synthetic_source_only': True})
        roster = {'model': self.template['model'], 'compiler': self.template['compiler'],
            'resource_policy_digest': self.template['resource_policy'], 'native_input_sha256': assembler.worker.digest(input_path),
            'original_reference_assets': original_assets, 'original_reference_files': {'opaque-original': source_file},
            'original_source_files': {'opaque-source': source_file}, 'original_source_input': core_pin(source_input)}
        roster_path = write('original-roster.json', roster)
        executions = []
        for kind in ['same_artifact', 'original_fp32']:
            run = execution(kind)
            run['execution_user_sid'] = 'synthetic-sid-never-authority'
            run['source_assets'] = candidate_assets if kind == 'same_artifact' else original_assets
            run['input_seals'] = {str(path): assembler.worker.digest(path) for path in [input_path, template_path, roster_path, asset]}
            raw = write(kind + '-raw.json', {'complete': True, 'rows': run['rows'], 'torch_forward_invocations': 1})
            log = write(kind + '-log.json', {'synthetic_log_only': True})
            run['raw_output'], run['stdout'] = asset_pin(raw), asset_pin(log)
            run['owned']['suspended_assignment'] = {'image': source_file}
            executions.append(write(kind + '-execution.json', run))
        output = scratch / 'assembled'
        args = SimpleNamespace(output_root=Path(__file__).resolve().parents[4] / 'artifacts', native_input=input_path, native_input_sha256=assembler.worker.digest(input_path),
            template=template_path, template_sha256=assembler.worker.digest(template_path),
            original_roster=roster_path, original_roster_sha256=assembler.worker.digest(roster_path),
            same_artifact=executions[0], original_fp32=executions[1], output=output)
        with contextlib.redirect_stdout(io.StringIO()):
            assembler.assemble(args)
        result = json.loads((output / 'assembly.json').read_bytes())
        fixture_path = output / 'numeric-fixture.json'
        expected = {'path': str(fixture_path), 'sha256': assembler.worker.digest(fixture_path)}
        self.assertEqual(result['fixture'], expected)
        self.assertEqual(set(result['fixture']), set(request['profile']))
        self.assertNotIn('bytes', result['fixture'])
        self.assertIn('bytes', result['producer'])
        forwarded_request = copy.deepcopy(request)
        forwarded_request['numeric_fixture'] = result['fixture']
        self.assertEqual(forwarded_request['numeric_fixture'], expected)
        self.assertEqual(request, request_before)
        fixture = json.loads(fixture_path.read_bytes())
        self.assertEqual(set(fixture['reference_producer']), set(request['profile']))
        self.assertEqual(fixture['rows'][0]['input'], self.template['rows'][0]['input'])

    def test_failed_partial_renewed_or_foreign_process_never_assembles(self):
        for key, changed in [('complete', False), ('whole_phase_expired', True), ('native_airk_nn_invocations', 1),
                             ('post_input_drift', ['changed']), ('failure', {'actual': 'failed'}), ('no_automatic_retry', False)]:
            with self.assertRaises(ValueError):
                assembler.joined_rows(self.template, dict(self.same, **{key: changed}), self.original)

    def test_empty_job_and_handle_closure_are_both_required(self):
        for path, value in [('owned_total_processes', 2), ('owned_job_final_member_pids', [1]), ('unexpected_third_process', True)]:
            altered = copy.deepcopy(self.same)
            altered['owned'][path] = value
            with self.assertRaises(ValueError):
                assembler.original_settlement(altered)
        altered = copy.deepcopy(self.same)
        altered['owned']['handle_close_proof']['final_active'] = 1
        with self.assertRaises(ValueError):
            assembler.original_settlement(altered)

    def test_input_identity_full_array_and_two_independent_kinds_required(self):
        for kind in ['wrong_kind', 'original_fp32']:
            with self.assertRaises(ValueError):
                assembler.joined_rows(self.template, dict(self.same, reference_kind=kind), self.original)
        changed = copy.deepcopy(self.same)
        changed['rows'][0]['input']['unknown_native']['hash'] = 'b' * 64
        with self.assertRaises(ValueError):
            assembler.joined_rows(self.template, changed, self.original)
        for scores in [[1.0], [1.0, float('nan')]]:
            changed = copy.deepcopy(self.same)
            changed['rows'][0]['scores'] = scores
            with self.assertRaises(ValueError):
                assembler.joined_rows(self.template, changed, self.original)

    def test_opaque_stable_binding_and_prepopulated_columns_refuse(self):
        for key in ['model', 'compiler', 'resource_policy_digest']:
            with self.assertRaises(ValueError):
                assembler.joined_rows(self.template, dict(self.same, **{key: 'changed'}), self.original)
        self.template['rows'][0]['reference_same_artifact'] = [1.25, 2.5]
        with self.assertRaises(ValueError):
            assembler.joined_rows(self.template, self.same, self.original)


if __name__ == '__main__':
    unittest.main()
