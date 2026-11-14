# SPDX-License-Identifier: Apache-2.0
"""TRAIN-only visible-prefix mining ablation; no model execution."""
import json
import unittest
from unittest.mock import patch

import training_recipe as recipe


class VisibleMiningTests(unittest.TestCase):
    def test_frozen315_master_refuses_any_row_or_cardinality_change(self):
        rows = [{'task_id': str(i), 'positive': 'p', 'distractor': 'n'} for i in range(315)]
        recipe.assert_training_rows(rows, [dict(row) for row in rows])
        changed = [dict(row) for row in rows]
        changed[0]['distractor'] = 'other'
        with self.assertRaisesRegex(ValueError, 'frozen315'):
            recipe.assert_training_rows(changed, rows)
        with self.assertRaisesRegex(ValueError, 'frozen315'):
            recipe.assert_training_rows(rows[:-1], rows)
    def test_negative_mining_uses_encoded_visible_text_not_discarded_tail(self):
        full = {'a_tail': 'common ' * 254 + 'important', 'b_visible': 'important visible', 'positive': 'important'}
        old = sorted((d for d in full if d != 'positive'), key=lambda d: (-len({'important'} & set(full[d].split())), d))
        self.assertEqual(old[0], 'a_tail')
        visible = {'a_tail': 'common ' * 254, 'b_visible': 'important visible', 'positive': 'important'}
        self.assertEqual(recipe.negative_order('important', sorted(full), {'positive'}, visible)[0], 'b_visible')
        self.assertNotIn('positive', recipe.negative_order('important', sorted(full), {'positive'}, visible))

    def test_real_tokenizer_prefix_preserves_unicode_offsets_padding_and_truncation(self):
        from tokenizers import Tokenizer, models, pre_tokenizers, processors
        vocab = {'[UNK]': 0, '[CLS]': 1, '[SEP]': 2, '[PAD]': 3, 'é': 4, '漢字': 5, 'tail': 6}
        tokenizer = Tokenizer(models.WordLevel(vocab, unk_token='[UNK]'))
        tokenizer.pre_tokenizer = pre_tokenizers.Whitespace()
        tokenizer.post_processor = processors.TemplateProcessing(single='[CLS] $A [SEP]', special_tokens=[('[CLS]', 1), ('[SEP]', 2)])
        tokenizer.enable_truncation(max_length=256)
        tokenizer.enable_padding(pad_id=3, pad_token='[PAD]')
        before = tokenizer.to_str()
        text = ('é 漢字 ' * 127) + 'tail'
        result = recipe.visible_prefix(tokenizer, text)
        self.assertNotIn('tail', result)
        self.assertEqual(result, ('é 漢字 ' * 127).rstrip())
        self.assertEqual(tokenizer.to_str(), before)
        encoded = tokenizer.encode_batch(['é', 'é 漢字'])
        self.assertEqual(len(encoded[0].ids), len(encoded[1].ids))
        tokenizer.no_truncation()
        with self.assertRaisesRegex(ValueError, '256-token'):
            recipe.visible_prefix(tokenizer, text)

    def test_candidate_loader_never_decodes_nontrain_text_or_hotpot(self):
        raw = json.dumps({'documents': [
            {'id': 'p', 'source_group': 'qasper:TRAIN', 'dataset': 'QASPER', 'text': 'TRAIN text'},
            {'id': 'held', 'source_group': 'qasper:TEST', 'dataset': 'QASPER', 'text': 'FORBIDDEN_FINAL_DOC'},
            {'id': 'hot', 'source_group': 'wikipedia:TRAIN', 'dataset': 'HotpotQA', 'text': 'FORBIDDEN_HOTPOT'}],
            'tasks': [{'id': 'q', 'split': 'train', 'dataset': 'QASPER', 'source_group': 'qasper:TRAIN', 'question': 'TRAIN question'},
                      {'id': 'heldq', 'split': 'test', 'dataset': 'QASPER', 'source_group': 'qasper:TEST', 'question': 'FORBIDDEN_FINAL_Q'},
                      {'id': 'hotq', 'split': 'train', 'dataset': 'HotpotQA', 'source_group': 'wikipedia:TRAIN', 'question': 'FORBIDDEN_HOTPOT_Q'}]}).encode()
        real = json.loads
        seen = []
        def observe(data, *args, **kwargs):
            seen.append(data)
            return real(data, *args, **kwargs)
        with patch.object(recipe.json, 'loads', side_effect=observe):
            selected = recipe.train_only_corpus(raw)
        self.assertEqual([t['id'] for t in selected['tasks']], ['q'])
        self.assertEqual([d['id'] for d in selected['documents']], ['p'])
        self.assertFalse(any(b'FORBIDDEN' in data for data in seen if isinstance(data, bytes)))



if __name__ == '__main__':
    unittest.main()
