# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,math,subprocess,sys,time
import admission as a
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];E=OUT.parent
spec=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h);identity=h.identity();assert h.security(OUT)['owner_sid']==h.HUMAN
load=lambda name:json.loads((OUT/name).read_bytes())
def put(name,value):
 p=OUT/name;assert not p.exists();p.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf8');assert h.security(p)['owner_sid']==h.HUMAN;return {'path':str(p),'bytes':p.stat().st_size,'sha256':a.digest(p.read_bytes())}
summary=load('summary.json');card=load('candidate-card.json');components=load('frozen-components.json');rows=load('all-original1955-roster.json');byid={r['id']:r for r in rows};examples=[json.loads(l)for l in(OUT/'native-F0-examples1867.jsonl').read_text(encoding='utf8').splitlines()]
checks=[]
def check(name,condition):
 checks.append({'name':name,'passed':bool(condition)});assert condition,name
check('all original1955 rows preserved',len(rows)==1955 and len(byid)==1955)
check('actual1867 native examples no invented rows',len(examples)==1867 and {r['id']for r in examples}=={r['id']for r in rows if r['eligibility']=='NATIVE_FIT_SUPPORTED_PAIR'})
check('original932positive935negative retained',sum(r['label']==1 for r in examples)==932 and sum(r['label']==0 for r in examples)==935)
check('86UNKNOWN and2overflow remain excluded',sum(r['eligibility']=='UNKNOWN_RIGHTS'for r in rows)==86 and sum(r['eligibility']=='NATIVE_FOCUS512_OVERFLOW'for r in rows)==2)
check('frozen400TRAIN and85DEV count literals',sum(r['role']=='TRAIN'for r in examples)==400 and sum(r['role']=='DEV'for r in examples)==85)
check('all35components unique frozen role',len(components)==35 and len({r['component']for r in components})==35)
check('parseunknown components never train',all(c['role']not in('TRAIN','DEV')for c in components if c['parse_quarantine']))
check('CSNevaluationonly family never train or tune',all(c['role']not in('TRAIN','DEV')for c in components if c['forced_by_CSN_eval_only']))
check('all compiled tensor hashes intact',all(a.digest(a.canonical(r['native_compiled']))==r['compiled_example_sha256']for r in examples))
check('original question and function hashes intact',all(a.digest(r['question'].encode())==byid[r['id']]['question_sha256']and a.digest(r['function'].encode())==byid[r['id']]['function_sha256']for r in examples))
check('all native-fit rights scopes retained',all(r['rights_review']=='PERMITTED_PRIVATE_RESEARCH_SCOPE'and r['native_visibility']=='accepted'for r in rows if r['eligibility']=='NATIVE_FIT_SUPPORTED_PAIR'))
check('no pristineFINAL or temporalclaim',all(not r['pristine_FINAL']and not r['chronological_generalization']and r['source_time']is None and r['question_time']is None for r in rows))
check('no model scores in example',all(not any(k in r for k in('score','logits','prediction','loss'))for r in examples))
check('DEV3groups cannot support exactsignalphaPoint05',summary['roles']['DEV']['components']==3 and a.sign_bound(3)['positive_sign_threshold']is None)
train=summary['roles']['TRAIN']['rows'];replay=math.ceil(train/9);total=train+replay
card['general_document_replay'].update({'fraction':.10,'fraction_meaning':'fixed nominal target; nine code draws per one replay draw, finish400code rows exactly once','required_replay_draws':replay,'total_training_draws':total,'realized_fraction_if_full_epoch':replay/total,'accepted_native_roster':None,'status':'Fraction target frozen before any score. Exact originalTRAIN general positive-window roster/native-fit/semantic applicability unimplemented here. No replay rows materialized or inferred from code labels; no zero-replay default or general-source scope waiver.'})
card['training']['maximum_optimizer_steps']=math.ceil(total/32);card['training']['final_partial_batch_draws']=total%32;card['training']['reference_dtype']='FP32; TF32 disabled; same fullvocabulary head; refresh source custody before execution.'
card['training']['response_position_semantics']='Authentic512-token native prompt unchanged. Teacherforcing may append only original human yes/no response as513rd slot; labels[-1]only supervised, shifted logits[511]predictresponse. No evidence crop or native512inference ABI change.'
plan_path=ROOT/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/plans-native-r2/focus-original_fp32.json';plan=json.loads(plan_path.read_bytes());asset=plan['asset_root'];declared={p:s for p,s in plan['input_seals'].items()if p.lower().startswith(asset.lower())};inventory=Path(asset)/'research-asset-pins-full.json';assert a.digest(inventory.read_bytes())==declared[str(inventory)];inv=json.loads(inventory.read_bytes());model=next(m for m in inv['models']if m['id']=='Qwen/Qwen3-Reranker-0.6B');weight=next(x for x in model['assets']if x['name']=='model.safetensors');assert weight['sha256']=='27cd75a405b9c1b46b59abfd88aaa209e6fed2a1972cde9b70e7659537c5e65b'
card['starting_checkpoint_metadata']={'model':model['id'],'revision':model['revision'],'weight':weight,'source_inventory':{'path':str(inventory),'sha256':a.digest(inventory.read_bytes())},'source_declarations_from_original_plan':declared,'payloads_reopened':False,'must_refresh_same_pin_custody_before_execution':True}
card['instruction_selection']['F1_candidate_text_sha256']=a.digest(card['instruction_selection']['F1_candidate'].encode());card['instruction_selection']['text_hash_is_native_recipe_or_fit_proof']=False
card['execution_entrypoint']='execute_card.py --card candidate-card-r2.json --stage inspect|preflight. Implemented custody+failclosed gate only; model scorer/LoRA dispatch are remaining implementation, not missing human binary labels.'
cardpin=put('candidate-card-r2.json',card);processes=[]
for stage in('inspect','preflight'):
 argv=[sys.executable,'-B',str(OUT/'execute_card.py'),'--card',str(OUT/'candidate-card-r2.json'),'--stage',stage];start=time.perf_counter();r=subprocess.run(argv,cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 for stream,raw in(('stdout',r.stdout),('stderr',r.stderr)):
  p=OUT/f'card-{stage}-{stream}.log';assert not p.exists();p.write_bytes(raw)
 processes.append({'argv':argv,'exit_code':r.returncode,'elapsed_seconds':time.perf_counter()-start,'stdout_sha256':a.digest(r.stdout),'stderr_sha256':a.digest(r.stderr)})
 check('real card inspector custody'if stage=='inspect'else'real gate refuses noRootrelease',r.returncode==0 if stage=='inspect'else r.returncode!=0 and b'candidate has no execution release'in r.stderr)
put('actual-data-controls.json',{'selected':len(checks),'passed':sum(c['passed']for c in checks),'checks':checks,'card_processes':processes,'native_or_model_calls':False,'candidate_card':cardpin})
steps=[{'gate':'Exact pair/native F0','status':'CLOSED_IN_UNCHANGED_BINARY_SCOPE','facts':'1867bound native tensors/prompt/inputdigests +932positive/935negative;original1955/86UNKNOWN/2overflow retained'}, {'gate':'Family/symbol/clone','status':'CONSERVATIVE_COMPONENT_CLOSURE_FROZEN','facts':'35components; canonicalID/source/symbol/query/knownAST/alpha-renamed/full/partial constraints;5parsefailures quarantine3components249pairs. No universal semantic or historical fork-independence proof'}, {'gate':'Chronology/exposure','status':'NON_TEMPORAL_EXPLORATORY_PILOT_ONLY','facts':'No event timestamps. Versions co-located, upstreamTRAIN raw/labelaudit exposed, CSNevalonlyfamily reserved, existingconsumedFINAL preserved; no pristineFINAL'}, {'gate':'Pre-score split/power','status':'FROZEN_NARROW_DIAGNOSTIC_NO_PROMOTION','facts':'400TRAIN10components/85DEV3/55CAL2/139confirmation2/939research15/249quarantine3; DEV3cannot reach2sided.05sign. Broad40/35/15/10mix and20unrelatedTRAINfamilies notfulfilled'}, {'gate':'Replay','status':'FRACTION_FIXED_NATIVE_ROSTER_TO_IMPLEMENT','facts':'Nominal.10target/400code+45replaydraws/14steps; no replayrowscreated. ExistinggeneralTRAIN source/nativepositive-window review remains implementation beyond1867-onlyscope'}, {'gate':'F1/F2 card','status':'CANDIDATE_PLUS_EXECUTABLE_PREFLIGHT_ONLY','facts':'Checkpoint/source/data/roles/nativeF0 pinned;seed1908/rank16/lr5e-5/batch32/epoch1/fullvocabpromptmasked yesno loss.120s1-step probe/600spilot caps proposed. NativeF1recipe/census,DEVscoring/selection,controlledLoRAtrainer+ownedprobe still toimplement andRootreview'}, {'gate':'Human labels','status':'NO_NEW_LABELS_REQUIRED_FOR_EXACT_BINARY_TARGET','facts':'Broader judgedsamequeryranking and necessary/complementary/exception/terminal panels absent from these singletonpairs; they do not block pointwisepreparation.'}]
put('gate-closure-status.json',steps)
readme='''# Existing binary F2 private preparation

This packet freezes deterministic preparation for an exploratory exact human question/full-function relevance pilot. It does not admit gradients or complete Step8. Existing labels suffice for this pointwise target; no unjudged crosspair negatives or new evidence-role/terminal labels are added.

Actual closure:1867 native512 examples (932positive/935negative), original1955 denominator,86rightsUNKNOWN and2positiveoverflow refusals.658 existing TRAIN Python sourcefiles parsed;5failures quarantine entire3components/249pairs. Canonical/source/function/symbol/query/knownAST and conservative alpha-renamed/partial-copy closure yields35components. Whole existing CSNevaluation-only family footprint remains outside TRAIN and DEV. No original heldout/FINAL sourcebody was opened.

Frozen roles:400TRAIN/10components,85DEV/3,55CAL/2,139confirmation/2,939research-test/15,249quarantine/3. UpstreamTRAIN/raw-label exposure persists; research-test is never pristineFINAL. Unknown timestamps stay unknown, with no temporal-generalization claim. Three DEVcomponents cannot yield two-sided5%sign evidence even at3/3wins: this is no-promotion diagnostic scope, not broadsoftwaremix or20unrelatedTRAINfamilies.

NativeF0 prompt/tensors/inputdigests are projected directly from retained Rust output. No Python tokenizer/escape/crop/DTO generation. Original publisher checkpoint metadata and all same-source declarations are bound without reopening model weights. Response-position fullvocabulary CE masks all512prompt tokens and supervises only original yes9693/no2152 response. No sigmoidBCE or unknown inbatchnegative substitution.

`candidate-card-r2.json` fixes nominal10%general replay (400code+45replaydraws,14steps). No replay rows were materialized. Exact admitted generalTRAIN positive-window/native roster, nativeF1recipe+no-crop census, fixedDEVscoring/selection and controlledLoRAtrainer/resourceprobe remain implementation and future Rootreviewed execution. They are not missing human labels for the original binarytarget.

`prepare.py` is a create-new data closure script. `admission.py` owns pure logic with17literalcontrols. `execute_card.py --card <candidate-card-r2.json> --stage inspect` actually verifies frozen custody; `--stage preflight` truthfully refuses unreviewed execution. It does not yet dispatch a model or train: the candidate is executable preparation/preflight, not a claim of completed F1/F2 model implementation.

Original failures retained: first partial-copy synthetic literal had39ASTnodes below the declared40threshold. The literal was enlarged, not the rule weakened;17/17thenpassed. Data preparation completed66.079s. No new acquisition, scoring, gradients, model/native/GPU/Cargo execution, tracked changes, or mutation of historical exposures/FINAL occurred.
'''
p=OUT/'README.md';assert not p.exists();p.write_text(readme,encoding='utf8')
allpins={p.name:{'bytes':p.stat().st_size,'sha256':a.digest(p.read_bytes()),'security':h.security(p)}for p in OUT.iterdir()if p.is_file()}
put('handoff.json',{'schema':'airk.private.F2-existing-pairs-closure-handoff/1','recorded_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'identity':identity,'outputs':allpins,'candidate_card':cardpin,'literal_unit_controls':{'selected':17,'passed':17,'original_failure_preserved':True},'actual_data_controls':{'selected':len(checks),'passed':sum(c['passed']for c in checks)},'preparation_process_seconds':66.07864309998695,'native_or_model_or_GPU_or_Cargo':False,'new_data_or_labels':False,'tracked_edits':False,'fullStep8_accepted':False,'training_admitted':False,'promotion':False,'missing_human_binary_labels':False,'next':'Rootreview exactdatafreeze/narrow scope; implementnativeF1/replay/compiler/DEVscoring and boundedLoRAowner beforeactualexecution.'})
print(json.dumps({'handoff':str(OUT/'handoff.json'),'sha256':a.digest((OUT/'handoff.json').read_bytes()),'candidate_card':cardpin,'actual_controls':len(checks),'literal_controls':17}))
