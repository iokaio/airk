# SPDX-License-Identifier: Apache-2.0
"""Independent tiny byte/metadata controls. Never read real model/checkpoint payloads."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import tempfile
import time
import unittest
from unittest.mock import patch

import numpy as np
import onnx

SPEC = importlib.util.spec_from_file_location('focus_fp32_export', Path(__file__).resolve().parents[1] / 'arbiter-focus-export.py')
e = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(e)


def own_shapes():
    result = {'model.embed_tokens.weight': [151669, 1024], 'model.norm.weight': [1024]}
    for i in range(28):
        for section, name, shape in [
            ('self_attn', 'q_proj', [2048, 1024]), ('self_attn', 'k_proj', [1024, 1024]),
            ('self_attn', 'v_proj', [1024, 1024]), ('self_attn', 'o_proj', [1024, 2048]),
            ('mlp', 'gate_proj', [3072, 1024]), ('mlp', 'up_proj', [3072, 1024]),
            ('mlp', 'down_proj', [1024, 3072])]:
            result[f'model.layers.{i}.{section}.{name}.weight'] = shape
        for name, shape in [('input_layernorm', [1024]), ('post_attention_layernorm', [1024]),
                            ('self_attn.q_norm', [128]), ('self_attn.k_norm', [128])]:
            result[f'model.layers.{i}.{name}.weight'] = shape
    return result


def role_fixture():
    # Metadata only: no zero/placeholder model tensor payload is allocated.
    graph = onnx.GraphProto()
    expected = own_shapes()
    for i, (key, shape) in enumerate(expected.items()):
        tensor = graph.initializer.add()
        if '.weight' in key and ('_proj.weight' in key):
            tensor.name = 'projected-' + str(i)
            tensor.dims.extend(reversed(shape))
            parts = key.split('.')
            node = onnx.helper.make_node('MatMul', ['activation', tensor.name], ['out-' + str(i)],
                                        name=f'/base/layers.{parts[2]}/{parts[3]}/{parts[4]}/MatMul')
        else:
            tensor.name = 'base.' + key[6:]
            tensor.dims.extend(shape)
            node = onnx.helper.make_node('Gather' if key == 'model.embed_tokens.weight' else 'Mul',
                                        [tensor.name, 'arg'], ['out-' + str(i)], name='direct-' + str(i))
        tensor.data_type = 1
        graph.node.append(node)
    tensor = graph.initializer.add(); tensor.name = 'readout'; tensor.data_type = 1; tensor.dims.extend([1024, 2])
    graph.node.append(onnx.helper.make_node('MatMul', ['hidden', 'readout'], ['logits'], name='/MatMul'))
    model = onnx.ModelProto(); model.graph.CopyFrom(graph)
    return model, expected


def external_fixture(root):
    root = Path(root).resolve()
    graph_path = root / 'model.onnx'
    graph_path.write_bytes(b'graph metadata fixture only')
    payloads = {'weights.bin': bytes.fromhex('00000040'),
                'mask.bin': bytes.fromhex('00000000000080ff'),
                'indices.bin': struct.pack('<2q', 7, 11)}
    for name, raw in payloads.items():
        (root / name).write_bytes(raw)
    model = onnx.ModelProto()
    def external(tensor, dtype, shape, location, offset=0, length=None):
        tensor.data_type = dtype; tensor.dims.extend(shape); tensor.data_location = onnx.TensorProto.EXTERNAL
        for key, value in [('location', location), ('offset', str(offset))]:
            row = tensor.external_data.add(); row.key, row.value = key, value
        if length is not None:
            row = tensor.external_data.add(); row.key, row.value = 'length', str(length)
    learned = model.graph.initializer.add(); learned.name = 'learned'
    external(learned, onnx.TensorProto.FLOAT, [1], 'weights.bin')
    node = model.graph.node.add(); node.name = 'constant-mask'; node.op_type = 'Constant'
    attr = node.attribute.add(); attr.name = 'value'; attr.type = onnx.AttributeProto.TENSOR
    external(attr.t, onnx.TensorProto.FLOAT, [1, 2], 'mask.bin')
    node = model.graph.node.add(); node.name = 'tensor-list'; node.op_type = 'LiteralTensorList'
    attr = node.attribute.add(); attr.name = 'values'; attr.type = onnx.AttributeProto.TENSORS
    external(attr.tensors.add(), onnx.TensorProto.INT64, [1], 'indices.bin', 0, 8)
    external(attr.tensors.add(), onnx.TensorProto.INT64, [1], 'indices.bin', 8, 8)
    pins = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in root.iterdir() if path.is_file()}
    return model, graph_path, pins


class ExporterTests(unittest.TestCase):
    def test_bf16_expansion_literal_bits_including_signed_zero_and_subnormal(self):
        self.assertEqual(e.canonical(bytes.fromhex('803f00c0000000800100'), 'BF16', np),
                         bytes.fromhex('0000803f000000c0000000000000008000000100'))

    def test_bf16_and_f32_nonfinite_refuse_without_tolerance(self):
        for raw, dtype in [(bytes.fromhex('807f'), 'BF16'), (bytes.fromhex('c07f'), 'BF16'),
                           (bytes.fromhex('000080ff'), 'F32'), (bytes.fromhex('0100807f'), 'F32')]:
            with self.subTest(raw=raw), self.assertRaisesRegex(ValueError, 'nonfinite'):
                e.canonical(raw, dtype, np)
        with self.assertRaises(ValueError):
            e.canonical(b'\0', 'BF16', np)

    def test_rectangular_transpose_literal(self):
        raw = struct.pack('<6f', 1, 2, 3, 4, 5, 6)
        self.assertEqual(e.transpose(raw, [2, 3], np), struct.pack('<6f', 1, 4, 2, 5, 3, 6))
        with patch.object(e, 'CHUNK', 16):
            self.assertEqual(list(e.transpose_chunks(raw, [2, 3], np)),
                             [struct.pack('<4f', 1, 4, 2, 5), struct.pack('<2f', 3, 6)])

    def test_square_kv_still_requires_actual_transpose(self):
        self.assertEqual(e.transpose(struct.pack('<4f', 1, 2, 3, 4), [2, 2], np),
                         struct.pack('<4f', 1, 3, 2, 4))
        with self.assertRaises(ValueError):
            e.transpose(b'\0' * 8, [2, 2], np)
        with self.assertRaises(ValueError):
            e.transpose(b'\0' * ((16 << 20) + 4), [1, (4 << 20) + 1], np)

    def test_exact_known310_and112_rosters(self):
        self.assertEqual(e.shapes(), own_shapes())
        self.assertEqual(len(e.shapes()), 310)
        self.assertEqual(len(e.target_keys()), 112)
        self.assertIn('model.layers.27.self_attn.o_proj.weight', e.target_keys())
        self.assertNotIn('model.layers.0.mlp.gate_proj.weight', e.target_keys())
        self.assertNotIn('lm_head.weight', e.shapes())

    def test_header_independent_small_positive_and_exact_ranges(self):
        header = {'__metadata__': {'format': 'pt'}, 'a': {'dtype': 'F32', 'shape': [2], 'data_offsets': [0, 8]},
                  'b': {'dtype': 'F32', 'shape': [1], 'data_offsets': [8, 12]}}
        self.assertEqual(set(e.header_rows(header, 12, {'a': [2], 'b': [1]}, 'F32')), {'a', 'b'})
        mutations = []
        v = copy.deepcopy(header); v['b']['data_offsets'] = [4, 8]; mutations.append(v)
        v = copy.deepcopy(header); v['b']['data_offsets'] = [12, 16]; mutations.append(v)
        v = copy.deepcopy(header); v['a']['dtype'] = 'BF16'; mutations.append(v)
        v = copy.deepcopy(header); v['a']['shape'] = [True, 2]; mutations.append(v)
        v = copy.deepcopy(header); v['a']['unknown'] = 1; mutations.append(v)
        v = copy.deepcopy(header); v['lora_A'] = v.pop('a'); mutations.append(v)
        for v in mutations:
            with self.subTest(header=v), self.assertRaises(ValueError):
                e.header_rows(v, 12, {'a': [2], 'b': [1]}, 'F32')
        with self.assertRaises(ValueError):
            e.header_rows(header, 16, {'a': [2], 'b': [1]}, 'F32')

    def test_duplicate_json_and_alias_head_keys_refuse(self):
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            e.decode(b'{"a":1,"a":2}')
        with self.assertRaises(ValueError):
            e.decode(b'{"value":NaN}')
        header = {'__metadata__': {'format': 'pt'}, 'lm_head.weight': {'dtype': 'F32', 'shape': [1], 'data_offsets': [0, 4]}}
        with self.assertRaises(ValueError):
            e.header_rows(header, 4, {'embedding': [1]}, 'F32')

    def test_complete_graph311_roles_without_reading_model_values(self):
        model, expected = role_fixture()
        roles = e.graph_roles(model, expected)
        self.assertEqual(len(roles), 311)
        self.assertEqual(sum(role == 'transpose' for _, role in roles.values()), 196)
        self.assertEqual(sum(role == 'direct' for _, role in roles.values()), 114)
        self.assertEqual(roles['readout'], ('readout_rows', 'readout'))

    def test_missing_unknown_shared_and_misshaped_graph_roles_refuse(self):
        model, expected = role_fixture()
        broken = copy.deepcopy(model); del broken.graph.initializer[-1]
        with self.assertRaises(ValueError): e.graph_roles(broken, expected)
        broken = copy.deepcopy(model); broken.graph.node[-1].name = '/different-readout'
        with self.assertRaises(ValueError): e.graph_roles(broken, expected)
        broken = copy.deepcopy(model); projected = next(t for t in broken.graph.initializer if t.name.startswith('projected'))
        broken.graph.node.append(onnx.helper.make_node('MatMul', ['a', projected.name], ['extra'], name='extra'))
        with self.assertRaises(ValueError): e.graph_roles(broken, expected)
        broken = copy.deepcopy(model); broken.graph.initializer[0].dims[0] = 10
        with self.assertRaises(ValueError): e.graph_roles(broken, expected)

    def test_restoring_only_declared_storage_proves_entire_model(self):
        original = onnx.ModelProto(); t = original.graph.initializer.add(); t.name = 'weight'; t.data_type = 1; t.dims.extend([2, 2])
        x = t.external_data.add(); x.key = 'location'; x.value = 'old'
        original.graph.node.append(onnx.helper.make_node('MatMul', ['a', 'weight'], ['b'], name='matrix'))
        candidate = copy.deepcopy(original); candidate.graph.initializer[0].external_data[0].value = 'new'
        e.preserve_math(original, candidate, ['weight'])
        candidate.graph.node[0].op_type = 'Add'
        with self.assertRaisesRegex(ValueError, 'math/ABI'):
            e.preserve_math(original, candidate, ['weight'])
        candidate = copy.deepcopy(original); candidate.graph.initializer[0].dims[0] = 3
        with self.assertRaises(ValueError): e.preserve_math(original, candidate, ['weight'])

    def test_exact_bytes_reject_one_bit_and_missing_tail(self):
        e.equal_chunks([b'ab', b'cd'], [b'ab', b'cd'])
        e.equal_chunks([b'ab', b'cd'], [b'a', b'bcd'])
        with self.assertRaises(ValueError): e.equal_chunks([b'\x00\x00\x80\x3f'], [b'\x01\x00\x80\x3f'])
        with self.assertRaises(ValueError): e.equal_chunks([b'ab', b'cd'], [b'ab'])

    def test_sources_final_hash_catches_mutation_and_unsealed(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp).resolve() / 'input'; path.write_bytes(b'old')
            source = e.Sources({str(path): hashlib.sha256(b'old').hexdigest()}, time.monotonic() + 10)
            try:
                self.assertEqual(source.read(path, 10), b'old')
                path.write_bytes(b'new')
                with self.assertRaises(ValueError): source.verify()
            finally: source.close()
            path.write_bytes(b'old')
            with self.assertRaises(ValueError):
                e.Sources({str(path): hashlib.sha256(b'old').hexdigest(), str(path.parent) + '/./' + path.name: '1' * 64}, time.monotonic() + 10).hold(path)

    def test_actual_output_hash_and_deadline_refuse_late_mutation(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp).resolve() / 'output'
            pin = e.write_bytes(path, [b'first'], time.monotonic() + 10)
            e.verify_output(path, pin, time.monotonic() + 10)
            path.write_bytes(b'other')
            with self.assertRaises(ValueError): e.verify_output(path, pin, time.monotonic() + 10)
            with self.assertRaises(ValueError): e.verify_output(path, pin, time.monotonic() - 1)

    def test_external_escape_unknown_fields_and_length_refuse(self):
        tensor = onnx.TensorProto(); tensor.name = 'a'; tensor.data_type = 1; tensor.dims.extend([1])
        entry = tensor.external_data.add(); entry.key = 'location'; entry.value = '../escape'
        with self.assertRaises(ValueError): list(e.tensor_chunks(tensor, Path('/graph/model.onnx'), None))
        entry.value = 'safe'
        other = tensor.external_data.add(); other.key = 'basepath'; other.value = 'elsewhere'
        with self.assertRaises(ValueError): list(e.tensor_chunks(tensor, Path('/graph/model.onnx'), None))


    def test_actual_training_gate_class_has_literal_positive_and_refusals(self):
        merged = Path('/worker/merged-full-checkpoint/model.safetensors')
        custody = {'train_result': {'path': '/worker/result.json', 'sha256': 'a' * 64},
                   'train_plan': {'sha256': 'b' * 64}}
        plan = {'mode': 'train', 'max_optimizer_steps': 1, 'worker_output': '/worker'}
        owned = {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
                 'owned_descendants_settled': True, 'owned_job_forced_termination': False,
                 'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
                 'unexpected_third_process': False, 'handle_close_proof': {'settled': True,
                 'forced': False, 'final_active': 0, 'final_member_pids': []}}
        receipt = {'schema': 'airk.private.F1-F2-owned-execution/1', 'failure': None,
                   'final_hash_failures': [], 'disposition': {'passed': True, 'naturally_settled': True},
                   'owned': owned, 'worker_result_path': '/worker/result.json',
                   'worker_result_sha256': 'a' * 64, 'plan_sha256': 'b' * 64,
                   'qualification': False, 'promotion': False}
        result = {'schema': 'airk.private.F1-F2-worker-result/1', 'mode': 'train', 'complete': True,
                  'merged_full_checkpoint': True, 'plan_sha256': 'b' * 64,
                  'updates': [{'step': 1, 'draws': 2, 'ids': ['human-1', 'human-2']}],
                  'forward_attempts': 2, 'forward_calls': 2, 'backward_attempts': 2,
                  'backwards_completed': 2, 'optimizer_attempts': 1, 'gradient_steps': 1,
                  'lora_targets': [f'model.layers.{i}.self_attn.{name}' for i in range(28)
                                   for name in ('q_proj', 'k_proj', 'v_proj', 'o_proj')],
                  'qualification': False, 'promotion': False}
        e.validate_training_records(receipt, result, plan, custody, merged)
        for change in ('mode', 'counter', 'targets', 'failed', 'forced', 'unsettled', 'result-pin', 'promoted'):
            a, b = copy.deepcopy(receipt), copy.deepcopy(result)
            if change == 'mode': b['mode'] = 'probe'
            elif change == 'counter': b['forward_calls'] = 1
            elif change == 'targets': b['lora_targets'][0] = 'model.embed_tokens'
            elif change == 'failed': a['failure'] = 'failure'
            elif change == 'forced': a['owned']['handle_close_proof']['forced'] = True
            elif change == 'unsettled': a['owned']['owned_job_final_active_processes'] = 1
            elif change == 'result-pin': a['worker_result_sha256'] = 'c' * 64
            elif change == 'promoted': b['promotion'] = True
            with self.subTest(change=change), self.assertRaises(ValueError):
                e.validate_training_records(a, b, plan, custody, merged)


    def test_complete_external_constant_and_tensor_list_copy_preserves_negative_infinity(self):
        with tempfile.TemporaryDirectory() as tmp:
            model, graph_path, pins = external_fixture(tmp)
            original = model.SerializeToString()
            source = e.Sources(pins, time.monotonic() + 10)
            try:
                closure = e.external_tensor_closure(model, graph_path, source, onnx)
                self.assertEqual(len(closure), 4)
                self.assertEqual({r['name'] for r in closure}, {'weights.bin', 'mask.bin', 'indices.bin'})
                self.assertEqual([(r['offset'], r['bytes']) for r in closure if r['name'] == 'indices.bin'], [(0, 8), (8, 8)])
                output = Path(tmp).resolve() / 'fresh'; output.mkdir()
                rows = e.copy_unchanged_external(closure, {'weights.bin'}, graph_path, source, output, time.monotonic() + 10)
                self.assertEqual({r['name'] for r in rows}, {'mask.bin', 'indices.bin'})
                self.assertEqual((output / 'mask.bin').read_bytes(), bytes.fromhex('00000000000080ff'))
                self.assertEqual((output / 'indices.bin').read_bytes(), struct.pack('<2q', 7, 11))
                self.assertEqual(model.SerializeToString(), original)
                source.verify()
            finally:
                source.close()

    def test_missing_tampered_escaped_external_constant_refuse(self):
        for change in ('missing-pin', 'missing-file', 'tampered', 'escaped'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as tmp:
                model, graph_path, pins = external_fixture(tmp)
                mask = Path(tmp).resolve() / 'mask.bin'
                if change == 'missing-pin': pins.pop(str(mask))
                elif change == 'missing-file': mask.unlink()
                elif change == 'tampered': mask.write_bytes(b'changed!')
                elif change == 'escaped': model.graph.node[0].attribute[0].t.external_data[0].value = '../mask.bin'
                source = None
                try:
                    with self.assertRaises((ValueError, OSError)):
                        source = e.Sources(pins, time.monotonic() + 10)
                        e.external_tensor_closure(model, graph_path, source, onnx)
                finally:
                    if source is not None: source.close()

    def test_external_overlap_length_and_unsupported_graph_bodies_refuse(self):
        for change in ('overlap', 'wrong-length', 'nested-graph', 'function'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as tmp:
                model, graph_path, pins = external_fixture(tmp)
                if change == 'overlap': model.graph.node[1].attribute[0].tensors[1].external_data[1].value = '4'
                elif change == 'wrong-length':
                    row = model.graph.node[0].attribute[0].t.external_data.add(); row.key, row.value = 'length', '7'
                elif change == 'nested-graph':
                    attr = model.graph.node[0].attribute.add(); attr.name = 'body'; attr.type = onnx.AttributeProto.GRAPH
                    attr.g.name = 'unsupported'
                elif change == 'function': model.functions.add().name = 'unsupported'
                source = e.Sources(pins, time.monotonic() + 10)
                try:
                    with self.assertRaises(ValueError): e.external_tensor_closure(model, graph_path, source, onnx)
                finally: source.close()

    def test_no_half_runtime_or_model_import_in_exporter(self):
        import ast
        text = (Path(__file__).resolve().parents[1] / 'arbiter-focus-export.py').read_text()
        tree = ast.parse(text)
        names = [n.names[0].name for n in ast.walk(tree) if isinstance(n, ast.Import)]
        self.assertFalse(set(names) & {'torch', 'onnxruntime', 'arbiter_fp16_export'})


if __name__ == '__main__':
    unittest.main()
