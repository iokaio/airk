# SPDX-License-Identifier: Apache-2.0
"""Independent literal original/candidate DAGs. No exporter/model/GPU use."""
import copy
import importlib.util
import pathlib
import tempfile
import struct
import unittest
import onnx
from onnx import helper as h, TensorProto as T
spec = importlib.util.spec_from_file_location('independent_g5_graph', pathlib.Path(__file__).with_name('graph.py'))
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)

CONFIG = {'num_hidden_layers': 1, 'num_attention_heads': 2, 'num_key_value_heads': 1, 'head_dim': 2, 'hidden_size': 2, 'intermediate_size': 3, 'vocab_size': 4}
A = '/airk_g5/'

def literal(focus, half):
    """Two literal graphs, with handwritten arithmetic/edges and IEEE bytes."""
    nodes, tensors = [], []
    base = '/base/' if focus else '/scorer/'
    params = 'base.' if focus else 'scorer.text.'
    root, att = base + 'layers.0/', base + 'layers.0/self_attn/'
    def tensor(name, dtype, dims, values):
        tensors.append(h.make_tensor(name, dtype, dims, values))
        return name
    def node(op, inputs, name, outputs=None, **attrs):
        outputs = outputs or [name + '_output_0']
        nodes.append(h.make_node(op, inputs, outputs, name=name, **attrs))
        return outputs[0]
    def extra(op, inputs, name, **attrs):
        return node(op, inputs, A + name, [A + name], **attrs)
    def weight(name, shape, protected=False):
        return tensor(name, T.FLOAT if protected or not half else T.FLOAT16, shape, [1] * __import__('math').prod(shape))
    def consumer(op, inputs, name):
        if not half:
            return node(op, inputs, name)
        ins = list(inputs)
        if op == 'MatMul':
            ins[0] = extra('Cast', [ins[0]], 'weight/' + name + '/input16', to=10)
        raw = node(op, ins, name, [A + 'weight/' + name + '/output16'])
        return node('Cast', [raw], A + 'weight/' + name + '/output32', [name + '_output_0'], to=1)
    embed = weight(params + 'embed_tokens.weight', [4, 2])
    x = consumer('Gather', [embed, 'input_ids'], base + 'embedding')
    norm = weight(params + 'layers.0.input_layernorm.weight', [2], True)
    normed = node('Mul', [x, norm], root + 'input_norm')
    qkv = {}
    for label, shape in [('q_proj', [2, 4]), ('k_proj', [2, 2]), ('v_proj', [2, 2])]:
        w = weight('learned_' + label, shape)
        qkv[label] = consumer('MatMul', [normed, w], att + label + '/MatMul')
    # Q/K FP32 norm and position-sensitive protected islands.
    position = node('Cast', ['position_ids'], root + 'position_float', to=1)
    for label in ['q', 'k']:
        n = weight(params + 'layers.0.self_attn.' + label + '_norm.weight', [2], True)
        normalized = node('Mul', [qkv[label + '_proj'], n], att + label + '_norm')
        node('Add', [normalized, position], att + ('Add' if label == 'q' else 'Add_1'))
    sink = att + ('Reshape_5' if focus else 'Reshape_7')
    if not half:
        qk = node('MatMul', [att + 'Add_output_0', att + 'Add_1_output_0'], att + 'MatMul')
        scale = node('Constant', [], att + 'scale_literal', value=h.make_tensor('scale', T.FLOAT, [], [2 ** -.5]))
        scaled = node('Mul', [qk, scale], att + ('Mul_6' if focus else 'Mul_8'))
        mask = node('Constant', [], '/literal_mask', ['/Where_output_0' if focus else '/scorer/Where_output_0'], value=h.make_tensor('mask', T.FLOAT, [], [0]))
        masked = node('Add', [scaled, mask], att + 'Add_2')
        soft = node('Softmax', [masked], att + 'Softmax', axis=-1)
        av = node('MatMul', [soft, qkv['v_proj']], att + 'MatMul_1')
        sh = node('Constant', [], att + 'dense_shape', value=h.make_tensor('shape', T.INT64, [3], [1, 2, 4]))
        out = node('Reshape', [av, sh], sink)
    else:
        tensor(A + 'half_zero', T.FLOAT16, [], [0])
        tensor(A + 'axis1', T.INT64, [1], [1]); tensor(A + 'one', T.INT64, [1], [1]); tensor(A + 'axis2', T.INT64, [1], [2])
        shape = extra('Shape', ['attention_mask'], 'input_shape')
        total = extra('Gather', [shape, A + 'axis1'], 'T', axis=0)
        total32 = extra('Cast', [total], 'T32', to=6)
        if focus:
            mask32 = extra('Cast', ['attention_mask'], 'mask32', to=6)
            live = extra('ReduceSum', [mask32, A + 'axis1'], 'live', keepdims=0)
            pad = extra('Sub', [total32, live], 'pad')
            packed = extra('Concat', [live, pad, total32, pad, total32], 'packed_mask', axis=0)
            b = extra('Cast', ['attention_mask'], 'maskbool', to=9)
            live_queries = extra('Unsqueeze', [b, A + 'axis2'], 'live_queries')
        else:
            before = extra('Sub', [total, A + 'one'], 'Tminus1')
            seqlens = extra('Cast', [before], 'seqlens32', to=6)
        q = extra('Transpose', [att + 'Add_output_0'], 'attention/0/q_btnh', perm=[0, 2, 1, 3])
        k = extra('Transpose', [att + 'Add_1_output_0'], 'attention/0/k_btkh', perm=[0, 2, 1, 3])
        v = qkv['v_proj']
        if focus:
            tensor(A + 'attention/0/kv_shape', T.INT64, [4], [0, 0, 1, 2])
            tensor(A + 'attention/0/kv_indices', T.INT64, [2], [0, 0])
            k = extra('Gather', [k, A + 'attention/0/kv_indices'], 'attention/0/k_repeated', axis=2)
            v = extra('Reshape', [v, A + 'attention/0/kv_shape'], 'attention/0/v_btkh')
            v = extra('Gather', [v, A + 'attention/0/kv_indices'], 'attention/0/v_repeated', axis=2)
        tensor(A + 'attention/0/q_shape', T.INT64, [3], [0, 0, 4])
        tensor(A + 'attention/0/k_shape', T.INT64, [3], [0, 0, 4 if focus else 2])
        q = extra('Reshape', [q, A + 'attention/0/q_shape'], 'attention/0/q_flat')
        k = extra('Reshape', [k, A + 'attention/0/k_shape'], 'attention/0/k_flat')
        if focus:
            v = extra('Reshape', [v, A + 'attention/0/q_shape'], 'attention/0/v_flat')
        q = extra('Cast', [q], 'attention/0/q16', to=10)
        k = extra('Cast', [k], 'attention/0/k16', to=10)
        v = extra('Cast', [v], 'attention/0/v16', to=10)
        raw = A + 'attention/0/raw16'
        if focus:
            node('MultiHeadAttention', [q, k, v, '', packed], A + 'attention/0/efficient', [raw], domain='com.microsoft', num_heads=2, unidirectional=1, scale=2 ** -.5)
            raw = extra('Where', [live_queries, raw, A + 'half_zero'], 'attention/0/zero_padded_queries')
        else:
            node(
                'GroupQueryAttention', [q, k, v, '', '', seqlens, total32], A + 'attention/0/efficient',
                [raw, A + 'attention/0/present_key', A + 'attention/0/present_value'],
                domain='com.microsoft', num_heads=2, kv_num_heads=1, causal=1,
                local_window_size=-1, do_rotary=0, scale=2 ** -.5,
            )
        out = node('Cast', [raw], A + 'attention/0/output32', [sink + '_output_0'], to=1)
    w = weight('learned_o_proj', [4, 2]); out = consumer('MatMul', [out, w], att + 'o_proj/MatMul')
    residual = node('Add', [x, out], root + 'residual')
    norm = weight(params + 'layers.0.post_attention_layernorm.weight', [2], True)
    normalized = node('Mul', [residual, norm], root + 'post_norm')
    gate = consumer('MatMul', [normalized, weight('learned_gate_proj', [2, 3])], root + 'mlp/gate_proj/MatMul')
    up = consumer('MatMul', [normalized, weight('learned_up_proj', [2, 3])], root + 'mlp/up_proj/MatMul')
    sigmoid = node('Sigmoid', [gate], root + 'silu')
    activated = node('Mul', [sigmoid, gate], root + 'gate')
    activated = node('Mul', [activated, up], root + 'gated_up')
    down = consumer('MatMul', [activated, weight('learned_down_proj', [3, 2])], root + 'mlp/down_proj/MatMul')
    final = node('Add', [residual, down], root + 'mlp_residual')
    norm = weight(params + 'norm.weight', [2], True)
    final = node('Mul', [final, norm], base + 'final_norm')
    if focus:
        final = node('MatMul', [final, weight('trained_head', [2, 2], True)], '/MatMul')
        output = 'logits'
    else:
        rows = consumer('Gather', [embed, 'candidate_positions'], '/candidate_rows')
        final = node('Mul', [final, rows], '/readout')
        final = node('Where', ['option_mask', final, final], '/options')
        output = 'scores'
    node('Identity', [final], '/final', [output])
    names = ['input_ids', 'attention_mask', 'position_ids'] + ([] if focus else ['decision_positions', 'candidate_positions', 'option_mask'])
    inputs = [h.make_tensor_value_info(name, T.BOOL if name == 'option_mask' else T.INT64, [1, 2]) for name in names]
    return h.make_model(h.make_graph(nodes, 'independent-literal', inputs, [h.make_tensor_value_info(output, T.FLOAT, [1, 2])], tensors), opset_imports=[h.make_opsetid('', 18)] + ([h.make_opsetid('com.microsoft', 1)] if half else []))

class WholeGraphControls(unittest.TestCase):
    def audit(self, source, candidate, focus=False):
        return g.graph_invariants(source, candidate, CONFIG, focus, onnx)
    def test_complete_context_and_focus_independent_literals(self):
        for focus in (False, True):
            with self.subTest(focus=focus):
                proof = self.audit(literal(focus, False), literal(focus, True), focus)
                self.assertTrue(proof['all_output_reachable_edges_checked'])
                self.assertFalse(proof['qualification'])
    def test_protected_math_and_positions_refuse(self):
        for name in ['/scorer/layers.0/residual', '/scorer/layers.0/gated_up', '/scorer/layers.0/position_float', '/scorer/final_norm']:
            with self.subTest(name=name):
                candidate = literal(False, True)
                node = next(n for n in candidate.graph.node if n.name == name)
                if node.op_type == 'Cast': node.attribute[0].i = 10
                else: node.op_type = 'Sub'
                with self.assertRaisesRegex(ValueError, 'DAG differs'):
                    self.audit(literal(False, False), candidate)
    def test_each_attention_cast_input_output_target_refuse(self):
        for label in ['q16', 'k16', 'v16', 'output32']:
            for field in ['input', 'output', 'type']:
                with self.subTest(label=label, field=field):
                    candidate = literal(False, True)
                    node = next(n for n in candidate.graph.node if n.name == A + 'attention/0/' + label)
                    if field == 'input': node.input[0] = 'position_ids'
                    elif field == 'output': node.output[0] += '_bypassed'
                    else: node.attribute[0].i = 1 if label != 'output32' else 10
                    with self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
    def test_projection_cast_bypass_and_type_refuse(self):
        for label in ['input16', 'output32']:
            candidate = literal(False, True)
            node = next(n for n in candidate.graph.node if n.name == A + 'weight//scorer/layers.0/mlp/gate_proj/MatMul/' + label)
            node.attribute[0].i = 1 if label == 'input16' else 10
            with self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
    def test_context_lengths_scale_and_cache_refuse(self):
        for kind in ['length', 'scale', 'cache', 'causal']:
            candidate = literal(False, True)
            node = next(n for n in candidate.graph.node if n.name == A + 'attention/0/efficient')
            if kind == 'length': node.input[5] = node.input[6]
            elif kind == 'cache': node.input[3] = 'input_ids'
            else: next(a for a in node.attribute if a.name == kind).f = .5 if kind == 'scale' else 0
            with self.subTest(kind=kind), self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
    def test_context_present_output_slots_refuse_missing_swap_or_alias(self):
        source = literal(False, False)
        for mutation in ('missing_key', 'missing_value', 'swapped', 'aliased'):
            candidate = literal(False, True)
            outputs = next(n.output for n in candidate.graph.node if n.op_type == 'GroupQueryAttention')
            if mutation == 'missing_key':
                del outputs[1]
            elif mutation == 'missing_value':
                del outputs[2]
            elif mutation == 'swapped':
                outputs[1], outputs[2] = outputs[2], outputs[1]
            else:
                outputs[2] = outputs[1]
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'DAG differs|duplicate tensor producer'):
                self.audit(source, candidate)

    def test_context_present_outputs_refuse_public_exposure_or_consumer(self):
        source = literal(False, False)
        candidate = literal(False, True)
        candidate.graph.output.append(h.make_tensor_value_info(A + 'attention/0/present_key', T.FLOAT16, [1, 1, 2, 2]))
        with self.assertRaisesRegex(ValueError, 'ABI changed'):
            self.audit(source, candidate)

        candidate = literal(False, True)
        cast = next(n for n in candidate.graph.node if n.name == A + 'attention/0/output32')
        cast.input[0] = A + 'attention/0/present_value'
        with self.assertRaisesRegex(ValueError, 'DAG differs'):
            self.audit(source, candidate)

    def test_focus_packed_mask_repetition_and_sanitizer_refuse(self):
        for kind in ['packed', 'indices', 'condition', 'zero', 'multiply']:
            candidate = literal(True, True)
            if kind == 'indices':
                tensor = next(t for t in candidate.graph.initializer if t.name == A + 'attention/0/kv_indices'); tensor.int64_data[1] = 1
            elif kind == 'zero':
                tensor = next(t for t in candidate.graph.initializer if t.name == A + 'half_zero'); tensor.int32_data[0] = 0x3c00
            else:
                node = next(n for n in candidate.graph.node if n.name == A + ('packed_mask' if kind == 'packed' else 'attention/0/zero_padded_queries'))
                if kind == 'packed': node.input[0], node.input[1] = node.input[1], node.input[0]
                elif kind == 'condition': node.input[0] = 'option_mask'
                else: node.op_type = 'Mul'
            with self.subTest(kind=kind), self.assertRaises(ValueError): self.audit(literal(True, False), candidate, True)
    def test_sparse_readout_other_embedding_and_head_precision_refuse(self):
        candidate = literal(False, True)
        node = next(n for n in candidate.graph.node if n.name == '/candidate_rows'); node.input[0] = 'learned_q_proj'
        with self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
        candidate = literal(True, True)
        next(t for t in candidate.graph.initializer if t.name == 'trained_head').data_type = 10
        with self.assertRaises(ValueError): self.audit(literal(True, False), candidate, True)
    def test_extra_disconnected_node_and_tensor_and_constant_attribute_refuse(self):
        for kind in ['node', 'bf16', 'constant', 'duplicate']:
            candidate = literal(False, True)
            if kind == 'node': candidate.graph.node.append(h.make_node('Identity', ['input_ids'], ['unused'], name='/unused'))
            elif kind == 'bf16': candidate.graph.initializer.append(h.make_tensor('hidden_bf16', T.BFLOAT16, [1], [1]))
            elif kind == 'constant': candidate.graph.node[0].attribute.append(h.make_attribute('extra_tensor', h.make_tensor('unexpected', T.DOUBLE, [1], [1.])))
            else: candidate.graph.node.append(copy.deepcopy(candidate.graph.node[0]))
            with self.subTest(kind=kind), self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
    def test_dense_softmax_and_unexpected_function_refuse(self):
        candidate = literal(False, True)
        candidate.graph.node.append(h.make_node('Softmax', ['input_ids'], ['hidden_dense'], name='/dense'))
        with self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
        candidate = literal(False, True)
        candidate.functions.append(h.make_function('foreign', 'f', [], [], [], []))
        with self.assertRaises(ValueError): self.audit(literal(False, False), candidate)
    def test_abi_opset_and_value_info_refuse(self):
        for kind in ['input', 'output', 'opset', 'value_info', 'header']:
            candidate = literal(False, True)
            if kind == 'input': candidate.graph.input[-1].type.tensor_type.elem_type = T.INT64
            elif kind == 'output': candidate.graph.output[0].type.tensor_type.elem_type = T.FLOAT16
            elif kind == 'opset': candidate.opset_import[-1].version = 2
            elif kind == 'header': candidate.model_version = 42
            else: candidate.graph.value_info.append(h.make_tensor_value_info('/scorer/layers.0/residual_output_0', T.FLOAT16, [1]))
            with self.subTest(kind=kind), self.assertRaises(ValueError): self.audit(literal(False, False), candidate)

class SourceClosureControls(unittest.TestCase):
    def test_literal_source_and_candidate_pins_and_hash_drift(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            source, candidate = root / 'original.onnx', root / 'half.onnx'
            source.write_bytes(literal(False, False).SerializeToString())
            candidate.write_bytes(literal(False, True).SerializeToString())
            original_pin, candidate_pin = g.file_hash(source), g.file_hash(candidate)
            g.authenticated_models(source, original_pin, candidate, candidate_pin, {}, onnx)
            for wrong_source, wrong_candidate in [('0' * 64, candidate_pin), (original_pin, '0' * 64)]:
                with self.assertRaisesRegex(ValueError, 'graph identity'):
                    g.authenticated_models(source, wrong_source, candidate, wrong_candidate, {}, onnx)
            with self.assertRaisesRegex(ValueError, 'closure missing/extra'):
                g.authenticated_models(source, original_pin, candidate, candidate_pin, {'extra.bin': {'bytes': 1, 'sha256': '0' * 64}}, onnx)
    def test_actual_source_external_file_pin_and_drift(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            source, candidate = root / 'original.onnx', root / 'half.onnx'
            model = literal(False, False)
            tensor = model.graph.initializer[0]
            raw = b'\x00\x00\x80\x3f' * 8
            tensor.ClearField('float_data'); tensor.data_location = 1
            for key, value in [('location', 'original.bin'), ('offset', '0'), ('length', '32')]:
                entry = tensor.external_data.add(); entry.key = key; entry.value = value
            source.write_bytes(model.SerializeToString()); candidate.write_bytes(literal(False, True).SerializeToString())
            payload = root / 'original.bin'; payload.write_bytes(raw)
            pins = {'original.bin': {'bytes': 32, 'sha256': g.file_hash(payload)}}
            args = (source, g.file_hash(source), candidate, g.file_hash(candidate), pins, onnx)
            g.authenticated_models(*args)
            payload.write_bytes(raw[:-1] + b'\x01')
            with self.assertRaisesRegex(ValueError, 'source file drift'): g.authenticated_models(*args)
    def test_inline_or_external_ambiguous_storage_refuses(self):
        original, candidate = literal(False, False), literal(False, True)
        tensor = candidate.graph.initializer[0]
        tensor.raw_data = b'\x00' * 16
        with self.assertRaisesRegex(ValueError, 'ambiguous inline tensor'):
            g.graph_invariants(original, candidate, CONFIG, False, onnx)
    def test_reordered_nodes_still_compare_whole_semantic_dag(self):
        original, candidate = literal(False, False), literal(False, True)
        reversed_nodes = list(reversed(candidate.graph.node)); del candidate.graph.node[:]; candidate.graph.node.extend(reversed_nodes)
        self.assertTrue(g.graph_invariants(original, candidate, CONFIG, False, onnx)['all_output_reachable_edges_checked'])
class GateOrderingControls(unittest.TestCase):
    def test_public_acceptance_requires_original_and_native_candidate_pins(self):
        from unittest import mock
        spec = importlib.util.spec_from_file_location('whole_reference_public', pathlib.Path(__file__).resolve().parents[1] / 'arbiter-fp16-reference.py')
        owner = importlib.util.module_from_spec(spec); spec.loader.exec_module(owner)
        with mock.patch.object(owner, '_verify_parameter_bytes') as byte_only:
            with self.assertRaises(TypeError): owner.verify_actual_half_parameters(None, None, onnx, {})
            byte_only.assert_not_called()
        class UnapprovedTiny:
            focus = False
            layers = 1
            hidden = 2
        with mock.patch.object(owner, '_verify_parameter_bytes') as byte_only:
            with self.assertRaisesRegex(ValueError, 'unsupported selected geometry'):
                owner.verify_actual_half_parameters(UnapprovedTiny(), 'absent-half.onnx', onnx, {}, candidate_sha256='0' * 64, original_graph='absent-original.onnx', original_files={})
            byte_only.assert_not_called()
    def test_changed_retained_tensor_constant_bits_refuse(self):
        source, candidate = literal(False, False), literal(False, True)
        for model in [source, candidate]:
            constant = h.make_node('Constant', [], ['rms_epsilon'], name='/rms_epsilon', value=h.make_tensor('epsilon', T.FLOAT, [], [1e-6]))
            model.graph.node.insert(0, constant)
            node = next(n for n in model.graph.node if n.name == '/scorer/layers.0/input_norm')
            addition = h.make_node('Add', [node.input[0], 'rms_epsilon'], ['with_epsilon'], name='/norm_epsilon_add')
            model.graph.node.insert(1, addition)
            node.input[0] = 'with_epsilon'
        self.assertTrue(g.graph_invariants(source, candidate, CONFIG, False, onnx)['all_output_reachable_edges_checked'])
        next(n for n in candidate.graph.node if n.name == '/rms_epsilon').attribute[0].t.float_data[0] = 1e-5
        with self.assertRaisesRegex(ValueError, 'DAG differs'):
            g.graph_invariants(source, candidate, CONFIG, False, onnx)
if __name__ == '__main__': unittest.main()