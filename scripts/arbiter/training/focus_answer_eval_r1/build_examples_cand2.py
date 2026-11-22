# SPDX-License-Identifier: Apache-2.0
"""Candidate 2 examples: `build_examples.py` unchanged, then hold general replay rows out.

Runs `build_examples.main` with the given arguments, then moves a deterministic
share of the positive-only general replay rows (QASPER and TechQA) from
`train.jsonl` to `eval.jsonl` as set `replay_general_heldout`, so the candidate's
positive margin can be measured on general rows it never trained on. Counts and
the manifest are rewritten to match; every other row and the training order
are kept.
"""
import hashlib, json, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import build_examples as be  # noqa: E402

HELD_PER_KIND = 15


def split_replay(train):
    general = [r for r in train if r['set'] == 'replay_train' and r['kind'] in ('qasper', 'techqa')]
    chosen = set()
    for kind in ('qasper', 'techqa'):
        rows = sorted((r for r in general if r['kind'] == kind),
                      key=lambda r: hashlib.sha256(('replay-general-heldout\n' + r['id']).encode()).hexdigest())
        chosen |= {r['id'] for r in rows[:HELD_PER_KIND]}
    kept = [r for r in train if r['id'] not in chosen]
    moved = [dict(r, set='replay_general_heldout') for r in train if r['id'] in chosen]
    return kept, moved


def write_jsonl(path, rows):
    with path.open('w', encoding='utf-8', newline='\n') as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n')


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if '--per-path-cap' in argv:
        # Pairs per first evidence path; candidate 1 used the module default of 3.
        at = argv.index('--per-path-cap')
        be.PER_PATH_CAP = int(argv[at + 1])
        del argv[at:at + 2]
    out = Path(argv[argv.index('--output') + 1]).resolve()
    be.main(argv)
    read = lambda p: [json.loads(l) for l in p.read_text(encoding='utf-8').splitlines() if l.strip()]
    train, evaluation = read(out / 'train.jsonl'), read(out / 'eval.jsonl')
    kept, moved = split_replay(train)
    write_jsonl(out / 'train.jsonl', kept)
    write_jsonl(out / 'eval.jsonl', evaluation + moved)
    stats = json.loads((out / 'stats.json').read_bytes())
    stats['train_rows'] = len(kept)
    stats['eval_rows'] = len(evaluation) + len(moved)
    stats['train_labels'] = {str(k): sum(1 for r in kept if r['label'] == k) for k in (0, 1)}
    stats['counts']['replay_train_rows'] -= len(moved)
    stats['counts']['replay_general_heldout_rows'] = len(moved)
    (out / 'stats.json').write_text(json.dumps(stats, indent=2) + '\n', encoding='utf-8', newline='\n')
    manifest = json.loads((out / 'manifest.json').read_bytes())
    manifest['inputs'][str(Path(__file__).resolve())] = be.file_sha(Path(__file__))
    manifest['outputs'] = {n: be.file_sha(out / n) for n in ('train.jsonl', 'eval.jsonl', 'stats.json')}
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'train_rows': len(kept), 'eval_rows': len(evaluation) + len(moved), 'replay_general_heldout': len(moved)}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
