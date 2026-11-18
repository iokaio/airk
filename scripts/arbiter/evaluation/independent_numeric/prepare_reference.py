# SPDX-License-Identifier: Apache-2.0
"""Prepare source/runtime seals or an inert reviewed reference plan; no model imports."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
from maintained_helpers import worker, parent, HELPER_ROOT, private_root

HERE = Path(__file__).resolve().parent
REPOSITORY = Path(__file__).resolve().parents[4]
MODELS = REPOSITORY / 'artifacts/advanced-split/models'
EXPORT = REPOSITORY / 'scripts/arbiter/export/qwen'
BASE = Path('C:/Users/tdotn/AppData/Local/Programs/Python/Python313')
SITE = MODELS / 'training-env/venv/Lib/site-packages'
VERSIONS = {'torch': '2.10.0+cu128', 'transformers': '5.10.1', 'numpy': '2.3.2', 'safetensors': '0.8.0', 'onnx': '1.19.0'}


def pin(path):
    path = Path(path).resolve(strict=True)
    worker.regular(path)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': worker.digest(path)}


def write_new(path, value):
    with Path(path).open('x', encoding='utf-8') as out:
        json.dump(value, out, indent=2)
        out.write('\n')


def inventory(state_root):
    state_root = private_root(state_root)
    # Native protocol/profile/assets are deliberately absent: native emission is
    # a separate prerequisite, copied opaquely when preparing a concrete plan.
    selection = {
        'Qwen/Qwen3-Reranker-0.6B': ('qwen3-reranker06-assets-r1', 'research-asset-pins-full.json', 'qwen3-reranker06-experiment-r1/run_focus_export.py'),
        'Qwen/Qwen3-1.7B': ('qwen3-challenger-assets-r1', 'research-asset-pins.json', 'qwen3-challengers-r1/qwen3_reference_r2.py'),
        'Qwen/Qwen3-4B': ('qwen3-challenger-assets-r1', 'research-asset-pins.json', 'qwen3-challengers-r1/qwen3_reference_r2.py')}
    result = {'schema': 'airk.private.independent-reference-source-inventory/1', 'models': {}, 'model_executed': False}
    for name, (folder, manifest_name, module) in selection.items():
        asset_root = MODELS / folder
        manifest = asset_root / manifest_name
        rows = json.loads(manifest.read_bytes())['models']
        selected = [row for row in rows if row['id'] == name or name == 'Qwen/Qwen3-Reranker-0.6B' and row['id'] == 'Qwen/Qwen3-0.6B-Base']
        pins = {str(manifest.resolve()): worker.digest(manifest)}
        for model in selected:
            for asset in model['assets']:
                path = asset_root / 'candidate-assets' / model['id'].replace('/', '--') / asset['name']
                observed = pin(path)
                if observed['bytes'] != asset['bytes']:
                    raise ValueError('publisher asset length differs')
                if asset['sha256']:
                    valid = observed['sha256'] == asset['sha256']
                else:
                    valid = hashlib.sha1(('blob ' + str(observed['bytes']) + '\0').encode() + path.read_bytes()).hexdigest() == asset['git_blob']
                if not valid:
                    raise ValueError('pinned publisher asset differs')
                pins[observed['path']] = observed['sha256']
        reference = pin(EXPORT / module)
        pins[reference['path']] = reference['sha256']
        if name == 'Qwen/Qwen3-Reranker-0.6B':
            other = pin(EXPORT / 'qwen3-reranker06-experiment-r1/focus_reference.py')
            pins[other['path']] = other['sha256']
        result['models'][name] = {'asset_root': str(asset_root), 'reference_module': reference,
            'publisher_manifest': pin(manifest), 'input_seals': pins}
    write_new(state_root / 'source-inventory.json', result)
    files = {}
    extensions = {'.py', '.pyw', '.pyd', '.dll', '.json', '.toml', '.yaml', '.yml', '.txt', '.cfg'}
    for directory in [BASE / 'Lib', BASE / 'DLLs', SITE]:
        for path in directory.rglob('*'):
            if not path.is_file() or '__pycache__' in path.parts:
                continue
            if directory == BASE / 'Lib' and 'site-packages' in path.relative_to(directory).parts:
                continue
            if path.suffix.lower() not in extensions and path.name not in ('METADATA', 'WHEEL'):
                continue
            item = pin(path)
            files[item['path']] = item['sha256']
    for path in [BASE / 'python.exe', BASE / 'python313.dll', BASE / 'vcruntime140.dll', BASE / 'vcruntime140_1.dll']:
        if path.exists():
            item = pin(path)
            files[item['path']] = item['sha256']
    if len(files) > 16000:
        raise ValueError('runtime source census exceeds native seal bound; retain explicit inventory refusal')
    write_new(state_root / 'runtime-pins.json', {'schema': 'airk.private.exact-python-runtime-files/1', 'files': files})
    print(json.dumps({'publisher_models': len(result['models']), 'runtime_files': len(files), 'model_executed': False}))


def prepare(args):
    state_root = private_root(args.state_root)
    evidence_root = private_root(args.evidence_root)
    export_root = private_root(args.export_root)
    if not args.output.resolve().is_relative_to(state_root) or args.output.exists():
        raise ValueError("fresh plan under explicit private state root required")
    sources = json.loads((state_root / 'source-inventory.json').read_bytes())['models'][args.upstream]
    expected_reference = EXPORT / ('qwen3-reranker06-experiment-r1/run_focus_export.py' if args.upstream == 'Qwen/Qwen3-Reranker-0.6B' else 'qwen3-challengers-r1/qwen3_reference_r2.py')
    if sources['reference_module'] != pin(expected_reference):
        raise ValueError('fresh inventory must pin maintained reference source')
    runtime = json.loads((state_root / 'runtime-pins.json').read_bytes())
    native_input, template, original = pin(args.native_input), pin(args.template), pin(args.original_roster)
    request = json.loads(Path(native_input['path']).read_bytes())
    opaque = json.loads(Path(original['path']).read_bytes())
    seals = dict(runtime['files'])
    seals.update(sources['input_seals'])
    for path in list(HERE.glob('*.py')) + list(HELPER_ROOT.glob('*.py')) + [REPOSITORY / 'scripts/cuda-native-execution.py', state_root / 'source-inventory.json', state_root / 'runtime-pins.json', evidence_root / 'ownership-copy-provenance.json', args.native_input, args.template, args.original_roster,
            Path(request['profile']['path']), Path(request['source_manifest']['path']), REPOSITORY / 'tools/retrieval-train/training_execution.py']:
        item = pin(path)
        seals[item['path']] = item['sha256']
    if getattr(args, 'execution_powershell', None) is not None:
        for path in [args.execution_powershell, REPOSITORY / 'scripts/cuda-component-execution.py',
                     REPOSITORY / 'scripts/dev/assert-human-execution.ps1']:
            item = pin(path)
            seals[item['path']] = item['sha256']
    for path in request['files'].values():
        item = pin(path)
        seals[item['path']] = item['sha256']
    source_input = pin(opaque['original_source_input']['path'])
    if source_input['sha256'] != opaque['original_source_input']['sha256']:
        raise ValueError('native original-source input changed')
    seals[source_input['path']] = source_input['sha256']
    # The actual retained FP32 export supplies structural/weight provenance only,
    # never expected scores. Fresh Torch computes every new reference array.
    if args.upstream == 'Qwen/Qwen3-Reranker-0.6B':
        export = export_root
        report = json.loads((export / 'receipt.json').read_bytes())
        graph = export / 'model.onnx'
        export_files = report['graph_closure']['files']
    else:
        tier = '17' if args.upstream == 'Qwen/Qwen3-1.7B' else '4'
        export = export_root
        report = json.loads((export / 'receipt.json').read_bytes())
        graph = export / 'model.onnx'
        export_files = [{'name': name, 'sha256': value} for name, value in report['artifact_sha256'].items() if name != 'receipt.json']
    for entry in export_files:
        item = pin(export / entry['name'])
        if item['sha256'] != entry['sha256']:
            raise ValueError('retained original export source changed')
        seals[item['path']] = item['sha256']
    item = pin(export / 'receipt.json')
    seals[item['path']] = item['sha256']
    for mapping in ['original_reference_files', 'original_source_files']:
        for item in opaque[mapping].values():
            seals[item['path']] = item['sha256']
    root = Path(request['source_manifest']['path'])
    manifest = json.loads(root.read_bytes())
    for item in [manifest['native_build_receipt'], *manifest['source_files']]:
        checked = pin(item['path'])
        if checked['sha256'] != item['sha256']:
            raise ValueError('native frozen source closure changed')
        seals[checked['path']] = checked['sha256']
    plan = {'schema': 'airk.private.independent-torch-plan/1', 'kind': args.kind, 'upstream': args.upstream,
        'human_sid': args.human_sid, 'expected_gpu_name': args.gpu_name, 'repository_root': str(REPOSITORY), 'asset_root': sources['asset_root'],
        'reference_module': sources['reference_module'], 'original_export_graph': pin(graph), 'base_python': pin(BASE / 'python.exe'), 'site_packages': str(SITE),
        'runtime_manifest': pin(state_root / 'runtime-pins.json'), 'worker': pin(HELPER_ROOT / 'reference_worker.py'),
        'ownership_module': pin(HELPER_ROOT / 'reference_owned.py'),
        'native_input': native_input, 'native_template': template, 'original_roster': original, 'versions': VERSIONS,
        'limits': {'wall_seconds': args.wall_seconds, 'host_committed_bytes': args.host_gib << 30,
            'cuda_allocator_bytes': args.cuda_gib << 30, 'minimum_cuda_free_bytes': args.minimum_cuda_free_gib << 30, 'threads': args.threads},
        'input_seals': seals}
    if args.upstream == 'Qwen/Qwen3-Reranker-0.6B' and args.kind == 'same_artifact':
        plan['focus_adapter'] = pin(HERE / 'focus_w8_reference.py')
        for path in [evidence_root / 'focus-w8-reference-contract.md', evidence_root / 'focus-w8-review-r1.json']:
            item = pin(path)
            plan['input_seals'][item['path']] = item['sha256']
        for reviewed in json.loads((evidence_root / 'focus-w8-review-r1.json').read_bytes())['files']:
            item = pin(reviewed['path'])
            if item['sha256'] != reviewed['sha256']:
                raise ValueError('independently reviewed Focus source/spec changed')
            plan['input_seals'][item['path']] = item['sha256']
    parent.preflight(plan)
    if not args.output.resolve().is_relative_to(state_root) or args.output.exists():
        raise ValueError('fresh plan under this exact private preparation required')
    write_new(args.output, plan)
    print(json.dumps({'path': str(args.output.resolve()), 'sha256': worker.digest(args.output), 'model_executed': False, 'qualification_granted': False}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', action='store_true')
    parser.add_argument('--state-root', type=Path, required=True)
    parser.add_argument('--evidence-root', type=Path)
    parser.add_argument('--export-root', type=Path)
    parser.add_argument('--native-input', type=Path)
    parser.add_argument('--template', type=Path)
    parser.add_argument('--original-roster', type=Path)
    parser.add_argument('--upstream')
    parser.add_argument('--kind', choices=['same_artifact', 'original_fp32'])
    parser.add_argument('--human-sid')
    parser.add_argument('--gpu-name')
    parser.add_argument('--wall-seconds', type=int)
    parser.add_argument('--host-gib', type=int)
    parser.add_argument('--cuda-gib', type=int)
    parser.add_argument('--minimum-cuda-free-gib', type=int)
    parser.add_argument('--threads', type=int)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--execution-powershell', type=Path)
    args = parser.parse_args()
    if args.inventory:
        inventory(args.state_root)
    else:
        if any(getattr(args, name) is None for name in ['evidence_root', 'export_root', 'native_input', 'template', 'original_roster', 'upstream', 'kind', 'human_sid', 'gpu_name', 'wall_seconds', 'host_gib', 'cuda_gib', 'minimum_cuda_free_gib', 'threads', 'output']):
            parser.error('a concrete plan requires every explicit source and limit argument')
        prepare(args)


if __name__ == '__main__':
    main()
