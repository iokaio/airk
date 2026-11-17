# SPDX-License-Identifier: Apache-2.0
"""Descriptive Dev9 comparison only. Never final qualification or gold retuning."""
from pathlib import Path
from fractions import Fraction
import json,hashlib
base=Path(__file__).resolve().parent;plan=json.loads((base/'native-a6-command-plan-r8-runtime-r2.json').read_bytes());conditions=plan['phases'][0]['conditions'];rows={};pins={}
for c in conditions:
 p=Path(c['result']);v=json.loads(p.read_bytes());assert v['complete'] is True and not v.get('campaign_error') and v['split']=='development' and len(v['trials'])==9;assert v['expected_profile']==c['emitter_environment']['AIRK_ARBITER_EVALUATION_PROFILE'] if c['arm']=='arbiter' else v['expected_profile'] is None
 rows[c['name']]=v;pins[c['name']]=hashlib.sha256(p.read_bytes()).hexdigest()
assert len({r['native_fixture_ready_sha256'] for r in rows.values()})==1
assert len({r['native_index_bindings_sha256'] for r in rows.values()})==1
assert len({r['rubric_sha256'] for r in rows.values()})==1
assert len({json.dumps(r['agent_asset_seals'],sort_keys=True) for r in rows.values()})==1
ids=[t['task_id'] for t in next(iter(rows.values()))['trials']];assert len(set(ids))==9
assert all([t['task_id'] for t in r['trials']]==ids for r in rows.values())
def success(r):return {t['task_id'] for t in r['trials'] if t['success']}
def mean_turns(r,subset):
 ts=[t['external_model_turns'] for t in r['trials'] if t['task_id'] in subset];return Fraction(sum(ts),len(ts)) if ts else None
def pair(v,b):
 common=success(v)&success(b);a=mean_turns(v,common);z=mean_turns(b,common)
 return {'common_success':len(common),'optional_turns_sum':sum(t['external_model_turns'] for t in v['trials'] if t['task_id'] in common),'baseline_turns_sum':sum(t['external_model_turns'] for t in b['trials'] if t['task_id'] in common),'relative_turn_reduction':float(1-a/z) if a is not None and z else None,'completion_difference_pp':100*(len(success(v))-len(success(b)))/9}
baselines=['ordinary-followup','deterministic-packet'];strongest=sorted(baselines,key=lambda n:(-len(success(rows[n])),mean_turns(rows[n],success(rows[n]))) if success(rows[n]) else (-len(success(rows[n])),Fraction(10**9)),reverse=False)[0]
if len(success(rows[baselines[0]]))==len(success(rows[baselines[1]])) and mean_turns(rows[baselines[0]],success(rows[baselines[0]]))==mean_turns(rows[baselines[1]],success(rows[baselines[1]])):strongest='deterministic-packet'
summary={}
for name,v in rows.items():
 ts=v['trials'];summary[name]={'completed':len(ts),'goal_success':len(success(v)),'external_turns_all':sum(t['external_model_turns'] for t in ts),'canonical_calls_all':sum(t['canonical_calls'] for t in ts),'questions_using_all8_calls':sum(t['canonical_calls']==8 for t in ts),'success_external_turns':[t['external_model_turns'] for t in ts if t['success']],'native_answer_inference_known_us':sum(c['full_canonical_value']['work']['inference_us'] for t in ts for c in t['all_native_calls'] if isinstance(c.get('full_canonical_value'),dict) and 'work' in c['full_canonical_value'] and c['full_canonical_value']['work'].get('inference_us') is not None),'native_answer_inference_unreported_count':sum(c['full_canonical_value']['work'].get('inference_us') is None for t in ts for c in t['all_native_calls'] if isinstance(c.get('full_canonical_value'),dict) and 'work' in c['full_canonical_value']),'agent_load_seconds':v['agent_load_seconds'],'wall_seconds':v['wall_seconds'],'trial_wall_seconds':sum(t['wall_seconds'] for t in ts),'shown_mean_fact_recall':float(sum((Fraction(*t['human_evidence']['shown_recall_fraction']) for t in ts),Fraction())/9),'cited_mean_fact_recall':float(sum((Fraction(*t['human_evidence']['cited_recall_fraction']) for t in ts),Fraction())/9),'agent_citation_valid_references':sum(t['agent_final_valid_citation_references'] for t in ts),'agent_citation_all_references':sum(t['agent_final_citation_references'] for t in ts),'failures':[t['failure'] for t in ts],'paired_baselines':{n:pair(v,rows[n]) for n in baselines} if name not in baselines else {},'forecast100_seconds_including_once_load':v['agent_load_seconds']+100/9*sum(t['wall_seconds'] for t in ts)+(v['wall_seconds']-v['agent_load_seconds']-sum(t['wall_seconds'] for t in ts))}
out=base/'native-goal-r8-dev-comparison.private.json';assert not out.exists();out.write_text(json.dumps({'development_diagnostic_only':True,'final_evaluation':False,'questions':9,'strongest_baseline':strongest,'conditions_sha256':pins,'conditions':summary,'linear_forecast100_all6_seconds':sum(r['forecast100_seconds_including_once_load'] for r in summary.values()),'forecast_limits':'Dev is QASPER only; final includes global Hotpot. Linear forecast is not upper bound and excludes configuration/warm/setup/index and scheduling margins. No final acceptance inference.'},indent=2)+'\n');print(out)
