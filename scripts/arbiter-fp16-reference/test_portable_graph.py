# SPDX-License-Identifier: Apache-2.0
"""Independent literal original/portable DAGs. No exporter/model/GPU use."""
import contextlib
import copy
import importlib.util
import io
import math
import pathlib
import re
import tempfile
import types
import unittest
from unittest import mock
import onnx
from onnx import helper as h, TensorProto as T

def load(name, leaf):
    spec = importlib.util.spec_from_file_location(name, pathlib.Path(__file__).with_name(leaf))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

p = load('independent_portable_graph', 'portable_graph.py')
g = load('independent_g5_graph', 'graph.py')
default = load('independent_g5_literals', 'test_graph.py')
runner = load('independent_portable_runner', 'verify_portable.py')

CONFIG = {'num_hidden_layers': 1, 'num_attention_heads': 2, 'num_key_value_heads': 1, 'head_dim': 2, 'hidden_size': 2, 'intermediate_size': 3, 'vocab_size': 4}
A = '/airk_g5/'
ATT = '/scorer/layers.0/self_attn/'
GATE = '/scorer/layers.0/mlp/gate_proj/MatMul'
UP = '/scorer/layers.0/mlp/up_proj/MatMul'
EFFICIENT = A + 'attention/0/efficient'

def literal(focus, portable):
    """One handwritten graph: the FP32 original or its portable form, with literal names, edges and values."""
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
    def learned(name, shape):
        return tensor(name, T.FLOAT16 if portable else T.FLOAT, shape, [1] * math.prod(shape))
    def protected(name, shape):
        return tensor(name, T.FLOAT, shape, [1] * math.prod(shape))
    def project(activation, weight, name):
        # Portable: only the stored weight crosses a cast. Activation and product keep their FP32 values.
        if portable:
            weight = extra('Cast', [weight], 'weight/' + name + '/weight32', to=T.FLOAT)
        return node('MatMul', [activation, weight], name)
    def rows(indices, name):
        if not portable:
            return node('Gather', [embed, indices], name)
        half = node('Gather', [embed, indices], name, [A + 'weight/' + name + '/output16'])
        return node('Cast', [half], A + 'weight/' + name + '/output32', [name + '_output_0'], to=T.FLOAT)
    embed = learned(params + 'embed_tokens.weight', [4, 2])
    x = rows('input_ids', base + 'embedding')
    normed = node('Mul', [x, protected(params + 'layers.0.input_layernorm.weight', [2])], root + 'input_norm')
    qkv = {}
    for label, shape in [('q_proj', [2, 4]), ('k_proj', [2, 2]), ('v_proj', [2, 2])]:
        qkv[label] = project(normed, learned('learned_' + label, shape), att + label + '/MatMul')
    # Q/K FP32 norm and position-sensitive protected islands.
    position = node('Cast', ['position_ids'], root + 'position_float', to=T.FLOAT)
    for label in ['q', 'k']:
        scale = protected(params + 'layers.0.self_attn.' + label + '_norm.weight', [2])
        normalized = node('Mul', [qkv[label + '_proj'], scale], att + label + '_norm')
        node('Add', [normalized, position], att + ('Add' if label == 'q' else 'Add_1'))
    sink = att + ('Reshape_5' if focus else 'Reshape_7')
    attended = sink + '_output_0'
    if not portable:
        qk = node('MatMul', [att + 'Add_output_0', att + 'Add_1_output_0'], att + 'MatMul')
        scale = node('Constant', [], att + 'scale_literal', value=h.make_tensor('scale', T.FLOAT, [], [2 ** -.5]))
        scaled = node('Mul', [qk, scale], att + ('Mul_6' if focus else 'Mul_8'))
        mask = node('Constant', [], '/literal_mask', ['/Where_output_0' if focus else '/scorer/Where_output_0'], value=h.make_tensor('mask', T.FLOAT, [], [0]))
        masked = node('Add', [scaled, mask], att + 'Add_2')
        soft = node('Softmax', [masked], att + 'Softmax', axis=-1)
        av = node('MatMul', [soft, qkv['v_proj']], att + 'MatMul_1')
        dense = node('Constant', [], att + 'dense_shape', value=h.make_tensor('shape', T.INT64, [3], [1, 2, 4]))
        node('Reshape', [av, dense], sink)
    else:
        tensor(A + 'float_zero', T.FLOAT, [], [0])
        tensor(A + 'axis1', T.INT64, [1], [1]); tensor(A + 'one', T.INT64, [1], [1]); tensor(A + 'axis2', T.INT64, [1], [2])
        shape = extra('Shape', ['attention_mask'], 'input_shape')
        total = extra('Gather', [shape, A + 'axis1'], 'T', axis=0)
        total32 = extra('Cast', [total], 'T32', to=T.INT32)
        if focus:
            mask32 = extra('Cast', ['attention_mask'], 'mask32', to=T.INT32)
            live = extra('ReduceSum', [mask32, A + 'axis1'], 'live', keepdims=0)
            pad = extra('Sub', [total32, live], 'pad')
            packed = extra('Concat', [live, pad, total32, pad, total32], 'packed_mask', axis=0)
            live_bool = extra('Cast', ['attention_mask'], 'maskbool', to=T.BOOL)
            live_queries = extra('Unsqueeze', [live_bool, A + 'axis2'], 'live_queries')
        else:
            before = extra('Sub', [total, A + 'one'], 'Tminus1')
            seqlens = extra('Cast', [before], 'seqlens32', to=T.INT32)
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
            raw = A + 'attention/0/raw32'
            node('MultiHeadAttention', [q, k, v, '', packed], EFFICIENT, [raw], domain='com.microsoft', num_heads=2, unidirectional=1, scale=2 ** -.5)
            node('Where', [live_queries, raw, A + 'float_zero'], A + 'attention/0/zero_padded_queries', [attended])
        else:
            node(
                'GroupQueryAttention', [q, k, v, '', '', seqlens, total32], EFFICIENT,
                [attended, A + 'attention/0/present_key', A + 'attention/0/present_value'],
                domain='com.microsoft', num_heads=2, kv_num_heads=1, causal=1,
                local_window_size=-1, do_rotary=0, scale=2 ** -.5,
            )
    out = project(attended, learned('learned_o_proj', [4, 2]), att + 'o_proj/MatMul')
    residual = node('Add', [x, out], root + 'residual')
    normalized = node('Mul', [residual, protected(params + 'layers.0.post_attention_layernorm.weight', [2])], root + 'post_norm')
    gate = project(normalized, learned('learned_gate_proj', [2, 3]), root + 'mlp/gate_proj/MatMul')
    up = project(normalized, learned('learned_up_proj', [2, 3]), root + 'mlp/up_proj/MatMul')
    sigmoid = node('Sigmoid', [gate], root + 'silu')
    activated = node('Mul', [sigmoid, gate], root + 'gate')
    activated = node('Mul', [activated, up], root + 'gated_up')
    down = project(activated, learned('learned_down_proj', [3, 2]), root + 'mlp/down_proj/MatMul')
    final = node('Add', [residual, down], root + 'mlp_residual')
    final = node('Mul', [final, protected(params + 'norm.weight', [2])], base + 'final_norm')
    if focus:
        final = node('MatMul', [final, protected('trained_head', [2, 2])], '/MatMul')
        output = 'logits'
    else:
        final = node('Mul', [final, rows('candidate_positions', '/candidate_rows')], '/readout')
        final = node('Where', ['option_mask', final, final], '/options')
        output = 'scores'
    node('Identity', [final], '/final', [output])
    # The original declares four FP32 values. The dense probability value has no portable producer.
    declared = [x, attended, gate] + ([] if portable else [att + 'Softmax_output_0'])
    names = ['input_ids', 'attention_mask', 'position_ids'] + ([] if focus else ['decision_positions', 'candidate_positions', 'option_mask'])
    inputs = [h.make_tensor_value_info(name, T.BOOL if name == 'option_mask' else T.INT64, [1, 2]) for name in names]
    graph = h.make_graph(nodes, 'independent-literal', inputs, [h.make_tensor_value_info(output, T.FLOAT, [1, 2])], tensors, value_info=[h.make_tensor_value_info(name, T.FLOAT, None) for name in declared])
    return h.make_model(graph, opset_imports=[h.make_opsetid('', 18)] + ([h.make_opsetid('com.microsoft', 1)] if portable else []))

def find(model, name):
    return next(n for n in model.graph.node if n.name == name)

def stored(model, name):
    return next(t for t in model.graph.initializer if t.name == name)

def restore(model, name, dtype, dims):
    model.graph.initializer.remove(stored(model, name))
    model.graph.initializer.append(h.make_tensor(name, dtype, dims, [1] * math.prod(dims)))

def dense_nodes(names):
    return [copy.deepcopy(n) for n in literal(False, False).graph.node if n.name in names]

class PortableGraphControls(unittest.TestCase):
    def audit(self, candidate, focus=False, source=None):
        return p.portable_invariants(source or literal(focus, False), candidate, CONFIG, focus, onnx)
    def refuse(self, pattern, candidate, focus=False, source=None):
        with self.assertRaisesRegex(ValueError, pattern):
            self.audit(candidate, focus, source)
        # Without the census, the remaining gate still refuses the same graph.
        with mock.patch.object(p, 'precision_census'), self.assertRaises(ValueError):
            self.audit(candidate, focus, source)

    def test_complete_context_and_focus_portable_literals(self):
        for focus in (False, True):
            with self.subTest(focus=focus):
                candidate = literal(focus, True)
                self.assertEqual(self.audit(candidate, focus), {
                    'nodes': len(candidate.graph.node), 'layers': 1, 'all_output_reachable_edges_checked': True,
                    'recipe': 'portable', 'arithmetic': 'float32', 'qualification': False})

    def test_reordered_nodes_still_compare_whole_semantic_dag(self):
        candidate = literal(False, True)
        reversed_nodes = list(reversed(candidate.graph.node)); del candidate.graph.node[:]; candidate.graph.node.extend(reversed_nodes)
        self.assertTrue(self.audit(candidate)['all_output_reachable_edges_checked'])

    def test_binary16_producers_refuse(self):
        for kind in ['weight_cast_target', 'activation_round_trip', 'half_constant', 'half_dtype_attribute']:
            candidate = literal(False, True)
            if kind == 'weight_cast_target':
                find(candidate, A + 'weight/' + GATE + '/weight32').attribute[0].i = T.FLOAT16
            elif kind == 'activation_round_trip':
                product = find(candidate, GATE)
                narrow = A + 'weight/' + GATE + '/input16'
                candidate.graph.node.append(h.make_node('Cast', [product.input[0]], [narrow], name=narrow, to=T.FLOAT16))
                candidate.graph.node.append(h.make_node('Cast', [narrow], ['widened'], name='/widened', to=T.FLOAT))
                product.input[0] = 'widened'
            elif kind == 'half_constant':
                candidate.graph.node.append(h.make_node('Constant', [], ['half_literal'], name='/half_literal', value=h.make_tensor('literal', T.FLOAT16, [], [0])))
            else:
                candidate.graph.node.append(h.make_node('RandomNormalLike', ['/scorer/layers.0/residual_output_0'], ['noise'], name='/noise', dtype=T.FLOAT16))
            with self.subTest(kind=kind):
                self.refuse('binary16 produced inside the graph', candidate)

    def test_attention_half_operands_refuse(self):
        candidate = literal(False, True)
        attention = find(candidate, EFFICIENT)
        for slot, label in enumerate(['q16', 'k16', 'v16']):
            name = A + 'attention/0/' + label
            candidate.graph.node.append(h.make_node('Cast', [attention.input[slot]], [name], name=name, to=T.FLOAT16))
            attention.input[slot] = name
        candidate.graph.node.append(h.make_node('Cast', [A + 'attention/0/raw16'], [attention.output[0]], name=A + 'attention/0/output32', to=T.FLOAT))
        attention.output[0] = A + 'attention/0/raw16'
        self.refuse('binary16 produced inside the graph', candidate)
        candidate = literal(False, True)
        find(candidate, EFFICIENT).input[2] = 'learned_v_proj'
        self.refuse('half projection weight is not cast once to FP32', candidate)
        candidate = literal(False, True)
        find(candidate, EFFICIENT).input[1] = A + 'weight//scorer/embedding/output16'
        self.refuse('gathered half rows are not cast once to FP32', candidate)

    def test_default_recipe_graph_refuses_and_default_verifier_refuses_portable(self):
        for focus in (False, True):
            with self.subTest(focus=focus):
                self.refuse('binary16 produced inside the graph', default.literal(focus, True), focus, default.literal(focus, False))
                with self.assertRaisesRegex(ValueError, 'DAG differs'):
                    g.graph_invariants(literal(focus, False), literal(focus, True), CONFIG, focus, onnx)

    def test_projection_reading_fp32_or_uncast_weight_refuses(self):
        cast = A + 'weight/' + GATE + '/weight32'
        expected = {
            'fp32_storage_direct': 'binary16 storage differs from the learned half roles',
            'fp32_storage_behind_cast': 'binary16 storage differs from the learned half roles',
            'half_storage_direct': 'half projection weight is not cast once to FP32',
            'fp32_duplicate': 'weight cast does not feed one MatMul weight operand',
        }
        for kind, pattern in expected.items():
            candidate = literal(False, True)
            if kind.startswith('fp32_storage'):
                restore(candidate, 'learned_gate_proj', T.FLOAT, [2, 3])
            if kind == 'fp32_duplicate':
                candidate.graph.initializer.append(h.make_tensor('learned_gate_proj_fp32', T.FLOAT, [2, 3], [1] * 6))
                find(candidate, GATE).input[1] = 'learned_gate_proj_fp32'
            elif kind != 'fp32_storage_behind_cast':
                candidate.graph.node.remove(find(candidate, cast))
                find(candidate, GATE).input[1] = 'learned_gate_proj'
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate)

    def test_weight_cast_wrong_operand_refuses(self):
        operand = 'weight cast does not feed one MatMul weight operand'
        expected = {
            'activation_operand': operand, 'both_operands': operand, 'shared_cast': operand, 'not_a_product': operand,
            'second_cast': 'half projection weight is not cast once to FP32',
            'crossed_weights': 'DAG differs: ' + re.escape(GATE + ', ' + UP),
        }
        for kind, pattern in expected.items():
            candidate = literal(False, True)
            product = find(candidate, GATE)
            if kind == 'activation_operand':
                product.input[0], product.input[1] = product.input[1], product.input[0]
            elif kind == 'both_operands':
                product.input[0] = product.input[1]
            elif kind == 'shared_cast':
                find(candidate, UP).input[1] = product.input[1]
            elif kind == 'not_a_product':
                product.op_type = 'Mul'
            elif kind == 'second_cast':
                candidate.graph.node.append(h.make_node('Cast', ['learned_gate_proj'], ['second'], name='/second', to=T.FLOAT))
            else:
                other = find(candidate, UP)
                product.input[1], other.input[1] = other.input[1], product.input[1]
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate)

    def test_context_attention_bias_cache_and_present_outputs_refuse(self):
        slots = 'Context attention input/output slots'
        expected = {
            'past_key': slots, 'past_value': slots, 'rotary_caches': slots, 'attention_bias': slots, 'missing_present_value': slots,
            'present_consumed': 'present outputs have a consumer', 'present_public': 'ABI changed',
            'present_swapped': 'DAG differs: ' + re.escape(EFFICIENT) + '$', 'present_aliased': 'duplicate tensor producer',
        }
        for kind, pattern in expected.items():
            candidate = literal(False, True)
            attention = find(candidate, EFFICIENT)
            if kind == 'past_key':
                attention.input[3] = '/scorer/layers.0/self_attn/k_proj/MatMul_output_0'
            elif kind == 'past_value':
                attention.input[4] = '/scorer/layers.0/self_attn/v_proj/MatMul_output_0'
            elif kind == 'rotary_caches':
                attention.input.extend(['/scorer/layers.0/position_float_output_0'] * 2)
            elif kind == 'attention_bias':
                attention.input.extend(['', '', '', '/scorer/layers.0/position_float_output_0'])
            elif kind == 'missing_present_value':
                del attention.output[2]
            elif kind == 'present_consumed':
                candidate.graph.node.append(h.make_node('Identity', [attention.output[1]], ['carried_state'], name='/carried_state'))
            elif kind == 'present_public':
                candidate.graph.output.append(h.make_tensor_value_info(attention.output[1], T.FLOAT, None))
            elif kind == 'present_swapped':
                attention.output[1], attention.output[2] = attention.output[2], attention.output[1]
            else:
                attention.output[2] = attention.output[1]
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate)

    def test_context_attention_attributes_lengths_operands_and_sink_refuse(self):
        for kind in ['scale', 'causal', 'num_heads', 'kv_num_heads', 'do_rotary', 'local_window_size', 'lengths', 'operands', 'value', 'sink']:
            candidate = literal(False, True)
            attention = find(candidate, EFFICIENT)
            if kind == 'lengths':
                attention.input[5], attention.input[6] = attention.input[6], attention.input[5]
            elif kind == 'operands':
                attention.input[0], attention.input[1] = attention.input[1], attention.input[0]
            elif kind == 'value':
                attention.input[2] = '/scorer/layers.0/self_attn/k_proj/MatMul_output_0'
            elif kind == 'sink':
                candidate.graph.node.append(h.make_node('Identity', ['detour'], [attention.output[0]], name='/detour'))
                attention.output[0] = 'detour'
            elif kind == 'scale':
                next(a for a in attention.attribute if a.name == kind).f = .5
            else:
                next(a for a in attention.attribute if a.name == kind).i = {'causal': 0, 'num_heads': 1, 'kv_num_heads': 2, 'do_rotary': 1, 'local_window_size': 4}[kind]
            with self.subTest(kind=kind):
                self.refuse('DAG differs', candidate)

    def test_focus_attention_bias_mask_zero_and_sanitizer_refuse(self):
        slots = 'Focus attention input/output slots'
        expected = {
            'projection_bias': slots, 'attention_bias': slots, 'missing_mask': slots,
            'packed_order': 'DAG differs: ' + A + 'packed_mask', 'condition': 'DAG differs', 'multiply': 'DAG differs', 'unsanitized': 'DAG differs',
            'repetition': 'mask/layout/zero Constant differs', 'zero_value': 'mask/layout/zero Constant differs',
            'half_zero': 'binary16 storage differs from the learned half roles',
        }
        for kind, pattern in expected.items():
            candidate = literal(True, True)
            attention = find(candidate, EFFICIENT)
            sanitizer = find(candidate, A + 'attention/0/zero_padded_queries')
            if kind == 'projection_bias':
                attention.input[3] = '/base/layers.0/position_float_output_0'
            elif kind == 'attention_bias':
                attention.input.append('/base/layers.0/position_float_output_0')
            elif kind == 'missing_mask':
                del attention.input[4]
            elif kind == 'packed_order':
                packed = find(candidate, A + 'packed_mask'); packed.input[0], packed.input[1] = packed.input[1], packed.input[0]
            elif kind == 'condition':
                sanitizer.input[0] = A + 'maskbool'
            elif kind == 'multiply':
                sanitizer.op_type = 'Mul'
            elif kind == 'unsanitized':
                attention.output[0] = sanitizer.output[0]
                candidate.graph.node.remove(sanitizer)
            elif kind == 'repetition':
                stored(candidate, A + 'attention/0/kv_indices').int64_data[1] = 1
            elif kind == 'zero_value':
                stored(candidate, A + 'float_zero').float_data[0] = 1
            else:
                candidate.graph.initializer.remove(stored(candidate, A + 'float_zero'))
                candidate.graph.initializer.append(h.make_tensor(A + 'float_zero', T.FLOAT16, [], [0]))
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate, True)

    def test_surviving_softmax_refuses(self):
        chain = {ATT + leaf for leaf in ['MatMul', 'scale_literal', 'Mul_8', 'Add_2', 'Softmax', 'MatMul_1']} | {'/literal_mask'}
        for kind in ['disconnected_dense_chain', 'dense_attention_kept', 'spliced']:
            candidate = literal(False, True)
            if kind == 'disconnected_dense_chain':
                candidate.graph.node.extend(dense_nodes(chain))
            elif kind == 'dense_attention_kept':
                replaced = [n for n in candidate.graph.node if n.name.startswith(A + 'attention/')]
                for node in replaced:
                    candidate.graph.node.remove(node)
                candidate.graph.node.extend(dense_nodes(chain | {ATT + 'dense_shape', ATT + 'Reshape_7'}))
            else:
                attention = find(candidate, EFFICIENT)
                candidate.graph.node.append(h.make_node('Softmax', ['attended'], [attention.output[0]], name='/renormalized', axis=-1))
                attention.output[0] = 'attended'
            with self.subTest(kind=kind):
                self.refuse('dense Softmax survived', candidate)

    def test_protected_math_and_positions_refuse(self):
        for name in ['/scorer/layers.0/residual', '/scorer/layers.0/gated_up', '/scorer/layers.0/position_float', '/scorer/final_norm']:
            candidate = literal(False, True)
            node = find(candidate, name)
            if node.op_type == 'Cast': node.attribute[0].i = T.DOUBLE
            else: node.op_type = 'Sub'
            with self.subTest(name=name):
                self.refuse('DAG differs: ' + re.escape(name) + '$', candidate)

    def test_changed_retained_tensor_constant_bits_refuse(self):
        source, candidate = literal(False, False), literal(False, True)
        for model in [source, candidate]:
            model.graph.node.insert(0, h.make_node('Constant', [], ['rms_epsilon'], name='/rms_epsilon', value=h.make_tensor('epsilon', T.FLOAT, [], [1e-6])))
            node = find(model, '/scorer/layers.0/input_norm')
            model.graph.node.insert(1, h.make_node('Add', [node.input[0], 'rms_epsilon'], ['with_epsilon'], name='/norm_epsilon_add'))
            node.input[0] = 'with_epsilon'
        self.assertTrue(self.audit(candidate, source=source)['all_output_reachable_edges_checked'])
        find(candidate, '/rms_epsilon').attribute[0].t.float_data[0] = 1e-5
        self.refuse('DAG differs: /rms_epsilon$', candidate, source=source)

    def test_embedding_rows_refuse(self):
        rows = A + 'weight//scorer/embedding/output'
        expected = {
            'whole_embedding_cast': 'half embedding consumer', 'rows_uncast': 'gathered half rows are not cast once to FP32',
            'rows_cast_twice': 'gathered half rows are not cast once to FP32', 'other_half_tensor': 'half projection weight is not cast once to FP32',
            'fp32_duplicate_embedding': 'DAG differs: ' + re.escape(A + 'weight//candidate_rows/output32, /candidate_rows') + '$',
        }
        for kind, pattern in expected.items():
            candidate = literal(False, True)
            if kind == 'whole_embedding_cast':
                candidate.graph.node.append(h.make_node('Cast', ['scorer.text.embed_tokens.weight'], ['embedding32'], name='/embedding32', to=T.FLOAT))
                find(candidate, '/scorer/embedding').input[0] = 'embedding32'
            elif kind == 'rows_uncast':
                gather, cast = find(candidate, '/scorer/embedding'), find(candidate, rows + '32')
                gather.output[0] = cast.output[0]
                candidate.graph.node.remove(cast)
            elif kind == 'rows_cast_twice':
                candidate.graph.node.append(h.make_node('Cast', [rows + '16'], ['second_rows'], name='/second_rows', to=T.FLOAT))
            elif kind == 'other_half_tensor':
                find(candidate, '/candidate_rows').input[0] = 'learned_q_proj'
            else:
                candidate.graph.initializer.append(h.make_tensor('duplicate_embedding', T.FLOAT, [4, 2], [1] * 8))
                gather, cast = find(candidate, '/candidate_rows'), find(candidate, A + 'weight//candidate_rows/output32')
                gather.input[0], gather.output[0] = 'duplicate_embedding', cast.output[0]
                candidate.graph.node.remove(cast)
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate)

    def test_abi_opset_header_and_value_info_refuse(self):
        declarations = 'intermediate dtype declarations differ'
        expected = {
            'input': 'ABI changed', 'output': 'ABI changed', 'renamed_output': 'ABI changed', 'opset': 'opset mutation', 'missing_opset': 'opset mutation',
            'default_opset': 'opset mutation', 'header': 'model header semantics changed', 'ir': 'IR version changed',
            'half_declaration': declarations, 'dense_declaration': declarations, 'dropped_declaration': declarations,
        }
        for kind, pattern in expected.items():
            candidate = literal(False, True)
            if kind == 'input': candidate.graph.input[-1].type.tensor_type.elem_type = T.INT64
            elif kind == 'output': candidate.graph.output[0].type.tensor_type.elem_type = T.FLOAT16
            elif kind == 'renamed_output':
                candidate.graph.output[0].name = 'hidden'; find(candidate, '/final').output[0] = 'hidden'
            elif kind == 'opset': candidate.opset_import[-1].version = 2
            elif kind == 'missing_opset': del candidate.opset_import[-1]
            elif kind == 'default_opset': candidate.opset_import[0].version = 17
            elif kind == 'header': candidate.model_version = 42
            elif kind == 'ir': candidate.ir_version += 1
            elif kind == 'half_declaration': candidate.graph.value_info[2].type.tensor_type.elem_type = T.FLOAT16
            elif kind == 'dense_declaration': candidate.graph.value_info.append(h.make_tensor_value_info(ATT + 'Softmax_output_0', T.FLOAT, None))
            else: del candidate.graph.value_info[1]
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate)

    def test_extra_node_tensor_function_and_storage_refuse(self):
        storage = 'binary16 storage differs from the learned half roles'
        expected = {
            'node': 'DAG differs: /unused', 'tensor': 'extra/missing candidate tensor', 'bf16': 'extra/missing candidate tensor',
            'default_half_zero': storage, 'protected_as_half': storage, 'duplicate': 'missing/duplicate node identity',
            'function': 'functions/training/sparse graph', 'ambiguous_storage': 'ambiguous inline tensor', 'learned_shape': 'learned candidate dtype/shape',
        }
        for kind, pattern in expected.items():
            candidate = literal(False, True)
            if kind == 'node': candidate.graph.node.append(h.make_node('Identity', ['input_ids'], ['unused'], name='/unused'))
            elif kind == 'tensor': candidate.graph.initializer.append(h.make_tensor('unexpected', T.INT64, [1], [1]))
            elif kind == 'bf16': candidate.graph.initializer.append(h.make_tensor('hidden_bf16', T.BFLOAT16, [1], [1]))
            elif kind == 'default_half_zero': candidate.graph.initializer.append(h.make_tensor(A + 'half_zero', T.FLOAT16, [], [0]))
            elif kind == 'protected_as_half': restore(candidate, 'scorer.text.norm.weight', T.FLOAT16, [2])
            elif kind == 'duplicate': candidate.graph.node.append(copy.deepcopy(candidate.graph.node[0]))
            elif kind == 'function': candidate.functions.append(h.make_function('foreign', 'f', [], [], [], []))
            elif kind == 'ambiguous_storage': stored(candidate, 'scorer.text.embed_tokens.weight').raw_data = b'\x00' * 16
            else: restore(candidate, 'learned_gate_proj', T.FLOAT16, [3, 2])
            with self.subTest(kind=kind):
                self.refuse(pattern, candidate)

class FileGateControls(unittest.TestCase):
    def test_unapproved_geometry_refuses_before_file_reads(self):
        tiny = types.SimpleNamespace(focus=False, layers=1, hidden=2)
        with self.assertRaisesRegex(ValueError, 'unsupported selected geometry'):
            p.verify_portable_files(tiny, 'absent-portable.onnx', '0' * 64, 'absent-original.onnx', {}, {}, onnx)
        wide = types.SimpleNamespace(focus=False, layers=28, hidden=2048, qheads=32, kvheads=8, dim=128)
        with self.assertRaisesRegex(ValueError, 'selected geometry differs'):
            p.verify_portable_files(wide, 'absent-portable.onnx', '0' * 64, 'absent-original.onnx', {}, {}, onnx)

    def test_literal_original_is_not_the_pinned_original(self):
        compact = types.SimpleNamespace(focus=False, layers=28, hidden=2048, qheads=16, kvheads=8, dim=128, config=CONFIG)
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            source, candidate = root / 'original.onnx', root / 'portable.onnx'
            source.write_bytes(literal(False, False).SerializeToString())
            candidate.write_bytes(literal(False, True).SerializeToString())
            with self.assertRaisesRegex(ValueError, 'graph identity'):
                p.verify_portable_files(compact, candidate, g.file_hash(candidate), source, {}, {}, onnx)

class RunnerControls(unittest.TestCase):
    def arguments(self, pins, model='compact'):
        return runner.arguments([
            '--model', model, '--original', 'absent-original.onnx', '--original-pins', str(pins), '--original-pins-sha256', '0' * 64,
            '--graph', 'absent-portable.onnx', '--graph-sha256', '0' * 64, '--publisher', 'absent-publisher'])

    def test_changed_original_pins_refuse_before_publisher_or_graph_reads(self):
        with tempfile.TemporaryDirectory() as folder:
            pins = pathlib.Path(folder) / 'pins.json'
            pins.write_bytes(b'{"model.onnx.data":{"bytes":1,"sha256":"' + b'1' * 64 + b'"}}')
            with self.assertRaisesRegex(ValueError, 'original closure pins differ'):
                runner.run(self.arguments(pins), onnx)

    def test_runner_names_context_models_only(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.arguments('absent-pins.json', 'focus')

if __name__ == '__main__': unittest.main()
