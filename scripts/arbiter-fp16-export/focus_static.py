# SPDX-License-Identifier: Apache-2.0
"""Static Focus census and G5 export (plan 12.5.2, L5 phase A).

``census`` loads only the authenticated a16fcaa1 static-1024 graph bytes (no
external data), runs the exporter's role census and the metadata-only
attention rewrite on a copy, and writes a JSON report. No weights are read.
A refusal is reported with the topology facts that decide it.

``export --key focus|focus1024`` runs the exporter's
``export_after_owner_admission`` into a fresh output directory and writes the
same export manifest shape as ``scripts/export-arbiter-fp16.py``. ``focus``
is the FP32 static-512 original (19b20ad4); its external-file pins come from
the original export receipt's ``graph_closure`` (``--pins-receipt`` plus
``--pins-receipt-sha256``), never from hashing the candidate.
``focus1024fp32`` is 19b20ad4 widened by ``focus_widen.py`` (794fc0b2); its
sidecar pin comes from the pinned widen receipt.

Neither mode runs a model, touches a GPU or qualifies anything. Run with
``python -I`` as the verified human, with ``--python-site`` naming a site
directory that holds onnx and numpy.
"""
import argparse, copy, hashlib, importlib.util, json, pathlib, sys, time

GRAPH = 'a16fcaa1bf2d67b019b30cda56d76ba23d589c346ef2ae76d75c229ebe866810'
GRAPH_BYTES = 5_046_780
WEIGHTS = {'model.onnx.data': {'sha256': '1c819b7781fbf9b811f6ec1453e91d7f32a1bd6e5489168e0612d4fcf015b143', 'bytes': 1_117_980_672}}
# Widened FP32 source sidecar (focus_widen.py); the same pin as the Rust full-FP32 Focus1024 arm.
FP32_1024_WEIGHTS = {'model.onnx.data': {'sha256': '3a90bdb2e5d970275d5fe517786219a7123d020ba40bda39f1e8c46e249ec098', 'bytes': 2_382_852_096}}
# Both FP32 Focus sources carry one learned roster, so their G5 weights.bin is one artifact.
FOCUS_FP16_WEIGHTS = 'f94f74a26875a930cbf7d2ef3b3756f01ac927e3132f8e6f428787a707b83aff'


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def exporter():
    path = pathlib.Path(__file__).resolve().parents[1] / 'arbiter-fp16-export.py'
    spec = importlib.util.spec_from_file_location('airk_fp16_export', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, digest(path.read_bytes())


def attention_roster(model):
    return [{'name': n.name, 'operator': n.op_type, 'domain': n.domain}
            for n in model.graph.node if n.op_type in ('GroupQueryAttention', 'MultiHeadAttention')]


def topology_facts(model, onnx):
    """Bounded facts the exporter's role census and chain matcher depend on."""
    dtypes = {}
    non_fp32 = []
    for t in model.graph.initializer:
        dtypes[t.data_type] = dtypes.get(t.data_type, 0) + 1
        if t.data_type != 1 and len(non_fp32) < 64:
            non_fp32.append({'name': t.name, 'data_type': t.data_type, 'dims': list(t.dims), 'external': t.data_location == 1})
    names = [t.name for t in model.graph.initializer]
    ops = {}
    for n in model.graph.node:
        ops[n.op_type] = ops.get(n.op_type, 0) + 1
    layer0 = sorted(n.name for n in model.graph.node if n.name.startswith(('/base/layers.0/self_attn/', '/model/layers.0/self_attn/')))
    prefixes = sorted({n.name.split('/')[1] for n in model.graph.node if n.name.startswith('/') and n.name.count('/') > 1})
    return {'initializer_dtypes': dtypes, 'non_fp32_initializers': non_fp32,
            'duplicate_initializer_names': len(names) - len(set(names)), 'operator_counts': ops,
            'node_name_prefixes': prefixes[:32], 'layer0_self_attn_nodes': layer0[:96]}


def census(args, onnx, e, exporter_sha):
    started = time.monotonic()
    raw = args.source.read_bytes()
    if len(raw) != GRAPH_BYTES or digest(raw) != GRAPH:
        raise ValueError('source graph is not the pinned a16fcaa1 Focus 1024 export')
    original = onnx.load_model_from_string(raw)
    external = {}
    for t in original.graph.initializer:
        if t.data_location == 1:
            fields = {x.key: x.value for x in t.external_data}
            external.setdefault(fields.get('location', ''), 0)
            external[fields.get('location', '')] += 1
    constants_external = sum(1 for n in original.graph.node for a in n.attribute if a.type == 4 and a.t.data_location == 1)
    topology = topology_facts(original, onnx)
    try:
        roles = e.classify(original, 'focus1024')
    except ValueError as error:
        # The census reports the refusal with the facts that decide it; it never adapts.
        return {
            'schema': 'airk.private.focus1024-exporter-census/1',
            'source_graph': {'path': str(args.source), 'sha256': GRAPH, 'bytes': len(raw)},
            'exporter_sha256': exporter_sha, 'external_locations': external,
            'external_constant_tensors': constants_external, 'topology': topology,
            'refusal': {'stage': 'classify', 'error': str(error)},
            'weights_read': False, 'model_inference': False, 'qualification': False,
            'elapsed_s': round(time.monotonic() - started, 3), 'passed': False,
        }
    counts = {}
    for role in roles.values():
        counts[role] = counts.get(role, 0) + 1
    candidate = copy.deepcopy(original)
    report = e.rewrite(candidate, 'focus1024', onnx)
    roster = attention_roster(candidate)
    mha = [n for n in candidate.graph.node if n.op_type == 'MultiHeadAttention']
    mha_attributes = sorted({json.dumps({a.name: onnx.helper.get_attribute_value(a) for a in n.attribute}, sort_keys=True, default=str) for n in mha})
    softmax = sum(1 for n in candidate.graph.node if n.op_type == 'Softmax')
    inputs = [(x.name, [d.dim_value or d.dim_param for d in x.type.tensor_type.shape.dim]) for x in original.graph.input]
    result = {
        'schema': 'airk.private.focus1024-exporter-census/1',
        'source_graph': {'path': str(args.source), 'sha256': GRAPH, 'bytes': len(raw)},
        'exporter_sha256': exporter_sha,
        'graph_inputs': inputs,
        'source_nodes': len(original.graph.node),
        'source_initializers': len(original.graph.initializer),
        'external_locations': external,
        'external_constant_tensors': constants_external,
        'role_counts': counts,
        'rewrite': {k: v for k, v in report.items() if k != 'roles'},
        'candidate_nodes': len(candidate.graph.node),
        'attention_nodes': len(roster),
        'attention_roster_sha256': digest(json.dumps(roster, sort_keys=True, separators=(',', ':')).encode()),
        'mha_attribute_sets': mha_attributes,
        'softmax_remaining': softmax,
        'mask_rule_1024': e.validate_inputs('focus1024', [0] * 1000 + [1] * 24, list(range(1024))),
        'weights_read': False,
        'model_inference': False,
        'qualification': False,
        'elapsed_s': round(time.monotonic() - started, 3),
        'toolchain': {'python': sys.version, 'onnx': onnx.__version__},
        'passed': len(roster) == 28 and softmax == 0,
    }
    return result


def closure_pins(args):
    if args.key == 'focus1024':
        return WEIGHTS
    if args.key == 'focus1024fp32':
        # The widened FP32 source carries one sidecar; its pin comes from the widen receipt.
        raw = args.pins_receipt.read_bytes()
        if digest(raw) != args.pins_receipt_sha256:
            raise ValueError('widen receipt differs from its pin')
        widen = json.loads(raw)
        pins = {'model.onnx.data': {'sha256': widen['external_weights'], 'bytes': widen['external_bytes']}}
        if pins != FP32_1024_WEIGHTS:
            raise ValueError('widen receipt sidecar differs from the hard pin')
        return FP32_1024_WEIGHTS
    raw = args.pins_receipt.read_bytes()
    if digest(raw) != args.pins_receipt_sha256:
        raise ValueError('original export receipt differs from its pin')
    files = json.loads(raw)['graph_closure']['files']
    pins = {f['name']: {'sha256': f['sha256'], 'bytes': f['bytes']} for f in files if f['name'] != 'model.onnx'}
    if len(pins) != len(files) - 1:
        raise ValueError('original closure roster differs')
    return pins


def export(args, onnx, np, e, exporter_sha):
    deadline = time.monotonic() + args.deadline_s
    result = e.export_after_owner_admission(args.source, args.key, closure_pins(args), args.output, onnx, np, deadline)
    graph = args.output / 'model.onnx'
    candidate = onnx.load_model(str(graph), load_external_data=False)
    roster = attention_roster(candidate)
    if len(roster) != 28 or time.monotonic() >= deadline:
        raise ValueError('actual attention roster/deadline differs')
    result.update({'attention_nodes': roster,
                   'attention_roster_sha256': digest(json.dumps(roster, sort_keys=True, separators=(',', ':')).encode()),
                   'exporter_sha256': exporter_sha, 'source_key': args.key,
                   'toolchain': {'python': sys.version, 'onnx': onnx.__version__, 'numpy': np.__version__},
                   'model_inference': False, 'qualification': False, 'files': []})
    for path in [graph, args.output / 'weights.bin']:
        hasher = hashlib.sha256()
        with path.open('rb') as handle:
            while chunk := handle.read(4 * 1024 * 1024):
                hasher.update(chunk)
        result['files'].append({'name': path.name, 'bytes': path.stat().st_size, 'sha256': hasher.hexdigest()})
    weights = next(f for f in result['files'] if f['name'] == 'weights.bin')
    if args.key in ('focus', 'focus1024fp32') and weights['sha256'] != FOCUS_FP16_WEIGHTS:
        raise ValueError('Focus G5 weights.bin differs from the shared f94f74a2 artifact')
    with (args.output / 'export-manifest.private.json').open('x', encoding='utf-8') as handle:
        json.dump(result, handle, indent=2)
        handle.write('\n')
    return {'output': str(args.output), 'files': result['files'], 'attention_roster_sha256': result['attention_roster_sha256']}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode', choices=['census', 'export'])
    p.add_argument('--source', type=pathlib.Path, required=True)
    p.add_argument('--python-site', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    p.add_argument('--deadline-s', type=float, default=1800.0)
    p.add_argument('--key', choices=['focus', 'focus1024', 'focus1024fp32'], default='focus1024')
    p.add_argument('--pins-receipt', type=pathlib.Path)
    p.add_argument('--pins-receipt-sha256')
    args = p.parse_args()
    sys.path.insert(0, str(args.python_site.resolve()))
    import onnx, numpy
    e, exporter_sha = exporter()
    if args.mode == 'census':
        if args.output.exists():
            raise ValueError('fresh census output required')
        result = census(args, onnx, e, exporter_sha)
        with args.output.open('x', encoding='utf-8') as handle:
            json.dump(result, handle, indent=2)
            handle.write('\n')
        print(json.dumps({'passed': result['passed'], 'attention_nodes': result.get('attention_nodes'),
                          'refusal': result.get('refusal')}))
        if not result['passed']:
            raise SystemExit(1)
    else:
        print(json.dumps(export(args, onnx, numpy, e, exporter_sha), indent=2))


if __name__ == '__main__':
    main()
