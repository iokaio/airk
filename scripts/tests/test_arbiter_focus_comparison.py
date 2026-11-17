# SPDX-License-Identifier: Apache-2.0
"""Pure comparison admission/custody/counter controls; no Torch or model calls."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import struct
import sys
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('focus_comparison', ROOT / 'scripts/arbiter-focus-comparison.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def tensors():
    names = [f'model.layers.{i}.self_attn.{part}.weight' for i in range(28) for part in ['q_proj', 'k_proj', 'v_proj', 'o_proj']]
    names += ['model.embed_tokens.weight'] + [f'frozen-{i}' for i in range(197)]
    return {name: {'shape': [1], 'dtype': 'BF16', 'data_offsets': [2 * i, 2 * i + 2]} for i, name in enumerate(names)}


def frozen_rows():
    rows = {}
    for role, size in [('DEV', 85), ('REPLAY', 45)]:
        for i in range(size):
            ident = f'{role}-{i}'
            rows[ident] = {'pair': {'id': ident, 'role': role, 'label': i % 2 if role == 'DEV' else 1,
                                   'rights_scope': 'PERMITTED_PRIVATE_RESEARCH_SCOPE'}, 'component': 'literal-component',
                           'publisher': {'kind': 'compiled', 'input': {'input_digest': 'a' * 64}},
                           'software_evidence_v1': {'kind': 'compiled', 'input': {'input_digest': 'b' * 64}}}
    return rows


def result_fixture():
    expected = M.population(frozen_rows(), 'software_evidence_v1')
    rows = [dict(x, no_logit=0., yes_logit=1., no_bits=0, yes_bits=1065353216) for x in expected]
    arm = {'rows': rows, 'loaded_state': {'tied_pointer': True, 'matched_parameters': 310},
           'teardown': {'model_collected': True, 'allocator_returned': True}}
    return {'complete': True, 'forward_attempts': 260, 'forward_calls': 260, 'backward_attempts': 0, 'gradient_steps': 0,
            'arms': {'original': copy.deepcopy(arm), 'adapted': copy.deepcopy(arm)},
            'parameter_proof': {'non_target_exact': True, 'non_target_count': 198}}, expected


def config():
    return {'architectures': ['Qwen3ForCausalLM'], 'attention_bias': False, 'attention_dropout': 0., 'bos_token_id': 151643,
            'eos_token_id': 151645, 'head_dim': 128, 'hidden_act': 'silu', 'hidden_size': 1024, 'initializer_range': .02,
            'intermediate_size': 3072, 'max_position_embeddings': 40960, 'max_window_layers': 28, 'model_type': 'qwen3',
            'num_attention_heads': 16, 'num_hidden_layers': 28, 'num_key_value_heads': 8, 'rms_norm_eps': 1e-6,
            'sliding_window': None, 'tie_word_embeddings': True, 'use_cache': True, 'use_sliding_window': False,
            'vocab_size': 151669, 'rope_theta': 1000000, 'rope_scaling': None, 'torch_dtype': 'bfloat16',
            'transformers_version': '4.51.3'}


class ComparisonControls(unittest.TestCase):
    def test_import_is_pure_and_limits_are_original(self):
        self.assertNotIn('torch', sys.modules)
        self.assertEqual(M.LIMITS, {'wall_seconds': 600, 'host_committed_bytes': 17179869184,
                                  'cuda_allocator_bytes': 8589934592, 'minimum_cuda_free_bytes': 8589934592})

    def test_canonical_BF16_bit_expansion_preserves_zero_subnormal_and_payload(self):
        raw = bytes.fromhex('00000080803f0100c17f')
        expected = b''.join(struct.pack('<I', x) for x in [0, 0x80000000, 0x3f800000, 0x00010000, 0x7fc10000])
        self.assertEqual(M.expand_bf16(raw), expected)
        with self.assertRaises(ValueError): M.expand_bf16(b'\x00')

    def test_streamed_parameter_hash_expands_without_float_tolerance(self):
        raw = bytes.fromhex('803f0000')
        entries = {'literal': {'data_offsets': [0, 4]}}
        calls = []
        with patch.object(M.Path, 'open', return_value=io.BytesIO(raw)), patch.object(M, 'CHUNK', 2):
            got = M.tensor_hashes(Path('unused'), entries, 0, True, lambda: calls.append('clock'))
        self.assertEqual(got['literal'], hashlib.sha256(bytes.fromhex('0000803f00000000')).hexdigest())
        self.assertGreaterEqual(len(calls), 4)

    def test_parameter_proof_allows_only112_literal_targets(self):
        original = tensors(); adapted = copy.deepcopy(original)
        left = {name: 'old' for name in original}; right = dict(left)
        right['model.layers.0.self_attn.q_proj.weight'] = 'changed'
        proof = M.parameter_proof(original, adapted, left, right)
        self.assertEqual(proof['non_target_count'], 198)
        self.assertEqual(proof['observed_changed_targets'], ['model.layers.0.self_attn.q_proj.weight'])
        right['model.embed_tokens.weight'] = 'changed'
        with self.assertRaisesRegex(ValueError, 'non-target'): M.parameter_proof(original, adapted, left, right)

    def test_missing_or_reshaped_checkpoint_cannot_pass(self):
        original = tensors(); hashes = {name: 'x' for name in original}
        adapted = copy.deepcopy(original); adapted['model.layers.0.self_attn.q_proj.weight']['shape'] = [2]
        with self.assertRaises(ValueError): M.parameter_proof(original, adapted, hashes, hashes)
        del adapted['model.embed_tokens.weight']
        with self.assertRaises(ValueError): M.parameter_proof(original, adapted, hashes, hashes)

    def test_full_header_layout_is_independent_and_bounded(self):
        values = tensors(); encoded = json.dumps(dict(values, __metadata__={'format': 'pt'})).encode()
        raw = struct.pack('<Q', len(encoded)) + encoded + bytes(620)
        with patch.object(M.Path, 'open', return_value=io.BytesIO(raw)), patch.object(M.Path, 'stat', return_value=types.SimpleNamespace(st_size=len(raw))):
            got, offset = M.header(Path('unused'), 'BF16', 2)
        self.assertEqual(len(got), 310); self.assertEqual(offset, len(encoded) + 8)
        with patch.object(M.Path, 'open', return_value=io.BytesIO(struct.pack('<Q', 1048577))):
            with self.assertRaises(ValueError): M.header(Path('unused'), 'BF16', 2)
        with self.assertRaises(ValueError): M.strict_pairs([('a', 1), ('a', 2)])

    def test_full85_DEV45_replay_fixed_native_inputs(self):
        expected = M.population(frozen_rows(), 'software_evidence_v1')
        self.assertEqual(len(expected), 130)
        self.assertEqual(sum(x['role'] == 'DEV' and x['recipe'] == 'software_evidence_v1' for x in expected), 85)
        self.assertEqual(sum(x['role'] == 'REPLAY' and x['recipe'] == 'publisher' for x in expected), 45)

    def test_missing_refused_unknown_rights_population_never_dropped(self):
        for mutate in ['missing', 'refused', 'rights']:
            rows = frozen_rows()
            if mutate == 'missing': del rows['DEV-0']
            elif mutate == 'refused': rows['REPLAY-0']['publisher'] = {'kind': 'refused'}
            else: rows['DEV-0']['pair']['rights_scope'] = 'UNKNOWN'
            with self.assertRaises(ValueError): M.population(rows, 'software_evidence_v1')

    def test_exact260_rows_raw_bits_and_sequential_teardown(self):
        result, expected = result_fixture(); M.validate_result(result, expected)
        for mutate in ['calls', 'subset', 'bits', 'input', 'teardown', 'loaded']:
            result, expected = result_fixture()
            if mutate == 'calls': result['forward_calls'] = 259
            elif mutate == 'subset': result['arms']['adapted']['rows'].pop()
            elif mutate == 'bits': result['arms']['adapted']['rows'][0]['yes_bits'] = 0
            elif mutate == 'input': result['arms']['adapted']['rows'][0]['input_digest'] = 'changed'
            elif mutate == 'teardown': result['arms']['original']['teardown']['allocator_returned'] = False
            else: result['arms']['adapted']['loaded_state']['tied_pointer'] = False
            with self.assertRaises(ValueError): M.validate_result(result, expected)

    def test_config_only_serialization_normalization_is_allowed(self):
        original = config(); adapted = copy.deepcopy(original)
        adapted.pop('torch_dtype'); adapted['dtype'] = 'float32'; adapted['transformers_version'] = '5.10.1'
        adapted.pop('rope_theta'); adapted.pop('rope_scaling'); adapted['rope_parameters'] = {'rope_theta': 1000000, 'rope_type': 'default'}
        adapted['layer_types'] = ['full_attention'] * 28; adapted['pad_token_id'] = None
        M.config_equivalence(original, adapted)
        for key, value in [('rms_norm_eps', 1e-5), ('hidden_size', 2048), ('rope_parameters', {'rope_theta': 100, 'rope_type': 'default'}), ('unknown_math', True)]:
            bad = copy.deepcopy(adapted); bad[key] = value
            with self.assertRaises(ValueError): M.config_equivalence(original, bad)

    def test_summary_does_not_invent_replay_AUROC_or_promotion(self):
        result, _ = result_fixture(); calls = []
        def metrics(labels, margins):
            calls.append(len(labels)); self.assertEqual(len(labels), 85)
            return {'auroc': .5, 'fixed_zero_balanced_accuracy': .5}
        summary = M.summarize(result, metrics)
        self.assertFalse(summary['promotion'])
        self.assertEqual(summary['arms']['adapted']['positive_replay']['rows'], 45)
        self.assertEqual(calls, [85, 85, 85, 85])

    def test_original_finalclock_is_checked_after_all_consumed_seals(self):
        events = []
        def check():
            events.append('clock')
            if len(events) == 5: raise TimeoutError('original deadline')
        with patch.object(M, 'pinned', side_effect=lambda value: events.append('pin')):
            with self.assertRaises(TimeoutError): M.seal({'input_seals': {'one': 'x', 'two': 'y'}}, check)
        self.assertEqual(events, ['clock', 'pin', 'clock', 'pin', 'clock'])

    def test_natural_exit_is_not_a_substitute_for_HANDLE_and_finalproofs(self):
        result, expected = result_fixture()
        owned = {'exit_code': 0, 'timed_out': False, 'log_oversized': False, 'owned_descendants_settled': True,
                 'owned_job_forced_termination': False, 'owned_total_processes': 1,
                 'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
                 'retained_member_exits': [{'exit_observed': True, 'exit_code': 0}]}
        receipt = {'owned': owned, 'disposition': {'passed': True}, 'failure': None, 'final_hash_failures': [],
                   'final_hashes': {'worker_result': {}, 'child_stdout_log': {}, 'actual_execution_plan': {}}}
        M.accepted_execution(receipt, result, expected)
        for mutate in ['unobserved', 'nonzero', 'forced', 'proof', 'parentfailure']:
            bad = copy.deepcopy(receipt)
            if mutate == 'unobserved': bad['owned']['retained_member_exits'][0]['exit_observed'] = False
            elif mutate == 'nonzero': bad['owned']['exit_code'] = 1
            elif mutate == 'forced': bad['owned']['owned_job_forced_termination'] = True
            elif mutate == 'proof': del bad['final_hashes']['worker_result']
            else: bad['failure'] = {'type': 'ParentFinalHashError'}
            with self.assertRaises(ValueError): M.accepted_execution(bad, result, expected)


if __name__ == '__main__': unittest.main()
