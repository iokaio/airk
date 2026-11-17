# SPDX-License-Identifier: Apache-2.0
import copy
from pathlib import Path
import unittest
from materialize_sources import frozen_question_scopes

class ScopeProjectionTests(unittest.TestCase):
    def setUp(self):
        self.tasks = [{'id': 'synthetic:'+str(i), 'question': 'Synthetic question '+str(i), 'dataset': 'HotpotQA', 'experiment_partition': 'CAL', 'split': 'calibration', 'human_alternatives': ['must not enter scopes']} for i in range(100)]
        self.corpus = {'tasks': self.tasks}
        self.fixture = {'calibration_ids': [t['id'] for t in reversed(self.tasks)], 'goal_ids': [], 'corpus_sha256': '01'*32}
    def test_exact_predeclared_order_and_global_source_root(self):
        result = frozen_question_scopes(self.corpus,self.fixture,Path('synthetic'), 'CAL')
        self.assertEqual([q['id'] for q in result['questions']], self.fixture['calibration_ids'])
        self.assertEqual({q['source_root'] for q in result['questions']}, set(result['source_roots']))
        self.assertEqual(len(result['source_roots']),1)
    def test_no_answer_support_or_native_dto_fields(self):
        result = frozen_question_scopes(self.corpus,self.fixture,Path('synthetic'), 'CAL')
        self.assertTrue(all(set(row)=={'id','question','source_root','split'} for row in result['questions']))
    def test_missing_row_cannot_refill_or_select(self):
        self.corpus['tasks'].pop()
        with self.assertRaisesRegex(ValueError,'exact admitted100'):
            frozen_question_scopes(self.corpus,self.fixture,Path('synthetic'), 'CAL')
    def test_duplicate_id_cannot_hide_missing_question(self):
        self.fixture['calibration_ids'][0]=self.fixture['calibration_ids'][1]
        with self.assertRaisesRegex(ValueError,'exact admitted100'):
            frozen_question_scopes(self.corpus,self.fixture,Path('synthetic'), 'CAL')
    def test_foreign_partition_refuses(self):
        self.tasks[0]['experiment_partition']='CONFIRMATION'
        with self.assertRaisesRegex(ValueError,'admitted Hotpot'):
            frozen_question_scopes(self.corpus,self.fixture,Path('synthetic'), 'CAL')
    def test_confirmation_preserves_original_fixed_goal_ids(self):
        for row in self.tasks:row.update(experiment_partition='CONFIRMATION',split='test')
        self.fixture['goal_ids']=self.fixture['calibration_ids'];self.fixture['calibration_ids']=[]
        result=frozen_question_scopes(self.corpus,self.fixture,Path('synthetic'),'CONFIRMATION')
        self.assertEqual([r['id'] for r in result['questions']],self.fixture['goal_ids'])

if __name__=='__main__':unittest.main()
