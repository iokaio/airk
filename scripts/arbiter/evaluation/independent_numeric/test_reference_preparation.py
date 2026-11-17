# SPDX-License-Identifier: Apache-2.0
"""Cheap decision/input controls; never imports Torch or executes any model."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest
from maintained_helpers import worker, parent


class Decisions(unittest.TestCase):
    def test_focus_w8_cannot_be_fp32_by_metadata(self):
        import builtins
        from unittest.mock import patch
        original_import = builtins.__import__
        def observed_import(name, *args, **kwargs):
            if name == 'torch' or name.startswith('torch.'):
                self.fail('missing-adapter decision attempted a Torch import')
            return original_import(name, *args, **kwargs)
        with patch('builtins.__import__', side_effect=observed_import):
            with self.assertRaisesRegex(ValueError, 'MatMulNBits8'):
                worker.require_supported({'kind': 'same_artifact', 'upstream': 'Qwen/Qwen3-Reranker-0.6B'})

    def test_context_and_original_kinds_are_explicit(self):
        for upstream in ['Qwen/Qwen3-1.7B', 'Qwen/Qwen3-4B']:
            for kind in ['same_artifact', 'original_fp32']:
                worker.require_supported({'kind': kind, 'upstream': upstream})
        with self.assertRaises(ValueError):
            worker.require_supported({'kind': 'old_verdict', 'upstream': 'Qwen/Qwen3-4B'})

    def test_candidate_binding_names_do_not_global_hash_match(self):
        self.assertEqual(worker.projection_binding('text.layers.23.self_attn.q_proj.weight'), '/scorer/layers.23/self_attn/q_proj/MatMul')
        self.assertEqual(worker.projection_binding('text.layers.0.mlp.down_proj.weight'), '/scorer/layers.0/mlp/down_proj/MatMul')
        self.assertIsNone(worker.projection_binding('text.norm.weight'))
        with self.assertRaises(ValueError):
            worker.projection_binding('text.layers.0.mlp.q_proj.weight')

    def test_partial_or_refreshed_limit_cards_refuse(self):
        limits = {'wall_seconds': 600, 'host_committed_bytes': 48 << 30, 'cuda_allocator_bytes': 30 << 30,
                  'minimum_cuda_free_bytes': 32 << 30, 'threads': 2}
        parent.validate_limits(limits)
        for key, changed in [('wall_seconds', 1201), ('threads', 8), ('minimum_cuda_free_bytes', 30 << 30), ('host_committed_bytes', True)]:
            value = dict(limits, **{key: changed})
            with self.assertRaises(ValueError):
                parent.validate_limits(value)
        with self.assertRaises(ValueError):
            parent.validate_limits(dict(limits, inherited_pass=True))

    def test_full_native_input_unknown_fields_are_immutable(self):
        native = {'rows': [{'name': 'native-8192', 'input': {'input_ids': [7, 8], 'candidate_positions': [32, 33],
                   'unknown_future': {'source_sha256': 'a' * 64}}}]}
        result = {'complete': True, 'torch_forward_invocations': 1,
                  'rows': [{'name': 'native-8192', 'input': copy.deepcopy(native['rows'][0]['input']), 'scores': [3.0, 2.0]}]}
        parent.complete_arrays(result, native)
        before = copy.deepcopy(native)
        for mutated in [dict(result, complete=False), dict(result, torch_forward_invocations=0)]:
            with self.assertRaises(ValueError):
                parent.complete_arrays(mutated, native)
        altered = copy.deepcopy(result)
        altered['rows'][0]['input']['unknown_future']['source_sha256'] = 'b' * 64
        with self.assertRaisesRegex(ValueError, 'input changed'):
            parent.complete_arrays(altered, native)
        self.assertEqual(native, before)

    def test_nonfinite_or_omitted_scores_cannot_qualify(self):
        native = {'rows': [{'name': 'x', 'input': {'candidate_positions': [32, 33]}}]}
        for scores in [[1.0], [1.0, float('nan')], [1.0, float('inf')]]:
            with self.assertRaises(ValueError):
                parent.complete_arrays({'complete': True, 'torch_forward_invocations': 1,
                    'rows': [{'name': 'x', 'input': native['rows'][0]['input'], 'scores': scores}]}, native)

    def test_actual_exactly_one_process_settlement_required(self):
        real = {'exit_code': 0, 'timed_out': False, 'log_oversized': False, 'owned_descendants_settled': True,
                'owned_job_forced_termination': False, 'owned_job_final_active_processes': 0,
                'owned_job_final_member_pids': [], 'parent_assigned_to_job': False,
                'child_assigned_before_resume': True, 'owned_total_processes': 1,
                'retained_member_exits': [{'exit_observed': True, 'exit_code': 0}],
                'unexpected_third_process': False, 'whole_process_deadline_not_refreshed': True,
                'handle_close_proof': {'settled': True, 'forced': False, 'final_active': 0, 'members_before': [], 'final_member_pids': []}}
        self.assertTrue(parent.settled(real))
        for key, value in [('owned_total_processes', 2), ('owned_job_forced_termination', True),
                           ('owned_job_final_member_pids', [999]), ('exit_code', 1)]:
            self.assertFalse(parent.settled(dict(real, **{key: value})))


class ParentBoundaries(unittest.TestCase):
    @staticmethod
    def owned():
        return {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
            'owned_descendants_settled': True, 'owned_job_forced_termination': False,
            'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
            'parent_assigned_to_job': False, 'child_assigned_before_resume': True,
            'owned_total_processes': 1, 'unexpected_third_process': False,
            'whole_process_deadline_not_refreshed': True,
            'retained_member_exits': [{'exit_observed': True, 'exit_code': 0}],
            'handle_close_proof': {'forced': False, 'settled': True, 'final_active': 0, 'members_before': [], 'final_member_pids': []}}

    def test_every_native_settlement_field_is_mandatory(self):
        original = self.owned()
        self.assertTrue(parent.settled(original))
        for key in original:
            value = copy.deepcopy(original)
            del value[key]
            self.assertFalse(parent.settled(value), key)
        for key in original['handle_close_proof']:
            value = copy.deepcopy(original)
            del value['handle_close_proof'][key]
            self.assertFalse(parent.settled(value), key)
        for key in original['retained_member_exits'][0]:
            value = copy.deepcopy(original)
            del value['retained_member_exits'][0][key]
            self.assertFalse(parent.settled(value), key)

    def test_original_deadline_fences_sid_and_image_authentication(self):
        from types import SimpleNamespace
        info = SimpleNamespace(process='actual-handle', pid=42)
        calls = []
        def sid(handle):
            calls.append('sid')
            return 'human'
        def image(api, handle, pin):
            calls.append('image')
            return pin
        result = parent.suspended_authentication(None, info, 200, 190, 'human', {'frozen': True}, image, sid, iter([100, 101, 102]).__next__)
        self.assertEqual((result['pid'], calls), (42, ['sid', 'image']))
        for clocks, expected_calls in [([200], []), ([100, 200], ['sid']), ([100, 101, 200], ['sid', 'image'])]:
            calls.clear()
            with self.assertRaisesRegex(ValueError, 'expired'):
                parent.suspended_authentication(None, info, 200, 190, 'human', {}, image, sid, iter(clocks).__next__)
            self.assertEqual(calls, expected_calls)
        calls.clear()
        with self.assertRaisesRegex(ValueError, 'extended'):
            parent.suspended_authentication(None, info, 200, 201, 'human', {}, image, sid, lambda: 100)
        self.assertEqual(calls, [])

    def test_expiry_is_truthful_on_failed_and_late_success_paths(self):
        failed = {'complete': False, 'failure': {'type': 'InputDrift'}}
        self.assertFalse(parent.final_phase_observation(failed, 200, 100, lambda: 201))
        self.assertTrue(failed['whole_phase_expired'])
        self.assertEqual(failed['failure'], {'type': 'InputDrift'})
        late = {'complete': True, 'failure': None}
        self.assertTrue(parent.final_phase_observation(late, 200, 100, lambda: 200))
        self.assertFalse(late['complete'])
        self.assertEqual(late['failure']['type'], 'TimeoutError')

    def test_ownership_copy_changes_only_declared_absolute_deadline_seam(self):
        proof = json.loads((Path(__file__).resolve().parents[4] / 'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/ownership-copy-provenance.json').read_bytes())
        actual = Path(proof['original_path']).read_text(encoding='utf-8')
        for before, after in proof['exact_transforms']:
            self.assertEqual(actual.count(before), 1)
            actual = actual.replace(before, after, 1)
        self.assertEqual(actual, Path(proof['private_path']).read_text(encoding='utf-8'))


class GraphProof(unittest.TestCase):
    @staticmethod
    def model():
        import onnx
        return onnx.helper.make_model(onnx.helper.make_graph(
            [onnx.helper.make_node('MatMul', ['x', 'w'], ['y'], name='project')], 'synthetic',
            [onnx.helper.make_tensor_value_info('x', onnx.TensorProto.FLOAT, [1, 2])],
            [onnx.helper.make_tensor_value_info('y', onnx.TensorProto.FLOAT, [1, 2])],
            [onnx.helper.make_tensor('w', onnx.TensorProto.FLOAT, [2, 2], [1, 2, 3, 4])]))

    def test_all_graph_nodes_and_numeric_values_are_bound(self):
        original = self.model()
        baseline = worker.graph_semantics(original, Path(__file__).parent)
        altered = copy.deepcopy(original)
        altered.graph.node[0].op_type = 'Add'
        self.assertNotEqual(worker.graph_semantics(altered, Path(__file__).parent), baseline)
        altered = copy.deepcopy(original)
        altered.graph.initializer[0].float_data[1] = 7
        self.assertNotEqual(worker.graph_semantics(altered, Path(__file__).parent), baseline)

    def test_external_storage_can_move_without_changing_math(self):
        import onnx
        import numpy as np
        import tempfile
        root = Path(__file__).resolve().parents[4] / 'artifacts/independent-numeric-test-state'
        root.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=root, prefix='tensor-') as temporary:
            directory = Path(temporary).resolve()
            self.assertTrue(directory.is_relative_to(root))
            original = self.model()
            expected = worker.graph_semantics(original, directory)
            (directory / 'actual.bin').write_bytes(np.array([1, 2, 3, 4], dtype=np.float32).tobytes())
            altered = copy.deepcopy(original)
            tensor = altered.graph.initializer[0]
            tensor.ClearField('float_data')
            tensor.data_location = onnx.TensorProto.EXTERNAL
            for key, value in [('location', 'actual.bin'), ('offset', '0'), ('length', '16')]:
                entry = tensor.external_data.add()
                entry.key, entry.value = key, value
            self.assertEqual(worker.graph_semantics(altered, directory), expected)

    def test_actual_pin_refuses_changed_same_length_bytes(self):
        import tempfile
        root = Path(__file__).resolve().parents[4] / 'artifacts/independent-numeric-test-state'
        root.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=root, prefix='pin-') as temporary:
            directory = Path(temporary).resolve()
            self.assertTrue(directory.is_relative_to(root))
            source = directory / 'input.json'
            source.write_bytes(b'{"native":1}')
            original = {'path': str(source), 'bytes': source.stat().st_size, 'sha256': worker.digest(source)}
            self.assertEqual(worker.pinned(original), source)
            source.write_bytes(b'{"native":2}')
            with self.assertRaisesRegex(ValueError, 'hash changed'):
                worker.pinned(original)


if __name__ == '__main__':
    unittest.main()
