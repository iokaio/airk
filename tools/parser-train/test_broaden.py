# SPDX-License-Identifier: Apache-2.0
import unittest

import numpy as np

from broaden import calibrate, check_splits, metrics, public_negatives
from phrasing import PHRASES, split_families
from train import LABELS


class CorpusContracts(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
