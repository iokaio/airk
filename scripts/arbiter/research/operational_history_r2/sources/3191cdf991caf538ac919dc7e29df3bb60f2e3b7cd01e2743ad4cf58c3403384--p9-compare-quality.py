# SPDX-License-Identifier: Apache-2.0
"""Compare unchanged-corpus P9 quality with frozen P5 and preceding P8 evidence."""
import argparse
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'artifacts/v190'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, default=BASE / 'p9-relevance.json')
    parser.add_argument('--output', type=Path, default=BASE / 'p9-quality-comparison.json')
    args = parser.parse_args()
    candidate = json.loads(args.candidate.read_text(encoding='utf-8'))
    assert candidate['passed']
    assert candidate['binary_sha256'] == candidate['binary_sha256_after']
    actual, checks = candidate['representative'], []
    for filename in ('p5-188-relevance.json', 'p8-relevance.json'):
        prior = json.loads((BASE / filename).read_text(encoding='utf-8'))
        expected = prior['representative']
        assert prior['passed'] and actual['complete']
        assert actual['corpus_sha256'] == expected['corpus_sha256']
        assert actual['source_hashes'] == expected['source_hashes']
        assert set(actual['scores']) == set(expected['scores'])
        for group, metrics in expected['scores'].items():
            assert set(actual['scores'][group]) == set(metrics)
            for key, value in metrics.items():
                if key in ('latency_samples', 'p50_ms', 'p95_ms'):
                    continue
                observed = actual['scores'][group][key]
                passed = (observed is None if value is None else
                          math.isclose(observed, value, rel_tol=0, abs_tol=1e-12))
                checks.append({'reference': filename, 'group': group, 'metric': key,
                               'observed': observed, 'reference_value': value, 'passed': passed})
    result = {'schema': 'airk.p9.quality-comparison/1', 'passed': all(row['passed'] for row in checks),
              'candidate_sha256': hashlib.sha256(args.candidate.read_bytes()).hexdigest(), 'checks': checks}
    with args.output.open('x', encoding='utf-8') as destination:
        destination.write(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'passed': result['passed'], 'checks': len(checks)}))
    raise SystemExit(0 if result['passed'] else 1)


if __name__ == '__main__':
    main()
