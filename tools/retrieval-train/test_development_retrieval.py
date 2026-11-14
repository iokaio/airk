# SPDX-License-Identifier: Apache-2.0
"""Synthetic-only DEV20/44 denominator and exact shown-byte controls."""
import copy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import development_retrieval as dev


def mask():
    return {'quality_question_ids': ['q' + str(i) for i in range(20)],
            'refused_question_ids': ['q' + str(i) for i in range(20, 64)]}


class DevelopmentRetrievalTests(unittest.TestCase):
    def test_twenty_quality_and_forty_four_refusals_never_become_sixty_four_passes(self):
        quality, refused = dev.mask_sets(mask())
        rows = [{'id': q, 'quality_status': 'ALIGNED', 'evidence_recall_fraction': [1, 1],
                 'complete_original_alternative': 1} for q in quality]
        rows += [{'id': q, 'quality_status': 'NOT_EVALUABLE', 'evidence_recall_fraction': None,
                  'complete_original_alternative': None} for q in refused]
        summary = dev.summarize_quality(rows, quality, refused)
        self.assertEqual(summary['quality_denominator'], 20)
        self.assertEqual(summary['mean_evidence_recall_fraction'], [1, 1])
        self.assertEqual(summary['coverage_inclusive_complete_lower_bound_fraction'], [20, 64])
        self.assertFalse(summary['promotion_or_final_quality_pass'])
        altered = copy.deepcopy(rows)
        altered[-1]['complete_original_alternative'] = 1
        with self.assertRaisesRegex(ValueError, 'refusals'):
            dev.summarize_quality(altered, quality, refused)
        with self.assertRaisesRegex(ValueError, 'all64'):
            dev.summarize_quality(rows[:-1], quality, refused)

    def test_empty_support_or_no_shown_bytes_never_earns_perfect_recall(self):
        self.assertEqual(dev.evidence_recall({'alternatives': [{'facts': []}]}, []), (Fraction(0), 0))
        task = {'alternatives': [{'facts': [{'locations': [{'file': 'a', 'byte_start': 3, 'byte_end': 5}]}]}]}
        self.assertEqual(dev.evidence_recall(task, []), (Fraction(0), 0))
        self.assertEqual(dev.evidence_recall(task, [{'file': 'a', 'byte_start': 0, 'byte_end': 3}]), (Fraction(0), 0))

    def test_actual_source_ledger_grades_only_shown_unicode_bytes_and_retains_full_record(self):
        quality, refused = dev.mask_sets(mask())
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary)
            source_root = fixture / 'sources/hotpot'
            source_root.mkdir(parents=True)
            body = 'préface é.\nOriginal support 漢字.\n'.encode()
            (source_root / 'a.txt').write_bytes(body)
            source_sha = hashlib.sha256(body).hexdigest()
            manifest = {'files': [{'file': 'sources/hotpot/a.txt', 'bytes': len(body), 'sha256': source_sha,
                                   'paragraphs': [{'original_url': 'https://example.invalid/original'}]}]}
            manifest_bytes = json.dumps(manifest).encode()
            (fixture / 'manifest.private.json').write_bytes(manifest_bytes)
            start = len('préface é.\n'.encode())
            publication = {'collection': 'synthetic-native', 'token': 'synthetic-token'}
            rubric = [{'id': q, 'quality_status': 'ALIGNED' if q in quality else 'NOT_EVALUABLE',
                       'source_root': str(source_root), 'alternatives': [{'facts': [{'locations': [
                           {'file': 'sources/hotpot/a.txt', 'byte_start': start, 'byte_end': len(body) - 1}]}]}]}
                      for q in sorted(quality | refused)]
            def family(text, first, last):
                result = {'publication': publication, 'hits': [{'text': text, 'citation': {'path': 'a.txt',
                           'start_line': first, 'end_line': last, 'content_hash': source_sha}, 'unknown_native_field': {'keep': True}}]}
                return {'family': 'synthetic', 'complete': True,
                        'collections': [{'source_root': str(source_root), 'publication': publication}],
                        'records': [{'id': q, 'source_root': str(source_root), 'primary_vector': result,
                                     'query_us': 1, 'unknown_record_field': ['retain']} for q in sorted(quality | refused)]}
            shown_prefix = dev.grade_family(family('préface é.', 1, 1), rubric, mask(), fixture, hashlib.sha256(manifest_bytes).hexdigest())
            self.assertEqual(shown_prefix['summary']['complete_count'], 0)
            self.assertEqual(shown_prefix['summary']['mean_evidence_recall_fraction'], [0, 1])
            shown_support = dev.grade_family(family('Original support 漢字.', 2, 2), rubric, mask(), fixture, hashlib.sha256(manifest_bytes).hexdigest())
            self.assertEqual(shown_support['summary']['complete_count'], 20)
            self.assertTrue(all(r['complete_original_alternative'] is None for r in shown_support['rows'] if r['id'] in refused))
            self.assertEqual(shown_support['rows'][0]['native_record']['unknown_record_field'], ['retain'])
            self.assertEqual(shown_support['rows'][0]['native_record']['primary_vector']['hits'][0]['unknown_native_field'], {'keep': True})
            self.assertEqual((source_root / 'a.txt').read_bytes(), body)
            with self.assertRaisesRegex(ValueError, 'unambiguous'):
                dev.grade_family(family('UNSHOWN fabricated text', 2, 2), rubric, mask(), fixture, hashlib.sha256(manifest_bytes).hexdigest())

    def test_fixed_mask_rejects_overlap_or_missing_refusal(self):
        value = mask()
        value['refused_question_ids'][0] = value['quality_question_ids'][0]
        with self.assertRaisesRegex(ValueError, 'disjoint'):
            dev.mask_sets(value)
        value = mask()
        value['refused_question_ids'].pop()
        with self.assertRaisesRegex(ValueError, '20/44'):
            dev.mask_sets(value)

    def test_fixture_mapping_preserves_original_unicode_sentence_offsets_and_all_refusals(self):
        documents, tasks, originals = [], [], []
        for i in range(64):
            titles, sentences = [], []
            for j in range(6 if i < 46 else 5):
                title = f'Synthetic article {i}-{j}'
                parts = ['préface é.\n', f'Original support 漢字 {i}-{j}.']
                text = ''.join(parts)
                identifier = 'hotpot:' + dev.sha((title + '\n' + text).encode())
                documents.append({'id': identifier, 'source_group': 'wikipedia:' + title, 'title': title,
                                  'text': text, 'paragraph_sha256': dev.sha(text.encode()),
                                  'url': 'https://example.invalid/' + str(i) + '-' + str(j), 'license': 'CC-BY-SA-4.0'})
                titles.append(title)
                sentences.append(parts)
            source = documents[-len(titles)]
            tasks.append({'id': 'hotpot:q' + str(i), 'question_id': 'q' + str(i), 'split': 'development', 'question': 'Question ' + str(i),
                          'human_alternatives': [{'human_annotation_id': 'original' + str(i), 'original_human_answer': 'Answer',
                           'alignment': 'all_original_evidence_maps_to_allowed_text' if i < 20 else 'unsupported_or_excluded_evidence_not_admitted',
                           'original_evidence_count': 1, 'evidence': [{'source_title': titles[0], 'sentence_index': 1,
                           'source_evidence_sha256': dev.sha(sentences[0][1].encode()), 'allowed_locations': [source['id']] if i < 20 else []}]}]})
            originals.append({'id': 'q' + str(i), 'context': {'title': titles, 'sentences': sentences}})
        self.assertEqual(len(documents), 366)
        bodies, manifest, rubric = dev.build_fixture({'documents': documents, 'tasks': tasks}, mask(), originals, Path('synthetic-output'))
        self.assertEqual(len(bodies), 366)
        self.assertEqual(len(manifest['files']), 366)
        self.assertEqual(len(rubric), 64)
        task = next(x for x in rubric if x['id'] == 'hotpot:q0')
        location = task['alternatives'][0]['facts'][0]['locations'][0]
        self.assertEqual(location['byte_start'], len('préface é.\n'.encode()))
        self.assertEqual(bodies[location['file']][location['byte_start']:location['byte_end']], 'Original support 漢字 0-0.'.encode())
        refused = next(x for x in rubric if x['id'] == 'hotpot:q63')
        self.assertEqual(refused['quality_status'], 'NOT_EVALUABLE')
        self.assertEqual(refused['alternatives'][0]['facts'][0]['locations'], [])
        broken = copy.deepcopy(originals)
        broken.pop()
        with self.assertRaisesRegex(ValueError, 'missing/duplicate/foreign'):
            dev.build_fixture({'documents': documents, 'tasks': tasks}, mask(), broken, Path('synthetic-output'))


if __name__ == '__main__':
    unittest.main()
