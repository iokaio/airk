# SPDX-License-Identifier: Apache-2.0
"""CPU ONNX Runtime check of exported static-1024 weight8 Focus assets against PyTorch.

Scores a deterministic sample of evaluation rows with the base and candidate
asset folders on CPU (the evaluation fixture's runtime) and compares the raw
no/yes logits and relevance score with the FP32 PyTorch logits the owned
worker recorded. The weight8 tolerances are the exporter's (raw 0.25, score 0.05).
"""
import argparse, hashlib, json, math, sys, time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import worker_math as wm  # noqa: E402

RAW_TOLERANCE = 0.25
SCORE_TOLERANCE = 0.05


def relevance(no, yes):
    m = max(no, yes)
    return math.exp(yes - m) / (math.exp(no - m) + math.exp(yes - m))


def session(folder, threads):
    import onnxruntime as ort
    options = ort.SessionOptions(); options.intra_op_num_threads = threads; options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    s = ort.InferenceSession(str(Path(folder) / 'model.onnx'), options, providers=['CPUExecutionProvider'])
    if [i.name for i in s.get_inputs()] != ['input_ids', 'attention_mask', 'position_ids'] or [o.name for o in s.get_outputs()] != ['logits']:
        raise ValueError('Focus ABI differs')
    return s


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--eval', type=Path, required=True)
    p.add_argument('--result', type=Path, required=True)
    p.add_argument('--base-assets', type=Path, required=True)
    p.add_argument('--candidate-assets', type=Path, required=True)
    p.add_argument('--rows', type=int, default=12)
    p.add_argument('--threads', type=int, default=8)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args(argv)
    if args.output.exists():
        raise SystemExit('onnx_parity: fresh output required')
    rows = [json.loads(l) for l in args.eval.read_text(encoding='utf-8').splitlines() if l.strip()]
    rows = [r for r in rows if r['set'] in ('heldout_id', 'heldout_path')]
    sample = sorted(rows, key=lambda r: hashlib.sha256(('parity\n' + r['id']).encode()).hexdigest())[: args.rows]
    result = json.loads(args.result.read_bytes())
    torch_scores = {arm: {s[0]: (s[1], s[2]) for s in result['scores'][arm]} for arm in ('base', 'candidate')}
    report = {'schema': 'airk.private.focus-answer-eval-onnx-parity/1', 'rows': [], 'arms': {}}
    for arm, folder in (('base', args.base_assets), ('candidate', args.candidate_assets)):
        start = time.monotonic(); s = session(folder, args.threads); raw_max = score_max = 0.0
        for r in sample:
            ids, mask = wm.left_pad(r['live_ids'], 1024)
            feed = {'input_ids': np.array([ids], np.int64), 'attention_mask': np.array([mask], np.int64),
                    'position_ids': np.array([list(range(1024))], np.int64)}
            no, yes = (float(x) for x in s.run(['logits'], feed)[0][0])
            tno, tyes = torch_scores[arm][r['id']]
            raw = max(abs(no - tno), abs(yes - tyes)); score = abs(relevance(no, yes) - relevance(tno, tyes))
            raw_max, score_max = max(raw_max, raw), max(score_max, score)
            report['rows'].append({'arm': arm, 'id': r['id'], 'label': r['label'], 'onnx': [no, yes], 'torch': [tno, tyes], 'raw_error': raw, 'score_error': score})
        report['arms'][arm] = {'assets': str(folder), 'data_sha256': hashlib.sha256((Path(folder) / 'model.onnx.data').read_bytes()).hexdigest(),
                               'max_raw_error': raw_max, 'max_score_error': score_max,
                               'within_tolerance': raw_max <= RAW_TOLERANCE and score_max <= SCORE_TOLERANCE,
                               'seconds': time.monotonic() - start}
        del s
    margins = {(x['arm'], x['id']): x['onnx'][1] - x['onnx'][0] for x in report['rows']}
    report['candidate_minus_base_margin_onnx'] = [margins[('candidate', r['id'])] - margins[('base', r['id'])] for r in sample]
    report['candidate_minus_base_margin_torch'] = [(torch_scores['candidate'][r['id']][1] - torch_scores['candidate'][r['id']][0])
                                                   - (torch_scores['base'][r['id']][1] - torch_scores['base'][r['id']][0]) for r in sample]
    args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps(report['arms'], indent=2))


if __name__ == '__main__':
    main()
