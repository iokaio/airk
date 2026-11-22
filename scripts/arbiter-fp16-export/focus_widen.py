# SPDX-License-Identifier: Apache-2.0
"""Widen the authenticated FP32 static-512 Focus graph 19b20ad4 to static [1,1024].

Plan 12.5.2 / 12.7.3 (L5 phase B): the FP32 arm of the Focus precision check
needs an FP32 graph at the product width. This applies the same static-shape
edits that ``scripts/arbiter/export/qwen/focus1024/export_graph.py`` applied
to the weight8 graph 5072c663 (input dims, the 512-valued Reshape/Expand shape
constants, the two position grids and the zero mask base), to the FP32 source
it came from (receipt 55b9779a). Learned tensors are copied byte for byte and
repacked into one ``model.onnx.data`` sidecar (offsets recorded), so profiles
carry two model assets instead of about two hundred.

No model is run. Output: a fresh directory with model.onnx, model.onnx.data
and widen-receipt.json. Run with ``python -I`` and ``--python-site``.
"""
import argparse, hashlib, json, pathlib, sys
from collections import Counter

SOURCE_GRAPH = '19b20ad4e39e173f6da602094b9c1583726a65fa76742553f97ec6f0f58436a1'
SOURCE_RECEIPT = '55b9779a323b009a3ea02af059d75d4f4d396b93642e07faf1582f0c77c0dc43'
SHAPES = {(1, 512, -1, 128): 84, (1, 8, 2, 512, 128): 56, (1, 16, 512, 128): 56, (1, 512, -1): 28}


def require(value, reason):
    if not value:
        raise ValueError(reason)


def file_sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as handle:
        while chunk := handle.read(4 << 20):
            h.update(chunk)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source-dir', type=pathlib.Path, required=True)
    p.add_argument('--python-site', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    args = p.parse_args()
    sys.path.insert(0, str(args.python_site.resolve()))
    import numpy as np
    import onnx
    from onnx import numpy_helper

    receipt_raw = (args.source_dir / 'receipt.json').read_bytes()
    require(hashlib.sha256(receipt_raw).hexdigest() == SOURCE_RECEIPT, 'source receipt differs')
    pins = {f['name']: f for f in json.loads(receipt_raw)['graph_closure']['files']}
    raw = (args.source_dir / 'model.onnx').read_bytes()
    require(hashlib.sha256(raw).hexdigest() == SOURCE_GRAPH, 'source graph differs')
    graph = onnx.load_model_from_string(raw)
    require([i.name for i in graph.graph.input] == ['input_ids', 'attention_mask', 'position_ids'], 'wrong three input ABI')
    for inp in graph.graph.input:
        dims = inp.type.tensor_type.shape.dim
        require([d.dim_value for d in dims] == [1, 512] and inp.type.tensor_type.elem_type == 7, 'wrong input type/shape')
        dims[1].dim_value = 1024
    consumers = {}
    for op in graph.graph.node:
        for index, name in enumerate(op.input):
            consumers.setdefault(name, []).append((op.op_type, index))
    seen, axes = Counter(), set()
    for node in graph.graph.node:
        if node.op_type != 'Constant':
            continue
        tensor = node.attribute[0].t
        if tensor.data_location == 1:
            # The FP32 export stores the 1 MiB mask base externally.
            fields = {x.key: x.value for x in tensor.external_data}
            pin = pins[fields['location']]
            path = args.source_dir / fields['location']
            require(file_sha(path) == pin['sha256'], 'external constant differs')
            values = np.frombuffer(path.read_bytes()[int(fields.get('offset', 0)):][:int(fields.get('length', pin['bytes']))],
                                   dtype=np.float32).reshape(tuple(tensor.dims))
        else:
            values = numpy_helper.to_array(tensor)
        old = tuple(values.flatten().tolist()) if values.dtype == np.int64 and values.size <= 5 else None
        if old in SHAPES:
            expected = [('Equal', 0), ('Where', 2)] if old == (1, 8, 2, 512, 128) else [('Reshape', 1)]
            require(consumers[node.output[0]] == expected, 'unexpected static shape consumers ' + node.name)
            seen[old] += 1
            result = np.where(values == 512, 1024, values)
        elif node.name in ('/Constant', '/Constant_1'):
            column = node.name == '/Constant_1'
            shape = (lambda w: (w, 1)) if column else (lambda w: (1, w))
            require(np.array_equal(values, np.arange(512, dtype=np.int64).reshape(shape(512))), 'position grid differs')
            result = np.arange(1024, dtype=np.int64).reshape(shape(1024))
            axes.add(node.name)
        elif node.name == '/Constant_7':
            require(values.shape == (1, 1, 512, 512) and values.dtype == np.float32 and not np.any(values), 'mask base differs')
            result = np.zeros((1, 1, 1024, 1024), dtype=np.float32)
            axes.add(node.name)
        else:
            continue
        tensor.CopyFrom(numpy_helper.from_array(result, name=tensor.name))
    require(seen == SHAPES and axes == {'/Constant', '/Constant_1', '/Constant_7'},
            'static shape census differs: ' + json.dumps({str(k): v for k, v in seen.items()}) + ' ' + str(sorted(axes)))
    args.output.mkdir(exist_ok=False)
    sidecar = args.output / 'model.onnx.data'
    records = []
    with sidecar.open('xb') as out:
        for t in graph.graph.initializer:
            require(t.data_type == 1, 'unexpected non-FP32 initializer ' + t.name)
            if t.data_location != 1:
                # Small inline tensors (norm scales) stay inline and unchanged.
                records.append({'name': t.name, 'inline': True, 'sha256': hashlib.sha256(t.SerializeToString()).hexdigest()})
                continue
            fields = {x.key: x.value for x in t.external_data}
            pin = pins[fields['location']]
            src = args.source_dir / fields['location']
            data = src.read_bytes()
            require(hashlib.sha256(data).hexdigest() == pin['sha256'], 'initializer file differs ' + src.name)
            offset, length = int(fields.get('offset', 0)), int(fields.get('length', len(data)))
            chunk = data[offset:offset + length]
            pad = (-out.tell()) % 64
            out.write(b'\0' * pad)
            start = out.tell()
            out.write(chunk)
            del t.external_data[:]
            for key, value in (('location', 'model.onnx.data'), ('offset', str(start)), ('length', str(len(chunk)))):
                entry = t.external_data.add(); entry.key = key; entry.value = value
            records.append({'name': t.name, 'source': fields['location'], 'offset': start, 'bytes': len(chunk),
                            'sha256': hashlib.sha256(chunk).hexdigest()})
    with (args.output / 'model.onnx').open('xb') as out:
        out.write(graph.SerializeToString(deterministic=True))
    result = {'schema': 'airk.private.focus-fp32-static1024-widen/1', 'source_graph': SOURCE_GRAPH,
              'source_receipt': SOURCE_RECEIPT, 'method': 'export_graph.py static-shape edits applied to the FP32 source',
              'graph': file_sha(args.output / 'model.onnx'), 'graph_bytes': (args.output / 'model.onnx').stat().st_size,
              'external_weights': file_sha(sidecar), 'external_bytes': sidecar.stat().st_size,
              'reshape_constants': sum(seen.values()), 'position_and_mask_constants': len(axes),
              'initializers': len(records), 'records': records,
              'widener_sha256': file_sha(pathlib.Path(__file__)), 'model_inference': False, 'qualification': False}
    with (args.output / 'widen-receipt.json').open('x', encoding='utf-8') as handle:
        json.dump(result, handle, indent=2)
        handle.write('\n')
    print(json.dumps({k: result[k] for k in ('graph', 'graph_bytes', 'external_weights', 'external_bytes', 'initializers')}, indent=2))


if __name__ == '__main__':
    main()
