# SPDX-License-Identifier: Apache-2.0
"""PRIVATE bounded frozen-backbone/original-head adaptation prototype.

Source prepared only. No feature/training run has been authorized by this file.
Consumes existing native compiled TRAIN vectors, never constructs native DTOs.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
INDEX_SHA = 'cb6597cdb4ff0fc859f74c72124c3dbb91ca4858069e9c7f6e75f5017e60b5d5'
MODEL_NAMES = {'smallintern': 'intern08', 'strands': 'strands2', 'intern4': 'intern4', 'kev4': 'kev4'}

def sha(raw): return hashlib.sha256(raw).hexdigest()

def pinned(path, digest):
    raw = Path(path).read_bytes()
    if sha(raw) != digest: raise ValueError('input seal mismatch')
    return raw

def write_json(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2); stream.write('\n')

def base(args):
    source = Path(__file__).read_bytes()
    indices_path = ROOT / 'context-action-adaptation-train-indices-r1.private.json'
    indices = json.loads(pinned(indices_path, INDEX_SHA))
    examples = indices['per_model'][args.model]
    assert len(examples) == 44
    path = next(Path(p) for p in indices['inputs'] if f'context-action-{args.model}-raw-r24' in p)
    rows = json.loads(pinned(path, indices['inputs'][str(path)]))['rows']
    for example in examples:
        row = rows[example['native_row_index']]
        assert row['split'] == 'train' and row['id'] == example['id']
        assert row['compiled']['input_digest'] == example['native_input_digest']
        assert not example['negative_option_indices'] and example['semantic_labels'] is None
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT):
        raise ValueError('fresh private model-scratch output required')
    output.mkdir()
    return source, indices, examples, rows, output

def recheck(source, indices):
    if Path(__file__).read_bytes() != source: raise ValueError('runner changed')
    pinned(ROOT / 'context-action-adaptation-train-indices-r1.private.json', INDEX_SHA)
    for path, digest in indices['inputs'].items(): pinned(path, digest)

def extract(args):
    import torch
    from safetensors.torch import save_file
    source, indices, examples, rows, output = base(args)
    exporter = REPO / 'tools/retrieval-train/qwen_export.py'
    exporter_bytes = exporter.read_bytes()
    if sha(exporter_bytes) != args.exporter_sha256:
        raise ValueError('reviewed local exporter source required')
    # Compile precisely the already-verified local source, never remote code.
    spec = importlib.util.spec_from_loader('retained_local_qwen_export', loader=None)
    module = importlib.util.module_from_spec(spec); module.__file__ = str(exporter)
    exec(compile(exporter_bytes, str(exporter), 'exec'), module.__dict__)
    torch.set_num_threads(1); torch.set_num_interop_threads(1)
    torch.manual_seed(7)
    if not torch.cuda.is_available(): raise ValueError('declared CUDA extraction profile unavailable')
    config, state, head, kind, asset_seals = module.load_candidate(ROOT / 'candidate-assets', MODEL_NAMES[args.model])
    model = module.make_wrapper(config, state, head, kind).cuda().eval()
    del state, head
    for parameter in model.parameters(): parameter.requires_grad_(False)
    features, initial, observed = {}, {}, []
    symbols = None
    torch.cuda.reset_peak_memory_stats()
    started = time.monotonic()
    for number, example in enumerate(examples):
        if time.monotonic() - started >= 900:
            raise TimeoutError('fixed feature extraction cap; partial data not trainable')
        compiled = rows[example['native_row_index']]['compiled']
        ids = torch.tensor([compiled['input_ids']], dtype=torch.long, device='cuda')
        mask = torch.tensor([compiled['attention_mask']], dtype=torch.long, device='cuda')
        positions = torch.tensor([compiled['position_ids']], dtype=torch.long, device='cuda')
        if ids.ndim != 2 or ids.shape[1] > 2048: raise ValueError('native vector shape or context changed')
        with torch.no_grad(): hidden = model.hidden(ids, mask, positions)[0]
        features[f'{number}.decide'] = hidden[compiled['decision_position']].cpu().contiguous()
        if kind == 'intern':
            candidates = compiled['candidate_positions']
            if symbols is None:
                symbols = candidates
                # Native rosters can contain fewer than eight cards; the original
                # tokenizer's A..H IDs must be supplied by the reviewed compiler asset.
            features[f'{number}.symbol_ids'] = torch.tensor(candidates, dtype=torch.int64)
        else:
            features[f'{number}.options'] = hidden[compiled['candidate_positions']].cpu().contiguous()
        observed.append({'native_input_digest': example['native_input_digest'], 'tokens': ids.shape[1]})
    if kind == 'intern':
        all_ids = sorted({i for example in examples for i in rows[example['native_row_index']]['compiled']['candidate_positions']})
        initial['symbol_ids'] = torch.tensor(all_ids, dtype=torch.int64)
        initial['weight'] = model.lm_head.weight[all_ids].detach().cpu().contiguous()
    else:
        initial.update({f'q.{k}': v.detach().cpu().contiguous() for k, v in model.q.state_dict().items()})
        initial.update({f'k.{k}': v.detach().cpu().contiguous() for k, v in model.k.state_dict().items()})
        if kind == 'strands':
            initial.update({f'norm.{k}': v.detach().cpu().contiguous() for k, v in model.norm.state_dict().items()})
    peak = torch.cuda.max_memory_allocated()
    if peak > 32 * 1024**3: raise ValueError('declared extraction GPU allocation cap exceeded')
    del model
    recheck(source, indices)
    pinned(exporter, args.exporter_sha256)
    # Verify all original asset identities again before declaring complete.
    for path, digest in asset_seals.items():
        if module.digest(path) != digest: raise ValueError('model asset drift')
    save_file(features, str(output / 'features.safetensors'))
    save_file(initial, str(output / 'initial-head.safetensors'))
    write_json(output / 'features.json', {'complete': True, 'model': args.model, 'kind': kind,
        'indices_sha256': INDEX_SHA, 'examples': examples, 'observed': observed,
        'asset_seals': asset_seals, 'exporter_sha256': args.exporter_sha256,
        'features_sha256': sha((output / 'features.safetensors').read_bytes()),
        'head_sha256': sha((output / 'initial-head.safetensors').read_bytes()),
        'elapsed_seconds': time.monotonic()-started, 'peak_allocated_gpu_bytes': peak,
        'script_sha256': sha(source), 'task_qualified': False})

def train(args):
    import torch
    from safetensors.torch import load, save_file
    source, indices, examples, rows, output = base(args)
    feature_raw = pinned(args.features / 'features.json', args.features_sha256)
    manifest = json.loads(feature_raw)
    if not manifest['complete'] or manifest['model'] != args.model or manifest['examples'] != examples:
        raise ValueError('foreign feature population/model')
    tensors = load(pinned(args.features / 'features.safetensors', manifest['features_sha256']))
    initial = load(pinned(args.features / 'initial-head.safetensors', manifest['head_sha256']))
    torch.set_num_threads(1); torch.set_num_interop_threads(1); torch.manual_seed(7)
    kind = manifest['kind']; params = {}
    for name, tensor in initial.items():
        if name != 'symbol_ids': params[name] = torch.nn.Parameter(tensor.float().clone())
    optimizer = torch.optim.AdamW(list(params.values()), lr=1e-4, weight_decay=0.0)
    symbol_rows = {v: n for n, v in enumerate(initial.get('symbol_ids', torch.empty(0)).tolist())}
    losses, started = [], time.monotonic()
    for epoch in range(6):
        for number, example in enumerate(examples):
            if time.monotonic()-started >= 120: raise TimeoutError('fixed head training cap')
            d = tensors[f'{number}.decide'].float()
            if kind == 'intern':
                ids = [symbol_rows[v] for v in tensors[f'{number}.symbol_ids'].tolist()]
                scores = params['weight'][ids] @ d
            else:
                o = tensors[f'{number}.options'].float()
                if kind == 'strands':
                    d = torch.nn.functional.layer_norm(d, d.shape, params['norm.weight'], params['norm.bias'])
                    o = torch.nn.functional.layer_norm(o, (o.shape[-1],), params['norm.weight'], params['norm.bias'])
                q = torch.nn.functional.linear(d, params['q.weight'], params['q.bias'])
                k = torch.nn.functional.linear(o, params['k.weight'], params['k.bias'])
                scores = (k @ q) * (q.numel() ** -0.5)
            # Positive-unlabeled set likelihood. Unknown cards are NEVER assigned
            # semantic negative labels; normalization still competes for mass.
            loss = torch.logsumexp(scores, 0) - torch.logsumexp(scores[example['positive_option_indices']], 0)
            if not torch.isfinite(loss): raise ValueError('nonfinite loss')
            optimizer.zero_grad(); loss.backward()
            torch.nn.utils.clip_grad_norm_(list(params.values()), 1.0, error_if_nonfinite=True)
            optimizer.step(); losses.append(float(loss.detach()))
    recheck(source, indices)
    pinned(args.features / 'features.json', args.features_sha256)
    result = {name: value.detach().contiguous() for name, value in params.items()}
    if kind == 'intern': result['symbol_ids'] = initial['symbol_ids']
    save_file(result, str(output / 'adapted-head.safetensors'))
    write_json(output / 'training.json', {'complete': True, 'model': args.model, 'kind': kind,
        'features_manifest_sha256': args.features_sha256, 'indices_sha256': INDEX_SHA,
        'steps': len(losses), 'losses': losses, 'elapsed_seconds': time.monotonic()-started,
        'seed': 7, 'epochs': 6, 'learning_rate': 1e-4, 'optimizer': 'AdamW', 'weight_decay': 0,
        'trainable': list(params), 'backbone_frozen': True, 'task_qualified': False,
        'adapted_head_sha256': sha((output / 'adapted-head.safetensors').read_bytes()),
        'script_sha256': sha(source), 'label_authority': indices['label_authority'],
        'limits': 'TRAIN-only weak proxy experiment. No semantic action labels, calibration, export or activation.'})

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['extract', 'train'])
    parser.add_argument('--model', choices=MODEL_NAMES, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--exporter-sha256')
    parser.add_argument('--features', type=Path)
    parser.add_argument('--features-sha256')
    args = parser.parse_args()
    if args.operation == 'extract' and not args.exporter_sha256: parser.error('reviewed exporter pin required')
    if args.operation == 'train' and not (args.features and args.features_sha256): parser.error('sealed features required')
    extract(args) if args.operation == 'extract' else train(args)

if __name__ == '__main__': main()
