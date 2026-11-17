# SPDX-License-Identifier: Apache-2.0
"""Read-only independent diagnostic over already exposed CAL receipts; no model execution."""
import collections
import hashlib
import json
import math
from pathlib import Path
import struct
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parent
FAMILIES = ['smallintern', 'strands', 'intern4', 'kev4']

def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]

def read(path):
    raw = path.read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()

def run():
    report_path = ROOT / 'context-action-native-calibration-fit-r40/generated/fit-report.private.json'
    report, report_sha = read(report_path)
    assert report['complete'] and len(report['models']) == 4
    results = []
    seals = {str(report_path): report_sha}
    for family in FAMILIES:
        path = ROOT / f'context-action-{family}-raw-r24/raw-scores.private.json'
        raw, digest = read(path)
        seals[str(path)] = digest
        assert raw['complete'] and not raw['task_qualified']
        fitted = next(m for m in report['models'] if m['model'] == raw['model'])
        assert fitted['status'] == 'unsupported_no_positive_safe_margin'
        assert fitted['calibration'] is None and fitted['experimental_profile'] is None
        outcomes = {r['id']: r['diagnostic_zero_margin_replay'] for r in fitted['outcomes']}
        positives, unknowns, counts = [], [], collections.Counter()
        rows = [r for r in raw['rows'] if r['split'] == 'calibration']
        if not rows:
            rows = [r for r in raw['rows'] if r['split'] == 'cal']
        assert len(rows) == 19 and len({r['id'] for r in rows}) == 19
        for row in rows:
            expected = outcomes[row['id']]
            scores = [f32(s) for s in row['scores']] if row['status'] == 'raw_scores_only' else []
            cards = row['roster']['cards']
            assert all(math.isfinite(s) for s in scores)
            selected = None
            if len(scores) >= 2:
                order = sorted(range(len(scores)), key=lambda i: scores[i], reverse=True)
                top, second = order[:2]
                card = cards[top]
                margin = f32(scores[top] - scores[second])
                supported = card['action']['kind'] == 'read' and card['gap'] == 'unsupported'
                supported &= card['reason'] == 'additional_evidence_may_help'
                supported &= all(a['role'] == 'unknown' and a['relevance'] == 'uncertain' for a in card['assessments'])
                if scores[top] != scores[second] and supported:
                    selected = card['id']
                    cert = {p['card'] for p in row['reference_proxies']['valid_reference_proxy_whole_cards']}
                    (positives if selected in cert else unknowns).append(margin)
                    counts['certified' if selected in cert else 'uncertified_unknown'] += 1
                else:
                    counts['unsupported_top_or_tie'] += 1
            else:
                counts['compiler_refused_or_insufficient_options'] += 1
            assert selected == expected['selected_card'], 'independent zero-margin replay differs'
        # A lower-bound margin selector cannot admit any positive without an
        # unknown if the highest unknown margin is >= the highest positive.
        blocked = not positives or bool(unknowns and max(unknowns) >= max(positives))
        assert blocked, 'safe positive margin may exist; investigate rather than promote'
        results.append({'model': raw['model'], 'cal_questions': len(rows), 'counts': dict(counts),
                        'maximum_certified_margin': max(positives, default=None),
                        'maximum_uncertified_unknown_margin': max(unknowns, default=None),
                        'independent_no_positive_safe_margin_proof': blocked,
                        'all_zero_margin_replay_matches_native_report': True})
    assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == sha for p, sha in seals.items())
    output = ROOT / 'context-action-independent-fit-audit-r41.json'
    result = {'schema': 'airk.private-independent-fit-audit/1', 'utc': datetime.now(timezone.utc).isoformat(),
              'complete': True, 'inputs': seals, 'models': results,
              'limits': 'Independent existing CAL-only arithmetic diagnostic; unknown is not semantic false. No model/FINAL execution, calibration assets, qualification or activation.',
              'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    with output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'output': str(output), 'sha256': hashlib.sha256(output.read_bytes()).hexdigest(), 'models': results}))

if __name__ == '__main__':
    run()
