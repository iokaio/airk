# SPDX-License-Identifier: Apache-2.0
"""Replay exact fitted bytes, or fit an explicitly unqualified local experiment.

No vocabulary extraction, corpus regeneration, network or product-asset write.
Without extra examples, the frozen calibration and exact asset hash must match.
"""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.multiclass import OneVsRestClassifier

from broaden import asset, calibrate, metrics, normalized, save_report, scores
from qualify import quantize
from retention import retain, WORK
from train import LABELS, ROOT, matrix


def load_examples(path, record=None):
    raw = path.read_bytes()
    rows = [json.loads(line) for line in raw.splitlines()]
    if record and (hashlib.sha256(raw).hexdigest() != record['sha256'] or len(rows) != record['rows']):
        raise ValueError('Saved dataset differs from its exact recorded bytes or row count')
    for row in rows:
        if (not isinstance(row.get('text'), str) or not 0 < len(row['text'].encode('utf-8')) <= 4096
                or row.get('intent') not in LABELS + ['unsupported']):
            raise ValueError('Examples require bounded text and a declared finite intent or unsupported label')
    return rows, {'path': str(path), 'sha256': hashlib.sha256(raw).hexdigest(), 'rows': len(rows)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, default=Path('tools/parser-train/qualification-broader-v5.json'))
    parser.add_argument('--extra-training', type=Path, help='Append original labeled JSONL examples; creates a local experiment, never a promotion')
    args = parser.parse_args()
    report = json.loads(args.report.read_text(encoding='utf-8'))
    parameters = report['fit_parameters']
    if (parameters['feature_spec'] != 2 or parameters['solver'] != 'liblinear'
            or parameters['algorithm'] != 'OneVsRestClassifier(LogisticRegression)'):
        raise ValueError('This replay supports the recorded feature-2 logistic regression contract')
    rows, fitted = load_examples(ROOT / report['fitted_training']['path'], report['fitted_training'])
    extras = None
    held = {}
    if args.extra_training:
        added, extras = load_examples(args.extra_training)
        for split in ('validation', 'calibration'):
            held[split], _ = load_examples(ROOT / report['datasets'][split]['path'], report['datasets'][split])
        held_texts = {normalized(row['text']) for values in held.values() for row in values}
        if any(normalized(row['text']) in held_texts for row in added):
            raise ValueError('Extra training examples overlap the frozen validation/calibration texts')
        rows += added
    x = matrix([row['text'] for row in rows], parameters['feature_spec'])
    y = np.array([[int(row['intent'] == label) for label in LABELS] for row in rows])
    model = OneVsRestClassifier(LogisticRegression(C=parameters['C'], max_iter=parameters['max_iter'],
                                                  solver=parameters['solver'], random_state=parameters['random_state']), n_jobs=1)
    model.fit(x, y)
    weights, bias = quantize(model)
    threshold, margin = report['threshold'], report['margin']
    observations = {}
    if args.extra_training:
        threshold, margin = calibrate(held['calibration'], scores(held['calibration'], weights, bias, 2))
        observations = {split: metrics(values, scores(values, weights, bias, 2), threshold, margin)
                        for split, values in held.items()}
    content = asset(weights, bias, threshold, margin)
    digest = hashlib.sha256(content).hexdigest()
    if not args.extra_training and digest != report['asset_sha256']:
        raise ValueError(f"Exact selected-data replay differs: expected {report['asset_sha256']}, observed {digest}; product asset preserved")
    destination = WORK / 'models' / ('intent-' + digest[:16] + '.bin')
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and destination.read_bytes() != content:
        raise ValueError('Existing immutable model differs; it was preserved')
    if not destination.exists():
        destination.write_bytes(content)
    retain(destination)
    receipt = {'schema': 1, 'protocol': 'saved-data-experiment/1' if args.extra_training else 'saved-data-replay/1',
               'base_report': str(args.report), 'base_report_sha256': hashlib.sha256(args.report.read_bytes()).hexdigest(),
               'fitted_training': report['fitted_training'], 'fit_rows': len(rows), 'extra_training': extras,
               'fit_parameters': parameters, 'threshold': threshold, 'margin': margin,
               'asset_sha256': digest, 'asset_bytes': len(content), 'model_path': destination.relative_to(ROOT).as_posix(),
               'exact_reproduction': not bool(args.extra_training), 'automatic_execution': False, 'qualified': False,
               'held_out_observations': observations,
               'scope': 'Local experimental artifact or exact selected-data replay only. No product asset, manifest or catalog changes. No final regression fixtures evaluated or qualification/promotion inferred.'}
    saved = save_report(receipt, 'saved-data-experiment' if args.extra_training else 'saved-data-replay')
    print(json.dumps({'receipt': saved.relative_to(ROOT).as_posix(), 'asset_sha256': digest,
                      'fit_rows': len(rows), 'exact_reproduction': receipt['exact_reproduction'],
                      'qualified': False, 'product_changed': False}), flush=True)


if __name__ == '__main__':
    main()
