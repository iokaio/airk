# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import collections,datetime,hashlib,importlib.util,json,math,subprocess,time
ROOT=Path(__file__).resolve().parents[5]
E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z'
S=ROOT/'artifacts/v190/final-plan/candidate-worktree-r3/artifacts/r154-fresh-current-section-r1'
OUT=Path(__file__).resolve().parent/'completed-r2'
spec=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
started=time.perf_counter();identity=h.identity();assert h.security(OUT.parent)['owner_sid']==h.HUMAN;assert not OUT.exists();OUT.mkdir();assert h.security(OUT)['owner_sid']==h.HUMAN
inputs={}
def read(path,require_human=True):
    path=Path(path); security=h.security(path);assert not require_human or security['owner_sid']==h.HUMAN
    data=path.read_bytes();inputs[str(path)]={'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'security':security};return data
def load(path):return json.loads(read(path))
def compact(value):return json.dumps(value,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode('utf-8')
def put(name,value):
    path=OUT/name;assert not path.exists();path.write_bytes(value if isinstance(value,bytes) else json.dumps(value,ensure_ascii=False,indent=2).encode('utf-8')+b'\n')
checks=[]
def check(name,condition):
    checks.append({'name':name,'passed':bool(condition)});assert condition,name
r8=load(S/'composed-route-r8/route/receipt.json');outer=load(S/'composed-route-r8/receipt.json');owned=load(S/'work-r8/r154-fresh-attribution-r8/owned-execution.json');d=load(S/'work-r8/r154-fresh-attribution-r8/worker/diagnostic.json');call=d['canonical_call'];v=call['full_canonical_value']
r7=load(S/'composed-route-r7/route/receipt.json');old=load(S/'work-r7/r154-fresh-attribution-r7/worker/diagnostic.json')['canonical_call']
closed=load(S/'work-r8/closed.json');summary=load(S/'work-r8/composed-observer.closed.json');journal=read(S/'work-r8/composed-observer.jsonl');rows=[json.loads(x) for x in journal.splitlines()];oldrows=[json.loads(x) for x in read(S/'work-r7/composed-observer.jsonl').splitlines()]
arguments=load(S/'work-r8/r154-fresh-attribution-r8/worker/canonical-call/call-0001/arguments.json')
check('R8 original route and observer naturally settled',outer['complete'] and r8['complete'] and owned['complete'] and outer['owned_outcome']=='naturally_settled_success' and r8['first_failure'] is None and owned['failure'] is None)
check('one fresh canonical call successful delivery only',d['canonical_call_attempts']==1 and call['validated'] and call['returncode']==0 and not call['timed_out'])
check('no phase exceeded original deadline',not outer['original_phase_expired'] and not r8['original_phase_expired'] and not owned['original_phase_expired'])
check('original native HANDLE exit0 with closed custody',r8['service_process_wait_exit0'] and all(r8['original_service_after'][k] for k in ['retained_original_handle','exit_observed','handle_closed','observed_before_original_deadline']) and r8['original_service_after']['exit_code']==0)
check('native canonical shutdown and lease reacquired',closed['shutdown_completed'] and closed['service_lease_reacquired'])
check('full753 secure readiness freshly observed',r8['full753_semantic_ready_and_no_active_publication_work'] and r8['native_index_setup_complete'])
check('no unsupported factual Answer promoted',v['paragraphs']==[] and v['gaps']==['unsupported'] and v['assessment']['coverage']=={'reason':'basis_omitted','status':'unassessed'} and v['stop']=='budget')
check('all three context decisions actually abstained',len(v['assessment_history'])==3 and all(x['result']['status']=='abstained' and x['result']['advice']['outcome']['status']=='abstained' for x in v['assessment_history']))
check('native source verification remains unverified',v['source_verification']['status']=='unverified' and not v['source_verification']['requirements_satisfied'] and v['source_verification']['counts']['checked_sources']==0)
check('native journal complete no dropped records',summary['complete'] and summary['records']==len(rows)==35 and summary['dropped']==summary['writer_failed']==0 and summary['bytes']==len(journal) and summary['error'] is None)
counts=collections.Counter(x['kind'] for x in rows);check('seven full score and ownership lifecycles',dict(counts)=={k:7 for k in ['dispatch_admitted','score_returned','job_settled','profile_released','charge_released']})
scores=[x for x in rows if x['kind']=='score_returned'];oldscores=[x for x in oldrows if x['kind']=='score_returned'];check('29 finite current raw scores',sum(len(x['raw_scores']) for x in scores)==29 and all(math.isfinite(y) for x in scores for y in x['raw_scores']))
check('all seven dispatches accounted and settled',all(len([x for x in rows if x['dispatch_identity']==score['dispatch_identity'] and x['kind']==kind])==1 for score in scores for kind in counts))
check('scores and settled operations report no errors',all(x['error'] is None for x in rows))
check('R7 original failure preserved distinct',not r7['complete'] and r7['first_failure'] is not None and not old['validated'])
check('R7 and R8 raw score bit equality',len(scores)==len(oldscores)==7 and all(x['raw_score_bits']==y['raw_score_bits'] for x,y in zip(scores,oldscores)))
comparisons=[{'dispatch':x['dispatch_identity'],'task':x['task'],'raw_bits_equal':x['raw_score_bits']==y['raw_score_bits'],'input_digest_equal':x['input_digest']==y['input_digest'],'R8_input_digest':x['input_digest'],'R7_input_digest':y['input_digest']} for x,y in zip(scores,oldscores)]
check('same Context3 input digests; Focus4 differ',sum(x['input_digest_equal'] for x in comparisons)==3 and all(x['input_digest_equal']==('context_action' in x['task']) for x in comparisons))
put('full-canonical-result.json',call['envelope']['content'][0]['text'].encode('utf-8'))
check('text and structured envelope preserve same full native value',json.loads(call['envelope']['content'][0]['text'])==v==call['envelope']['structuredContent'])
put('full-native-journal.jsonl',journal);put('all-model-scores.json',scores);put('all-resource-and-settlement-records.json',[x for x in rows if x['kind'] in ['job_settled','profile_released','charge_released']]);put('original-service-settlement.json',{'outer':outer,'route_service_before':r8['original_service_before'],'route_service_after':r8['original_service_after'],'owned_service_after':owned['original_service_after'],'native_closed':closed,'recorder_closed':summary})
# Exact native body serialization is checked before reproducing the existing Core arithmetic.
native_text=call['envelope']['content'][0]['text'];check('retained native compact serialization equals audit serializer',compact(v).decode()==native_text)
without=dict(v);receipt=without.pop('source_verification');encoded=compact(without);quoted=compact(encoded.decode());allowance=(65536-len(quoted)-2048)//2
check('actual 3283 verification receipt allocation reproduced',allowance==receipt['limits']['receipt_bytes_budget']==3283)
check('actual request limits remain 8192 tokens120s262144bytes',arguments['budget']['input_tokens']==8192 and arguments['budget']['deadline_ms']==120000 and arguments['budget']['max_bytes']==262144)
check('no receipt metadata admitted before any source IO',receipt['sources']==receipt['origins']==receipt['references']==[] and receipt['source_rows_omitted']==18 and receipt['limits']['hit']==['receipt_bytes'] and receipt['counts']['bytes_read']==0)
byte_accounting={'request_budget':arguments['budget'],'native_MAX_RESPONSE_BYTES':65536,'Core_MAX_RECEIPT_BYTES':32768,'final_body_compact_bytes':len(compact(v)),'before_verification_compact_bytes':len(encoded),'before_verification_quoted_JSON_bytes':len(quoted),'native_frame_headroom_bytes':2048,'formula':'min(32768, floor((65536 - quoted_native_text_bytes - 2048) / 2))','actual_result':allowance,'receipt_compact_bytes':len(compact(receipt)),'native_work_bytes':v['work']['bytes'],'stdout_bytes':call['stdout_bytes'],'interpretation':'Native 64KiB text-frame headroom, distinct from the262144B request work/packet ceiling. Quoting and /2 reserve are the existing framing policy, not repeated native work. The final delivered body cannot establish the discarded pre-pack size. No duplicate-work accounting or byte-budget regression proven. Existing packing may omit uncharged records to preserve mandatory NN custody and marks basis_omitted; the result exposes omitted_count2. Metadata admission removed all18 source rows before source IO, as explicitly disclosed. No source-verification pass is claimed.'}
put('byte-budget-accounting.json',byte_accounting)
# This audit covers citation objects in the delivered body only. It does not substitute for native verification or account for all native reference kinds.
source_root=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2/r154-fresh-attribution-preparation-r3/source-fixture/sources/hotpot'
citations=[]
def visit(x,pointer=''):
    if isinstance(x,dict):
        if isinstance(x.get('citation'),dict):citations.append({'pointer':pointer,'citation':x['citation'],'text':x.get('text')})
        for k,y in x.items():visit(y,pointer+'/'+k.replace('~','~0').replace('/','~1'))
    elif isinstance(x,list):
        for i,y in enumerate(x):visit(y,pointer+'/'+str(i))
visit(v);unique={}
for row in citations:
    c=row['citation'];path=source_root/c['path'];assert path.resolve().parent==source_root.resolve();key=(c['path'],c['content_hash'],c['start_line'],c['end_line'])
    if key not in unique:
        data=read(path);lines=data.decode('utf-8').splitlines();unique[key]={'citation':c,'actual_sha256':hashlib.sha256(data).hexdigest(),'sha256_matches':hashlib.sha256(data).hexdigest()==c['content_hash'],'bytes':len(data),'line_count':len(lines),'range_within_file':1<=c['start_line']<=c['end_line']<=len(lines)}
    row['text_contained_in_cited_lines']=None if not isinstance(row['text'],str) else row['text'] in '\n'.join(path.read_text(encoding='utf-8').splitlines()[c['start_line']-1:c['end_line']])
check('all18 unique delivered citation file hashes and ranges match',len(citations)==25 and len(unique)==18 and all(x['sha256_matches'] and x['range_within_file'] for x in unique.values()))
put('separate-read-only-citation-audit.json',{'native_verification_status_unchanged':receipt,'scope':'25 citation-bearing objects18 unique files; distinct from native26 total references. Saved-byte/range check only; does not establish extraction correctness, factual support, full reference inventory or native source verification. No native operation invoked.','citations':citations,'unique_saved_source_checks':list(unique.values())})
source_paths=['crates/airk-core/src/verification.rs','crates/airk-core/src/lib.rs','crates/airk-kernel/src/verification.rs','crates/airk-kernel/src/arbiter/joint.rs','crates/airk-kernel/src/arbiter/CONTRACT.md','crates/airk-kernel/src/arbiter/joint/focus_pack_tests.rs','crates/airk-mcp/src/lib.rs','docs/source-verification.md']
for path in source_paths:read(ROOT/path,require_human=False)
changed=subprocess.run(['git','diff','e306258a2b6913e6ba91a20fe0b50dbe6ae8361f','--',*source_paths],cwd=ROOT,check=True,capture_output=True).stdout
check('audited live budget-owner files unchanged from R8 e306 identity',not changed)
put('source-audit.json',{'source_identity':'e306258a2b6913e6ba91a20fe0b50dbe6ae8361f; current relevant owner files unchanged by later Root Mac-only patch','paths':source_paths,'anchors':{'formula':'crates/airk-core/src/verification.rs:29','native_frame':'crates/airk-core/src/lib.rs:90','metadata_admission':'crates/airk-kernel/src/verification.rs:159','admit_before_IO':'crates/airk-kernel/src/verification.rs:761','final_projection_budget':'crates/airk-mcp/src/lib.rs:1576','reserve':'crates/airk-mcp/src/lib.rs:760','packing':'crates/airk-kernel/src/arbiter/joint.rs:766'},'finding':'Consistent existing closed-frame and metadata-admission behavior. No demonstrated native byte-accounting defect; no limit widening, replay or code change proposed. Existing independently fitted Context gate abstained3/3; Step8/CAL/quality acceptance remain unsatisfied.'})
timings={'outer_route_seconds':outer['elapsed_seconds'],'route_seconds':r8['elapsed_seconds'],'observer_seconds':owned['elapsed_seconds'],'worker_seconds':d['elapsed_seconds'],'canonical_adapter_seconds':call['elapsed_seconds'],'canonical_read_process_seconds':call['canonical_read_process_seconds'],'adapter_preprocessing_seconds':call['adapter_preprocessing_seconds'],'original_Answer_budget_ms':arguments['budget']['deadline_ms'],'delivery_budget_seconds':call['deadline_seconds'],'native_work':v['work'],'per_task_inference_us':dict((task,sum(x['work']['inference_us'] for x in scores if x['task']==task)) for task in sorted({x['task'] for x in scores})),'warm_stages':r8['stages'],'p95_claim':False}
put('R7-R8-comparison.json',{'R7_call':{k:old[k] for k in ['validated','elapsed_seconds','canonical_read_process_seconds','returncode','timed_out']},'R7_first_failure':r7['first_failure'],'R8_call':{k:call[k] for k in ['validated','elapsed_seconds','canonical_read_process_seconds','returncode','timed_out']},'raw_scores_comparison':comparisons,'timings':timings,'limits':'Single fresh functional delivery observation with current Service/Inference pair; no old numerical/resource evidence retag, no p95 measurement, no factual Answer support, CAL/gold, release/promotion or GPU qualification.'})
check('all retained inputs preserved byteexact',all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==row['sha256'] for path,row in inputs.items()))
put('input-custody.json',inputs);put('audit-controls.json',{'selected':len(checks),'passed':sum(x['passed'] for x in checks),'checks':checks,'scope':'Read-only actual-byte evidence audit and arithmetic; no native/model/Cargo execution or product behavior test.'})
outputs={p.name:{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size,'security':h.security(p)} for p in OUT.iterdir() if p.is_file()}
put('handoff.json',{'schema':'airk.private.r154-r8-postmortem/1','complete':True,'qualification_granted':False,'identity':identity,'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'elapsed_seconds':time.perf_counter()-started,'audit_selected':len(checks),'audit_passed':sum(x['passed'] for x in checks),'outputs':outputs,'outcome':'Canonical evidence delivery succeeded. No supported factual Answer: paragraphs0, unsupported, stopbudget, Context3abstained, coveragebasis_omitted. Native source verification remains unverified receipt_bytes3283/check0of18; separate source citation audit preserves18hashmatches. OriginalHANDLEexit0+lease. Native model7jobs/29scores allsettled; no pass promotion.'})
print(json.dumps({'handoff':str(OUT/'handoff.json'),'sha256':hashlib.sha256((OUT/'handoff.json').read_bytes()).hexdigest(),'audit_selected':len(checks),'elapsed_seconds':time.perf_counter()-started}))
