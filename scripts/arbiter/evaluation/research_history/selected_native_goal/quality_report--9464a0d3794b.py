# SPDX-License-Identifier: Apache-2.0
"""Descriptive existing-grade accounting only; no grader, neural calls or acceptance edits."""
import argparse,hashlib,itertools,json,math,statistics
from collections import Counter
from pathlib import Path
COSTS=('external_model_turns','canonical_calls','native_rpc_attempts','canonical_returned_bytes','native_rpc_returned_bytes','canonical_seconds','external_inference_seconds','preprocessing_seconds','wall_seconds','client_initialization_seconds','known_neural_input_tokens')
QUALITY=('max_answer_token_f1','mean_annotation_token_f1','max_joint_supported_token_f1','exact_match_any','shown_evidence_recall','cited_evidence_recall','citation_reference_precision')
def digest(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def load_pinned(path,pin):
 with Path(path).open('rb') as stream:raw=stream.read(128*1024**2+1)
 if len(raw)>128*1024**2 or hashlib.sha256(raw).hexdigest()!=pin:raise ValueError('bounded retained input bytes differ from expected source pin')
 return json.loads(raw)
def number(value,unit=False):
 if type(value) not in (int,float) or not math.isfinite(value) or value<0 or (unit and value>1):raise ValueError('nonfinite/negative/out-of-range observed score')
 return value
def fraction(value):
 if not isinstance(value,list) or len(value)!=2 or any(type(x) is not int for x in value) or value[1]<=0 or not 0<=value[0]<=value[1]:raise ValueError('invalid original evidence fraction')
 return value[0]/value[1]
def distribution(values,expected):
 known=[float(v) for v in values if v is not None]
 ordered=sorted(known)
 def quantile(q):
  if not ordered:return None
  x=(len(ordered)-1)*q;i=int(x);j=min(i+1,len(ordered)-1);return ordered[i]+(ordered[j]-ordered[i])*(x-i)
 return {'expected_tasks':expected,'observed_tasks':len(known),'unobserved_tasks':expected-len(known),'observed_sum':sum(known),'observed_mean':statistics.mean(known) if known else None,'all_task_mean':statistics.mean(known) if known and len(known)==expected else None,'min':min(known) if known else None,'p25':quantile(.25),'median':quantile(.5),'p75':quantile(.75),'p90':quantile(.9),'max':max(known) if known else None,'zero_count':sum(v==0 for v in known),'one_count':sum(v==1 for v in known)}
def annotation_distribution(values):
 result=distribution(values,len(values))
 for old,new in [('expected_tasks','expected_alternatives'),('observed_tasks','observed_alternatives'),('unobserved_tasks','unobserved_alternatives'),('all_task_mean','all_alternative_mean')]:result[new]=result.pop(old)
 result['aggregation_unit']='human alternative, not question'
 return result
def normalize_trial(row):
 if not isinstance(row.get('task_id'),str) or type(row.get('success')) is not bool:raise ValueError('existing task ID and binary outcome required')
 values=row.get('grade',{}).get('alternatives');alternatives=None
 if values is not None:
  if not isinstance(values,list):raise ValueError('existing per-alternative grade must be a list')
  alternatives=[];seen=set()
  for value in values:
   identifier=value['human_annotation_id']
   if identifier in seen:raise ValueError('duplicate human alternative')
   seen.add(identifier);number(value['token_f1'],True)
   if any(type(value.get(k)) is not bool for k in ['exact_match','complete_support','completion']):raise ValueError('existing alternative flags required')
   alternatives.append(dict(value))
 metrics={key:None for key in QUALITY}
 if alternatives:
  f1=[v['token_f1'] for v in alternatives];supported=[v['token_f1'] for v in alternatives if v['complete_support']]
  metrics.update(max_answer_token_f1=max(f1),mean_annotation_token_f1=statistics.mean(f1),max_joint_supported_token_f1=max(supported) if supported else None,exact_match_any=int(any(v['exact_match'] for v in alternatives)))
 evidence=row.get('human_evidence')
 if evidence is not None:
  metrics['shown_evidence_recall']=fraction(evidence['shown_recall_fraction']);metrics['cited_evidence_recall']=fraction(evidence['cited_recall_fraction'])
 refs=row.get('agent_final_citation_references');valid=row.get('agent_final_valid_citation_references')
 if refs is not None or valid is not None:
  if type(refs) is not int or type(valid) is not int or not 0<=valid<=refs:raise ValueError('invalid existing citation reference counts')
  metrics['citation_reference_precision']=valid/refs if refs else None
 costs={key:(number(row[key]) if row.get(key) is not None else None) for key in COSTS}
 return {'task_id':row['task_id'],'trial_present':True,'final_answer_present':isinstance(row.get('final_response'),dict),'alternatives':alternatives,'descriptive_metrics':metrics,'original_human_evidence':evidence,'citation_reference_counts':{'valid':valid,'total':refs},'costs':costs,'hard_gates_retained':{'success':row['success'],'grade_completion':row.get('grade',{}).get('completion'),'failure':row.get('failure'),'citation_identity_errors':row.get('citation_identity_errors'),'unobserved_native_inner_work':row.get('unobserved_native_inner_work'),'unobserved_neural_work':row.get('unobserved_neural_work')},'scope':'Max lexicalF1 is descriptive only. Supported F1 uses the SAME human alternative. Independently projected shown/cited recall is never combined into joint support. Missing is not zero.'}
def summarize_condition(rows,expected_ids):
 if len(expected_ids)!=len(set(expected_ids)) or not expected_ids:raise ValueError('unique nonempty authorized expected cohort required')
 normalized={}
 for row in rows:
  key=row.get('task_id')
  if key not in expected_ids or key in normalized:raise ValueError('foreign or duplicate trial cannot change frozen denominator')
  normalized[key]=normalize_trial(row)
 ordered=[normalized.get(key,{'task_id':key,'trial_present':False,'alternatives':None,'descriptive_metrics':{k:None for k in QUALITY},'costs':{k:None for k in COSTS},'hard_gates_retained':None}) for key in expected_ids]
 count=len(expected_ids);present=list(normalized.values());summaries={key:distribution([r['descriptive_metrics'][key] for r in ordered],count) for key in QUALITY}
 citations=[r['citation_reference_counts'] for r in present if r['citation_reference_counts']['total'] is not None];total=sum(r['total'] for r in citations);valid=sum(r['valid'] for r in citations)
 return {'expected_tasks':count,'observed_trial_tasks':len(present),'missing_trial_tasks':count-len(present),'final_answers_present':sum(r['final_answer_present'] for r in present),'graded_tasks':sum(bool(r['alternatives']) for r in present),'metric_distributions':summaries,'all_annotation_token_f1_distribution':annotation_distribution([a['token_f1'] for r in present for a in (r['alternatives'] or [])]),'cost_distributions':{key:distribution([r['costs'][key] for r in ordered],count) for key in COSTS},'citation_reference_precision_micro':{'valid':valid,'total':total,'precision':valid/total if total else None,'tasks_with_reference_counts':len(citations),'meaning':'Identity-valid returned citation references, not semantic claim correctness.'},'binary_hard_gates':{'recorded_successes':sum(r['hard_gates_retained']['success'] for r in present),'recorded_failures':sum(not r['hard_gates_retained']['success'] for r in present),'missing_trials':count-len(present),'failure_reasons':dict(Counter(r['hard_gates_retained']['failure'] or 'no_recorded_infrastructure_failure' for r in present if not r['hard_gates_retained']['success'])),'original_acceptance_recomputed':False},'per_task':ordered}
def paired(left,right,ids):
 a={r['task_id']:r for r in left['per_task']};b={r['task_id']:r for r in right['per_task']};rows=[]
 for key in ids:
  x,y=a[key],b[key];delta={}
  for group,keys in [('descriptive_metrics',QUALITY),('costs',COSTS)]:
   delta[group]={k:x[group][k]-y[group][k] if x[group][k] is not None and y[group][k] is not None else None for k in keys}
  rows.append({'task_id':key,'left_trial_present':x['trial_present'],'right_trial_present':y['trial_present'],'left_success':x['hard_gates_retained']['success'] if x['hard_gates_retained'] else None,'right_success':y['hard_gates_retained']['success'] if y['hard_gates_retained'] else None,'left_minus_right':delta})
 # Signed paired deltas are descriptive; failures remain included when observations exist.
 def signed(values):
  known=[x for x in values if x is not None];return {'expected_pairs':len(ids),'observed_pairs':len(known),'missing_pairs':len(ids)-len(known),'observed_mean_delta':statistics.mean(known) if known else None,'all_task_mean_delta':statistics.mean(known) if len(known)==len(ids) else None,'positive':sum(x>0 for x in known),'zero':sum(x==0 for x in known),'negative':sum(x<0 for x in known)}
 return {'direction':'left_minus_right','expected_pairs':len(ids),'quality_deltas':{k:signed([r['left_minus_right']['descriptive_metrics'][k] for r in rows]) for k in QUALITY},'cost_deltas':{k:signed([r['left_minus_right']['costs'][k] for r in rows]) for k in COSTS},'per_task':rows,'scope':'Same task IDs; all failures retained. No winner selection, common-success filtering, significance, semantic accuracy or acceptance inference.'}
def build(manifest):
 ids=manifest['expected_ids'];conditions={};provenance=[]
 for entry in manifest['conditions']:
  path=Path(entry['path'])
  data=load_pinned(path,entry['sha256'])
  if data.get('split')!=manifest['split'] or data.get('rubric_sha256')!=manifest['rubric_sha256']:raise ValueError('different population/rubric cannot be silently pooled')
  if entry['name'] in conditions:raise ValueError('duplicate condition label')
  conditions[entry['name']]=summarize_condition(data['trials'],ids);provenance.append({'name':entry['name'],'path':str(path),'sha256':entry['sha256'],'rubric_sha256':data['rubric_sha256'],'complete_source_receipt':data.get('complete'),'expected_profile_as_recorded':data.get('expected_profile')})
 result={'schema':'airk.private.descriptive-answer-quality-report/1','cohort':manifest['cohort'],'split':manifest['split'],'expected_tasks':len(ids),'conditions':conditions,'paired_comparisons':[{'left':a,'right':b,**paired(conditions[a],conditions[b],ids)} for a,b in itertools.combinations(conditions,2)],'source_conditions':provenance,'source_rubric_sha256':manifest['rubric_sha256'],'reporter_sha256':digest(__file__),'acceptance_changed':False,'rubric_recomputed':False,'semantic_accuracy_measured':False,'scope':'Existing lexical token-overlap and exact normalized-string scores only; no new model calls, gold reads, thresholds, human judgments, recipe selection or qualification. Historical conditions do not describe selected Qwen behavior.'}
 if manifest.get('selected_heldout_count_metadata'):
  entry=manifest['selected_heldout_count_metadata'];path=Path(entry['path'])
  count=load_pinned(path,entry['sha256'])['partitions']['CONFIRMATION']['questions']
  if count!=100:raise ValueError('selected planned100 metadata changed')
  result['selected_Qwen_heldout_status']={'expected_tasks':100,'condition_names':['ordinary_followup','deterministic_packet','ContextCompact','ContextLarge'],'observed_trials':0,'unrun_tasks_per_condition':100,'quality_metrics':None,'per_task_roster_opened':False,'metadata_source':entry,'status':'NOT_RUN as confirmed by owning Query/Root; no selected outputs supplied. Historical nine-task DEV scores are not confirmation scores.'}
 return result
def markdown(report):
 lines=['# Descriptive answer-quality report','',report['scope'],'','Lexical F1 is not semantic accuracy. Original binary gates remain separate. Missing observations are not scored as zero.','',f"Cohort: {report['cohort']} ({report['split']}); denominator {report['expected_tasks']} per condition.",'','| Condition | Observed/expected | Final answers | Mean best lexical F1 | Median | Exact match tasks | Shown recall | Cited recall | Recorded successes |','|---|---:|---:|---:|---:|---:|---:|---:|---:|']
 def fmt(x):return 'unavailable' if x is None else f'{x:.3f}'
 for name,c in report['conditions'].items():
  m=c['metric_distributions'];f=m['max_answer_token_f1'];lines.append(f"| {name} | {c['observed_trial_tasks']}/{c['expected_tasks']} | {c['final_answers_present']} | {fmt(f['all_task_mean'])} | {fmt(f['median'])} | {m['exact_match_any']['one_count']} | {fmt(m['shown_evidence_recall']['all_task_mean'])} | {fmt(m['cited_evidence_recall']['all_task_mean'])} | {c['binary_hard_gates']['recorded_successes']} |")
 lines+=['','| Condition | Mean turns | Mean canonical calls | Mean wall seconds | Wall p90 | Citation-reference precision | Source receipt complete |','|---|---:|---:|---:|---:|---:|---|']
 source={x['name']:x for x in report['source_conditions']}
 for name,c in report['conditions'].items():
  cost=c['cost_distributions'];lines.append(f"| {name} | {fmt(cost['external_model_turns']['all_task_mean'])} | {fmt(cost['canonical_calls']['all_task_mean'])} | {fmt(cost['wall_seconds']['all_task_mean'])} | {fmt(cost['wall_seconds']['p90'])} | {fmt(c['citation_reference_precision_micro']['precision'])} | {source[name]['complete_source_receipt']} |")
 lines+=['','Paired deltas are left minus right on the same frozen tasks, including failures; no winner or acceptance claim.','', '| Left / right | Mean lexical-F1 delta | Mean turn delta | Mean wall-second delta |','|---|---:|---:|---:|']
 for pair in report['paired_comparisons']:
  lines.append(f"| {pair['left']} / {pair['right']} | {fmt(pair['quality_deltas']['max_answer_token_f1']['all_task_mean_delta'])} | {fmt(pair['cost_deltas']['external_model_turns']['all_task_mean_delta'])} | {fmt(pair['cost_deltas']['wall_seconds']['all_task_mean_delta'])} |")
 lines+=['','Best lexical F1 and same-alternative supported F1 are separate distributions in JSON; all original alternative scores/flags are retained. Shown/cited evidence projections are not joined across human alternatives. Zero-citation precision is undefined. Costs and every task-matched quality/cost delta include failures and disclose missing pairs.','']
 if report.get('selected_Qwen_heldout_status'):lines+=['Selected Qwen held-out A6: **not run**. Planned denominator100 per condition; observed0, unrun100, graded metrics unavailable. Confirmation scopes/rubrics were not opened.','']
 return '\n'.join(lines)
def main():
 p=argparse.ArgumentParser();p.add_argument('--manifest',type=Path,required=True);p.add_argument('--manifest-sha256',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 if a.output.exists() or not a.output.resolve().is_relative_to(Path(__file__).resolve().parent):raise ValueError('fresh output under private report root required')
 result=build(load_pinned(a.manifest,a.manifest_sha256));result['manifest_sha256']=a.manifest_sha256;a.output.mkdir();(a.output/'report.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n',encoding='utf-8');(a.output/'report.md').write_text(markdown(result),encoding='utf-8');print(json.dumps({'report':str(a.output/'report.json'),'sha256':digest(a.output/'report.json'),'conditions':len(result['conditions']),'expected_tasks':result['expected_tasks'],'NN_executed':False,'acceptance_changed':False}))
if __name__=='__main__':main()
