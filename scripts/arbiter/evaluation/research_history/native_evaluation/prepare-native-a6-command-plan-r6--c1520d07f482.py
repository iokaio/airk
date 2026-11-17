# SPDX-License-Identifier: Apache-2.0
"""Prepare exact caller commands from actual Ready and frozen scope rosters.

Preparation only: never create indexes, IDs, bindings, configuration or Ready.
"""
import argparse
import hashlib
import json
from pathlib import Path

base=Path(__file__).resolve().parent;repo=base.parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--runtime-pair',type=Path,required=True)
parser.add_argument('--output',type=Path,default=base/'native-a6-command-plan-r6.json')
args=parser.parse_args();pair=args.runtime_pair.resolve();output=args.output.resolve()
if output.exists() or not output.is_relative_to(base):raise ValueError('fresh private output required')
ready_path=base/'native-goal-private-anchor-r6/work/ready.json';ready_raw=ready_path.read_bytes()
assert hashlib.sha256(ready_raw).hexdigest()=='e299a7ea257ec2343c3a9e9179c04fb526fccaa12c666fd200b2e263096a803d'
ready=json.loads(ready_raw);work=ready_path.parent;control=ready['control_root']
assert ready['evaluation'] is True and ready['release_qualified'] is False
assert ready['health']['instance']=='5f5fb0dfca23566736f69c0045020bb5'
assert ready['inputs_sha256']=='868810174d6c287502653d49ded5bddbfe3d1b8c43519bd97ae023c4797fd3d4'
scopes={};pins={}
for split,filename,expected in (
 ('development','native-goal-development-question-scopes-r1.json','ef6b48b6d86fa2e4ccdd1097c1e261a49f7b79521e257518f4cff9320c9540ce'),
 ('final','native-goal-100-question-scopes-r1.json','4470916861293fe1185bb4c0469a8c051437f3f3451ba5c8a813a2c5df5f4a0c')):
 path=base/filename;raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()==expected
 value=json.loads(raw);assert len(value['questions'])==(9 if split=='development' else 100)
 assert set(value['source_roots'])=={row['source_root'] for row in value['questions']}
 scopes[split]=(path,value);pins[str(path)]=expected
roots=sorted({root for _,value in scopes.values() for root in value['source_roots']})
fixture=base/'native-human-fixtures-r2'
assert all(Path(root).resolve().is_relative_to(fixture/'sources') for root in roots)
cli=str(pair/'airkctl.exe');prefix=[cli,'--service-runtime-dir',control]
python=str(base/'training-env/venv/Scripts/python.exe')
service_exe=str(repo/'artifacts/advanced-split/native-harnesses/goal-fixture-cpu24-r6/airk_service-7285a460d5f02399.exe')
emitter=[service_exe,'service_runtime::evaluation_fixture_tests::emit_private_arbiter_configuration','--ignored','--exact','--nocapture','--test-threads=1']
index_steps=[]
for ordinal,root in enumerate(roots):
 data=work/'canonical-index-data'/('source-'+str(ordinal).zfill(2))
 collection='goal-source-'+str(ordinal).zfill(2)
 selected=prefix+['--root',root,'--data-dir',str(data)]
 index_steps.append({'source_root':root,'data_root':str(data),'collection':collection,
  'setup_argv':selected+['--output','json','library','setup','--name',collection,'--index-collection',collection,'--watch','off','--apply'],
  'index_argv':selected+['--collection',collection,'--output','json','index'],
  'observe_original_job_argv_template':prefix+['--output','json','jobs','get','<ACTUAL job.id from index response>']})
profiles={row['model']:row['manifest'] for row in ready['profiles']}
models=['arbiter-small-intern-08b','arbiter-small-strands-2b','arbiter-large-intern-4b','arbiter-large-kev-4b']
assert set(profiles)==set(models)
phases=[]
for split,(scope_path,value) in scopes.items():
 short='dev' if split=='development' else 'final'
 shared_bindings=work/(short+'-base-off.bindings.json')
 conditions=[]
 choices=[('ordinary-followup','ordinary_followup',None),('deterministic-packet','deterministic_packet',None)]+[(m,'arbiter',m) for m in models]
 for ordinal,(name,arm,model) in enumerate(choices):
  tag=short+'-base-off' if ordinal==0 else short+'-'+str(ordinal).zfill(2)
  env={'AIRK_ARBITER_EVALUATION_WORK':str(work),'AIRK_ARBITER_EVALUATION_SCOPES':str(scope_path),
       'AIRK_ARBITER_EVALUATION_SCOPES_SHA256':pins[str(scope_path)],
       'AIRK_ARBITER_EVALUATION_PROFILE':profiles[model or models[0]],
       'AIRK_ARBITER_EVALUATION_ENABLED':'true' if model else 'false',
       'AIRK_ARBITER_EVALUATION_CONTROL_PREFIX':tag}
  condition_dir=base/('native-goal-r6-'+short+'-'+name)
  command=[python,str(repo/'tools/retrieval-train/run_goal_condition.py'),'--runtime-pair',str(pair),
   '--control-root',control,'--fixture-ready',str(ready_path),'--index-bindings',str(shared_bindings),
   '--request-template',str(work/'context-request.json'),'--question-scopes',str(scope_path),
   '--rubric',str(base/'human-goal-rubric-r6/rubric.private.json'),'--fixture',str(fixture),
   '--output',str(condition_dir),'--split',split,'--arm',arm,'--assets',str(base/'candidate-assets')]
  if model:command+=['--profile-manifest',profiles[model]]
  conditions.append({'name':name,'arm':arm,'model':model,'emitter_environment':env,
   'emitter_argv':emitter if ordinal!=1 else [],
   'apply_native_configuration_argv':prefix+['--output','json','arbiter','configure','--file',str(work/(tag+'.configure.json'))] if ordinal!=1 else [],
   'observe_actual_ready_argv':prefix+['--output','json','arbiter','status'],
   'condition_argv':command,'result':str(condition_dir/'condition.private.json')})
 phases.append({'split':split,'questions':len(value['questions']),'source_roots':value['source_roots'],
                'shared_immutable_bindings':str(shared_bindings),'conditions':conditions})
final=phases[1]
summary=[python,str(repo/'tools/retrieval-train/summarize_goal_campaign.py')]
for condition in final['conditions']:summary+=['--condition',condition['result']]
summary+=['--ready',str(ready_path),'--rubric',str(base/'human-goal-rubric-r6/rubric.private.json'),
 '--question-scopes',str(scopes['final'][0]),'--request-template',str(work/'context-request.json'),
 '--fixture',str(fixture),'--output',str(base/'native-goal-r6-final-comparison')]
plan={'schema':'airk.private-a6-command-plan/1','preparation_only':True,'native_fixture_ready':str(ready_path),
 'native_fixture_ready_sha256':hashlib.sha256(ready_raw).hexdigest(),'instance':ready['health']['instance'],
 'runtime_pair':str(pair),'service_test_executable':service_exe,'source_scope_pins':pins,
 'index_steps':index_steps,'phases':phases,'final_summary_argv':summary,
 'requirements':[
  'Wait for the fresh matched pair; existing older airkctl is read-only help/source inspection only.',
  'Seal actual pair artifacts and original Ready/PID birth/instance/namespace before and after every control stage. Never service-start/restart or adopt a replacement fixture.',
  'Setup only creates mapping. Preserve BOTH native draft+apply stdout records/stderr; never invent index IDs.',
  'Submit each exact source/data/collection index tuple ONCE, retain actual job.id and full native replies. Observe that ORIGINAL job under a deadline until succeeded/outcome succeeded; no retry/new nonce on failed/unknown outcome.',
  'All indexing must finish before timed goal trials; require actual trained-v2 semanticReady/default Hybrid and incomplete=false, not setupReady or lexical substitution.',
  'Native emitter owns all bindings and Configure JSON; read actual files only after its selected test passes. All six conditions in each phase use the SAME original baseline binding receipt. Later emitters refresh only configuration CAS; if source/index/namespace/publication/window-selection identity changes, STOP for review, not silent rebinding.',
  'Apply disabled configuration for both baselines; second baseline reuses the unchanged disabled state. Before each optional condition apply newly emitted configuration and observe actual Ready for that exact manifest.',
  'Run Development9 six conditions FIRST, inspect actual results/failures and retain source seals; final100 remains unscored until that review and algorithm/profile/transport freeze.',
  'Run final100 six conditions serially with identical model/greedy/read/token/byte/deadline budgets and all original failures/turns retained. No question resampling or hidden retries.',
  'Final summary rejects incomplete conditions/campaign_error/nonunique four profile roster or inconsistent binding/Ready/source/algorithm identities. A6 research cannot promote or replace the failed A1 encoder gate.'
 ]}
output.write_text(json.dumps(plan,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'output':str(output),'source_roots':len(roots),'development_questions':9,'final_questions':100,'conditions_each':6,'preparation_only':True}))
