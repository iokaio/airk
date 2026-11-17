# SPDX-License-Identifier: Apache-2.0
"""Existing-byte deterministic closure, create-new outputs; never model/native calls."""
from pathlib import Path
import collections, datetime, hashlib, importlib.util, json, math, sys, time
import admission as a
ROOT=Path(__file__).resolve().parents[5]
E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z'
OUT=Path(__file__).resolve().parent
B=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1'
J=B/'cosqa-source-join-r1'; N=J/'native-visibility-preparation-r2'; S=B/'split-clone-chronology-r1'
spec=importlib.util.spec_from_file_location('custody', E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h)
identity=h.identity();assert h.security(OUT)['owner_sid']==h.HUMAN
started=time.perf_counter(); cutoff=started+180; pins={}
def pin(p):
 p=Path(p);meta=p.stat();hasher=hashlib.sha256()
 with p.open('rb')as f:
  while chunk:=f.read(1024*1024):hasher.update(chunk)
 result={'path':str(p.resolve()),'bytes':meta.st_size,'sha256':hasher.hexdigest(),'security':h.security(p)};pins[str(p.resolve())]=result;return result

def load(p):
 pin(p);return json.loads(Path(p).read_bytes())

def put(n,value):
 p=OUT/n;assert not p.exists();p.write_bytes(value if isinstance(value,bytes)else (json.dumps(value,ensure_ascii=False,indent=2)+'\n').encode());assert h.security(p)['owner_sid']==h.HUMAN;return {'path':str(p),'bytes':p.stat().st_size,'sha256':a.digest(p.read_bytes())}

def deadline():
 if time.perf_counter()>=cutoff:raise RuntimeError('original180-second preparation deadline exceeded; no automatic retry')

tasks=load(J/'source-and-rights-candidate-tasks.json');rights=load(J/'source-rights-applicability-review-r1/review.json');native=load(N/'launch-native-optimized-r1/visibility-summary.json');arm=load(N/'launch-native-optimized-r1/visible-arm-review-candidates.json');canon=load(J/'canonical-repository-identities.json');sources=load(S/'source-file-descriptors.json');descriptors=load(S/'TRAIN-row-family-descriptors.json');known_clones=load(S/'substantive-python-AST-copy-candidates.json');csn=load(B/'f2-code-pilot-applicability-r1/CSN-eval-only-row-metadata.json');exposure=load(S/'prior-exposure-metadata-and-partition-check.json');qasper=load(B/'split-clone-chronology-r2/qasper-exposure-clarification.json');freeze=load(ROOT/'tools/retrieval-train/software-v2-freeze.json');native_input=load(N/'native-visibility-input-optimized-r1.json');focus_input=load(native_input['focus_input']['path']);original_plan=load(ROOT/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/plans-native-r2/focus-original_fp32.json')
for p in [ROOT/'docs/arbiter/final-plan.md',ROOT/'docs/arbiter/weights-v2-plan.md',ROOT/'crates/airk-retrieval/src/arbiter/compiler/focus.rs',ROOT/'tools/retrieval-train/README.md',E/'f2-existing-pairs-admission-audit-r1/handoff.json',E/'f2-existing-pairs-admission-audit-r1/gate-and-cheap-closure-steps.json']:pin(p)
assert len(tasks)==1955 and arm['F2_candidate_count']==1867
assert pins[str(Path(native_input['focus_input']['path']).resolve())]['sha256']==native_input['focus_input']['sha256']
ids=set(arm['F2_source_window_candidate_ids']);rb={r['original_train_idx']:r for r in rights['human_pairs']};nb={r['id']:r for r in native['per_original_row_observations']};db={r['id']:r for r in descriptors if r['dataset']=='CoSQA'}
repo_alias={}
for c in canon['results']:
 if c['status']==200:
  key='github-id:'+str(c['source_id']or c['parent_id']or c['id'])
  for name in (c['requested_repository'],c['full_name']):repo_alias[name.lower()]=key

def repo_key(name):return repo_alias.get(name.lower(),'repository:'+name.lower())
keys={repo_key(r) for s in sources for r in s['repos']}|{repo_key(t['repository']) for t in tasks}|{repo_key(r['repository_literal']) for r in csn}
u=a.DSU(keys);edge_rows=[]
def join(repos,reason,fingerprint):
 keys=sorted({repo_key(r)for r in repos})
 if len(keys)>1:
  for k in keys[1:]:u.union(keys[0],k)
  edge_rows.append({'reason':reason,'fingerprint':fingerprint,'keys':keys})
for c in known_clones:join(c['repos'],'retained exact original AST clone',c['ast_sha256'])
for field in ['labeled_code_sha256','question_sha256','symbol_family']:
 groups=collections.defaultdict(set)
 for r in descriptors:
  if r.get(field)and r.get('repo'):groups[r[field]].add(r['repo'])
 for fp,repos in groups.items():join(repos,'retained all-TRAIN '+field,fp)
# All source descriptor bodies are existing TRAIN closure only. CSN remains
# metadata-only; no original heldout/gold, oldQASPER, sourceFINAL body is read.
parsed=0;parse_refusals=[];fp_repos=collections.defaultdict(set);source_sha_repos=collections.defaultdict(set);total_source_bytes=0
for s in sources:
 deadline();source_sha_repos[s['sha256']].update(s['repos'])
 if not s['logical_path'].endswith('.py')or 'TechQA'in s['datasets']:continue
 assert set(s['datasets'])<= {'CoSQA','ContextBench'}
 p=Path(s['path']);raw=p.read_bytes();assert len(raw)==s['bytes']and a.digest(raw)==s['sha256'];pins[str(p.resolve())]={'path':str(p.resolve()),'bytes':len(raw),'sha256':s['sha256'],'security':h.security(p)};total_source_bytes+=len(raw);assert total_source_bytes<=64*1024*1024
 try:
  if len(raw)>2*1024*1024:raise ValueError('source cap')
  fingerprints=a.fingerprints(raw.decode('utf8'));parsed+=1
 except (SyntaxError,UnicodeDecodeError,ValueError,RecursionError)as error:
  parse_refusals.append({'path':s['path'],'sha256':s['sha256'],'repos':s['repos'],'exception_type':type(error).__name__});continue
 for fp in fingerprints:fp_repos[fp].update(s['repos'])
for fp,repos in source_sha_repos.items():join(repos,'exact TRAIN source file',fp)
for(kind,fp),repos in fp_repos.items():join(repos,kind,fp)
components=u.components();fit_repos={repo_key(t['repository']) for t in tasks if t['original_train_idx']in ids};fit_components={components[k]for k in fit_repos}
quarantine={components[repo_key(r)]for s in parse_refusals for r in s['repos']} &fit_components
# The entire already-retained CSN EVALUATION_ONLY source family footprint stays
# outside F2 TRAIN, DEV recipe choice and mining. Rights UNKNOWN doesn't erase
# its evaluation-only historical exclusion. No new candidate panel is created.
forced={components[repo_key(r['repository_literal'])]for r in csn}&fit_components
roles=a.roles_for(fit_components,forced,quarantine)
rows=[]
for t in tasks:
 id=t['original_train_idx'];r=rb[id];native_obs=nb[id];d=db[id];component=components[repo_key(t['repository'])]
 state='NATIVE_FIT_SUPPORTED_PAIR'if id in ids else 'UNKNOWN_RIGHTS'if r['rights_review']!='PERMITTED_PRIVATE_RESEARCH_SCOPE'else 'NATIVE_FOCUS512_OVERFLOW'
 row={'id':id,'code_idx':t['code_idx'],'label':t['label'],'target_token':a.supervised_target(t['label']),'upstream_role':'publisher_TRAIN','rights_review':r['rights_review'],'native_visibility':native_obs['focus_compile_visibility'],'eligibility':state,'repository':t['repository'],'repository_family':repo_key(t['repository']),'component':component,'role':roles.get(component,'NO_ADMITTED_COMPONENT'),'source_url':t['source_url'],'commit':t['commit'],'source_sha256':r['source_sha256'],'source_path':r['source_path'],'symbol':d['symbol'],'question_sha256':t['question_sha256'],'function_sha256':t['code_sha256'],'source_time':None,'question_time':None,'chronological_generalization':False,'pristine_FINAL':False,'unjudged_cross_pair':False}
 if id in ids:a.assert_pair_scope(row)
 rows.append(row)
byid={r['id']:r for r in rows};assert len(byid)==1955
# Strict streaming projection of existing pretty-printed native result rows;
# no handwritten compiler, tokenization, cropping, prompt escaping or scoring.
report_path=Path(native['native_report']['path']);report_pin=pin(report_path);assert report_pin['sha256']==native['native_report']['sha256'];compiled_rows=[];native_header={};native_seen=0
with report_path.open(encoding='utf8')as f:
 for line in f:
  if line.startswith('  "native_binary_sha256":'):native_header['native_binary_sha256']=json.loads(line.strip().split(':',1)[1].rstrip(','))
  if line=='  "rows": [\n':break
 else:raise ValueError('retained native row array missing')
 block=[]
 for line in f:
  if line=='  ],\n':assert not block;break
  block.append(line)
  if line not in ('    },\n','    }\n'):continue
  value=json.loads(''.join(block).strip().rstrip(','));block=[];native_seen+=1;original=value['original'];id=original['id'];row=byid[id]
  assert original['question_sha256']==row['question_sha256'] and original['document_sha256']==row['function_sha256'] and original['published_human_relevance_label']==row['label']
  assert a.digest(original['question'].encode())==row['question_sha256']and a.digest(original['document'].encode())==row['function_sha256']
  if id not in ids:continue
  assert value['focus']['visibility']=='accepted';compiled=value['focus']['compiled'];a.assert_compiled(compiled)
  native_hash=a.digest(a.canonical(compiled));row['compiled_example_sha256']=native_hash;row['native_input_digest']=compiled['input_digest'];row['native_original_report']=report_pin['sha256']
  compiled_rows.append({'id':id,'question':original['question'],'function':original['document'],'original_source_span':value['function_span'],'label':row['label'],'target_token':row['target_token'],'role':row['role'],'component':row['component'],'compiled_example_sha256':native_hash,'native_compiled':compiled})
assert native_seen==1955 and len(compiled_rows)==1867 and len({r['id']for r in compiled_rows})==1867
assert collections.Counter(r['label']for r in rows if r['id']in ids)=={0:935,1:932}
role_stats={};component_rows=[]
for c in sorted(fit_components):
 z=[r for r in rows if r['id']in ids and r['component']==c];component_rows.append({'component':c,'role':roles[c],'family_keys':sorted({r['repository_family']for r in z}),'repositories':sorted({r['repository']for r in z}),'pairs':len(z),'labels':dict(collections.Counter(r['label']for r in z)),'functions':len({r['function_sha256']for r in z}),'forced_by_CSN_eval_only':c in forced,'parse_quarantine':c in quarantine,'assignment_sha256':a.digest((a.DOMAIN+c).encode())})
for role in sorted({r['role']for r in rows if r['id']in ids}):
 z=[r for r in rows if r['id']in ids and r['role']==role];counts=collections.Counter(r['component']for r in z);role_stats[role]={'rows':len(z),'labels':dict(collections.Counter(r['label']for r in z)),'components':len(counts),'repositories':len({r['repository']for r in z}),'functions':len({r['function_sha256']for r in z}),'largest_component_rows':max(counts.values()),'kish_concentration_descriptor_only':len(z)**2/sum(n*n for n in counts.values()),'conditional_sign_math':a.sign_bound(len(counts))}
assert sum(v['rows']for v in role_stats.values())==1867
# A row or repeated function cannot cross component roles. Exact same pair
# conflicts refuse; repeated full functions with distinct questions stay judged.
function_roles=collections.defaultdict(set);query_roles=collections.defaultdict(set);pair_labels=collections.defaultdict(set)
for r in rows:
 if r['id']not in ids:continue
 function_roles[r['function_sha256']].add(r['role']);query_roles[r['question_sha256']].add(r['role']);pair_labels[(r['question_sha256'],r['function_sha256'])].add(r['label'])
assert all(len(v)==1 for v in function_roles.values())and all(len(v)==1 for v in query_roles.values())and all(len(v)==1 for v in pair_labels.values())
assert not any(r['role']in ('TRAIN','DEV')and r['component']in forced for r in rows if r['id']in ids)
assert not any(r['role']=='TRAIN'and r['component']in quarantine for r in rows if r['id']in ids)
outputs={}
outputs['cohort']=put('all-original1955-roster.json',rows);outputs['compiled']=put('native-F0-examples1867.jsonl',b''.join(a.canonical(r)+b'\n'for r in compiled_rows));outputs['components']=put('frozen-components.json',component_rows);outputs['edges']=put('conservative-clone-family-edges.json',edge_rows);outputs['parse_refusals']=put('source-parse-quarantine.json',parse_refusals)
outputs['exposure']=put('exposure-and-rights-scope.json',{'upstream':'CoSQA publisher TRAIN14ebcacf9e9bc3e7109102632bc63047876f27d2','rights':'unchanged permitted unpublished private research only; retain each source/revision/root notice pin and86UNKNOWN/2overflow refusals','CSN_original_eval_only_rows':len(csn),'CSN_forced_components':sorted(forced),'heldout_source_bodies_opened':False,'CoSQA_prior_raw_schema_and_label_audits':True,'model_pretraining_exposure':'unknown; no clean-pretraining claim','chronology':'No acquired event timestamps. Versions/symbols stay co-located; no temporal generalization claim.','QASPER':qasper,'prior_source_metadata_only_check':exposure,'oldFINAL_preserved':True,'fresh_FINAL_created':False,'broad_Step8_mixture_or_terminal_gold_satisfied':False})
outputs['native_binding']=put('native-original-compiler-binding.json',{'native_report':report_pin,'original_native_input':pins[str((N/'native-visibility-input-optimized-r1.json').resolve())],'focus_input':pins[str(Path(native_input['focus_input']['path']).resolve())],'original_binary':native_header,'reference_checkpoint_plan':pins[str((ROOT/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/plans-native-r2/focus-original_fp32.json').resolve())],'checkpoint_asset_root':original_plan['asset_root'],'exact_tensor_example_projection':True,'native_fit_reexecuted':False,'F1_fit_qualified':False})
card={'schema':'airk.private.F1-F2-exact-human-pair-candidate-card/1','scope':'Exploratory Python exact original question/full-function binary relevance only; candidate for Root review, no broad software/ranking/terminal/promotion acceptance.','root_review_accepted':False,'promotion':False,'seed':1908,'roles':outputs['components'],'cohort':outputs['cohort'],'native_F0_examples':outputs['compiled'],'native_binding':outputs['native_binding'],'primary_diagnostic':'component-macro pointwise AUROC on original judged pairs; fixed-zero balanced accuracy secondary. No nDCG on singleton pools, probability calibration or new evidence-role labels.','selection_rule':'F1 improves DEV component-macro AUROC vsF0 with no negative DEV component fixed-zero balanced-accuracy change; otherwise retain F0. Ties retain F0; native overflows count as refusals and never refill. F2 original/adapted use exact same selected instruction and DEV inputs; no acceptance/promotion from diagnostics.','instruction_selection':{'F0':'existing exact Rust-emitted publisher instruction','F1_candidate':'Given a question about software behavior, retrieve a Python function relevant to answering the question. Judge relevance of the whole provided function to the question; answer yes or no.','F1_changes_only_instruction':True,'native_F1_compilation':None,'DEV_choice_receipt':None,'mandatory':'Native owner emits separately versioned F1 recipe/fixtures and both prompt refusal rosters before scoring. Python must not replace strings or tokenize native input.'},'training':{'objective':'full-vocabulary response-position cross-entropy on yes9693/no2152; all prompt positions masked; no sigmoidBCE, inbatchnegatives, invented crosspairs or terminal labels','rank':16,'learning_rate':5e-5,'effective_batch':32,'microbatch':1,'gradient_accumulation':32,'epochs':1,'dropout':0.0,'sample_order':'SHA256(seed1908 +id) once per full epoch; no score-driven resampling','example_weights':'uniform original judged pairs; function/family clustered metrics disclose correlation','maximum_optimizer_steps':math.ceil(role_stats.get('TRAIN',{}).get('rows',0)/32),'probe_max_optimizer_steps':1,'probe_max_wall_seconds':120,'pilot_max_wall_seconds':600,'host_committed_bytes_cap':16*1024**3,'CUDA_allocator_bytes_candidate_cap':8*1024**3,'measured_probe_receipt':None,'starting_checkpoint':outputs['native_binding'],'merge_before_native_ONNX_comparison':True,'resource_caps_are_proposals_not_measured_fits':True},'general_document_replay':{'fraction':None,'accepted_native_roster':None,'status':'Not available from1867code-only rows; no0replay default or invented replaylabels. Full Step10 F2 release still requires existing general source/native positive-window review and frozen fraction, or explicit separately reviewed narrowpilot exception.'},'critical_limits':freeze['acceptance'],'stop_rules':['Any native input/refusal, nonfinite loss/score, source hash change, rights/custody discrepancy or budget/cancellation failure stops ownedrun, preserves diagnostics, no retry or refill.','No model/GPU/scoring/gradient execution until Root reviews exact candidate and filled native/replay/measurement bindings.','At original cutoff naturally settle ownership and retain evidence; no assumed cleanup or pass.'],'five_population_status':'TRAIN/DEV/CAL/confirmation/research-test roles frozen from metadata only; none is pristine FINAL. Original upstreamTRAIN/exposure/evalonly restrictions persist.','power':'Conditional exact sign math insummary; component independence not guaranteed. This small conservative pilot cannot establish fullStep8 superiority/strata; no promotion even if pointwise metric improves.','execution_entrypoint':'execute_card.py --card candidate-card.json --stage preflight; scoring/training deliberately unavailable until missing native/replay/owned execution bindings are implemented and independently reviewed.'}
outputs['card']=put('candidate-card.json',card)
summary={'schema':'airk.private.F2-existing-pairs-cheap-closure/1','identity':identity,'recorded_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'original_denominator':1955,'native_fit_supported_pairs':1867,'rights_unknown_refused':86,'positive_overflow_refused':2,'fit_labels':{'0':935,'1':932},'functions':len(function_roles),'repositories':37,'conservative_components':len(fit_components),'roles':role_stats,'source_parse':{'parsed_files':parsed,'refusals':len(parse_refusals),'bytes':total_source_bytes,'scope':'existing originalTRAIN CoSQA/Context source bodies only; new conservative grouping, not human review or semantic no-clone proof'},'dedup':{'exact_pair_duplicates':1867-len(pair_labels),'exact_question_cross_role_violations':sum(len(v)!=1 for v in query_roles.values()),'exact_function_cross_role_violations':sum(len(v)!=1 for v in function_roles.values()),'family_constraints':len(edge_rows),'unjudged_negative_pairs_added':0},'outputs':outputs,'new_acquisition':False,'native_or_model_calls':False,'scoring':False,'training':False,'promotion':False,'split_frozen_before_score':True,'pilot_scope_accepted_by_this_script':False,'remaining_implementation':['Versioned nativeF1 instruction compiler+no-crop census; originalF0 compiledexamples are fullybound now.','F1 fixedDEV scoring/selection and genuine bounded resourceprobe+controlled full-vocabLoRA trainer require later Root-reviewed execution.','Mandatory general-document replay native-positive-window admission/fraction not satisfiable from code-only1867; do not pretend missing unrelatedM1/N8labels block pointwise work.'],'unavoidable_missing_human_labels_for_original_binary_target':False,'human_label_gaps_for_broader_claims':['Unjudged same-query candidate grades and necessary/complementary/exception evidence retention roles; terminal complete-evidence judgments. Existing singleton pairs cannot supply them.'],'chronology_unknown_is_disclosed_not_manufactured':True,'elapsed_seconds':time.perf_counter()-started}
outputs['summary']=put('summary.json',summary)
# Whole consumed inputs retain exact bytes; original state/FINAL closures never mutated.
for p,v in pins.items():
 deadline();assert Path(p).stat().st_size==v['bytes'];hasher=hashlib.sha256()
 with Path(p).open('rb')as f:
  while chunk:=f.read(1024*1024):hasher.update(chunk)
 assert hasher.hexdigest()==v['sha256']
put('input-custody.json',list(pins.values()));print(json.dumps({'summary':str(OUT/'summary.json'),'sha256':a.digest((OUT/'summary.json').read_bytes()),'roles':role_stats,'components':len(fit_components),'parsed':parsed,'parse_refusals':len(parse_refusals),'elapsed_seconds':time.perf_counter()-started}))
