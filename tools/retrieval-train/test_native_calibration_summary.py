# SPDX-License-Identifier: Apache-2.0
import copy
import unittest

from summarize_native_calibration import summarize_rows


class NativeCalibrationSummaryTests(unittest.TestCase):
    def setUp(self):
        self.ids=[str(i) for i in range(19)];self.tasks=[];self.scores=[];self.labels=[]
        for identifier in self.ids:
            candidates=[{'id':'c1','text':'one'},{'id':'c2','text':'two'}]
            self.tasks.append({'id':identifier,'applied':True,'prompt':{'candidates':candidates,'aliases':[{'alias':'c1','choice':'one'},{'alias':'c2','choice':'two'}]},'compiled':{'input_digest':'exact','input_ids':[1,2,3]}})
            self.scores.append({'id':identifier,'applied':True,'candidates':copy.deepcopy(candidates),'raw_scores':[1.,2.],'input_digest':'exact','input_tokens':3,'inference_us':5})
            self.labels.append({'id':identifier,'before_any_original_alternative_complete':False,'has_useful_choice':True,'choices':[{'choice_id':'one','useful':False},{'choice_id':'two','useful':True}]})

    def result(self):return summarize_rows(self.tasks,self.scores,self.labels,self.ids)

    def test_ties_abstain_and_native_refusal_is_retained(self):
        self.scores[0]['raw_scores']=[2.,2.]
        self.tasks[1]={'id':'1','applied':False,'reason':'native token refusal'}
        self.scores[1]={'id':'1','applied':False,'reason':'native token refusal'}
        result=self.result()
        self.assertEqual(result['applied_episodes'],18)
        self.assertEqual(result['exact_tie_abstentions'],1)
        self.assertEqual(result['selected_projected_useful_count'],17)
        self.assertEqual(result['missing_support_opportunity_episodes'],19)
        self.assertEqual(result['missing_support_opportunity_selected_useful'],17)

    def test_actual_input_and_alias_drift_refused(self):
        self.scores[0]['input_digest']='different'
        with self.assertRaisesRegex(ValueError,'compiled scoring input'):self.result()
        self.scores[0]['input_digest']='exact';self.tasks[0]['prompt']['aliases'][0]['choice']='foreign'
        with self.assertRaisesRegex(ValueError,'legal counterfactual labels'):self.result()

    def test_nonfinite_and_incomplete_population_refused(self):
        self.scores[0]['raw_scores']=[float('nan'),2.]
        with self.assertRaisesRegex(ValueError,'trained-head scores'):self.result()
        self.scores.pop()
        with self.assertRaisesRegex(ValueError,'nineteen'):self.result()


if __name__=='__main__':unittest.main()
