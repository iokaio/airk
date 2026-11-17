# SPDX-License-Identifier: Apache-2.0
"""Prepare TRAIN-only weak-reference target indices from retained native rows.

No inference, training, native DTO construction, CAL labels or semantic labels.
Whole native rows remain opaque, immutable source documents.
"""
import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent
PINS = {
    'smallintern': '822a474a9ea4f9997d6e584ec936f607a5f5a00c6efb028b13491014fbbd55ae',
    'strands': 'e7dbe89dadb91d61c0c62459aa5ac2e7d9a32997d2c1da59662c58671a2ef49c',
    'intern4': '20ce785a6b4f71a1a861daa4afb5f731aa5a72d2bd66a89d3a419879fb1c2ff4',
    'kev4': '845f9c291afcbeb1b5aa42ae261adc68c0af063235b1c56e97e9f704736a8a89',
}

def targets(row):
    if row['split'] != 'train':
        raise ValueError('only original TRAIN rows may supply adaptation labels')
    cards = row['roster']['cards']
    by_id = {card['id']: (index, card) for index, card in enumerate(cards)}
    if len(by_id) != len(cards):
        raise ValueError('duplicate native card ID')
    witnesses = row['reference_proxies']['valid_reference_proxy_whole_cards']
    selected, authorities = set(), []
    for witness in witnesses:
        if witness['proxy'] != 'generic_read_original_reference_complement':
            raise ValueError('unsupported reference authority')
        index, card = by_id[witness['card']]
        if not (card['action']['kind'] == 'read' and card['gap'] == 'unsupported'
                and card['reason'] == 'additional_evidence_may_help'
                and all(a['role'] == 'unknown' and a['relevance'] == 'uncertain'
                        for a in card['assessments'])):
            raise ValueError('witness does not qualify every hypothesis on whole card')
        selected.add(index)
        # Keep each ORIGINAL annotation witness; do not union evidence sets.
        authorities.append(dict(witness))
    return {'positive_option_indices': sorted(selected), 'original_witnesses': authorities,
            'negative_option_indices': [], 'semantic_labels': None}

def prepare():
    sources, examples, review_queue, signatures, summaries = {}, {}, None, {}, []
    for name, expected in PINS.items():
        path = ROOT / f'context-action-{name}-raw-r24/raw-scores.private.json'
        raw_bytes = path.read_bytes()
        if hashlib.sha256(raw_bytes).hexdigest() != expected:
            raise ValueError('raw input changed')
        raw = json.loads(raw_bytes)
        rows = [r for r in raw['rows'] if r['split'] == 'train']
        if len(rows) != 107 or len({r['id'] for r in rows}) != 107:
            raise ValueError('original TRAIN population changed')
        if len({r['source_group'] for r in rows}) != 36:
            raise ValueError('original TRAIN family population changed')
        current, queue, elapsed, refused = [], [], 0, 0
        for row_index, row in enumerate(raw['rows']):
            if row['split'] != 'train':
                continue
            target = targets(row)
            signature = (row['source_group'], row['roster'])
            if row['id'] in signatures and signatures[row['id']] != signature:
                raise ValueError('cross-model source/card roster mismatch')
            signatures[row['id']] = signature
            queue.append({'id': row['id'], 'family': row['source_group'], 'native_row_index': row_index,
                          'original_scope_and_span_authority': 'opaque retained native roster/basis',
                          'human_card_annotations': None, 'independent_review': None})
            if not target['positive_option_indices']:
                continue
            if row['status'] != 'raw_scores_only' or row['compiled'] is None:
                refused += 1
                continue
            compiled = row['compiled']
            if not compiled['input_ids'] or not compiled['input_digest']:
                raise ValueError('missing native compiler binding')
            current.append({'id': row['id'], 'family': row['source_group'], 'native_row_index': row_index,
                            'native_input_digest': compiled['input_digest'], **target})
            elapsed += row['inference_us']
        if review_queue is not None and review_queue != queue:
            raise ValueError('cross-model annotation queue mismatch')
        review_queue = queue
        sources[str(path)] = expected
        examples[name] = current
        summaries.append({'model': raw['model'], 'examples': len(current),
                          'positive_card_pairs': sum(len(r['positive_option_indices']) for r in current),
                          'positive_but_compiler_refused': refused,
                          'historical_native_inference_us_on_these_rows': elapsed,
                          'timing_is_not_GPU_training_forecast': True})
    for path, expected in sources.items():
        if hashlib.sha256(Path(path).read_bytes()).hexdigest() != expected:
            raise ValueError('input drift during preparation')
    result = {'schema': 'airk.private-context-action-train-indices/1', 'inputs': sources,
              'complete': True, 'evaluation_or_product_qualified': False,
              'label_authority': 'mechanical original-human-reference complement proxy; not human action judgments',
              'per_model': examples, 'summaries': summaries,
              'train_only_blank_human_review_queue': review_queue,
              'excluded_supervision': ['CAL', 'DEV', 'FINAL', 'unknown-as-negative', 'Sufficient',
                                       'Clarify', 'NoJustifiedRead', 'semantic roles/gaps/relevance'],
              'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    out = ROOT / 'context-action-adaptation-train-indices-r1.private.json'
    with out.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'output': str(out), 'sha256': hashlib.sha256(out.read_bytes()).hexdigest(),
                      'summaries': summaries, 'blank_review_questions': len(review_queue)}))

class Controls(unittest.TestCase):
    def row(self):
        return {'split': 'train', 'roster': {'cards': [{'id': 'c1', 'action': {'kind': 'read'},
                'gap': 'unsupported', 'reason': 'additional_evidence_may_help',
                'assessments': [{'role': 'unknown', 'relevance': 'uncertain'}]}]},
                'reference_proxies': {'valid_reference_proxy_whole_cards': [
                    {'card': 'c1', 'annotation_id': 'a1', 'proxy': 'generic_read_original_reference_complement'}]}}
    def test_alternatives_remain_separate_without_multiplying_card_weight(self):
        row = self.row()
        row['reference_proxies']['valid_reference_proxy_whole_cards'].append(
            {'card': 'c1', 'annotation_id': 'a2', 'proxy': 'generic_read_original_reference_complement'})
        result = targets(row)
        self.assertEqual(result['positive_option_indices'], [0])
        self.assertEqual([w['annotation_id'] for w in result['original_witnesses']], ['a1', 'a2'])
    def test_unknown_is_never_negative(self):
        result = targets(self.row())
        self.assertEqual(result['negative_option_indices'], [])
        self.assertIsNone(result['semantic_labels'])
    def test_nontrain_refuses(self):
        for split in ['calibration', 'cal', 'dev', 'test', 'final']:
            row = self.row(); row['split'] = split
            with self.assertRaises(ValueError): targets(row)
    def test_terminal_or_semantic_hypothesis_cannot_inherit_reference_label(self):
        for key, value in [('action', {'kind': 'sufficient'}), ('gap', 'definition'),
                           ('reason', 'missing_test'),
                           ('assessments', [{'role': 'test', 'relevance': 'useful'}])]:
            row = self.row(); row['roster']['cards'][0][key] = value
            with self.assertRaises(ValueError): targets(row)
    def test_missing_witness_produces_no_fake_positive(self):
        row = self.row(); row['reference_proxies']['valid_reference_proxy_whole_cards'] = []
        self.assertEqual(targets(row)['positive_option_indices'], [])
    def test_duplicate_or_foreign_card_refuses(self):
        row = self.row(); row['roster']['cards'].append(dict(row['roster']['cards'][0]))
        with self.assertRaises(ValueError): targets(row)
        row = self.row(); row['reference_proxies']['valid_reference_proxy_whole_cards'][0]['card'] = 'foreign'
        with self.assertRaises(KeyError): targets(row)

if __name__ == '__main__':
    import sys
    unittest.main(argv=[sys.argv[0]]) if '--test' in sys.argv else prepare()
