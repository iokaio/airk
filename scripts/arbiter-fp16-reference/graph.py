# SPDX-License-Identifier: Apache-2.0
"""Independent bounded original-to-half DAG proof; no exporter import/inference."""
import collections
import copy
import hashlib
import math
import pathlib
import struct

ORIGINAL = {
    'focus': '19b20ad4e39e173f6da602094b9c1583726a65fa76742553f97ec6f0f58436a1',
    'compact': 'c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4',
    'large': 'f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94',
}
NS = '/airk_g5/'

def require(condition, message):
    if not condition:
        raise ValueError('independent G5 graph: ' + message)

def regular(path):
    path = pathlib.Path(path)
    require(path.is_file() and not path.is_symlink() and not path.is_junction(), 'aliased/missing file')
    return path

def file_hash(path):
    digest = hashlib.sha256()
    with regular(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(4 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()

def storage_form(tensor):
    fields = {field.name for field, _ in tensor.ListFields()}
    payloads = fields & {'raw_data', 'float_data', 'int32_data', 'int64_data', 'double_data', 'uint64_data', 'string_data'}
    require(fields <= {'dims', 'data_type', 'name', 'doc_string', 'data_location', 'external_data'} | payloads, 'unknown tensor semantics')
    if tensor.data_location == 1:
        require(not payloads and tensor.external_data, 'ambiguous external tensor')
    else:
        require(tensor.data_location == 0 and not tensor.external_data and len(payloads) == 1, 'ambiguous inline tensor')

def tensor_bytes(tensor, directory, files, onnx):
    """Normalize storage only, never dtype or values. Constants are bounded."""
    storage_form(tensor)
    value = copy.deepcopy(tensor)
    if tensor.data_location == 1:
        entries = {x.key: x.value for x in tensor.external_data}
        require(len(entries) == len(tensor.external_data) and set(entries) <= {'location', 'offset', 'length'}, 'Constant external fields')
        name = entries.get('location', '')
        require(name in files and name == pathlib.Path(name).name and ':' not in name and '\\' not in name, 'Constant outside sealed closure')
        path = regular(pathlib.Path(directory) / name)
        start, size = int(entries.get('offset', '0')), int(entries.get('length', str(path.stat().st_size)))
        require(start >= 0 and 0 < size <= 4 << 20 and start + size <= path.stat().st_size, 'Constant range/cap')
        with path.open('rb') as stream:
            stream.seek(start)
            raw = stream.read(size)
        require(len(raw) == size, 'short Constant')
        value.ClearField('external_data')
        value.data_location = 0
        value.raw_data = raw
    require(not value.external_data and value.data_location == 0, 'ambiguous Constant storage')
    data = onnx.numpy_helper.to_array(value).tobytes(order='C')
    require(len(data) <= 4 << 20, 'Constant capacity')
    return (value.data_type, tuple(value.dims), data)

def node_record(node, directory, files, onnx):
    allowed = {'input', 'output', 'name', 'op_type', 'domain', 'attribute', 'doc_string'}
    require(all(field.name in allowed for field, _ in node.ListFields()), 'unknown node semantics')
    attrs = {}
    for attribute in node.attribute:
        require(attribute.name not in attrs, 'duplicate node attribute')
        require(attribute.type not in (5, 10), 'subgraph is unsupported')
        if attribute.type == 4:
            attrs[attribute.name] = ('tensor', tensor_bytes(attribute.t, directory, files, onnx))
        elif attribute.type == 9:
            attrs[attribute.name] = ('tensors', tuple(tensor_bytes(x, directory, files, onnx) for x in attribute.tensors))
        else:
            attrs[attribute.name] = attribute.SerializeToString(deterministic=True)
    return (node.op_type, node.domain, tuple(node.input), tuple(node.output), attrs)

def catalog(model, directory, files, onnx):
    require(not model.functions and not model.training_info and not model.graph.sparse_initializer, 'functions/training/sparse graph')
    require(all(f.name in {'node', 'name', 'initializer', 'input', 'output', 'value_info', 'doc_string'} for f, _ in model.graph.ListFields()), 'unknown graph field')
    require(0 < len(model.graph.node) <= 65536 and len(model.graph.initializer) <= 4096, 'graph capacity')
    nodes, producers = {}, {}
    for node in model.graph.node:
        require(node.name and node.name not in nodes, 'missing/duplicate node identity')
        record = node_record(node, directory, files, onnx)
        require(record[3] and all(record[3]), 'empty node outputs')
        nodes[node.name] = record
        for output in record[3]:
            require(output not in producers, 'duplicate tensor producer')
            producers[output] = node.name
    initial = {x.name: x for x in model.graph.initializer}
    inputs = [x.name for x in model.graph.input]
    require(len(initial) == len(model.graph.initializer) and len(set(inputs)) == len(inputs), 'duplicate initializer/input')
    require(not (set(initial) & set(inputs)) and not ((set(initial) | set(inputs)) & set(producers)), 'ambiguous graph source')
    return nodes, initial

def reachable(nodes, outputs, known):
    owners = {value: name for name, record in nodes.items() for value in record[3]}
    seen, active, result = set(), set(), set()
    # Explicit DFS catches cycles, including candidate control/data cycles.
    stack = [(value, False) for value in outputs]
    while stack:
        value, done = stack.pop()
        if not value or value in known or value in seen:
            continue
        require(value in owners, 'unknown graph input edge ' + value)
        name = owners[value]
        if done:
            active.remove(name)
            result.add(name)
            seen.update(nodes[name][3])
            continue
        require(name not in active, 'cyclic graph')
        active.add(name)
        stack.append((value, True))
        stack.extend((item, False) for item in reversed(nodes[name][2]))
    return result

def graph_invariants(original, candidate, config, focus, onnx, original_directory='.', source_files=None, candidate_directory='.', candidate_files=None):
    """Pure metadata gate. Caller authenticates original and publisher config.

    Expectations originate only in original semantics plus this explicit policy.
    No candidate node/Constant supplies an expected edge or value.
    """
    source_files, candidate_files = source_files or {}, candidate_files or {}
    source, initial = catalog(original, original_directory, source_files, onnx)
    actual, actual_initial = catalog(candidate, candidate_directory, candidate_files, onnx)
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
    expected_roles = {param_prefix + 'embed_tokens.weight': ('half', (c['vocab_size'], hidden)), param_prefix + 'norm.weight': ('protected', (hidden,))}
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
        storage_form(tensor)
        require(tensor.data_type == 1 and tuple(tensor.dims) == shape, 'original learned dtype/shape')
        uses = consumers[name]
        require(uses, 'unused original learned tensor')
        if name in projection_nodes:
            require(uses == [(projection_nodes[name], 1)], 'projection consumer/alias')
        elif name.endswith('embed_tokens.weight'):
            require(all(source[n][0] == 'Gather' and index == 0 for n, index in uses), 'embedding consumer')
        else:
            require(all(source[n][0] == 'Mul' for n, _ in uses), 'norm consumer')
    expected = copy.deepcopy(source)
    constants = {}
    def add(op, ins, name, outputs=None, domain='', **attrs):
        name = NS + name
        require(name not in expected, 'policy identity collision')
        value = onnx.helper.make_node(op, ins, outputs or [name], name=name, domain=domain, **attrs)
        expected[name] = node_record(value, '.', {}, onnx)
        return (outputs or [name])[0]
    def const(label, dtype, shape, values):
        name = NS + label
        constants[name] = tensor_bytes(onnx.helper.make_tensor(name, dtype, shape, values), '.', {}, onnx)
        return name
    axis = const('axis1', 7, [1], [1]); one = const('one', 7, [1], [1]); last_axis = const('axis2', 7, [1], [2])
    zero = const('half_zero', 10, [], [0.0])
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
                ins[0] = add('Cast', [ins[0]], 'weight/' + name + '/input16', to=10)
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
        q = add('Cast', [q], target + 'q16', to=10)
        k = add('Cast', [k], target + 'k16', to=10)
        v = add('Cast', [v], target + 'v16', to=10)
        if focus:
            attention = add('MultiHeadAttention', [q, k, v, '', packed], target + 'efficient', outputs=[NS + target + 'raw16'], domain='com.microsoft', num_heads=heads, unidirectional=1, scale=scale)
            attention = add('Where', [live_queries, attention, zero], target + 'zero_padded_queries')
        else:
            # ORT's CUDA GQA requires present outputs even without past inputs.
            outputs = [NS + target + label for label in ('raw16', 'present_key', 'present_value')]
            attention = add(
                'GroupQueryAttention', [q, k, v, '', '', seqlens, length32], target + 'efficient',
                outputs=outputs, domain='com.microsoft', num_heads=heads, kv_num_heads=kv,
                causal=1, local_window_size=-1, do_rotary=0, scale=scale,
            )
        del expected[sink]
        add('Cast', [attention], target + 'output32', outputs=list(source[sink][3]), to=1)
    outputs = [v.name for v in original.graph.output]
    keep = reachable(expected, outputs, set(expected_inputs) | set(initial) | set(constants))
    expected = {name: record for name, record in expected.items() if name in keep}
    require(not any(r[0] == 'Softmax' for r in expected.values()), 'dense attention survived supported rewrite')
    changed = sorted(name for name in set(actual) | set(expected) if actual.get(name) != expected.get(name))
    require(not changed, 'complete candidate node/attribute/edge DAG differs: ' + ', '.join(changed[:8]))
    require(reachable(actual, outputs, set(expected_inputs) | set(actual_initial)) == set(actual), 'unreachable candidate node')
    require(set(actual_initial) == set(initial) | set(constants), 'extra/missing candidate tensor')
    for name, (role, shape) in expected_roles.items():
        value = actual_initial[name]
        storage_form(value)
        require(value.data_type == (10 if role == 'half' else 1) and tuple(value.dims) == shape, 'learned candidate dtype/shape')
    for name, expected_tensor in constants.items():
        require(tensor_bytes(actual_initial[name], candidate_directory, candidate_files, onnx) == expected_tensor, 'mask/layout/zero Constant differs')
    # ValueInfo must not add a conflicting dtype or a new semantic value. Source
    # declarations whose original value disappears may only be removed.
    retained = {v for record in expected.values() for v in record[3]}
    allowed_info = {v.name: v.SerializeToString() for v in original.graph.value_info if v.name in retained}
    actual_info = {v.name: v.SerializeToString() for v in candidate.graph.value_info}
    require(len(actual_info) == len(candidate.graph.value_info) and actual_info == allowed_info, 'intermediate dtype declarations differ')
    return {'nodes': len(actual), 'layers': layers, 'all_output_reachable_edges_checked': True, 'qualification': False}

def authenticated_models(original_path, original_sha256, graph_path, candidate_sha256, source_files, onnx):
    original_path, graph_path = regular(original_path), regular(graph_path)
    require(0 < original_path.stat().st_size <= 64 << 20 and 0 < graph_path.stat().st_size <= 64 << 20, 'graph byte capacity')
    require(file_hash(original_path) == original_sha256 and file_hash(graph_path) == candidate_sha256, 'graph identity')
    original = onnx.load_model(str(original_path), load_external_data=False)
    candidate = onnx.load_model(str(graph_path), load_external_data=False)
    external = set()
    for tensor in list(original.graph.initializer) + [a.t for n in original.graph.node for a in n.attribute if a.type == 4]:
        if tensor.data_location == 1:
            entries = {x.key: x.value for x in tensor.external_data}
            require(len(entries) == len(tensor.external_data), 'original external duplicate')
            external.add(entries.get('location', ''))
    require(set(source_files) == external, 'original external closure missing/extra')
    for name, pin in source_files.items():
        require(name == pathlib.Path(name).name and ':' not in name and '\\' not in name and name not in ('', '.', '..'), 'unsafe source file')
        path = regular(original_path.parent / name)
        require(set(pin) == {'bytes', 'sha256'} and type(pin['bytes']) is int and path.stat().st_size == pin['bytes'] and file_hash(path) == pin['sha256'], 'original source file drift')
    require(file_hash(original_path) == original_sha256 and file_hash(graph_path) == candidate_sha256, 'graph changed')
    return original, candidate

def verify_graph_files(reference, graph_path, candidate_sha256, original_path, source_files, candidate_files, onnx):
    key = 'focus' if reference.focus else ('compact' if reference.layers == 28 and reference.hidden == 2048 else 'large' if reference.layers == 36 and reference.hidden == 2560 else None)
    require(key in ORIGINAL, 'unsupported selected geometry')
    require((reference.layers, reference.qheads, reference.kvheads, reference.dim, reference.hidden) == {'focus': (28, 16, 8, 128, 1024), 'compact': (28, 16, 8, 128, 2048), 'large': (36, 32, 8, 128, 2560)}[key], 'selected geometry differs')
    original, candidate = authenticated_models(original_path, ORIGINAL[key], graph_path, candidate_sha256, source_files, onnx)
    proof = graph_invariants(original, candidate, reference.config, reference.focus, onnx, pathlib.Path(original_path).parent, source_files, pathlib.Path(graph_path).parent, candidate_files)
    authenticated_models(original_path, ORIGINAL[key], graph_path, candidate_sha256, source_files, onnx)
    return proof