# SPDX-License-Identifier: Apache-2.0
"""Run the independent portable graph proof for one Context export. Run with -I -B and retain stdout privately.

Metadata only: reads the pinned original graph and external file, the pinned
publisher release and the candidate graph. Opens no candidate weights, imports
no exporter and runs no model.
"""
import argparse
import hashlib
import importlib.util
import json
import pathlib
import runpy
import sys
import types

HERE = pathlib.Path(__file__).resolve()
ROOT = HERE.parents[2]
OWNER = ROOT / 'scripts/arbiter/export/qwen/qwen3-challengers-r1/qwen3_reference_r2.py'
OWNER_SHA = '4c6f43d0d81539cfc6bf7b6ddf5ae69273e61508d0db3d22a5e7488c60560b6b'
MODELS = {'compact': 'Qwen/Qwen3-1.7B', 'large': 'Qwen/Qwen3-4B'}

def require(condition, message):
    if not condition:
        raise ValueError('portable graph run: ' + message)

def pin(path):
    path = pathlib.Path(path)
    with path.open('rb') as stream:
        return {'path': str(path), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}

def run(args, onnx):
    owner = pin(OWNER)
    require(owner['sha256'] == OWNER_SHA, 'publisher owner source differs')
    raw = pathlib.Path(args.original_pins).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == args.original_pins_sha256, 'original closure pins differ')
    source_files = json.loads(raw)
    spec = importlib.util.spec_from_file_location('airk_fp16_independent_portable_graph', HERE.with_name('portable_graph.py'))
    portable = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(portable)
    publisher = runpy.run_path(str(OWNER))
    identity = MODELS[args.model]
    _, config, _ = publisher['authenticate'](args.publisher, identity)
    reference = types.SimpleNamespace(
        focus=False, config=config, layers=config['num_hidden_layers'], hidden=config['hidden_size'],
        qheads=config['num_attention_heads'], kvheads=config['num_key_value_heads'], dim=config['head_dim'])
    require((reference.layers, reference.qheads, reference.kvheads, reference.dim, reference.hidden) == portable.GEOMETRY[args.model],
            'publisher geometry is not the named model')
    # An empty candidate closure refuses any candidate Constant left in external storage.
    proof = portable.verify_portable_files(reference, args.graph, args.graph_sha256, args.original, source_files, {}, onnx)
    return {
        'schema': 'airk.private.portable-graph-proof/1', 'model': args.model, 'recipe': 'portable',
        'original': {'path': str(args.original), 'sha256': portable.base.ORIGINAL[args.model], 'files': source_files},
        'candidate': {'path': str(args.graph), 'sha256': args.graph_sha256},
        'publisher': {'owner': owner, 'model_id': identity, 'revision': publisher['REVISIONS'][identity]},
        'sources': [pin(HERE), pin(HERE.with_name('portable_graph.py')), pin(HERE.with_name('graph.py'))],
        'proof': proof, 'learned_bytes_verified': False, 'native_execution': False, 'qualification': False,
    }

def arguments(argv=None):
    parser = argparse.ArgumentParser(description='Independent portable graph proof for one Context export.')
    parser.add_argument('--model', required=True, choices=sorted(MODELS))
    parser.add_argument('--original', required=True, type=pathlib.Path)
    parser.add_argument('--original-pins', required=True, type=pathlib.Path)
    parser.add_argument('--original-pins-sha256', required=True)
    parser.add_argument('--graph', required=True, type=pathlib.Path)
    parser.add_argument('--graph-sha256', required=True)
    parser.add_argument('--publisher', required=True, type=pathlib.Path)
    return parser.parse_args(argv)

if __name__ == '__main__':
    require(sys.flags.isolated and sys.dont_write_bytecode, 'run with -I -B')
    import onnx
    print(json.dumps(run(arguments(), onnx), sort_keys=True, allow_nan=False))
