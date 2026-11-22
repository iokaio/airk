# SPDX-License-Identifier: Apache-2.0
"""Export a merged Focus LoRA checkpoint into the static-1024 weight8 asset layout.

The evaluation's static-1024 graph (`model.onnx`) is reused byte for byte. Its
external weights (`model.onnx.data`) are rewritten only at the 112 attention
q/k/v/o MatMulNBits ranges, requantized from the merged checkpoint with the
same ONNX Runtime 8-bit, block-32, symmetric quantizer that produced the
original. Before writing, the quantizer must reproduce every original range
byte for byte from the publisher weights, and every merged tensor outside the
112 projections must equal the publisher tensor, so the untouched bytes stay
valid. The other small assets are copied; the calibration metadata gets the
new weights' SHA-256. Run with the training venv (onnx, onnxruntime, numpy).
"""
import argparse, hashlib, json, re, shutil, struct, sys, time
from pathlib import Path

import numpy as np

PROJECTION = re.compile(r'^/base/layers\.(\d+)/self_attn/([qkvo])_proj/MatMul_Q8$')
SMALL = ('LICENSE', 'README.md', 'compiler.json', 'model.onnx', 'native-runtime1024-fixture.json',
         'rights-source-provenance.json', 'tokenizer.json')


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


class SafeTensors:
    """Minimal read-only safetensors reader (F32 and BF16 to float32)."""

    def __init__(self, path):
        self.path = Path(path)
        with self.path.open('rb') as f:
            n = struct.unpack('<Q', f.read(8))[0]
            self.header = json.loads(f.read(n))
        self.base = 8 + n
        self.header.pop('__metadata__', None)

    def names(self):
        return set(self.header)

    def get(self, name):
        entry = self.header[name]
        start, end = entry['data_offsets']
        with self.path.open('rb') as f:
            f.seek(self.base + start)
            raw = f.read(end - start)
        if entry['dtype'] == 'F32':
            return np.frombuffer(raw, np.float32).reshape(entry['shape'])
        if entry['dtype'] == 'BF16':
            return (np.frombuffer(raw, np.uint16).astype(np.uint32) << 16).view(np.float32).reshape(entry['shape'])
        raise ValueError('unsupported dtype ' + entry['dtype'])


def quantize(weight):
    """ORT MatMulNBits 8-bit, block 32, symmetric: weight is [out, in] (torch Linear)."""
    from onnxruntime.capi._pybind_state import quantize_matmul_8bits
    b = np.ascontiguousarray(weight.T.astype(np.float32))
    rows, cols = b.shape
    blocks = (rows + 31) // 32
    packed = np.zeros((cols, blocks, 32), np.uint8)
    scales = np.zeros((cols, blocks), np.float32)
    zero = np.zeros(cols * blocks, np.uint8)
    quantize_matmul_8bits(packed, b, scales, zero, 32, cols, rows, True)
    return packed.tobytes(), scales.tobytes()


def projection_ranges(graph_path):
    import onnx
    model = onnx.load(str(graph_path), load_external_data=False)
    inits = {i.name: i for i in model.graph.initializer}
    ranges = {}
    for node in model.graph.node:
        match = PROJECTION.match(node.name)
        if not match:
            continue
        if node.op_type != 'MatMulNBits' or len(node.input) != 3:
            raise ValueError('unexpected projection node ' + node.name)
        attrs = {a.name: a.i for a in node.attribute}
        if attrs.get('bits') != 8 or attrs.get('block_size') != 32:
            raise ValueError('projection is not 8-bit block-32')
        spans = []
        for name in node.input[1:]:
            ext = {e.key: e.value for e in inits[name].external_data}
            if ext.get('location') != 'model.onnx.data':
                raise ValueError('projection weight not in model.onnx.data')
            spans.append((int(ext['offset']), int(ext['length'])))
        key = f'model.layers.{match[1]}.self_attn.{match[2]}_proj.weight'
        ranges[key] = {'node': node.name, 'packed': spans[0], 'scales': spans[1], 'K': attrs['K'], 'N': attrs['N']}
    if len(ranges) != 112:
        raise ValueError(f'expected 112 q/k/v/o projections, found {len(ranges)}')
    return ranges


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--merged', type=Path, required=True, help='merged-full-checkpoint/model.safetensors')
    p.add_argument('--publisher', type=Path, required=True, help='publisher model.safetensors')
    p.add_argument('--source-assets', type=Path, required=True, help='static-1024 weight8 asset folder')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args(argv)
    out = args.output.resolve()
    if out.exists() or 'artifacts' not in out.parts:
        raise SystemExit('export_candidate: fresh output under artifacts required')
    started = time.monotonic()
    source = args.source_assets.resolve()
    ranges = projection_ranges(source / 'model.onnx')
    merged, publisher = SafeTensors(args.merged), SafeTensors(args.publisher)
    # Untouched bytes stay valid only if nothing else changed.
    others = sorted(merged.names() - set(ranges))
    if set(ranges) - merged.names():
        raise ValueError('merged checkpoint lacks a projection')
    mismatched = [n for n in others if n in publisher.names() and not np.array_equal(merged.get(n), publisher.get(n))]
    missing = [n for n in others if n not in publisher.names() and n != 'lm_head.weight']
    if mismatched or missing:
        raise ValueError(f'merged tensors outside q/k/v/o differ: {mismatched[:5]} {missing[:5]}')
    data_path = source / 'model.onnx.data'
    with data_path.open('rb') as f:
        original = bytearray(f.read())
    reproduced, changed, delta = 0, 0, {}
    replacement = bytearray(original)
    for key, r in sorted(ranges.items()):
        (po, pl), (so, sl) = r['packed'], r['scales']
        base_packed, base_scales = quantize(publisher.get(key))
        if base_packed != bytes(original[po:po + pl]) or base_scales != bytes(original[so:so + sl]):
            raise ValueError('quantizer does not reproduce the original bytes of ' + key)
        reproduced += 1
        weight = merged.get(key)
        new_packed, new_scales = quantize(weight)
        if len(new_packed) != pl or len(new_scales) != sl:
            raise ValueError('requantized layout differs for ' + key)
        replacement[po:po + pl] = new_packed
        replacement[so:so + sl] = new_scales
        changed += new_packed != base_packed
        diff = weight - publisher.get(key)
        delta[key] = float(np.linalg.norm(diff) / max(np.linalg.norm(publisher.get(key)), 1e-12))
    touched = sorted((r[s][0], r[s][0] + r[s][1]) for r in ranges.values() for s in ('packed', 'scales'))
    cursor = 0
    for a, b in touched:
        if replacement[cursor:a] != original[cursor:a]:
            raise ValueError('bytes outside projection ranges changed')
        cursor = b
    if replacement[cursor:] != original[cursor:]:
        raise ValueError('trailing bytes changed')
    out.mkdir()
    for name in SMALL:
        shutil.copyfile(source / name, out / name)
    (out / 'model.onnx.data').write_bytes(bytes(replacement))
    data_sha = digest(out / 'model.onnx.data')
    raw_meta = (source / 'focus-relevance-metadata.json').read_bytes()
    meta = json.loads(raw_meta)
    if (json.dumps(meta, indent=2, ensure_ascii=False).encode('utf-8') + (b'\n' if raw_meta.endswith(b'\n') else b'')) != raw_meta:
        raise ValueError('calibration metadata cannot be re-emitted byte-faithfully')
    meta['assets']['model.onnx.data']['sha256'] = data_sha
    (out / 'focus-relevance-metadata.json').write_bytes(json.dumps(meta, indent=2, ensure_ascii=False).encode('utf-8')
                                                        + (b'\n' if raw_meta.endswith(b'\n') else b''))
    receipt = {'schema': 'airk.private.focus-answer-eval-export/1', 'merged': {'path': str(args.merged), 'sha256': digest(args.merged)},
               'publisher': {'path': str(args.publisher), 'sha256': digest(args.publisher)},
               'source_assets': str(source), 'source_data_sha256': digest(data_path),
               'projections': len(ranges), 'reproduced_original_projections': reproduced, 'changed_projections': changed,
               'relative_weight_delta': {'max': max(delta.values()), 'mean': sum(delta.values()) / len(delta)},
               'other_tensors_equal_publisher': len(others), 'quantizer': 'onnxruntime quantize_matmul_8bits, bits=8, block=32, symmetric',
               'assets': {n.name: {'bytes': n.stat().st_size, 'sha256': digest(n)} for n in sorted(out.iterdir())},
               'graph_reused_unchanged': digest(out / 'model.onnx') == digest(source / 'model.onnx'),
               'elapsed_seconds': time.monotonic() - started}
    (out.parent / (out.name + '-export.json')).write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()
