# SPDX-License-Identifier: Apache-2.0
import unittest

import numpy as np

from broaden import calibrate, check_splits, metrics, public_negatives, selected_fit_rows, synthetic
from conversational_v4 import CONVERSATIONAL, UNSUPPORTED, families
from phrasing import PHRASES, split_families
from train import LABELS, features


class CorpusContracts(unittest.TestCase):
    def test_selected_fitted_rows_preserve_exact_supported_negative_and_public_order(self):
        rows = [{'text': str(i), 'intent': 'search', 'origin': 'original-role-inventory'} for i in range(8)]
        rows += [{'text': 'original' + str(i), 'intent': 'unsupported', 'origin': 'original-role-inventory'} for i in range(4)]
        rows += [{'text': 'public' + str(i), 'intent': 'unsupported', 'origin': 'clinc150-non-repository-task'} for i in range(4)]
        report = {'learning_curve': [{'positive_training_rows': 4, 'total_training_rows': 8, 'C': .5},
                                     {'positive_training_rows': 8, 'total_training_rows': 16, 'C': .5}],
                  'selected_positive_training_rows': 4, 'selected_C': .5}
        self.assertEqual([row['text'] for row in selected_fit_rows(report, rows)],
                         ['0', '1', '2', '3', 'original0', 'original1', 'public0', 'public1'])
        report['learning_curve'][0]['total_training_rows'] = 7
        with self.assertRaisesRegex(ValueError, 'Selected fitted-row count'):
            selected_fit_rows(report, rows)

    def test_feature_two_adds_identifier_shape_without_changing_feature_one(self):
        upper = 'tell me about Owner'
        lower = 'tell me about owner'
        self.assertEqual(features(upper), features(lower))
        self.assertNotEqual(features(upper, 2), features(lower, 2))
        for text in (upper, lower, 'find packet_cache', 'find net::Peer', 'find `owner`'):
            self.assertTrue(set(features(text)) <= set(features(text, 2)))
        with self.assertRaisesRegex(ValueError, 'Unknown intent feature'):
            features(upper, 3)

    def test_conversational_families_are_split_and_cover_simple_prefixes(self):
        for label in ('search', 'find_definition'):
            groups = [families(label, split) for split in ('train', 'validation', 'calibration')]
            names = [{name for name, _ in group} for group in groups]
            self.assertFalse(names[0] & names[1] or names[0] & names[2] or names[1] & names[2])
            for group in groups:
                texts = [text for _, variants in group for text in variants]
                for prefix in ('find the ', 'tell me about '):
                    self.assertTrue(any(text.startswith(prefix) for text in texts), (label, prefix))
        self.assertEqual(set(CONVERSATIONAL), {'search', 'find_definition'})
        self.assertEqual(set(UNSUPPORTED), {'train', 'validation', 'calibration'})
        before_and_after = ' '.join(text for _, variants in families('find_definition', 'train') for text in variants)
        self.assertIn('{k} {s}', before_and_after)
        self.assertIn('{s} {k}', before_and_after)

    def test_generated_conversational_examples_keep_exclusions_and_provenance(self):
        repo = {'repository': 'fictional/repo', 'revision': 'fixture', 'vocabulary_sha256': 'fixture',
                'data': {'symbols': ['Owner', 'packet_cache', 'net::Peer'],
                         'headings': ['Example'], 'paths': ['src/client.rs', 'lib/reader.py']}}
        rows = synthetic(2600, [repo], 'train', [], excluded={'tell me about owner'})
        self.assertNotIn('tell me about owner', {row['text'].lower() for row in rows})
        for label in ('search', 'find_definition'):
            conversation = [row for row in rows if row['intent'] == label and row['origin'] == 'original-conversational-family']
            self.assertGreater(len(conversation), 30)
            for prefix in ('find the ', 'tell me about '):
                self.assertTrue(any(prefix in row['text'].lower() for row in conversation), (label, prefix))
            self.assertTrue(all(row['family'].startswith(label + '/conversational-') for row in conversation))

    def test_phrase_families_never_cross_splits(self):
        self.assertEqual(set(PHRASES), set(LABELS))
        for families in PHRASES.values():
            groups = [{name for name, _ in split_families(families, split)}
                      for split in ('train', 'validation', 'calibration')]
            self.assertTrue(all(groups))
            self.assertFalse(groups[0] & groups[1] or groups[0] & groups[2] or groups[1] & groups[2])

    def test_normalized_duplicate_and_family_leakage_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'Normalized utterance'):
            check_splits({'train': [{'text': '  Read A ', 'origin': 'original'}],
                          'validation': [{'text': 'read   a', 'origin': 'original'}]})
        with self.assertRaisesRegex(ValueError, 'Phrase family'):
            check_splits({'train': [{'text': 'first', 'family': 'read/contents'}],
                          'calibration': [{'text': 'second', 'family': 'read/contents'}]})

    def test_public_official_split_duplicates_and_ambiguous_help_are_excluded(self):
        clinc = {'train': [['book a flight', 'travel'], ['Shared request', 'travel'],
                           ['help me', 'what_can_i_ask_you']],
                 'val': [], 'test': [['shared   REQUEST', 'travel'], ['order a pizza', 'food']]}
        rows = public_negatives(clinc, 'train', 20)
        self.assertEqual([row['text'] for row in rows], ['book a flight'])
        self.assertEqual(rows[0]['intent'], 'unsupported')
        self.assertEqual([row['text'] for row in public_negatives(clinc, 'test', 20)], ['order a pizza'])

    def test_calibration_prioritizes_precision_and_rejects_confident_unsupported(self):
        rows = [{'intent': 'search', 'role': 'developer'}, {'intent': 'unsupported', 'role': 'public'}]
        values = np.full((2, 13), -100, dtype=np.int32)
        values[0, 0] = 100
        values[1, 0] = 50
        threshold, margin = calibrate(rows, values)
        result = metrics(rows, values, threshold, margin)
        self.assertEqual(result['unsupported_accepted'], 0)
        self.assertEqual(result['accepted_precision'], 1)
        self.assertEqual(result['supported_ask_rate'], 0)

    def test_unsupported_rows_are_wrong_if_any_primitive_is_accepted(self):
        result = metrics([{'intent': 'unsupported', 'role': 'public'}],
                         np.array([[100] + [-100] * 12], dtype=np.int32), 0, 80)
        self.assertEqual(result['wrong_accepted'], 1)
        self.assertEqual(result['unsupported_accept_rate'], 1)
        self.assertIsNone(result['intent_accuracy'])

    def test_role_outcomes_count_supported_abstention_and_unsupported_acceptance(self):
        rows = [{'intent': 'search', 'role': 'writer'}, {'intent': 'search', 'role': 'writer'},
                {'intent': 'unsupported', 'role': 'tester'}]
        values = np.full((3, 13), -100, dtype=np.int32)
        values[0, 0] = 100
        values[1, 0] = -100
        values[2, 0] = 100
        result = metrics(rows, values)
        self.assertEqual(result['per_role_outcomes']['writer']['accepted'], 1)
        self.assertEqual(result['per_role_outcomes']['writer']['abstained'], 1)
        self.assertEqual(result['per_role_outcomes']['writer']['supported_ask_rate'], .5)
        self.assertEqual(result['per_role_outcomes']['tester']['wrong_accepted'], 1)

    def test_joint_role_split_keeps_template_families_and_roles_material(self):
        from role_campaign import canonical, coverage, generated, prepare
        rows = prepare()
        known = {}
        for row in rows:
            group = row['group']
            self.assertEqual(known.setdefault(group, row['split']), row['split'])
        self.assertEqual(canonical('Please Tell me about {s}.'), canonical('tell me about {s}?'))
        repo = {'repository': 'fictional/repo', 'revision': 'fixture', 'vocabulary_sha256': 'fixture',
                'data': {'symbols': ['Owner', 'packet_cache', 'net::Peer'], 'headings': ['Example'],
                         'paths': ['src/client.rs', 'lib/reader.py', 'docs/guide.md']}}
        expanded = generated(2600, [repo], 'train', [], excluded={'tell me about owner'})
        self.assertGreaterEqual(sum(row['origin'] == 'original-role-inventory' for row in expanded), len(expanded) // 2)
        self.assertEqual({row['role'] for row in expanded}, {'developer', 'tester', 'architect', 'writer'})
        measured = coverage(expanded + [{'intent': 'unsupported', 'role': 'public-assistant-task',
                                         'origin': 'clinc150-non-repository-task'}])
        self.assertEqual(measured['public_negative_rows'], 1)
        for row in expanded:
            if row['intent'] == 'changes' and '{' not in row['text']:
                self.assertTrue(row['literal_slots']['token'].startswith('airk-r1:'))


if __name__ == '__main__':
    unittest.main()
