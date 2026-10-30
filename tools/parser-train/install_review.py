# SPDX-License-Identifier: Apache-2.0
"""Install a measured model for explicitly confirmed local-review proposals only."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct

import numpy as np

from broaden import scores
from retention import ROOT, WORK, retain
from train import LABELS

HERE = Path(__file__).parent


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('report', type=Path)
    parser.add_argument('--suggestions-only', action='store_true', required=True)
    args = parser.parse_args()
    report = json.loads(args.report.read_text(encoding='utf-8'))
    if (report['regression_fixtures']['evaluation']['intent_accuracy'] < .92
            or report['regression_fixtures']['evaluation']['wrong_accepted'] != 0
            or not report['gates']['repository_accuracy'] or not report['gates']['role_accuracy']
            or not report['gates']['public_unsupported']):
        raise ValueError('Review accuracy/precision checks failed; product asset preserved')
    model = ROOT / report['model_path']
    content = model.read_bytes()
    if (len(content) != 852116 or content[:8] != b'AIRKINT1'
            or hashlib.sha256(content).hexdigest() != report['asset_sha256']
            or content[32:64] != hashlib.sha256('\n'.join(LABELS).encode()).digest()
            or content[64:96] != hashlib.sha256(content[96:]).digest()):
        raise ValueError('Review model integrity check failed')
    asset = ROOT / 'crates/airk-retrieval/assets/intent-v1.bin'
    prior = asset.read_bytes()
    prior_hash = hashlib.sha256(prior).hexdigest()
    saved = WORK / 'models' / ('intent-' + prior_hash[:16] + '.bin')
    if not saved.exists():
        saved.write_bytes(prior)
    retain(saved)
    previous = json.loads((HERE / 'manifest.json').read_text(encoding='utf-8'))
    if previous.get('corpus', '').startswith('original fictional') and not (HERE / 'manifest-smoke.json').exists():
        shutil.copyfile(HERE / 'manifest.json', HERE / 'manifest-smoke.json')
    asset.write_bytes(content)
    threshold, margin = struct.unpack_from('<ii', content, 24)
    manifest = {'model_version': 'intent-broader/2', 'feature_spec': 1, 'seed': report['seed'],
                'examples': report['datasets']['train']['rows'], 'datasets': report['datasets'],
                'asset_bytes': len(content), 'asset_sha256': report['asset_sha256'],
                'threshold': threshold, 'margin': margin, 'automatic_execution': False,
                'proposal_confirmation_required': True, 'qualified': report['qualified'],
                'qualification_report': args.report.resolve().relative_to(ROOT).as_posix(),
                'qualification_report_sha256': hashlib.sha256(args.report.read_bytes()).hexdigest(),
                'gates': report['gates'], 'online_lock_sha256': report['online_lock_sha256'],
                'qualification': 'Local VSIX review candidate. Learned text yields inert proposals requiring an explicit plan submission. Rejection and ask-rate qualification remain incomplete.'}
    (HERE / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    weights = np.frombuffer(content, dtype=np.int8, offset=96, count=65536*13).reshape(65536,13)
    bias = np.frombuffer(content, dtype='<i4', offset=96+65536*13, count=13)
    texts = [row['text'] for row in json.loads((HERE / 'fixtures.json').read_text(encoding='utf-8'))]
    values = scores([{'text': text} for text in texts], weights, bias)
    goldens = []
    for text, row in zip(texts, values):
        ranked = sorted(range(13), key=lambda index: (-row[index], index))
        top = int(row[ranked[0]])
        gap = top - int(row[ranked[1]])
        goldens.append({'text': text, 'intent': LABELS[ranked[0]], 'logit': top, 'margin': gap,
                        'accepted': top >= threshold and gap >= margin})
    (HERE / 'inference.json').write_text(json.dumps(goldens, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'installed_review_asset': str(asset), 'sha256': report['asset_sha256'],
                      'automatic_execution': False, 'qualified': report['qualified']}))


if __name__ == '__main__':
    main()
