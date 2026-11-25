# SPDX-License-Identifier: Apache-2.0
"""Before/after Focus metrics and the memorization gap from an owned worker result.

Inputs: the evaluation examples (`eval.jsonl`) and the worker's `result.json`
(base and candidate [id, no, yes] logits). Output: a fresh JSON report with
AUROC, hit@1, MRR and mean positive margin per set and repository, and the
train-minus-held-out gap for both held-out splits (id hash and path group).
"""
import argparse, hashlib, json, math, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import worker_math as wm  # noqa: E402

SETS = ('heldout_id', 'heldout_path', 'train_sample', 'replay_dev')
L7_REPOS = ('bookbuilder', 'munarium')
HELDOUT = ('heldout_id', 'heldout_path')
GENERAL = 'replay_general_heldout'
ARMS = ('base', 'candidate')


def gaps(metrics, repos=('airk', 'bookbuilder', 'all')):
    out = {}
    for repo in repos:
        train = metrics.get(f'train_sample/{repo}')
        for held in ('heldout_id', 'heldout_path'):
            other = metrics.get(f'{held}/{repo}')
            if not train or not other:
                continue
            out[f'{repo}:train_minus_{held}'] = {
                k: (train[k] - other[k]) if train[k] is not None and other[k] is not None else None
                for k in ('auroc', 'hit_at_1', 'mrr')}
    return out


def report(rows, result, *, repos=('airk', 'bookbuilder', 'all')):
    out = {'schema': 'airk.private.focus-answer-eval-score/1', 'arms': {}}
    for arm in ('base', 'candidate'):
        if arm in result['scores']:
            metrics = wm.summarize(rows, result['scores'][arm])
            out['arms'][arm] = {'metrics': metrics, 'gaps': gaps(metrics, repos)}
    if len(out['arms']) == 2:
        base, cand = out['arms']['base']['gaps'], out['arms']['candidate']['gaps']
        out['memorization_signal'] = {k: {m: (cand[k][m] - base[k][m]) if cand[k][m] is not None and base[k][m] is not None else None
                                          for m in cand[k]} for k in cand if k in base}
    return out


def report_l7(rows, result):
    """Score both arms over the same complete owned result and disclose positive-only replay."""
    if (result.get('schema') != 'airk.private.focus-answer-eval-worker-result/1'
            or result.get('mode') != 'run' or result.get('complete') is not True or result.get('failure')
            or type(result.get('gradient_steps')) is not int or result['gradient_steps'] < 1
            or result['gradient_steps'] != result.get('optimizer_steps_planned')):
        raise ValueError('complete owned Focus weight run required')
    ids = [row['id'] for row in rows]
    if not ids or len(set(ids)) != len(ids):
        raise ValueError('unique evaluation row identities required')
    heldout = set()
    general_rows = 0
    for row in rows:
        split, repo, label = row['set'], row['repo'], row['label']
        wm.target(label)
        if repo in L7_REPOS and split in (*HELDOUT, 'train_sample'):
            if row.get('instruction') != 'publisher':
                raise ValueError('local L7 rows must use the publisher instruction')
            heldout.add((repo, split))
        elif repo == 'replay' and split == GENERAL and label == 1:
            general_rows += 1
        elif repo != 'replay' or split != 'replay_dev':
            raise ValueError('unexpected L7 repository or split, or nonpositive general replay')
    if not general_rows or any((repo, split) not in heldout for repo in L7_REPOS for split in HELDOUT):
        raise ValueError('both local held-out splits and positive-only general replay are required')
    for arm in ARMS:
        scores = result.get('scores', {}).get(arm)
        if (not isinstance(scores, list) or any(
            not isinstance(score, list) or len(score) != 3
            or any(type(value) not in (int, float) or not math.isfinite(value) for value in score[1:])
            for score in scores
        ) or [score[0] for score in scores] != ids):
            raise ValueError(f'{arm}: exact ordered finite score roster required')
    out = report(rows, result, repos=(*L7_REPOS, 'all'))
    out['mode'] = 'l7_publisher_pilot'
    general = {'set': GENERAL, 'rows': general_rows, 'positive_rows': general_rows, 'negative_rows': 0,
               'ranking_and_auc': 'undefined_without_negatives', 'arms': {}}
    heldout_cells = {f'{split}/{repo}' for repo in (*L7_REPOS, 'all') for split in HELDOUT}
    for arm in ARMS:
        cells = out['arms'][arm]['metrics']
        general['arms'][arm] = {key.split('/', 1)[1]: cells.pop(key)
                                for key in list(cells) if key.startswith(GENERAL + '/')}
    base, candidate = (out['arms'][arm]['metrics'] for arm in ARMS)
    out['heldout_candidate_minus_base'] = {
        cell: {metric: candidate[cell][metric] - base[cell][metric]
               if candidate[cell][metric] is not None and base[cell][metric] is not None else None
               for metric in ('auroc', 'hit_at_1', 'mrr', 'mean_positive_margin')}
        for cell in sorted(heldout_cells)
    }
    out['general_replay'] = general
    return out


def read_pinned(path, expected):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError(f'input SHA-256 differs: {path.name}')
    return raw


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
    p.add_argument('--l7-publisher-pilot', action='store_true')
    p.add_argument('--eval-sha256')
    p.add_argument('--result-sha256')
    args = p.parse_args(argv)
    if args.l7_publisher_pilot and not (args.eval_sha256 and args.result_sha256):
        p.error('L7 publisher pilot requires both audited input SHA-256 values')
    if not args.l7_publisher_pilot and (args.eval_sha256 or args.result_sha256):
        p.error('explicit input pins require --l7-publisher-pilot')
    if args.output.exists():
        raise SystemExit('score: fresh output required')
    if args.l7_publisher_pilot:
        evaluation = read_pinned(args.eval, args.eval_sha256)
        worker = read_pinned(args.result, args.result_sha256)
        rows = [json.loads(line) for line in evaluation.decode('utf-8').splitlines() if line.strip()]
        out = report_l7(rows, json.loads(worker))
        out.update(eval_sha256=args.eval_sha256, result_sha256=args.result_sha256)
    else:
        rows = [json.loads(l) for l in args.eval.read_text(encoding='utf-8').splitlines() if l.strip()]
        result = json.loads(args.result.read_bytes())
        out = report(rows, result)
        for name, path in (('eval', args.eval), ('result', args.result)):
            out[name + '_sha256'] = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    args.output.write_text(json.dumps(out, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(table(out))
    if args.l7_publisher_pilot:
        print(json.dumps({'general_replay': out['general_replay']}, indent=1))
    print(json.dumps({'gaps': {a: v['gaps'] for a, v in out['arms'].items()}, 'memorization_signal': out.get('memorization_signal')}, indent=1))


if __name__ == '__main__':
    main()
