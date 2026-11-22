# SPDX-License-Identifier: Apache-2.0
"""Write a deterministic sample of SoftwareEvidenceV1 example rows for the native compiler parity check.

Output rows are `{query, document, live_ids}`, read by the ignored Rust test
`crates/airk-retrieval/tests/focus_answer_eval_candidate.rs`.
"""
import argparse, hashlib, json
from pathlib import Path


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--examples', type=Path, nargs='+', required=True)
    p.add_argument('--rows', type=int, default=400)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args(argv)
    if args.output.exists():
        raise SystemExit('parity_sample: fresh output required')
    rows = []
    for path in args.examples:
        rows += [json.loads(l) for l in path.read_text(encoding='utf-8').splitlines() if l.strip()]
    rows = [r for r in rows if r['instruction'] == 'software_evidence_v1']
    key = lambda r: hashlib.sha256(('native-parity\n' + r['id']).encode()).hexdigest()
    sample = sorted(rows, key=key)[: args.rows]
    with args.output.open('w', encoding='utf-8', newline='\n') as f:
        for r in sample:
            f.write(json.dumps({'id': r['id'], 'query': r['query'], 'document': r['document'], 'live_ids': r['live_ids']}, ensure_ascii=False) + '\n')
    print(json.dumps({'rows': len(sample), 'sha256': hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == '__main__':
    main()
