# SPDX-License-Identifier: Apache-2.0
"""Pure schedule, padding, target and metric logic. Never imports Torch."""
import collections
import math

PAD_ID = 151643
NO_ID = 2152
YES_ID = 9693


def schedule(rows, max_rows=None):
    """The example file's order is the training order (hash order, replay interleaved)."""
    if not rows or len({r['id'] for r in rows}) != len(rows):
        raise ValueError('distinct training rows required')
    for row in rows:
        target(row['label'])
    return rows if max_rows is None else rows[:max_rows]


def batches(rows, size):
    if size < 1:
        raise ValueError('batch size')
    return [rows[i:i + size] for i in range(0, len(rows), size)]


def left_pad(ids, width):
    pad = width - len(ids)
    if pad < 0 or not ids:
        raise ValueError('over-width or empty pair; truncation is forbidden')
    return [PAD_ID] * pad + list(ids), [0] * pad + [1] * len(ids)


def target(label):
    if type(label) is not int or label not in (0, 1):
        raise ValueError('unknown relevance label')
    return (NO_ID, YES_ID)[label]


def auroc(labels, margins):
    pos = sorted(m for y, m in zip(labels, margins) if y == 1)
    neg = sorted(m for y, m in zip(labels, margins) if y == 0)
    if not pos or not neg:
        return None
    # Mann-Whitney U with ties counted half, O(n log n).
    wins, j, k = 0.0, 0, 0
    for p in pos:
        while j < len(neg) and neg[j] < p:
            j += 1
        k = j
        while k < len(neg) and neg[k] == p:
            k += 1
        wins += j + 0.5 * (k - j)
    return wins / (len(pos) * len(neg))


def ranking(groups):
    """groups: {pair_id: [(label, margin), ...]} -> hit@1 and MRR of the first positive."""
    hits, rr, n = 0, 0.0, 0
    for rows in groups.values():
        if not any(y == 1 for y, _ in rows) or not any(y == 0 for y, _ in rows):
            continue
        ordered = sorted(rows, key=lambda r: -r[1])
        rank = next(i for i, (y, _) in enumerate(ordered, 1) if y == 1)
        hits += rank == 1
        rr += 1.0 / rank
        n += 1
    return {'pairs': n, 'hit_at_1': hits / n if n else None, 'mrr': rr / n if n else None}


def summarize(rows, scores):
    """Metrics per (set, repo) and per (set, repo, kind) from eval rows and [id, no, yes] scores."""
    by_id = {s[0]: s[2] - s[1] for s in scores}
    if set(by_id) != {r['id'] for r in rows}:
        raise ValueError('score rows differ from evaluation rows')
    cells = collections.defaultdict(list)
    for r in rows:
        margin = by_id[r['id']]
        if not math.isfinite(margin):
            raise ValueError('nonfinite margin')
        for key in ((r['set'], r['repo']), (r['set'], r['repo'], r['kind']), (r['set'], 'all')):
            cells[key].append((r, margin))
    out = {}
    for key, items in sorted(cells.items()):
        labels = [r['label'] for r, _ in items]
        margins = [m for _, m in items]
        groups = collections.defaultdict(list)
        for r, m in items:
            groups[r['pair_id']].append((r['label'], m))
        pos = [m for y, m in zip(labels, margins) if y == 1]
        out['/'.join(key)] = {'rows': len(items), 'positives': len(pos), 'auroc': auroc(labels, margins),
                              'mean_positive_margin': sum(pos) / len(pos) if pos else None,
                              'positive_above_zero': sum(m > 0 for m in pos) / len(pos) if pos else None,
                              **ranking(groups)}
    return out
