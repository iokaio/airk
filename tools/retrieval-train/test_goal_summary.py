# SPDX-License-Identifier: Apache-2.0
import copy
import unittest

from goal_summary import summarize


class GoalSummaryTests(unittest.TestCase):
    def setUp(self):
        self.ids = [str(i) for i in range(100)]
        self.candidates = ['arbiter-' + str(i) for i in range(4)]
        self.conditions = {}
        for name in ['ordinary_followup', 'deterministic_packet', *self.candidates]:
            self.conditions[name] = [{'task_id': key, 'success': int(key) < 80,
                                     'external_model_turns': 4 if name in self.candidates else 5,
                                     'citation_identity_errors': 0, 'failure': None} for key in self.ids]

    def result(self): return summarize(self.conditions, self.ids, self.candidates)

    def test_pairing_and_all_task_denominators_preserve_failed_work(self):
        self.conditions[self.candidates[0]][99]['external_model_turns'] = 8
        result = self.result()
        self.assertTrue(result['primary_turn_floor_pass'])
        condition = result['conditions'][self.candidates[0]]
        self.assertEqual(condition['all_task_mean_external_turns'], 4.04)
        self.assertEqual(condition['failed_task_mean_external_turns'], 4.2)
        self.assertEqual(result['paired_common_success'][0]['common_success_questions'], 80)

    def test_exact_two_point_completion_boundary(self):
        for key in (78, 79): self.conditions[self.candidates[0]][key]['success'] = False
        self.assertTrue(self.result()['all_candidate_completion_comparisons_pass'])
        self.conditions[self.candidates[0]][77]['success'] = False
        self.assertFalse(self.result()['all_candidate_completion_comparisons_pass'])

    def test_empty_common_success_cannot_pass_turn_gate(self):
        for row in self.conditions[self.candidates[0]]: row['success'] = False
        result = self.result()
        self.assertFalse(result['primary_turn_floor_pass'])
        self.assertIsNone(result['paired_common_success'][0]['turn_reduction'])

    def test_duplicate_population_refused(self):
        self.conditions[self.candidates[0]][99] = copy.deepcopy(self.conditions[self.candidates[0]][0])
        with self.assertRaises(ValueError): self.result()

    def test_strongest_control_selected_only_from_baseline_completion_then_turns(self):
        self.assertEqual(self.result()['selected_strongest_baseline'], 'deterministic_packet')
        self.conditions['ordinary_followup'][80]['success'] = True
        self.assertEqual(self.result()['selected_strongest_baseline'], 'ordinary_followup')
        self.conditions['ordinary_followup'][80]['success'] = False
        self.conditions['ordinary_followup'][0]['external_model_turns'] = 4
        self.assertEqual(self.result()['selected_strongest_baseline'], 'ordinary_followup')

    def test_secondary_faster_control_is_reported_without_becoming_primary_gate(self):
        self.conditions['ordinary_followup'][80]['success'] = True
        for row in self.conditions['deterministic_packet']: row['external_model_turns'] = 1
        result = self.result()
        self.assertEqual(result['selected_strongest_baseline'], 'ordinary_followup')
        self.assertTrue(result['primary_turn_floor_pass'])
        self.assertTrue(any(not row['turn_floor_pass'] for row in result['paired_common_success']))

    def add_observed_evidence(self):
        for rows in self.conditions.values():
            for row in rows:
                row['human_evidence']={'shown_recall_fraction':[1,1],'cited_recall_fraction':[1 if row['success'] else 0,1]}
                row['agent_final_citation_references']=1
                row['agent_final_valid_citation_references']=int(row['success'])
                row['citation_identity_errors']=int(not row['success'])

    def test_critical_comparison_is_aggregate_and_agent_precision_is_not_absolute_one(self):
        self.add_observed_evidence()
        self.conditions['deterministic_packet'][0]['human_evidence']['shown_recall_fraction']=[1,2]
        self.conditions[self.candidates[0]][1]['human_evidence']['shown_recall_fraction']=[1,2]
        result=self.result();comparison=result['critical_evidence_comparisons'][0]
        self.assertEqual(comparison['baseline'],'deterministic_packet')
        self.assertTrue(comparison['shown_critical_evidence_floor_pass'])
        self.assertEqual(comparison['shown_per_question_losses'],1)
        self.assertEqual(comparison['shown_per_question_gains'],1)
        self.assertTrue(comparison['agent_citation_precision_no_regression'])
        self.assertEqual(result['conditions'][self.candidates[0]]['critical_evidence']['agent_citation_precision'],.8)

    def test_zero_references_do_not_manufacture_perfect_precision(self):
        self.add_observed_evidence()
        for row in self.conditions['deterministic_packet']:
            row['agent_final_citation_references']=0;row['agent_final_valid_citation_references']=0;row['citation_identity_errors']=0
        result=self.result()
        self.assertIsNone(result['conditions']['deterministic_packet']['critical_evidence']['agent_citation_precision'])
        self.assertEqual(result['conditions']['deterministic_packet']['critical_evidence']['zero_citation_tasks'],100)
        self.assertIsNone(result['critical_evidence_comparisons'][0]['agent_citation_precision_no_regression'])


if __name__ == '__main__': unittest.main()
