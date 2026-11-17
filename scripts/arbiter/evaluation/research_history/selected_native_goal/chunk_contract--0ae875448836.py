# SPDX-License-Identifier: Apache-2.0
"""Private proof validation only; native request/response values remain opaque."""
import hashlib,json,importlib.util
from pathlib import Path

PROTOCOL='airk.private-cal-chunked-census/1'
CAPTURE='airk.private-selected-Context-FocusON-CAL-native-capture/1'
CHUNK='airk.private-selected-Context-FocusON-CAL-native-chunk/1'
OWNED='airk.private-selected-cal-chunk-owned-execution/1'
AGGREGATE='airk.private-selected-cal-chunk-aggregate-owned-execution/1'
CAL_OWNER_SHA='a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044'
HEADER=('task_policy','Focus_configuration','Focus_profile','MiniLM_pipeline','source_manifest_sha256','corpus_sha256')

def need(value,reason):
    if not value:raise ValueError(reason)

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def add_pin(pins,path,digest):
    key=str(Path(path).resolve())
    need(key not in pins or pins[key]==digest,'conflicting proof identity')
    pins[key]=digest

def read_pin(row,pins):
    path=Path(row['path']);need(path.is_file()and not path.is_symlink()and sha(path)==row['sha256'],'proof file changed: '+str(path));add_pin(pins,path,row['sha256'])
    return json.loads(path.read_bytes())

def natural(value):
    return bool(isinstance(value,dict)and value.get('exit_code')==0 and value.get('timed_out')is False
        and value.get('log_oversized')is False and value.get('owned_descendants_settled')is True
        and value.get('owned_job_forced_termination')is False and value.get('owned_job_final_active_processes')==0
        and value.get('owned_job_final_member_pids')==[]and value.get('parent_assigned_to_job')is False
        and value.get('child_assigned_before_resume')is True and 'cleanup_refusal'not in value)

def ranges():return [(i*8,min(100,(i+1)*8))for i in range(13)]

def protocol_rows(protocol,scopes):
    need(protocol['schema']==PROTOCOL and protocol['limits']=={'questions':100,'maximum_probes_per_question':5,'maximum_total_probes':500,'per_native_call_seconds':30,'per_owned_phase_seconds':1800,'fixture_absolute_seconds':10800,'Context_task_NN_invocations':0},'exact Root chunk contract required')
    expected=[{'chunk_index':i,'question_start':start,'question_end':end,'questions':end-start}for i,(start,end)in enumerate(ranges())]
    need(protocol['partitions']==expected,'fixed13 partitions changed')
    rows=scopes['questions'];need(len(rows)==100 and len({r['id']for r in rows})==100,'original ordered100 scope census required')
    return [row['id']for row in rows]

def valid_call(call,release,pins):
    if call.get('returncode')==0:
        return bool(call.get('validated')is True and not call.get('envelope',{}).get('isError'))
    helper=Path(__file__).resolve().parents[2]/'source'/'selected_cal_refusals.py'
    expected=release['input_pins'].get(str(helper.resolve()))
    need(expected and sha(helper)==expected,'exact narrow refusal helper missing from source closure')
    add_pin(pins,helper,expected);spec=importlib.util.spec_from_file_location('frozen_chunk_budget_refusal',helper)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module.continuable_budget_refusal(call)

def phase_census(capture,index,ids,questions=None):
    start,end=ranges()[index]
    need(capture.get('schema')==CHUNK and capture.get('complete')is True and capture.get('stopped')is False
         and capture.get('chunk_index')==index and capture.get('question_start')==start and capture.get('question_end')==end
         and capture.get('total_census_questions')==100 and capture.get('Context_task_NN_invocations')==0
         and capture.get('human_semantic_gold_count')==0,'complete exact fixed chunk required')
    rows=capture['rows'];need([row['id']for row in rows]==ids[start:end]and all(row.get('split')=='calibration'for row in rows),'chunk missing/foreign/duplicate/reordered IDs')
    for ordinal,row in enumerate(rows):
        status=row.get('native_capture_status')
        need(status in ('native_request_captured','no_native_assessment_request','verification_budget_refusal_no_delivered_assessment'),'partial/foreign question outcome cannot qualify')
        if questions is not None:need(row['question']==questions[start+ordinal]['question'],'original question identity changed')
        if status!='native_request_captured':need(row.get('request')is None and row.get('admission')is None and not row.get('counterfactual_choices')and not row.get('counterfactual_refusals'),'undelivered observation associated or probed')
        probes=len(row.get('counterfactual_choices',[]))+len(row.get('counterfactual_refusals',[]))
        need(probes<=5,'original total probe cap exceeded')
    return rows

def bound_check(bound,start,finish,elapsed):
    origin=bound['original_start_unix_ms'];expiry=bound['absolute_deadline_unix_ms']
    need(type(origin)is int and type(expiry)is int and 0<expiry-origin<=10800000,'original immutable fixture bound required')
    need(isinstance(start,(int,float))and isinstance(finish,(int,float))and origin<=start<=finish<=expiry
         and 0<=elapsed<1800 and finish-start<=1800000,'phase exceeded original owned/global bound')

def closure_check(value):
    need(value.get('kind')=='closed'and value.get('helper_exit_observed')is True and value.get('helper_exit_code')==0
         and value.get('child_exit_observed')is True and value.get('child_exit_code')==0
         and value.get('child_signal_code')is None and value.get('poisoned')is None and value.get('launch_count')==1,'normal genuine SDK/session closure required')

def validate_members(protocol_pin,controller_pin,decision_pin,member_pins):
    pins={};protocol=read_pin(protocol_pin,pins);controller=read_pin(controller_pin,pins);decision=read_pin(decision_pin,pins)
    contract=read_pin(protocol['root_contract_decision'],pins)
    need(contract['status']=='APPROVED_PRIVATE_CONTRACT_ONLY_NOT_AN_EXECUTION_RELEASE'and contract['execution_authorized']is False,'recorded Root contract required')
    need(controller['schema']=='airk.private-selected-cal-chunk-controller-packet/1'and controller['protocol']['sha256']==protocol_pin['sha256'],'exact controller/protocol relation required')
    need(decision.get('schema')=='airk.private-selected-cal-chunk-controller-decision/1'and decision.get('approved')is True and decision.get('controller_packet_sha256')==controller_pin['sha256'],'actual Root exact controller release required')
    release=read_pin(controller['release'],pins);bound=read_pin(controller['fixture_bound'],pins)
    need(release.get('approved')is True and release.get('chunk_protocol_sha256')==protocol_pin['sha256']and release.get('Context_task_forward')is False and release['maximum_questions']==100 and release['maximum_probes']==500,'actual common Root release required')
    need(release.get('original_fixture_bound',{}).get('sha256')==controller['fixture_bound']['sha256']and Path(release['original_fixture_bound']['path']).resolve()==Path(controller['fixture_bound']['path']).resolve(),'common release global bound differs')
    scopes=read_pin(protocol['frozen_question_scopes'],pins);ids=protocol_rows(protocol,scopes)
    source=read_pin(protocol['frozen_source_manifest'],pins)
    source_audit=[{key:row[key]for key in ('file','bytes','sha256')}for row in sorted(source['files'],key=lambda r:r['file'])]
    binding_path=release['paths']['index_bindings'];binding_sha=release['input_pins'][str(Path(binding_path).resolve())]
    bindings=read_pin({'path':binding_path,'sha256':binding_sha},pins)
    ready_path=release['paths']['fixture_ready'];ready_sha=release['input_pins'][str(Path(ready_path).resolve())]
    ready=read_pin({'path':ready_path,'sha256':ready_sha},pins)
    need(ready['health']['instance']==bindings['instance']and ready['health']['catalog_clock']['namespace']==bindings['namespace'],'frozen actual Ready/binding instance differs')
    need(len(bindings['bindings'])==1,'one original global source index required')
    selection=bindings['bindings'][0]['window_scope_selection']
    evidence=read_pin(bound['original_start_evidence'],pins)
    need(int(evidence[bound['original_start_field']]*1000)==bound['original_start_unix_ms'],'original global start evidence differs')
    need(len(member_pins)==13 and len(controller['phase_packets'])==13,'all13 actual phase proofs required')
    phase_packets=controller['phase_packets'];need([r['chunk_index']for r in phase_packets]==list(range(13)),'controller phase order differs')
    reports=[];previous_finish=None;total_probes=0;original_instance=None;original_binding=None;scope_identity=None
    for index,row in enumerate(member_pins):
        receipt=read_pin(row,pins)
        need(receipt.get('schema')==OWNED and receipt.get('complete')is True and receipt.get('chunk_index')==index
             and receipt.get('protocol_sha256')==protocol_pin['sha256']and receipt.get('post_input_drift')==[]
             and receipt.get('controller_packet',{}).get('sha256')==controller_pin['sha256']
             and receipt.get('decision',{}).get('sha256')==decision_pin['sha256']and natural(receipt.get('owned')),'actual natural same-controller chunk proof required')
        expected_packet=phase_packets[index]
        need(receipt['packet']['sha256']==expected_packet['sha256']and Path(receipt['packet']['path']).resolve()==Path(expected_packet['path']).resolve(),'phase packet relation differs')
        packet=read_pin(expected_packet,pins)
        need(packet['chunk_index']==index and packet['release']['sha256']==controller['release']['sha256']and packet['protocol']['sha256']==protocol_pin['sha256']and packet['fixture_bound']['sha256']==controller['fixture_bound']['sha256'],'actual fixed phase packet relation differs')
        need(packet['owner']['sha256']==CAL_OWNER_SHA,'original frozen CAL process owner required')
        start,end=ranges()[index]
        need(packet.get('owner_seconds')==1800 and packet.get('question_start')==start and packet.get('question_end')==end and packet.get('questions')==ids[start:end],'phase original bounds/ordered subset differs')
        for role in ('runtime_python','capture_driver','owner'):
            read=packet[role];need(sha(read['path'])==read['sha256']and receipt.get('input_seals',{}).get(str(Path(read['path']).resolve()))==read['sha256'],'owned phase runtime/source changed or unsealed');add_pin(pins,read['path'],read['sha256'])
        capture=read_pin(receipt['capture'],pins);rows=phase_census(capture,index,ids,scopes['questions'])
        need(capture['chunk_protocol_sha256']==protocol_pin['sha256']and capture['release_sha256']==controller['release']['sha256'],'chunk capture release differs')
        need(all(capture.get(key)==release.get(key)for key in HEADER),'source/policy/profile/pipeline changed across chunks')
        bound_check(bound,receipt['started_unix_ms'],receipt['finished_unix_ms'],receipt['elapsed_seconds'])
        need(receipt['started_unix_ms']<=capture['phase_started_unix_ms']<=capture['phase_finished_unix_ms']<=receipt['finished_unix_ms'],'capture is outside owned phase interval')
        need(capture.get('source_audit_before')==capture.get('source_audit_after')==source_audit and source_audit,'source audit changed or omitted')
        need(all(receipt.get('input_seals',{}).get(path)==digest for path,digest in release['input_pins'].items()),'phase silently dropped original input seals')
        need(all(receipt.get('absent_build_inputs',{}).get(path)==sentinel for path,sentinel in release.get('absent_build_inputs',{}).items()),'phase silently dropped recorded absences')
        need(capture['fixture_absolute_deadline_unix_ms']==bound['absolute_deadline_unix_ms'],'fixture clock was refreshed')
        if previous_finish is not None:need(capture['phase_started_unix_ms']>=previous_finish,'phases overlap/reordered')
        previous_finish=capture['phase_finished_unix_ms'];closure_check(capture['native_session_close'])
        identity=capture['native_session_close']['identity']
        if original_instance is None:original_instance=identity['service_instance'];original_binding=identity['bindings_sha256']
        need(identity['ready_sha256']==ready_sha and identity['service_instance']==original_instance==bindings['instance']and identity['bindings_sha256']==original_binding==binding_sha and identity['catalog_namespace']==bindings['namespace']and identity['index_id']==bindings['bindings'][0]['index_id'],'actual instance/source binding changed')
        for phase in ('stage_before','stage_after'):need(capture[phase]['full_native_response']['configuration']==release['frozen_configuration'],'actual stage configuration changed')
        # Fresh native nonces/fingerprints stay inside each ORIGINAL opaque row.
        # Never compare or normalize them across questions.
        need(capture.get('scope_management')==[row['scope_management']for row in rows],'missing/extra management refresh outside question boundaries')
        for offset,item in enumerate(rows):
            need(item.get('scope_management'),'actual per-question SDK grant proof missing')
            management=item['scope_management'];grant=management['original_native_scope_result'];actual=grant['scope_result']
            if scope_identity is None:scope_identity=actual['scope_identity']
            need(actual['status']=='scope'and actual['scope_identity']==scope_identity and actual['catalog_revision']==selection['expected_catalog_revision']and grant['identity']==identity and grant.get('launch_count')==1,'same frozen native selection regrant differs')
            need(management['after_question_id']==(rows[offset-1]['id']if offset else None)and management.get('canonical_read')is False and 0<management['management_deadline_seconds']<=20 and capture['phase_started_unix_ms']<=management['started_unix_ms']<=management['completed_unix_ms']<=capture['phase_finished_unix_ms']and management['completed_unix_ms']-management['started_unix_ms']<=20000,'scope regrant outside completed-question boundary/budget')
            calls=[item['answer_call']]+[r['actual_call']for r in item.get('counterfactual_choices',[])]+[r['actual_call']for r in item.get('counterfactual_refusals',[])]
            for call in calls:
                need(valid_call(call,release,pins),'unknown/fatal canonical error cannot qualify a chunk')
                need(not call.get('timed_out')and call.get('identity_error')is None and call.get('parse_error')is None and 0<=call['elapsed_seconds']<=call['deadline_seconds'] and 0<call['deadline_seconds']<=30,'actual original call budget/identity failed')
                need(call['actual_service_identity']['actual_native_health']['instance']==original_instance and call['native_binding_sha256']==original_binding,'call instance/source identity differs')
            total_probes+=len(calls)-1
        reports.append(capture)
    need(total_probes<=500,'whole-census probe cap exceeded')
    return {'pins':pins,'protocol':protocol,'controller':controller,'release':release,'bound':bound,'ids':ids,'reports':reports,'instance':original_instance,'total_probes':total_probes}

def validate_aggregate(execution_pin,capture_pin,release_pin):
    pins={};execution=read_pin(execution_pin,pins)
    need(execution.get('schema')==AGGREGATE and execution.get('complete')is True and execution.get('qualification_granted')is False and execution.get('post_input_drift')==[],'genuine complete aggregate proof required')
    need(execution['capture']['sha256']==capture_pin['sha256']and Path(execution['capture']['path']).resolve()==Path(capture_pin['path']).resolve(),'aggregate capture relation differs')
    checked=validate_members(execution['protocol'],execution['controller_packet'],execution['decision'],execution['phases'])
    need(checked['controller']['release']['sha256']==release_pin['sha256'],'foreign aggregate Root release')
    capture=read_pin(capture_pin,pins)
    rows=[row for report in checked['reports']for row in report['rows']]
    need(capture['schema']==CAPTURE and capture.get('complete')is True and capture['rows']==rows
         and capture['phase_count']==13 and capture['chunk_protocol_sha256']==execution['protocol']['sha256']
         and capture['release_sha256']==release_pin['sha256']and [r['id']for r in rows]==checked['ids'],'aggregate omitted/transformed native rows')
    need(all(capture.get(key)==checked['release'].get(key)for key in HEADER)and capture.get('Context_task_NN_invocations')==0 and capture.get('human_semantic_gold_count')==0,'aggregate semantic identity changed')
    need(execution.get('instance')==checked['instance']and execution.get('native_execution_phases')==13 and 'owned'not in execution,'aggregate fabricated or omitted separate owner proofs')
    pins.update(checked['pins']);return pins
