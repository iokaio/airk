# SPDX-License-Identifier: Apache-2.0
"""Independent hand-authored grades; no production grader/model/driver imports."""
import copy,json,math,unittest,tempfile,hashlib
from pathlib import Path
from quality_report import normalize_trial,summarize_condition,paired,distribution,build,load_pinned

def trial(identifier='q1',success=False,failure=None):
 return {'task_id':identifier,'success':success,'failure':failure,'grade':{'completion':False,'alternatives':[{'human_annotation_id':'answer-A','token_f1':.95,'exact_match':False,'complete_support':False,'completion':False},{'human_annotation_id':'answer-B','token_f1':.2,'exact_match':False,'complete_support':True,'completion':False}]},'final_response':{'answer':'fixture text only'},'human_evidence':{'shown_recall_fraction':[1,1],'cited_recall_fraction':[1,1],'shown_complete_original_alternative':True,'cited_complete_original_alternative':True},'agent_final_citation_references':2,'agent_final_valid_citation_references':1,'external_model_turns':8,'canonical_seconds':12.,'wall_seconds':20.,'citation_identity_errors':1}
class QualityControls(unittest.TestCase):
 def test_no_cross_alternative_answer_support_union_or_changed_gate(self):
  r=normalize_trial(trial());self.assertEqual(r['descriptive_metrics']['max_answer_token_f1'],.95);self.assertEqual(r['descriptive_metrics']['max_joint_supported_token_f1'],.2);self.assertAlmostEqual(r['descriptive_metrics']['mean_annotation_token_f1'],.575);self.assertFalse(r['hard_gates_retained']['success']);self.assertFalse(r['hard_gates_retained']['grade_completion']);self.assertEqual([a['token_f1'] for a in r['alternatives']],[.95,.2]);self.assertEqual(r['descriptive_metrics']['citation_reference_precision'],.5)
 def test_full100_denominator_missing_is_not_zero_or_dropped(self):
  ids=['q'+str(i) for i in range(100)];r=summarize_condition([trial('q0')],ids);self.assertEqual(r['expected_tasks'],100);self.assertEqual(r['missing_trial_tasks'],99);self.assertEqual(len(r['per_task']),100);self.assertIsNone(r['per_task'][1]['descriptive_metrics']['max_answer_token_f1']);self.assertIsNone(r['metric_distributions']['max_answer_token_f1']['all_task_mean']);self.assertEqual(r['metric_distributions']['max_answer_token_f1']['observed_mean'],.95)
 def test_failures_with_partial_credit_remain_in_quality_and_cost_pair(self):
  a1=trial('a',False,'native_failure');a2=trial('b',True);b1=trial('a',True);b2=trial('b',False,'budget_exhausted')
  a1['external_model_turns']=8;a2['external_model_turns']=2;b1['external_model_turns']=1;b2['external_model_turns']=1
  a1['grade']['alternatives']=[{'human_annotation_id':'human','token_f1':.4,'exact_match':False,'complete_support':False,'completion':False}];a2['grade']['alternatives']=[{'human_annotation_id':'human','token_f1':1.,'exact_match':True,'complete_support':True,'completion':True}]
  a=summarize_condition([a1,a2],['a','b']);b=summarize_condition([b1,b2],['a','b']);self.assertAlmostEqual(a['metric_distributions']['max_answer_token_f1']['all_task_mean'],.7);self.assertEqual(a['cost_distributions']['external_model_turns']['all_task_mean'],5);p=paired(a,b,['a','b']);self.assertEqual(p['cost_deltas']['external_model_turns']['all_task_mean_delta'],4);self.assertEqual(p['cost_deltas']['external_model_turns']['observed_pairs'],2);self.assertEqual(a['binary_hard_gates']['recorded_failures'],1)
 def test_absent_answer_zero_grade_and_missing_grade_are_distinct(self):
  r=trial();r['final_response']=None;r['grade']['alternatives'][0]['token_f1']=0;r['grade']['alternatives'][1]['token_f1']=0;a=normalize_trial(r);self.assertFalse(a['final_answer_present']);self.assertEqual(a['descriptive_metrics']['max_answer_token_f1'],0)
  r['grade']={};a=normalize_trial(r);self.assertIsNone(a['descriptive_metrics']['max_answer_token_f1']);self.assertIsNone(summarize_condition([r],['q1'])['all_annotation_token_f1_distribution']['all_alternative_mean'])
 def test_duplicate_foreign_task_and_alternative_refusals(self):
  for rows in [[trial(),trial()],[trial('foreign')]]:
   with self.assertRaises(ValueError):summarize_condition(rows,['q1'])
  r=trial();r['grade']['alternatives'][1]['human_annotation_id']='answer-A'
  with self.assertRaises(ValueError):normalize_trial(r)
 def test_bad_score_fraction_and_zero_citation_denominator(self):
  for value in [float('nan'),float('inf'),-.1,1.1,True]:
   r=trial();r['grade']['alternatives'][0]['token_f1']=value
   with self.assertRaises(ValueError):normalize_trial(r)
  r=trial();r['human_evidence']['shown_recall_fraction']=[1,0]
  with self.assertRaises(ValueError):normalize_trial(r)
  r=trial();r['agent_final_citation_references']=0;r['agent_final_valid_citation_references']=0;self.assertIsNone(normalize_trial(r)['descriptive_metrics']['citation_reference_precision'])
 def test_independent_distribution_expected_values(self):
  d=distribution([0,.25,.5,1],4);self.assertEqual(d['all_task_mean'],.4375);self.assertEqual(d['median'],.375);self.assertEqual(d['p25'],.1875);self.assertEqual(d['p75'],.625);self.assertEqual(d['zero_count'],1);self.assertEqual(d['one_count'],1)
 def test_all100_keep_one_failed_partial_answer(self):
  ids=['q'+str(i) for i in range(100)];rows=[]
  for i,key in enumerate(ids):
   row=trial(key,success=i!=0,failure='budget_exhausted' if i==0 else None);row['grade']['alternatives']=[{'human_annotation_id':'human','token_f1':.4 if i==0 else 1.,'exact_match':i!=0,'complete_support':i!=0,'completion':i!=0}];rows.append(row)
  result=summarize_condition(rows,ids);self.assertAlmostEqual(result['metric_distributions']['max_answer_token_f1']['all_task_mean'],.994);self.assertEqual(result['binary_hard_gates']['recorded_successes'],99);self.assertEqual(result['binary_hard_gates']['recorded_failures'],1);self.assertEqual(result['metric_distributions']['max_answer_token_f1']['observed_tasks'],100)
 def test_pinned_source_and_rubric_cohort_refusals(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/'condition.json';data={'split':'development','rubric_sha256':'original','complete':False,'trials':[trial()]};raw=json.dumps(data).encode();path.write_bytes(raw);pin=hashlib.sha256(raw).hexdigest();self.assertEqual(load_pinned(path,pin),data)
   manifest={'cohort':'fixture','split':'development','rubric_sha256':'original','expected_ids':['q1'],'conditions':[{'name':'one','path':str(path),'sha256':pin}]};result=build(manifest);self.assertFalse(result['source_conditions'][0]['complete_source_receipt']);self.assertEqual(result['conditions']['one']['expected_tasks'],1)
   with self.assertRaises(ValueError):build({**manifest,'rubric_sha256':'foreign'})
   with self.assertRaises(ValueError):build({**manifest,'split':'confirmation'})
   path.write_bytes(raw+b' ')
   with self.assertRaises(ValueError):load_pinned(path,pin)
if __name__=='__main__':unittest.main()
