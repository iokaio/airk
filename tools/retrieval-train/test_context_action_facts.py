# SPDX-License-Identifier: Apache-2.0
"""Synthetic source-only controls; never a human/model qualification campaign."""
import copy
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import context_action_facts as facts


def document(identifier, text, group):
    return {'id': identifier, 'text': text, 'paragraph_sha256': facts.sha(text.encode()),
            'source_group': group, 'dataset': 'QASPER', 'license': 'CC-BY-4.0'}


def alternative(identifier, locations, documents, unanswerable=False):
    return {'human_annotation_id': identifier, 'unanswerable': unanswerable,
            'alignment': 'all_original_evidence_maps_to_allowed_text',
            'evidence': [{'source_evidence_sha256': documents[ids[0]]['paragraph_sha256'], 'allowed_locations': ids} for ids in locations]}


def task(identifier, split, group, alternatives):
    return {'id': identifier, 'dataset': 'QASPER', 'split': split, 'source_group': group,
            'publisher_split': 'train' if split == 'train' else 'validation', 'question': 'Original fictional question?',
            'human_alternatives': alternatives}


def corpus():
    documents = [document('one', 'Original training paragraph.', 'qasper:train'), document('two', 'Original calibration paragraph.', 'qasper:cal')]
    by_id = {doc['id']: doc for doc in documents}
    return {'tasks': [task('train-q', 'train', 'qasper:train', [alternative('a', [['one']], by_id)]),
                      task('cal-q', 'calibration', 'qasper:cal', [alternative('b', [['two']], by_id)])],
            'documents': documents,
            'qasper_rights_decisions': [{'paper_id': group, 'license': 'CC-BY-4.0', 'attribution': 'Fictional author, CC BY4.'} for group in ['train', 'cal']]}


class ColdScopeTests(unittest.TestCase):
    def test_held_out_annotations_text_and_hotpot_are_not_deserialized(self):
        value = corpus()
        for dataset, split, identifier in [('HotpotQA', 'train', 'hotpot'), ('QASPER', 'test', 'old205'),
                                            ('QASPER', 'development', 'dev9'), ('QASPER', 'calibration', 'unapproved-cal')]:
            value['tasks'].append({'id': identifier, 'dataset': dataset, 'split': split,
                                   'source_group': 'qasper:held', 'human_alternatives': 'CLOSED_GOLD_SENTINEL'})
        value['documents'].append({'id': 'held-document', 'dataset': 'QASPER', 'source_group': 'qasper:held', 'text': 'CLOSED_GOLD_SENTINEL'})
        value['qasper_rights_decisions'].append({'paper_id': 'held', 'attribution': 'CLOSED_GOLD_SENTINEL'})
        data = json.dumps(value).encode()
        original = json.loads
        def guarded(raw, *args, **kwargs):
            if 'CLOSED_GOLD_SENTINEL' in (raw.decode() if isinstance(raw, bytes) else raw):
                raise AssertionError('held-out content reached JSON decoder')
            return original(raw, *args, **kwargs)
        with patch.object(facts.json, 'loads', guarded):
            tasks, documents, rights = facts.scoped_qasper(data, {'cal-q'})
        self.assertEqual([row['id'] for row in tasks], ['train-q', 'cal-q'])
        self.assertEqual([row['id'] for row in documents], ['one', 'two'])
        self.assertEqual([row['paper_id'] for row in rights], ['train', 'cal'])

    def test_family_overlap_duplicate_selectors_and_source_drift_refuse(self):
        value = corpus();value['tasks'][1]['source_group'] = 'qasper:train'
        with self.assertRaisesRegex(ValueError, 'family overlap'):
            facts.scoped_qasper(json.dumps(value).encode(), {'cal-q'})
        with self.assertRaisesRegex(ValueError, 'duplicate JSON field'):
            facts.fields(b'{"split":"train","split":"test","text":"not decoded"}')
        value = corpus();value['documents'][0]['text'] = 'changed source'
        with self.assertRaisesRegex(ValueError, 'identity differs'):
            facts.scoped_qasper(json.dumps(value).encode(), {'cal-q'})
        value = corpus();value['tasks'][0]['human_alternatives'][0]['evidence'][0] = {'source_evidence_sha256': value['documents'][1]['paragraph_sha256'], 'allowed_locations': ['two']}
        with self.assertRaisesRegex(ValueError, 'outside its exact allowed paper'):
            facts.scoped_qasper(json.dumps(value).encode(), {'cal-q'})


class HumanReferenceTests(unittest.TestCase):
    def setUp(self):
        self.docs = {str(i): document(str(i), f'Unmodified paragraph {i}.', 'qasper:paper') for i in range(1, 5)}
        self.a = alternative('annotator-A', [['1'], ['2']], self.docs)
        self.b = alternative('annotator-B', [['3']], self.docs)

    def test_alternatives_are_not_unioned_and_completeness_does_not_prove_every_shown_unit_useful(self):
        first = facts.reference_fact(self.a, ['1', '3'], ['2'])
        second = facts.reference_fact(self.b, ['1', '3'], ['2'])
        self.assertFalse(first['reference_complete_before']);self.assertTrue(first['reference_complete_after'])
        self.assertTrue(first['adds_missing_reference_support'])
        self.assertTrue(second['reference_complete_before']);self.assertFalse(second['adds_missing_reference_support'])
        self.assertFalse(second['all_shown_paragraphs_are_this_alternatives_support'])
        row = task('question', 'train', 'qasper:paper', [self.a, self.b])
        controls = facts.question_controls(row, self.docs)
        self.assertLessEqual(len(controls), 4)
        distractor = next(control for control in controls if any(unit['document_id'] == '4' for unit in control['shown_full_paragraphs']))
        self.assertIn('annotator-A', distractor['reference_complete_alternatives'])
        self.assertNotIn('annotator-A', distractor['document_useful_whole_hypothesis_alternatives'])
        self.assertEqual([alt['human_annotation_id'] for alt in row['human_alternatives']], ['annotator-A', 'annotator-B'])

    def test_duplicate_locations_do_not_require_both_copies_and_unadmitted_support_is_unknown(self):
        duplicate = copy.deepcopy(self.docs['1']);duplicate['id'] = 'duplicate';self.docs['duplicate'] = duplicate
        original = alternative('repeated-location', [['1', 'duplicate']], self.docs)
        result = facts.reference_fact(original, ['duplicate'], ['1'])
        self.assertTrue(result['reference_complete_before']);self.assertFalse(result['adds_missing_reference_support'])
        unavailable = copy.deepcopy(self.a);unavailable['evidence'][0]['allowed_locations'] = []
        self.assertIsNone(facts.reference_fact(unavailable, ['2'])['reference_complete_before'])
        unanswerable = copy.deepcopy(self.a);unanswerable['unanswerable'] = True
        self.assertIsNone(facts.reference_fact(unanswerable, ['1', '2'])['reference_complete_before'])


class StructuralTests(unittest.TestCase):
    def test_names_and_assertions_alone_are_not_verified_tests(self):
        rows = facts.structural_facts('plain.py', b'def test_named_only():\n    assert True\n')
        self.assertEqual([row['predicate'] for row in rows], ['python_declaration_header'])
        self.assertEqual(rows[0]['qualified_declaration'], 'test_named_only')
        self.assertEqual(rows[0]['required_ranges'][0]['start_byte'], 0)
        self.assertEqual(rows[0]['required_ranges'][0]['end_byte'], len(b'def test_named_only():\n    '))

    def test_actual_framework_predicates_require_unshadowed_import_and_original_proof_ranges(self):
        raw = b'import unittest as u\nimport pytest as p\nclass Check(u.TestCase):\n    def test_rule(self):\n        assert True\n@p.mark.parametrize("x", [1])\ndef check(x):\n    return x\n'
        rows = facts.structural_facts('checks.py', raw)
        self.assertEqual(sum(row['predicate'] == 'unittest_testcase_method' for row in rows), 1)
        self.assertEqual(sum(row['predicate'] == 'explicit_pytest_mark_annotation' for row in rows), 1)
        for row in rows:
            for span in row['required_ranges']:
                self.assertTrue(raw[span['start_byte']:span['end_byte']])
        shadowed = facts.structural_facts('shadow.py', b'import pytest as p\np = object()\n@p.mark.slow\ndef test_item():\n    pass\n')
        self.assertFalse(any(row['predicate'] == 'explicit_pytest_mark_annotation' for row in shadowed))

    def test_doctest_is_parsed_not_executed_and_unicode_offsets_bind_exact_original_bytes(self):
        raw = 'def café():\n    """An example.\n\n    >>> raise RuntimeError("MUST NOT EXECUTE")\n    expected output\n    """\n    return 1\n'.encode()
        rows = facts.structural_facts('unicode.py', raw)
        example = next(row for row in rows if row['predicate'] == 'doctest_example_grammar')
        self.assertIn('raise RuntimeError', example['examples'][0]['source'])
        self.assertEqual(example['examples'][0]['expected_output'], 'expected output\n')
        self.assertEqual(example['source_file_sha256'], facts.sha(raw))
        self.assertIn(b'>>> raise RuntimeError', raw[example['required_ranges'][0]['start_byte']:example['required_ranges'][0]['end_byte']])


if __name__ == '__main__':
    unittest.main()
