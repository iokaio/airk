# SPDX-License-Identifier: Apache-2.0
"""Before/after Focus metrics and the memorization gap from an owned worker result.

Inputs: the evaluation examples (`eval.jsonl`) and the worker's `result.json`
(base and candidate [id, no, yes] logits). Output: a fresh JSON report with
AUROC, hit@1, MRR and mean positive margin per set and repository, and the
train-minus-held-out gap for both held-out splits (id hash and path group).
"""
import argparse, hashlib, json, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import worker_math as wm  # noqa: E402

SETS = ('heldout_id', 'heldout_path', 'train_sample', 'replay_dev')


def gaps(metrics):
    out = {}
    for repo in ('airk', 'bookbuilder', 'all'):
        train = metrics.get(f'train_sample/{repo}')
        for held in ('heldout_id', 'heldout_path'):
            other = metrics.get(f'{held}/{repo}')
            if not train or not other:
                continue
            out[f'{repo}:train_minus_{held}'] = {
                k: (train[k] - other[k]) if train[k] is not None and other[k] is not None else None
                for k in ('auroc', 'hit_at_1', 'mrr')}
    return out


def report(rows, result):
    out = {'schema': 'airk.private.focus-answer-eval-score/1', 'arms': {}}
    for arm in ('base', 'candidate'):
        if arm in result['scores']:
            metrics = wm.summarize(rows, result['scores'][arm])
            out['arms'][arm] = {'metrics': metrics, 'gaps': gaps(metrics)}
    if len(out['arms']) == 2:
        base, cand = out['arms']['base']['gaps'], out['arms']['candidate']['gaps']
        out['memorization_signal'] = {k: {m: (cand[k][m] - base[k][m]) if cand[k][m] is not None and base[k][m] is not None else None
                                          for m in cand[k]} for k in cand if k in base}
    return out


def table(out):
    lines = ['| Set / repo | rows | AUROC base | AUROC cand | hit@1 base | hit@1 cand | MRR base | MRR cand |', '| --- | --- | --- | --- | --- | --- | --- | --- |']
    base = out['arms']['base']['metrics']; cand = out['arms'].get('candidate', {}).get('metrics', {})
    f = lambda v: '-' if v is None else f'{v:.4f}'
    for key in sorted(base):
        if key.count('/') != 1:
            continue
        b, c = base[key], cand.get(key, {})
        lines.append(f"| {key} | {b['rows']} | {f(b['auroc'])} | {f(c.get('auroc'))} | {f(b['hit_at_1'])} | {f(c.get('hit_at_1'))} | {f(b['mrr'])} | {f(c.get('mrr'))} |")
    return '\n'.join(lines)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--eval', type=Path, required=True)
    p.add_argument('--result', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args(argv)
    if args.output.exists():
        raise SystemExit('score: fresh output required')
    rows = [json.loads(l) for l in args.eval.read_text(encoding='utf-8').splitlines() if l.strip()]
    result = json.loads(args.result.read_bytes())
    out = report(rows, result)
    for name, path in (('eval', args.eval), ('result', args.result)):
        out[name + '_sha256'] = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    args.output.write_text(json.dumps(out, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(table(out))
    print(json.dumps({'gaps': {a: v['gaps'] for a, v in out['arms'].items()}, 'memorization_signal': out.get('memorization_signal')}, indent=1))


if __name__ == '__main__':
    main()
