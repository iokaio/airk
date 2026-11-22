# SPDX-License-Identifier: Apache-2.0
"""Compare CAL panel replays across precisions and devices (plan 12.5.2 / 12.7.3).

Inputs are ``l5a_replay`` receipts (one per CUDA route) and the retained CPU
reference batches of the CAL R7 panel (``cal100-compact-protected-r7-*``
``execution.json``, roles ``publisher_fp32`` and ``matching``). Rows join on
the panel ordinal and must carry the same compiled input digest.

For every pair (candidate, reference) it reports the maximum absolute raw
score error, the maximum softmax mass error (as ``runtime/cuda/numeric.rs``
computes them), winner agreement, the reference top-two margin, and every
flip, marking a flip a near-tie when the reference margin is at most twice the
observed maximum raw error. Acceptance limits follow
``scripts/arbiter-fp16-export/CONTRACT.md`` (Context: raw <= 0.007, mass <= 0.05).

Run: python -I l5a_replay_compare.py --replay NAME=PATH ... --cpu-ref PATH ...
     --pair CAND:REF ... --output OUT.json
"""
import argparse, hashlib, json, math, pathlib


def softmax(values):
    peak = max(values)
    exps = [math.exp(v - peak) for v in values]
    total = sum(exps)
    return [e / total for e in exps]


def winner(values):
    best = 0
    for i, v in enumerate(values):
        if v > values[best]:
            best = i
    return best


def margin(values):
    ordered = sorted(values, reverse=True)
    return ordered[0] - ordered[1] if len(ordered) > 1 else math.inf


def load_replay(path):
    data = json.loads(path.read_bytes())
    if not data.get('complete') or len(data['rows']) != data['rows_requested']:
        raise ValueError(f'{path} is not a complete replay of every requested row')
    return {r['ordinal']: (r['input_digest'], r['scores']) for r in data['rows']}, data


def load_cpu(path):
    data = json.loads(path.read_bytes())
    rows = data['result']['rows']
    role = data['role']
    return role, {r['ordinal']: (r['compiled']['input_digest'], r['scores']) for r in rows}


def compare(candidate, reference):
    # Every reference row must be compared (a replay reference has its full requested roster).
    missing = sorted(set(reference) - set(candidate))
    if missing:
        raise ValueError(f'reference ordinals absent from the candidate: {missing[:10]}')
    rows = []
    for ordinal in sorted(set(candidate) & set(reference)):
        (cd, cs), (rd, rs) = candidate[ordinal], reference[ordinal]
        if cd != rd or len(cs) != len(rs):
            raise ValueError(f'ordinal {ordinal}: input digest or card count differs')
        raw = max(abs(a - b) for a, b in zip(cs, rs))
        mass = max(abs(a - b) for a, b in zip(softmax(cs), softmax(rs)))
        rows.append({'ordinal': ordinal, 'raw': raw, 'mass': mass, 'same_winner': winner(cs) == winner(rs),
                     'reference_margin': margin(rs), 'candidate_winner': winner(cs), 'reference_winner': winner(rs)})
    if not rows:
        return {'rows': 0}
    max_raw = max(r['raw'] for r in rows)
    flips = [r for r in rows if not r['same_winner']]
    return {
        'rows': len(rows), 'max_raw': max_raw, 'max_mass': max(r['mass'] for r in rows),
        'median_raw': sorted(r['raw'] for r in rows)[len(rows) // 2],
        'winner_agreement': len(rows) - len(flips),
        'flips': [{**f, 'near_tie': f['reference_margin'] <= 2 * max_raw} for f in flips],
        'min_reference_margin': min(r['reference_margin'] for r in rows),
        'context_raw_limit_0_007': max_raw <= 0.007, 'context_mass_limit_0_05': max(r['mass'] for r in rows) <= 0.05,
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--replay', action='append', default=[], help='NAME=PATH of an l5a_replay receipt')
    p.add_argument('--cpu-ref', action='append', default=[], help='CPU reference execution.json (merged by role)')
    p.add_argument('--pair', action='append', default=[], help='CANDIDATE:REFERENCE names to compare')
    p.add_argument('--output', type=pathlib.Path, required=True)
    args = p.parse_args()
    sources, pins = {}, {}
    for item in args.replay:
        name, _, path = item.partition('=')
        path = pathlib.Path(path)
        sources[name], data = load_replay(path)
        pins[name] = {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                      'profile': data['profile']['manifest'], 'rows': len(data['rows'])}
    for item in args.cpu_ref:
        path = pathlib.Path(item)
        role, rows = load_cpu(path)
        name = 'cpu_' + role
        sources.setdefault(name, {}).update(rows)
        pins.setdefault(name, {'files': []})['files'].append({'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    result = {'schema': 'airk.private.l5a-replay-compare/1', 'sources': pins, 'pairs': {}}
    for pair in args.pair:
        cand, _, ref = pair.partition(':')
        result['pairs'][pair] = compare(sources[cand], sources[ref])
    result['comparer_sha256'] = hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
    with args.output.open('x', encoding='utf-8') as handle:
        json.dump(result, handle, indent=2)
        handle.write('\n')
    for pair, r in result['pairs'].items():
        if r['rows']:
            print(f"{pair}: rows={r['rows']} max_raw={r['max_raw']:.6g} max_mass={r['max_mass']:.6g} "
                  f"winners={r['winner_agreement']}/{r['rows']} flips={len(r['flips'])} near_tie_flips={sum(f['near_tie'] for f in r['flips'])}")
        else:
            print(f'{pair}: no shared rows')


if __name__ == '__main__':
    main()
