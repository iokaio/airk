# SPDX-License-Identifier: Apache-2.0
"""Independent bounded original-to-portable DAG proof; no exporter import/inference.

The portable recipe keeps learned binary16 storage and computes in FP32.
graph.py owns the default recipe and the bounded primitives reused here. Its
bytes stay unchanged because execution owners pin its digest.
"""
import collections
import copy
import importlib.util
import math
import pathlib
import struct

_spec = importlib.util.spec_from_file_location('airk_fp16_independent_graph', pathlib.Path(__file__).with_name('graph.py'))
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
NS = base.NS
GEOMETRY = {'focus': (28, 16, 8, 128, 1024), 'compact': (28, 16, 8, 128, 2048), 'large': (36, 32, 8, 128, 2560)}

def require(condition, message):
    if not condition:
        raise ValueError('independent portable graph: ' + message)

def precision_census(candidate, half, embedding, layers, focus):
    """Portable contract stated on the candidate alone, apart from the expected DAG.

    binary16 exists only as learned storage and as rows gathered from the half
    embedding, and each is cast to FP32 before arithmetic. Nothing produces
    binary16, no Softmax remains and attention carries no bias or cache.
    """
    uses, attention = collections.defaultdict(list), []
    for node in candidate.graph.node:
        for index, value in enumerate(node.input):
            uses[value].append((node, index))
        require(node.op_type != 'Softmax', 'dense Softmax survived')
        for attribute in node.attribute:
            tensors = [attribute.t] if attribute.type == 4 else list(attribute.tensors) if attribute.type == 9 else []
            require(all(t.data_type != 10 for t in tensors) and not (attribute.name in ('to', 'dtype') and attribute.i == 10), 'binary16 produced inside the graph')
        if node.domain:
            attention.append(node)
    def cast32(node):
        return (node.op_type, node.domain, len(node.input), len(node.output)) == ('Cast', '', 1, 1) and [(a.name, a.type, a.i) for a in node.attribute] == [('to', 2, 1)]
    require({t.name for t in candidate.graph.initializer if t.data_type == 10} == set(half), 'binary16 storage differs from the learned half roles')
    for name in half:
        consumers = uses[name]
        if name == embedding:
            require(consumers and all((n.op_type, n.domain, i, len(n.output)) == ('Gather', '', 0, 1) for n, i in consumers), 'half embedding consumer')
            for gather, _ in consumers:
                rows = uses[gather.output[0]]
                require(len(rows) == 1 and cast32(rows[0][0]), 'gathered half rows are not cast once to FP32')
        else:
            require(len(consumers) == 1 and cast32(consumers[0][0]), 'half projection weight is not cast once to FP32')
            product = uses[consumers[0][0].output[0]]
            require(len(product) == 1 and (product[0][0].op_type, product[0][0].domain, product[0][1]) == ('MatMul', '', 1), 'weight cast does not feed one MatMul weight operand')
    operator = 'MultiHeadAttention' if focus else 'GroupQueryAttention'
    require(len(attention) == layers and all((n.op_type, n.domain) == (operator, 'com.microsoft') for n in attention), 'attention operator census')
    for node in attention:
        ins = list(node.input)
        if focus:
            # Query, key, value, empty projection bias, packed mask. A further slot would add attention bias or past state.
            require(len(ins) == 5 and all(ins[:3]) and not ins[3] and ins[4] and len(node.output) == 1, 'Focus attention input/output slots')
        else:
            # Query, key, value, empty past key and value, both lengths. A further slot would add rotary caches or attention bias.
            require(len(ins) == 7 and all(ins[:3]) and ins[3:5] == ['', ''] and all(ins[5:]) and len(node.output) == 3, 'Context attention input/output slots')
            require(not uses[node.output[1]] and not uses[node.output[2]], 'present outputs have a consumer')

def portable_invariants(original, candidate, config, focus, onnx, original_directory='.', source_files=None, candidate_directory='.', candidate_files=None):
    """Pure metadata gate. Caller authenticates original and publisher config.

    Expectations originate only in original semantics plus this explicit policy.
    No candidate node/Constant supplies an expected edge or value.
    """
    source_files, candidate_files = source_files or {}, candidate_files or {}
    source, initial = base.catalog(original, original_directory, source_files, onnx)
    actual, actual_initial = base.catalog(candidate, candidate_directory, candidate_files, onnx)
    require(not any(n.startswith(NS) or r[1] for n, r in source.items()), 'unsupported original domain/namespace')
    c = config
    layers, heads, kv, width, hidden = (c[x] for x in ('num_hidden_layers', 'num_attention_heads', 'num_key_value_heads', 'head_dim', 'hidden_size'))
    require(0 < layers <= 36 and heads > 0 and kv > 0 and heads % kv == 0 and width > 0, 'geometry')
    expected_inputs = ['input_ids', 'attention_mask', 'position_ids'] + ([] if focus else ['decision_positions', 'candidate_positions', 'option_mask'])
    require([v.name for v in original.graph.input] == expected_inputs, 'original ABI input catalog')
    require([v.name for v in original.graph.output] == (['logits'] if focus else ['scores']), 'original ABI output catalog')
    require([v.SerializeToString() for v in original.graph.input] == [v.SerializeToString() for v in candidate.graph.input] and [v.SerializeToString() for v in original.graph.output] == [v.SerializeToString() for v in candidate.graph.output], 'ABI changed')
    for x in original.graph.input:
        require(x.type.tensor_type.elem_type == (9 if x.name == 'option_mask' else 7), 'original ABI input type')
    require(all(x.type.tensor_type.elem_type == 1 for x in original.graph.output), 'original output precision')
    def imports(model):
        values = {x.domain: x.version for x in model.opset_import}
        require(len(values) == len(model.opset_import), 'duplicate opset')
        return values
    require(original.ir_version == candidate.ir_version, 'IR version changed')
    source_header, actual_header = copy.deepcopy(original), copy.deepcopy(candidate)
    for model in (source_header, actual_header):
        model.ClearField('graph'); model.ClearField('opset_import')
    require(source_header.SerializeToString(deterministic=True) == actual_header.SerializeToString(deterministic=True), 'model header semantics changed')
    source_opsets = imports(original)
    require(set(source_opsets) == {''}, 'unsupported original opset domain')
    require(imports(candidate) == {**source_opsets, 'com.microsoft': 1}, 'opset mutation')
    consumers = collections.defaultdict(list)
    for name, record in source.items():
        for index, value in enumerate(record[2]):
            consumers[value].append((name, index))
    prefix = '/base/' if focus else '/scorer/'
    param_prefix = 'base.' if focus else 'scorer.text.'
    embedding = param_prefix + 'embed_tokens.weight'
    expected_roles = {embedding: ('half', (c['vocab_size'], hidden)), param_prefix + 'norm.weight': ('protected', (hidden,))}
    projection_nodes = {}
    for layer in range(layers):
        stem = f'layers.{layer}.'
        for label, shape in [('input_layernorm', (hidden,)), ('post_attention_layernorm', (hidden,)), ('self_attn.q_norm', (width,)), ('self_attn.k_norm', (width,))]:
            expected_roles[param_prefix + stem + label + '.weight'] = ('protected', shape)
        dimensions = {'self_attn/q_proj': (hidden, heads * width), 'self_attn/k_proj': (hidden, kv * width), 'self_attn/v_proj': (hidden, kv * width), 'self_attn/o_proj': (heads * width, hidden), 'mlp/gate_proj': (hidden, c['intermediate_size']), 'mlp/up_proj': (hidden, c['intermediate_size']), 'mlp/down_proj': (c['intermediate_size'], hidden)}
        for label, shape in dimensions.items():
            route = prefix + f'layers.{layer}/' + label + '/MatMul'
            require(route in source and source[route][0:2] == ('MatMul', '') and len(source[route][2]) == 2 and len(source[route][3]) == 1 and not source[route][4], 'original projection role')
            weight = source[route][2][1]
            require(weight not in expected_roles, 'aliased learned projection')
            expected_roles[weight] = ('half', shape)
            projection_nodes[weight] = route
    if focus:
        require('/MatMul' in source and source['/MatMul'][0:2] == ('MatMul', '') and len(source['/MatMul'][2]) == 2, 'original Focus head')
        head = source['/MatMul'][2][1]
        require(head not in expected_roles, 'aliased readout')
        expected_roles[head] = ('protected', (hidden, 2))
        projection_nodes[head] = '/MatMul'
    require(set(initial) == set(expected_roles), 'original learned tensor catalog')
    for name, (role, shape) in expected_roles.items():
        tensor = initial[name]
        base.storage_form(tensor)
        require(tensor.data_type == 1 and tuple(tensor.dims) == shape, 'original learned dtype/shape')
        uses = consumers[name]
        require(uses, 'unused original learned tensor')
        if name in projection_nodes:
            require(uses == [(projection_nodes[name], 1)], 'projection consumer/alias')
        elif name == embedding:
            require(all(source[n][0] == 'Gather' and index == 0 for n, index in uses), 'embedding consumer')
        else:
            require(all(source[n][0] == 'Mul' for n, _ in uses), 'norm consumer')
    expected = copy.deepcopy(source)
    constants = {}
    def add(op, ins, name, outputs=None, domain='', **attrs):
        name = NS + name
        require(name not in expected, 'policy identity collision')
        value = onnx.helper.make_node(op, ins, outputs or [name], name=name, domain=domain, **attrs)
        expected[name] = base.node_record(value, '.', {}, onnx)
        return (outputs or [name])[0]
    def const(label, dtype, shape, values):
        name = NS + label
        constants[name] = base.tensor_bytes(onnx.helper.make_tensor(name, dtype, shape, values), '.', {}, onnx)
        return name
    axis = const('axis1', 7, [1], [1]); one = const('one', 7, [1], [1]); last_axis = const('axis2', 7, [1], [2])
    zero = const('float_zero', 1, [], [0.0])
    shape = add('Shape', ['attention_mask'], 'input_shape')
    length = add('Gather', [shape, axis], 'T', axis=0)
    length32 = add('Cast', [length], 'T32', to=6)
    if focus:
        mask32 = add('Cast', ['attention_mask'], 'mask32', to=6)
        live = add('ReduceSum', [mask32, axis], 'live', keepdims=0)
        pad = add('Sub', [length32, live], 'pad')
        packed = add('Concat', [live, pad, length32, pad, length32], 'packed_mask', axis=0)
        live_bool = add('Cast', ['attention_mask'], 'maskbool', to=9)
        live_queries = add('Unsqueeze', [live_bool, last_axis], 'live_queries')
    else:
        last = add('Sub', [length, one], 'Tminus1')
        seqlens = add('Cast', [last], 'seqlens32', to=6)
    for weight, (role, _) in expected_roles.items():
        if role != 'half':
            continue
        for name, index in consumers[weight]:
            op, domain, ins, outs, attrs = expected[name]
            require(len(outs) == 1, 'unsupported learned consumer output')
            ins = list(ins)
            if op == 'MatMul':
                # Weight-only boundary: the stored half weight is cast once. Activation and product stay FP32.
                require(index == 1, 'unsupported projection operand')
                ins[1] = add('Cast', [weight], 'weight/' + name + '/weight32', to=1)
                expected[name] = (op, domain, tuple(ins), outs, attrs)
            else:
                require(op == 'Gather' and index == 0, 'unsupported half consumer')
                half_out = NS + 'weight/' + name + '/output16'
                expected[name] = (op, domain, tuple(ins), (half_out,), attrs)
                add('Cast', [half_out], 'weight/' + name + '/output32', outputs=list(outs), to=1)
    for layer in range(layers):
        root = prefix + f'layers.{layer}/self_attn/'
        target = f'attention/{layer}/'
        sink = root + ('Reshape_5' if focus else 'Reshape_7')
        require(sink in source and source[sink][0] == 'Reshape' and len(source[sink][3]) == 1, 'original attention sink')
        # The authenticated original supplies its complete protected math. Scale
        # additionally must match head width; no candidate value is consulted.
        scale_node = root + ('Mul_6' if focus else 'Mul_8')
        require(scale_node in source and source[scale_node][0] == 'Mul' and len(source[scale_node][2]) == 2, 'original scale')
        scale_input = source[scale_node][2][1]
        scale_producers = [r for r in source.values() if scale_input in r[3]]
        require(len(scale_producers) == 1 and scale_producers[0][0] == 'Constant', 'literal original scale')
        attr = scale_producers[0][4].get('value')
        require(attr and attr[0] == 'tensor' and attr[1][0] == 1 and len(attr[1][2]) == 4, 'original scale type')
        scale = struct.unpack('<f', attr[1][2])[0]
        require(struct.pack('<f', scale) == struct.pack('<f', 1 / math.sqrt(width)), 'original head scale')
        q = add('Transpose', [root + 'Add_output_0'], target + 'q_btnh', perm=[0, 2, 1, 3])
        k = add('Transpose', [root + 'Add_1_output_0'], target + 'k_btkh', perm=[0, 2, 1, 3])
        v = root + 'v_proj/MatMul_output_0'
        if focus:
            kv_shape = const(target + 'kv_shape', 7, [4], [0, 0, kv, width])
            indices = const(target + 'kv_indices', 7, [heads], [j // (heads // kv) for j in range(heads)])
            k = add('Gather', [k, indices], target + 'k_repeated', axis=2)
            v = add('Reshape', [v, kv_shape], target + 'v_btkh')
            v = add('Gather', [v, indices], target + 'v_repeated', axis=2)
        q_shape = const(target + 'q_shape', 7, [3], [0, 0, heads * width])
        k_shape = const(target + 'k_shape', 7, [3], [0, 0, (heads if focus else kv) * width])
        q = add('Reshape', [q, q_shape], target + 'q_flat')
        k = add('Reshape', [k, k_shape], target + 'k_flat')
        if focus:
            v = add('Reshape', [v, q_shape], target + 'v_flat')
        # FP32 operands. The FP32 result takes the original sink value without a cast.
        result = list(source[sink][3])
        del expected[sink]
        if focus:
            attention = add('MultiHeadAttention', [q, k, v, '', packed], target + 'efficient', outputs=[NS + target + 'raw32'], domain='com.microsoft', num_heads=heads, unidirectional=1, scale=scale)
            add('Where', [live_queries, attention, zero], target + 'zero_padded_queries', outputs=result)
        else:
            # Slots 1 and 2 are the private present values. They have no consumer.
            add(
                'GroupQueryAttention', [q, k, v, '', '', seqlens, length32], target + 'efficient',
                outputs=result + [NS + target + 'present_key', NS + target + 'present_value'], domain='com.microsoft',
                num_heads=heads, kv_num_heads=kv, causal=1, local_window_size=-1, do_rotary=0, scale=scale,
            )
    outputs = [v.name for v in original.graph.output]
    keep = base.reachable(expected, outputs, set(expected_inputs) | set(initial) | set(constants))
    expected = {name: record for name, record in expected.items() if name in keep}
    require(not any(r[0] == 'Softmax' for r in expected.values()), 'dense attention survived supported rewrite')
    precision_census(candidate, [name for name, (role, _) in expected_roles.items() if role == 'half'], embedding, layers, focus)
    changed = sorted(name for name in set(actual) | set(expected) if actual.get(name) != expected.get(name))
    require(not changed, 'complete candidate node/attribute/edge DAG differs: ' + ', '.join(changed[:8]))
    require(base.reachable(actual, outputs, set(expected_inputs) | set(actual_initial)) == set(actual), 'unreachable candidate node')
    require(set(actual_initial) == set(initial) | set(constants), 'extra/missing candidate tensor')
    for name, (role, shape) in expected_roles.items():
        value = actual_initial[name]
        base.storage_form(value)
        require(value.data_type == (10 if role == 'half' else 1) and tuple(value.dims) == shape, 'learned candidate dtype/shape')
    for name, expected_tensor in constants.items():
        require(base.tensor_bytes(actual_initial[name], candidate_directory, candidate_files, onnx) == expected_tensor, 'mask/layout/zero Constant differs')
    # ValueInfo must not add a conflicting dtype or a new semantic value. Source
    # declarations whose original value disappears may only be removed.
    retained = {v for record in expected.values() for v in record[3]}
    allowed_info = {v.name: v.SerializeToString() for v in original.graph.value_info if v.name in retained}
    actual_info = {v.name: v.SerializeToString() for v in candidate.graph.value_info}
    require(len(actual_info) == len(candidate.graph.value_info) and actual_info == allowed_info, 'intermediate dtype declarations differ')
    return {'nodes': len(actual), 'layers': layers, 'all_output_reachable_edges_checked': True, 'recipe': 'portable', 'arithmetic': 'float32', 'qualification': False}

def verify_portable_files(reference, graph_path, candidate_sha256, original_path, source_files, candidate_files, onnx):
    key = 'focus' if reference.focus else ('compact' if reference.layers == 28 and reference.hidden == 2048 else 'large' if reference.layers == 36 and reference.hidden == 2560 else None)
    require(key in base.ORIGINAL, 'unsupported selected geometry')
    require((reference.layers, reference.qheads, reference.kvheads, reference.dim, reference.hidden) == GEOMETRY[key], 'selected geometry differs')
    original, candidate = base.authenticated_models(original_path, base.ORIGINAL[key], graph_path, candidate_sha256, source_files, onnx)
    proof = portable_invariants(original, candidate, reference.config, reference.focus, onnx, pathlib.Path(original_path).parent, source_files, pathlib.Path(graph_path).parent, candidate_files)
    base.authenticated_models(original_path, base.ORIGINAL[key], graph_path, candidate_sha256, source_files, onnx)
    return proof
