# SPDX-License-Identifier: Apache-2.0
"""Mechanical adapted Focus FP32 storage export. No Torch, ORT, G5 or model execution.
The caller owns actual execution identity, output/process custody and one deadline.
"""
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import struct
import time

ORIGINAL_GRAPH = '19b20ad4e39e173f6da602094b9c1583726a65fa76742553f97ec6f0f58436a1'
PUBLISHER = '27cd75a405b9c1b46b59abfd88aaa209e6fed2a1972cde9b70e7659537c5e65b'
CHUNK = 4 << 20
MATRIX_LIMIT = 16 << 20
PROJECTIONS = ('q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj')
READOUT = (2152, 9693)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def in_time(deadline):
    require(math.isfinite(deadline) and time.monotonic() < deadline, 'original export deadline expired')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def decode(raw):
    return json.loads(raw, object_pairs_hook=unique_object,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite JSON')))


def ordinary(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path, 'canonical absolute source required')
    for item in (path, *path.parents):
        meta = item.lstat()
        require(not item.is_symlink() and not getattr(meta, 'st_file_attributes', 0) & 0x400,
                'source/output ancestor reparse refused')
    require(path.is_file() and path.stat().st_nlink == 1, 'single-link regular source required')
    return path


def signature(meta):
    return (meta.st_dev, meta.st_ino, meta.st_size, meta.st_mtime_ns, meta.st_ctime_ns)


class Sources:
    """Held explicit inputs with streamed before/after hashes; no execution authority."""
    def __init__(self, pins, deadline):
        require(isinstance(pins, dict) and 0 < len(pins) <= 4096, 'bounded explicit source pins required')
        self.pins, self.deadline, self.opened = {}, deadline, {}
        identities = set()
        for name, value in pins.items():
            in_time(deadline)
            require(isinstance(name, str) and isinstance(value, str)
                    and re.fullmatch('[0-9a-f]{64}', value) and value != '0' * 64, 'invalid source pin')
            normalized = str(ordinary(name))
            identity = os.path.normcase(normalized)
            require(identity not in identities, 'duplicate/aliased source pin')
            identities.add(identity)
            self.pins[normalized] = value

    def hold(self, path):
        in_time(self.deadline)
        path = ordinary(path)
        key = str(path)
        require(key in self.pins, 'unsealed source: ' + key)
        if key not in self.opened:
            handle = path.open('rb')
            try:
                state = signature(os.fstat(handle.fileno()))
                require(state == signature(path.stat()), 'source replaced during open')
                require(0 < state[2] <= 8 << 30, 'source size bound')
                self.opened[key] = (handle, state)
                self.check(key)
            except BaseException:
                handle.close()
                self.opened.pop(key, None)
                raise
        return self.opened[key][0]

    def check(self, key):
        handle, state = self.opened[key]
        require(signature(os.fstat(handle.fileno())) == state
                and signature(ordinary(key).stat()) == state, 'held/named source changed')
        handle.seek(0)
        h = hashlib.sha256()
        while raw := handle.read(CHUNK):
            in_time(self.deadline)
            h.update(raw)
        require(h.hexdigest() == self.pins[key], 'source hash differs: ' + key)
        require(signature(os.fstat(handle.fileno())) == state
                and signature(Path(key).stat()) == state, 'source changed during hash')
        in_time(self.deadline)

    def read(self, path, maximum):
        handle = self.hold(path)
        require(os.fstat(handle.fileno()).st_size <= maximum, 'bounded metadata input exceeded')
        handle.seek(0)
        raw = handle.read(maximum + 1)
        in_time(self.deadline)
        require(len(raw) <= maximum, 'metadata input exceeded')
        return raw

    def range(self, path, offset, length):
        require(type(offset) is int and type(length) is int and offset >= 0 and length >= 0,
                'invalid input byte range')
        handle = self.hold(path)
        require(offset + length <= os.fstat(handle.fileno()).st_size, 'source range exceeds file')
        handle.seek(offset)
        while length:
            in_time(self.deadline)
            raw = handle.read(min(CHUNK, length))
            require(raw, 'source ended early')
            length -= len(raw)
            yield raw

    def verify(self):
        for key in sorted(self.opened):
            self.check(key)
        in_time(self.deadline)

    def close(self):
        for handle, _ in self.opened.values():
            handle.close()


def shapes():
    result = {'model.embed_tokens.weight': [151669, 1024], 'model.norm.weight': [1024]}
    sizes = {'q_proj': [2048, 1024], 'k_proj': [1024, 1024], 'v_proj': [1024, 1024],
             'o_proj': [1024, 2048], 'gate_proj': [3072, 1024],
             'up_proj': [3072, 1024], 'down_proj': [1024, 3072]}
    for layer in range(28):
        prefix = f'model.layers.{layer}.'
        for name, size in sizes.items():
            section = 'self_attn' if name in PROJECTIONS[:4] else 'mlp'
            result[prefix + section + '.' + name + '.weight'] = size
        for name, width in [('self_attn.q_norm', 128), ('self_attn.k_norm', 128),
                            ('input_layernorm', 1024), ('post_attention_layernorm', 1024)]:
            result[prefix + name + '.weight'] = [width]
    return result


def target_keys():
    return {f'model.layers.{i}.self_attn.{name}.weight' for i in range(28) for name in PROJECTIONS[:4]}


def header_rows(header, data_bytes, expected, dtype):
    require(isinstance(header, dict) and header.get('__metadata__') == {'format': 'pt'}, 'checkpoint metadata differs')
    rows = {k: v for k, v in header.items() if k != '__metadata__'}
    require(set(rows) == set(expected), 'checkpoint key roster differs; aliases/LoRA/extra/missing keys refuse')
    ranges = []
    for key, row in rows.items():
        require(isinstance(row, dict) and set(row) == {'dtype', 'shape', 'data_offsets'}, 'unknown tensor header')
        require(row['dtype'] == dtype and row['shape'] == expected[key]
                and all(type(x) is int and x > 0 for x in row['shape']), 'checkpoint dtype/shape differs')
        offsets = row['data_offsets']
        require(isinstance(offsets, list) and len(offsets) == 2
                and all(type(x) is int for x in offsets), 'invalid checkpoint offsets')
        start, end = offsets
        require(0 <= start < end <= data_bytes and end - start == math.prod(row['shape']) * (2 if dtype == 'BF16' else 4),
                'checkpoint range length differs')
        ranges.append((start, end))
    cursor = 0
    for start, end in sorted(ranges):
        require(start == cursor, 'checkpoint overlap/alias/hole refused')
        cursor = end
    require(cursor == data_bytes, 'checkpoint trailing/unbound bytes refused')
    return rows


def canonical(raw, dtype, np):
    width = 2 if dtype == 'BF16' else 4
    require(dtype in ('BF16', 'F32') and len(raw) % width == 0, 'unaligned checkpoint payload')
    if dtype == 'BF16':
        bits = np.frombuffer(raw, dtype='<u2').astype('<u4') << 16
    else:
        bits = np.frombuffer(raw, dtype='<u4')
    require(not bool(((bits & 0x7f800000) == 0x7f800000).any()), 'nonfinite learned tensor')
    return bits.astype('<u4', copy=False).tobytes()


class Checkpoint:
    def __init__(self, path, source, dtype, expected):
        self.path, self.source, self.dtype = Path(path), source, dtype
        handle = source.hold(path)
        handle.seek(0)
        prefix = handle.read(8)
        require(len(prefix) == 8, 'short checkpoint')
        length = struct.unpack('<Q', prefix)[0]
        require(0 < length <= 4 << 20, 'checkpoint header bound')
        raw = handle.read(length)
        require(len(raw) == length, 'short checkpoint header')
        self.start = length + 8
        self.rows = header_rows(decode(raw), os.fstat(handle.fileno()).st_size - self.start, expected, dtype)

    def chunks(self, key, np):
        start, end = self.rows[key]['data_offsets']
        # BF16 input chunks are half the FP32 chunk size, preserving chunk equality.
        width = 2 if self.dtype == 'BF16' else 4
        for offset in range(start, end, CHUNK // (4 // width)):
            count = min(CHUNK // (4 // width), end - offset)
            raw = b''.join(self.source.range(self.path, self.start + offset, count))
            yield canonical(raw, self.dtype, np)

    def matrix(self, key, np):
        require(math.prod(self.rows[key]['shape']) * 4 <= MATRIX_LIMIT, 'matrix transpose buffer exceeds16MiB')
        result = bytearray()
        for raw in self.chunks(key, np):
            result.extend(raw)
        return result

    def readout(self, np):
        start, _ = self.rows['model.embed_tokens.weight']['data_offsets']
        width = 2 if self.dtype == 'BF16' else 4
        rows = [canonical(b''.join(self.source.range(self.path, self.start + start + row * 1024 * width,
                                                    1024 * width)), self.dtype, np) for row in READOUT]
        return transpose(b''.join(rows), [2, 1024], np)


def transpose_chunks(raw, shape, np):
    require(len(shape) == 2 and len(raw) == math.prod(shape) * 4
            and len(raw) <= MATRIX_LIMIT, 'exact bounded matrix transpose required')
    for offset in range(0, len(raw), CHUNK):
        canonical(raw[offset:offset + CHUNK], 'F32', np)
    view = np.frombuffer(raw, dtype='<u4').reshape(shape).T
    rows = max(1, CHUNK // (shape[0] * 4))
    for start in range(0, shape[1], rows):
        yield view[start:start + rows].tobytes(order='C')


def transpose(raw, shape, np):
    # Tiny literal controls and the8KiB readout only. Production matrices stream.
    return b''.join(transpose_chunks(raw, shape, np))


def equal_chunks(left, right):
    left, right = iter(left), iter(right)
    a, b = memoryview(b''), memoryview(b'')
    ended_a = ended_b = False
    while True:
        while not a and not ended_a:
            raw = next(left, None)
            ended_a = raw is None
            a = memoryview(b'' if raw is None else raw)
        while not b and not ended_b:
            raw = next(right, None)
            ended_b = raw is None
            b = memoryview(b'' if raw is None else raw)
        if ended_a and ended_b:
            return
        require(not ended_a and not ended_b, 'canonical tensor length differs')
        count = min(len(a), len(b))
        require(a[:count] == b[:count], 'canonical tensor bytes differ')
        a, b = a[count:], b[count:]


def authenticate_checkpoints(original, merged, np):
    lineage = {}
    changed = 0
    for key in sorted(shapes()):
        a, b = hashlib.sha256(), hashlib.sha256()
        for raw in original.chunks(key, np):
            a.update(raw)
        for raw in merged.chunks(key, np):
            b.update(raw)
        if key not in target_keys():
            equal_chunks(original.chunks(key, np), merged.chunks(key, np))
        elif a.digest() != b.digest():
            changed += 1
        lineage[key] = {'publisher_canonical_fp32': a.hexdigest(), 'merged_fp32': b.hexdigest(),
                        'allowed_adaptation': key in target_keys()}
    require(changed > 0, 'merged checkpoint has no allowed learned change')
    return lineage


def graph_roles(model, expected):
    uses, result, mapped = {}, {}, set()
    for node in model.graph.node:
        for index, value in enumerate(node.input):
            uses.setdefault(value, []).append((node, index))
    for tensor in model.graph.initializer:
        require(tensor.name not in result and tensor.data_type == 1, 'duplicate/non-FP32 initializer')
        consumers = uses.get(tensor.name, [])
        if tensor.name == 'base.embed_tokens.weight':
            key, role = 'model.embed_tokens.weight', 'direct'
            require(consumers and all(n.op_type == 'Gather' and i == 0 for n, i in consumers), 'embedding use differs')
        elif tensor.name.startswith('base.') and tensor.name.endswith(('norm.weight', 'layernorm.weight')):
            key, role = 'model.' + tensor.name[5:], 'direct'
            require(consumers and all(n.op_type == 'Mul' for n, _ in consumers), 'norm use differs')
        else:
            require(len(consumers) == 1 and consumers[0][1] == 1, 'aliased/non-RHS learned projection')
            node = consumers[0][0]
            require(node.op_type == 'MatMul' and node.domain in ('', 'ai.onnx'), 'projection operator differs')
            if node.name == '/MatMul':
                key, role = 'readout_rows', 'readout'
            else:
                match = re.fullmatch(r'/base/layers\.(\d+)/(self_attn|mlp)/(\w+)/MatMul', node.name)
                require(match is not None, 'unknown learned initializer role')
                key, role = f'model.layers.{int(match[1])}.{match[2]}.{match[3]}.weight', 'transpose'
        require(key not in mapped, 'duplicate checkpoint-to-graph binding')
        require(key == 'readout_rows' or key in expected, 'unexpected checkpoint role')
        shape = [1024, 2] if role == 'readout' else (list(reversed(expected[key])) if role == 'transpose' else expected[key])
        require(list(tensor.dims) == shape, 'original initializer shape differs')
        mapped.add(key)
        result[tensor.name] = (key, role)
    require(mapped == set(expected) | {'readout_rows'} and len(result) == 311, 'incomplete311 initializer mapping')
    require(sum(key in target_keys() for key, _ in result.values()) == 112, 'incomplete112 target mapping')
    return result


def external_range(tensor, graph_path, source, item_bytes=4):
    info = {x.key: x.value for x in tensor.external_data}
    require(len(info) == len(tensor.external_data) and set(info) <= {'location', 'offset', 'length'}, 'unknown external storage')
    require(tensor.data_location == 1 and not tensor.raw_data and not tensor.float_data
            and not tensor.int32_data and not tensor.int64_data and not tensor.double_data
            and not tensor.uint64_data and not tensor.string_data, 'mixed external/inline storage')
    name = info.get('location', '')
    require(re.fullmatch(r'[A-Za-z0-9_.-]+', name) and name not in ('.', '..'), 'external path escape')
    require(all(type(d) is int and d >= 0 for d in tensor.dims), 'invalid external shape')
    expected = math.prod(tensor.dims) * item_bytes
    require(expected > 0, 'empty external tensor refused')
    for field in ('offset', 'length'):
        require(field not in info or re.fullmatch(r'0|[1-9][0-9]*', info[field]), 'invalid external byte range')
    path = Path(graph_path).parent / name
    handle = source.hold(path)
    offset, length = int(info.get('offset', '0')), int(info.get('length', str(expected)))
    require(length == expected and offset + length <= os.fstat(handle.fileno()).st_size, 'external payload length differs')
    return path, offset, length


def external_tensor_closure(model, graph_path, source, onnx):
    """Authenticate every external TensorProto, including nonfinite mask Constants.
    No value conversion/finite filter is applied to immutable attribute tensors.
    The closed original has no functions, sparse tensors or nested graph bodies.
    """
    require(not model.functions and not model.graph.sparse_initializer, 'functions/sparse graph unsupported')
    leaves = [('initializer:' + t.name, t) for t in model.graph.initializer]
    nodes = set()
    for node in model.graph.node:
        require(node.name and node.name not in nodes, 'duplicate/unnamed graph node')
        nodes.add(node.name)
        attributes = set()
        for attr in node.attribute:
            require(attr.name not in attributes, 'duplicate node attribute')
            attributes.add(attr.name)
            require(attr.type not in (onnx.AttributeProto.GRAPH, onnx.AttributeProto.GRAPHS,
                                     onnx.AttributeProto.SPARSE_TENSOR, onnx.AttributeProto.SPARSE_TENSORS),
                    'nested graph/sparse attribute unsupported')
            prefix = 'node:' + node.name + ':' + attr.name
            if attr.type == onnx.AttributeProto.TENSOR:
                require(attr.HasField('t'), 'missing tensor attribute')
                leaves.append((prefix, attr.t))
            elif attr.type == onnx.AttributeProto.TENSORS:
                leaves.extend((prefix + ':' + str(i), tensor) for i, tensor in enumerate(attr.tensors))
    rows, identifiers, ranges = [], set(), {}
    for identity, tensor in leaves:
        require(identity not in identifiers, 'duplicate tensor identity')
        identifiers.add(identity)
        if not tensor.external_data:
            require(tensor.data_location != 1, 'external tensor lacks storage declaration')
            continue
        dtype = onnx.helper.tensor_dtype_to_np_dtype(tensor.data_type)
        require(dtype.kind in 'bifu' and dtype.itemsize in (1, 2, 4, 8), 'unsupported external tensor dtype')
        path, offset, length = external_range(tensor, graph_path, source, dtype.itemsize)
        rows.append({'tensor': identity, 'name': path.name, 'offset': offset, 'bytes': length,
                     'data_type': tensor.data_type, 'dims': list(tensor.dims)})
        ranges.setdefault(path, []).append((offset, offset + length))
    for path, spans in ranges.items():
        cursor = 0
        for start, end in sorted(spans):
            require(start == cursor, 'external tensor overlap/alias/hole refused')
            cursor = end
        require(cursor == os.fstat(source.hold(path).fileno()).st_size, 'unbound external file bytes refused')
    return rows


def copy_unchanged_external(closure, changed_locations, graph_path, source, output, deadline):
    files = []
    for name in sorted({row['name'] for row in closure} - set(changed_locations)):
        path = Path(graph_path).parent / name
        length = os.fstat(source.hold(path).fileno()).st_size
        files.append(write_bytes(Path(output) / name, source.range(path, 0, length), deadline))
    return files


def tensor_chunks(tensor, graph_path, source):
    expected = math.prod(tensor.dims) * 4
    if tensor.external_data:
        path, offset, length = external_range(tensor, graph_path, source)
        yield from source.range(path, offset, length)
    else:
        require(len(tensor.raw_data) == expected and not tensor.float_data, 'unknown inline tensor storage')
        for offset in range(0, expected, CHUNK):
            yield bytes(tensor.raw_data[offset:offset + CHUNK])


def authenticate_graph(model, graph_path, source, original, np):
    roles = graph_roles(model, shapes())
    external = set()
    for tensor in model.graph.initializer:
        key, role = roles[tensor.name]
        if tensor.external_data:
            name = next(x.value for x in tensor.external_data if x.key == 'location')
            require(name not in external, 'shared original external initializer storage')
            external.add(name)
        if role == 'transpose':
            matrix = original.matrix(key, np)
            equal_chunks(tensor_chunks(tensor, graph_path, source), transpose_chunks(matrix, shapes()[key], np))
            del matrix
        elif role == 'readout':
            equal_chunks(tensor_chunks(tensor, graph_path, source), [original.readout(np)])
        else:
            equal_chunks(tensor_chunks(tensor, graph_path, source), original.chunks(key, np))
    return roles


def preserve_math(original, candidate, names):
    restored = copy.deepcopy(candidate)
    before = {x.name: x for x in original.graph.initializer}
    after = {x.name: x for x in restored.graph.initializer}
    require(set(before) == set(after), 'initializer roster changed')
    for name in names:
        require(name in before, 'unexpected adapted initializer')
        after[name].ClearField('external_data')
        after[name].external_data.extend(before[name].external_data)
    require(restored.SerializeToString() == original.SerializeToString(), 'graph math/ABI/non-target storage changed')


def validate_training_records(receipt, result, plan, custody, merged_path):
    require(receipt['schema'] == 'airk.private.F1-F2-owned-execution/1' and receipt['failure'] is None
            and receipt['final_hash_failures'] == [] and receipt['disposition']['passed'] is True
            and receipt['disposition']['naturally_settled'] is True, 'actual full TRAIN execution did not pass')
    owned = receipt['owned']
    for field, value in {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
                         'owned_descendants_settled': True, 'owned_job_forced_termination': False,
                         'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
                         'unexpected_third_process': False}.items():
        require(type(owned.get(field)) is type(value) and owned[field] == value, 'TRAIN process settlement differs')
    closed = owned['handle_close_proof']
    require(closed['settled'] is True and closed['forced'] is False
            and closed['final_active'] == 0 and closed['final_member_pids'] == [], 'TRAIN handle closure missing')
    require(result['schema'] == 'airk.private.F1-F2-worker-result/1' and result['mode'] == plan['mode'] == 'train'
            and result['complete'] is True and result['merged_full_checkpoint'] is True
            and receipt['worker_result_path'] == custody['train_result']['path']
            and receipt['worker_result_sha256'] == custody['train_result']['sha256']
            and result['plan_sha256'] == receipt['plan_sha256'] == custody['train_plan']['sha256'], 'TRAIN output/plan binding differs')
    updates = result['updates']
    require(type(plan['max_optimizer_steps']) is int and len(updates) == plan['max_optimizer_steps'] > 0 and all(type(r['step']) is int and type(r['draws']) is int and r['step'] == i + 1 and r['draws'] == len(r['ids']) > 0 for i, r in enumerate(updates)), 'TRAIN step/draw roster differs')
    draws = sum(row['draws'] for row in updates)
    require(all(type(result[key]) is int and result[key] == draws for key in ('forward_attempts', 'forward_calls', 'backward_attempts', 'backwards_completed'))
            and type(result['optimizer_attempts']) is int and type(result['gradient_steps']) is int
            and result['optimizer_attempts'] == result['gradient_steps'] == len(updates), 'TRAIN counters differ')
    require(set(result['lora_targets']) == {k[:-7] for k in target_keys()} and len(result['lora_targets']) == 112, 'TRAIN target roster differs')
    require(Path(plan['worker_output']) / 'merged-full-checkpoint' == Path(merged_path).parent, 'TRAIN output directory differs')
    require(receipt['qualification'] is False and receipt['promotion'] is False
            and result['qualification'] is False and result['promotion'] is False, 'TRAIN proof is not release qualification')


def training_custody(source, path, merged_path, original_config):
    custody = decode(source.read(path, 2 << 20))
    require(custody.get('schema') == 'airk.private.F2-trained-checkpoint-custody/1', 'unknown checkpoint custody')
    require(custody['exact_output_file_roster'] == ['config.json', 'generation_config.json', 'model.safetensors'], 'checkpoint output roster differs')
    require(Path(custody['checkpoint_root']) == Path(merged_path).parent, 'merged root differs from actual TRAIN custody')
    for name, row in custody['files'].items():
        require(name in custody['exact_output_file_roster'] and Path(row['path']) == Path(merged_path).parent / name,
                'checkpoint custody path differs')
        source.hold(row['path'])
        require(source.pins[row['path']] == row['sha256'] and Path(row['path']).stat().st_size == row['bytes'], 'checkpoint custody hash/size differs')
    require(set(p.name for p in Path(merged_path).parent.iterdir()) == set(custody['exact_output_file_roster']), 'checkpoint has extra files')
    for field in ('train_receipt', 'train_result', 'train_plan'):
        row = custody[field]
        require(source.pins.get(row['path']) == row['sha256'], 'TRAIN proof pin differs')
    receipt = decode(source.read(custody['train_receipt']['path'], 1 << 20))
    result = decode(source.read(custody['train_result']['path'], 1 << 20))
    plan = decode(source.read(custody['train_plan']['path'], 16 << 20))
    validate_training_records(receipt, result, plan, custody, merged_path)
    before, after = decode(source.read(original_config, 1 << 20)), decode(source.read(custody['files']['config.json']['path'], 1 << 20))
    fields = ('architectures', 'hidden_size', 'num_attention_heads', 'num_key_value_heads', 'head_dim', 'num_hidden_layers', 'intermediate_size', 'vocab_size', 'tie_word_embeddings')
    require(all(before.get(k) == after.get(k) for k in fields) and before.get('tie_word_embeddings') is True,
            'tied-head architecture differs')
    for row in custody['unchanged_original_tokenizer_inputs']:
        require(source.pins.get(row['path']) == row['sha256'], 'original tokenizer pin changed')
        source.hold(row['path'])
    return custody


def write_bytes(path, chunks, deadline):
    h, length = hashlib.sha256(), 0
    with Path(path).open('xb') as output:
        for raw in chunks:
            in_time(deadline)
            output.write(raw)
            h.update(raw)
            length += len(raw)
        output.flush()
        os.fsync(output.fileno())
    in_time(deadline)
    return {'name': Path(path).name, 'bytes': length, 'sha256': h.hexdigest()}


def export_after_owner_admission(graph_path, publisher_path, publisher_config, merged_path, custody_path,
                                 pins, output, onnx, np, deadline):
    source = Sources(pins, deadline)
    output = Path(output)
    created = False
    try:
        require(source.pins.get(str(graph_path)) == ORIGINAL_GRAPH and source.pins.get(str(publisher_path)) == PUBLISHER,
                'only exact original Focus graph/publisher accepted')
        raw = source.read(graph_path, 4 << 20)
        require(sha(raw) == ORIGINAL_GRAPH, 'original graph differs')
        original = onnx.load_model_from_string(raw)
        require(original.SerializeToString() == raw, 'original protobuf serialization differs')
        require([v.name for v in original.graph.input] == ['input_ids', 'attention_mask', 'position_ids']
                and all(v.type.tensor_type.elem_type == 7 and [d.dim_value for d in v.type.tensor_type.shape.dim] == [1, 512] for v in original.graph.input)
                and len(original.graph.output) == 1 and original.graph.output[0].name == 'logits'
                and original.graph.output[0].type.tensor_type.elem_type == 1
                and [d.dim_value for d in original.graph.output[0].type.tensor_type.shape.dim] == [1, 2], 'original static512 ABI differs')
        custody = training_custody(source, custody_path, merged_path, publisher_config)
        publisher = Checkpoint(publisher_path, source, 'BF16', shapes())
        merged = Checkpoint(merged_path, source, 'F32', shapes())
        lineage = authenticate_checkpoints(publisher, merged, np)
        roles = authenticate_graph(original, graph_path, source, publisher, np)
        closure = external_tensor_closure(original, graph_path, source, onnx)
        require(len({row['name'] for row in closure}) == 199, 'closed original199 external-file roster differs')
        changed_locations = {next(x.value for x in tensor.external_data if x.key == 'location')
                             for tensor in original.graph.initializer if roles[tensor.name][0] in target_keys()}
        require(output.is_absolute() and not output.exists() and output.parent.resolve(strict=True) == output.parent,
                'fresh canonical output under caller-owned parent required')
        for p in (output.parent, *output.parent.parents):
            require(not p.is_symlink() and not getattr(p.lstat(), 'st_file_attributes', 0) & 0x400, 'output ancestor alias')
        in_time(deadline)
        output.mkdir()
        created = True
        candidate = copy.deepcopy(original)
        files, mapping, changed_names = [], [], []
        packed = output / 'adapted-attention.fp32'
        offset = 0
        with packed.open('xb') as stream:
            for tensor in candidate.graph.initializer:
                key, role = roles[tensor.name]
                if key in target_keys():
                    matrix = merged.matrix(key, np)
                    matrix_hash, matrix_bytes = hashlib.sha256(), 0
                    for block in transpose_chunks(matrix, shapes()[key], np):
                        in_time(deadline)
                        stream.write(block)
                        matrix_hash.update(block)
                        matrix_bytes += len(block)
                    del matrix
                    require(matrix_bytes == math.prod(shapes()[key]) * 4, 'transposed payload size differs')
                    tensor.ClearField('external_data')
                    for name, value in [('location', packed.name), ('offset', str(offset)), ('length', str(matrix_bytes))]:
                        entry = tensor.external_data.add(); entry.key, entry.value = name, value
                    changed_names.append(tensor.name)
                    mapping.append({'initializer': tensor.name, 'checkpoint_key': key, 'offset': offset,
                                    'bytes': matrix_bytes, 'transposed_fp32_sha256': matrix_hash.hexdigest()})
                    offset += matrix_bytes
            stream.flush(); os.fsync(stream.fileno())
        require(len(changed_names) == 112 and offset == 704643072, 'adapted weight byte/count roster differs')
        preserve_math(original, candidate, changed_names)
        files.extend(copy_unchanged_external(closure, changed_locations, graph_path, source, output, deadline))
        require(len(files) == 87, 'unchanged external initializer/Constant file roster differs')
        files.append(write_bytes(output / 'model.onnx', [candidate.SerializeToString()], deadline))
        with packed.open('rb') as stream:
            def packed_chunks():
                while value := stream.read(CHUNK):
                    yield value
            packed_pin = write_hash(packed, packed_chunks(), deadline)
        files.append(packed_pin)
        source.verify()
        for row in files:
            verify_output(output / row['name'], row, deadline)
        result = {'schema': 'airk.private.adapted-focus-fp32-export/1', 'qualification': False, 'model_inference': False,
                  'original_graph_sha256': ORIGINAL_GRAPH, 'publisher_checkpoint_sha256': PUBLISHER,
                  'merged_checkpoint_sha256': source.pins[str(merged_path)], 'custody_sha256': source.pins[str(custody_path)],
                  'train_receipt': custody['train_receipt'], 'dtype': 'float32', 'abi': 'B1T512 threeInt64inputs twoFloat32logits',
                  'initializer_count': 311, 'replaced_attention_matrices': mapping, 'checkpoint_tensors': lineage,
                  'non_target_checkpoint_tensors_bit_equal': 198, 'preserved_non_target_graph_initializers': 199,
                  'graph_math_restored_byte_equal': True, 'external_tensor_closure': closure,
                  'preserved_unchanged_external_files': 87, 'files': files,
                  'consumed_source_pins': {k: source.pins[k] for k in sorted(source.opened)},
                  'scope': 'Mechanical unqualified weight lineage only; fresh independent adapted references/native admission remain required'}
        payload = (json.dumps(result, indent=2) + '\n').encode()
        write_bytes(output / 'export-manifest.private.json', [payload], deadline)
        in_time(deadline)
        return result
    except BaseException as error:
        post_error = None
        try:
            source.verify()
        except BaseException as check_error:
            post_error = {'type': type(check_error).__name__, 'message': str(check_error)}
        if created:
            try:
                with (output / 'export-failure.private.json').open('x', encoding='utf-8') as handle:
                    json.dump({'qualification': False, 'conversion_returned_success': False,
                               'error': {'type': type(error).__name__, 'message': str(error)},
                               'source_final_recheck_error': post_error}, handle)
                    handle.write('\n'); handle.flush(); os.fsync(handle.fileno())
            except BaseException as retention_error:
                error.add_note('failure retention: ' + str(retention_error))
        raise
    finally:
        source.close()


def write_hash(path, chunks, deadline):
    h, length = hashlib.sha256(), 0
    for raw in chunks:
        in_time(deadline)
        h.update(raw); length += len(raw)
    in_time(deadline)
    return {'name': Path(path).name, 'bytes': length, 'sha256': h.hexdigest()}


def verify_output(path, expected, deadline):
    path = ordinary(path)
    with path.open('rb') as stream:
        def chunks():
            while raw := stream.read(CHUNK):
                yield raw
        actual = write_hash(path, chunks(), deadline)
    require(actual == expected, 'actual output hash/length differs')
