# SPDX-License-Identifier: Apache-2.0
"""No model execution: selective TRAIN decoding and actual crop mismatch guards."""
import json
import re
import unittest
from unittest.mock import patch

import training_recipe_audit as audit


class WordTokenizer:
    def no_padding(self):
        pass

    def no_truncation(self):
        self.limit = None

    def enable_truncation(self, max_length):
        self.limit = max_length

    def encode(self, text):
        offsets = [m.span() for m in re.finditer(r'\S+', text)]
        if self.limit:
            offsets = offsets[:self.limit - 2]
        return type('Tokens', (), {'ids': [1] * (len(offsets) + 2), 'offsets': [(0, 0)] + offsets + [(0, 0)]})()


class TrainingRecipeAuditTests(unittest.TestCase):
    def test_unselected_question_answer_and_document_strings_are_never_deserialized(self):
        data = json.dumps({'documents': [{'text': 'TRAIN text', 'id': 'wanted'},
                          {'id': 'foreign', 'text': 'FORBIDDEN_FINAL_OUTCOME_SENTINEL'}],
                           'tasks': [{'question': 'FORBIDDEN_FINAL_QUESTION_SENTINEL', 'id': 'foreign'},
                                     {'id': 'train', 'question': 'TRAIN question'}]}).encode()
        real = json.loads
        seen = []
        def observe(value, *args, **kwargs):
            seen.append(value)
            return real(value, *args, **kwargs)
        with patch.object(audit.json, 'loads', side_effect=observe):
            docs = list(audit.selected_array_objects(data, 'documents', {'wanted'}))
            tasks = list(audit.selected_array_objects(data, 'tasks', {'train'}))
        self.assertEqual(docs, [{'id': 'wanted', 'text': 'TRAIN text'}])
        self.assertEqual(tasks, [{'id': 'train', 'question': 'TRAIN question'}])
        self.assertFalse(any(b'FORBIDDEN' in value for value in seen if isinstance(value, bytes)))

    def test_prefix_crop_reports_discarded_positive_bytes_and_lost_hard_negative_signal(self):
        tokenizer = WordTokenizer()
        text = 'préface é ' + 'common ' * 252 + 'unique_tail'
        observed, visible = audit.crop_observation(tokenizer, text)
        self.assertTrue(observed['truncated'])
        self.assertEqual(observed['encoded_tokens'], 256)
        self.assertGreater(observed['original_tokens_including_specials'], 256)
        self.assertNotIn('unique_tail', visible)
        self.assertEqual(observed['visible_prefix_utf8_bytes'], len(visible.encode()))
        source = {'p': {'dataset': 'QASPER', 'source_group': 'qasper:TRAIN', 'text': text,
                        'paragraph_sha256': audit.hashlib.sha256(text.encode()).hexdigest()},
                  'n': {'dataset': 'QASPER', 'source_group': 'qasper:OTHERTRAIN', 'text': text,
                        'paragraph_sha256': audit.hashlib.sha256(text.encode()).hexdigest()}}
        task = {'id': 'q', 'split': 'train', 'dataset': 'QASPER', 'question': 'unique_tail',
                'human_alternatives': [{'unanswerable': False, 'alignment': 'all_original_evidence_maps_to_allowed_text',
                                        'evidence': [{'allowed_locations': ['p']}]}]}
        result = audit.audit([{'task_id': 'q', 'question': 'unique_tail', 'positive': 'p', 'distractor': 'n'}], [task], source, tokenizer)
        self.assertEqual(result['cropped_positive_rows'], 1)
        self.assertEqual(result['cropped_distractor_rows'], 1)
        self.assertEqual(result['hard_distractor_rows_losing_mined_lexical_terms_after_crop'], 1)

    def test_train_scope_and_full_original_positive_union_are_required(self):
        task = {'id': 'q', 'split': 'test', 'dataset': 'QASPER', 'question': 'Q', 'human_alternatives': []}
        with self.assertRaisesRegex(ValueError, 'nonTRAIN'):
            audit.validate_training_rows([{'task_id': 'q'}], [task], {})
        task['split'] = 'train'
        task['human_alternatives'] = [{'unanswerable': False, 'alignment': 'all_original_evidence_maps_to_allowed_text',
                                      'evidence': [{'allowed_locations': ['p', 'other_original_positive']}]}]
        docs = {'p': {'dataset': 'QASPER', 'text': 'P', 'paragraph_sha256': audit.hashlib.sha256(b'P').hexdigest()},
                'n': {'dataset': 'QASPER', 'text': 'N', 'paragraph_sha256': audit.hashlib.sha256(b'N').hexdigest()}}
        with self.assertRaisesRegex(ValueError, 'positive population'):
            audit.validate_training_rows([{'task_id': 'q', 'question': 'Q', 'positive': 'p', 'distractor': 'n'}], [task], docs)


if __name__ == '__main__':
    unittest.main()
