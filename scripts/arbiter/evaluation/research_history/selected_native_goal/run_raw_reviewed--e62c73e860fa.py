# SPDX-License-Identifier: Apache-2.0
"""One Root-released native Context CAL raw process; unchanged Windows job owner."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
ROOT=Path(__file__).resolve().parents[5]
OWNER=ROOT/'tools/retrieval-train/training_execution.py'
OWNER_SHA='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
WHOLE_CENSUS_DECISION=Path(__file__).resolve().with_name('whole-census-phase-bound-r1.json')
WHOLE_CENSUS_SHA='38e1f76f3eea02fa1be7d73a7b06884deb195673bfb4ff77d319ea2f5467dff4'
SELECTOR='inference_owner::context_action_tests::selected::selected_context_cal_complete_readout_raw_scores'
SCOPES={'arbiter-context-qwen3-17b':{'cpu':4,'reservation':16*1024**3,'host':32*1024**3,'seconds':3360},'arbiter-context-qwen3-4b':{'cpu':16,'reservation':40*1024**3,'host':48*1024**3,'seconds':3360}}

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def absolute(path):
    # Do not resolve the final entry: a dangling symlink is PRESENT, not absent.
    return str(Path(os.path.abspath(path)))

def verify_absences(absences):
    for path,sentinel in absences.items():
        if sentinel!='missing' or os.path.lexists(path):
            raise ValueError('Consumed source deletion must remain genuinely absent: '+str(path))

def source_pin(pins,absences,path,digest):
    key=absolute(path)
    if digest=='missing':
        if key in pins:raise ValueError('Conflicting present/absent consumed source')
        verify_absences({key:digest});absences[key]=digest
    else:
        if key in absences or len(digest)!=64 or any(c not in '0123456789abcdef'for c in digest) or sha(path)!=digest:
            raise ValueError('Actual source file/hash differs')
        if key in pins and pins[key]!=digest:raise ValueError('Conflicting actual source hash')
        pins[key]=digest

def capture_execution(path,digest,capture_pin,release_pin):
    if sha(path)!=digest:raise ValueError('Actual owned CAL capture proof changed')
    value=json.loads(Path(path).read_bytes());owned=value.get('owned')or {}
    if not(value.get('schema')=='airk.private-selected-cal-capture-owned-execution/1'and value.get('complete')is True
           and value.get('qualification_granted')is False and value.get('post_input_drift')==[]
           and value.get('capture',{}).get('sha256')==capture_pin['sha256']
           and Path(value['capture']['path']).resolve()==Path(capture_pin['path']).resolve()
           and owned.get('exit_code')==0 and owned.get('timed_out')is False and owned.get('log_oversized')is False
           and owned.get('owned_descendants_settled')is True and owned.get('owned_job_forced_termination')is False):
        raise ValueError('Actual natural complete owned CAL capture required')
    packet_pin=value['packet']
    if sha(packet_pin['path'])!=packet_pin['sha256']:raise ValueError('Actual owned CAL packet changed')
    packet=json.loads(Path(packet_pin['path']).read_bytes())
    if packet['release']['sha256']!=release_pin['sha256']or Path(packet['release']['path']).resolve()!=Path(release_pin['path']).resolve():
        raise ValueError('Owned CAL capture release chain differs')
    pins={str(Path(path).resolve()):digest,str(Path(packet_pin['path']).resolve()):packet_pin['sha256']}
    for name in ('release','capture_driver','runtime_python','owner'):
        row=packet[name]
        if sha(row['path'])!=row['sha256']:raise ValueError('Actual capture owner/source changed')
        pins[str(Path(row['path']).resolve())]=row['sha256']
    return pins

def natural(value,expected_image):
    if not(value is not None and value.get('exit_code')==0 and value.get('timed_out')is False and value.get('log_oversized')is False
           and value.get('owned_descendants_settled')is True and value.get('owned_job_forced_termination')is False
           and value.get('unexpected_third_process')is False and value.get('owned_total_processes')==1
           and value.get('owned_job_final_member_pids')==[] and value.get('owned_job_final_active_processes')==0
           and value.get('child_assigned_before_resume')is True and value.get('parent_assigned_to_job')is False
           and value.get('whole_process_deadline_not_refreshed')is True):
        return False
    exits=value.get('retained_member_exits',[])
    assignment=value.get('suspended_assignment')or {}
    image=assignment.get('image')or {}
    cleanup=value.get('handle_close_proof')or {}
    return bool(len(exits)==1 and all(x.get('exit_observed')is True and x.get('exit_code')==0 for x in exits)
        and assignment.get('members')==[value.get('owned_leader_pid')]and assignment.get('active')==1 and assignment.get('total')==1
        and image.get('sha256')==expected_image['sha256'] and image.get('path')
        and Path(image['path']).resolve()==Path(expected_image['path']).resolve()
        and cleanup.get('forced')is False and cleanup.get('settled')is True
        and cleanup.get('final_member_pids')==[]and cleanup.get('final_active')==0
        and len(cleanup.get('retained_exits',[]))==1
        and all(x.get('exit_observed')is True and x.get('exit_code')==0 for x in cleanup['retained_exits']))

def finalize_artifact(path,deadline,clock=None,hasher=None):
    clock=clock or time.monotonic;hasher=hasher or sha
    digest=hasher(path)if Path(path).is_file()else None
    finished=clock()
    return digest,finished,finished>=deadline

def remaining(deadline):
    value=deadline-time.monotonic()
    if value<=0:raise TimeoutError('Original whole-phase deadline expired')
    return value

def validate_raw_header(value,packet,preflight):
    expected=packet['raw_header']
    if not (value.get('schema')=='airk.arbiter.context-action.selected-cal-raw/1'and value.get('complete')is True
            and value.get('task_qualified')is False and value.get('release_qualified')is False
            and value.get('human_semantic_gold_count')==0 and len(value.get('rows',[]))==100
            and value.get('test_binary_sha256')==packet['binary']['sha256']
            and all(value.get(key)==pin for key,pin in expected.items())):
        raise ValueError('Exact complete raw100/source/profile/policy/image header differs')
    expected_ids={row['id']for row in preflight['rows']}
    actual_ids=[row['id']for row in value['rows']]
    if len(expected_ids)!=100 or len(set(actual_ids))!=100 or set(actual_ids)!=expected_ids or any(row.get('split')!='calibration'for row in value['rows']):
        raise ValueError('Exact independent CAL100 row identity/split census differs')

def main():
    started=time.monotonic()
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet',type=Path,required=True);parser.add_argument('--packet-sha256',required=True)
    parser.add_argument('--decision',type=Path);parser.add_argument('--decision-sha256')
    parser.add_argument('--execute-reviewed',action='store_true')
    args=parser.parse_args()
    if os.name!='nt'or sha(args.packet)!=args.packet_sha256:raise ValueError('Exact reviewed Windows packet required')
    packet=json.loads(args.packet.read_bytes());scope=SCOPES[packet['model']];deadline=started+scope['seconds']
    if packet.get('schema')!='airk.private-selected-context-cal-raw-packet/1'or packet.get('limits')!={'wall_seconds':scope['seconds'],'host_committed_bytes':scope['host']}or packet.get('candidate_memory_reservation_bytes')!=scope['reservation']or packet.get('cpu_fit')!={'minimum_physical_cores':scope['cpu'],'intra_threads':scope['cpu'],'inter_threads':1,'sequential':True,'spinning':False}:
        raise ValueError('Exact proposed Context CAL execution/resource scope required')
    if sha(WHOLE_CENSUS_DECISION)!=WHOLE_CENSUS_SHA or packet.get('whole_census_decision')!={'path':str(WHOLE_CENSUS_DECISION),'sha256':WHOLE_CENSUS_SHA}:raise ValueError('Exact pre-task whole-census resource decision required')
    if sha(OWNER)!=OWNER_SHA or packet['runner_sha256']!=sha(__file__):raise ValueError('Frozen original job owner/runner differs')
    pins=dict(packet['pins']);absences=dict(packet.get('absent_build_inputs',{}));verify_absences(absences);binary=Path(packet['binary']['path']).resolve()
    for row in packet['actual_build_provenance']:
        if pins.get(str(Path(row['path']).resolve()))!=row['sha256']:raise ValueError('Build provenance is not in exact input closure')
    if not packet['actual_build_provenance']:raise ValueError('Actual retained image/build provenance required')
    pins[str(WHOLE_CENSUS_DECISION)]=WHOLE_CENSUS_SHA;pins[str(OWNER)]=OWNER_SHA;pins[str(binary)]=packet['binary']['sha256'];pins[str(Path(__file__).resolve())]=packet['runner_sha256'];pins[str(args.packet.resolve())]=args.packet_sha256
    def verify():
        verify_absences(absences)
        if any(sha(path)!=digest for path,digest in pins.items()):raise ValueError('CAL source/runtime/image/profile/asset/preflight closure drift')
    verify();remaining(deadline)
    env=dict(os.environ);env.update(packet['environment'])
    if any(env.get(key)!=value for key,value in {'CUDA_VISIBLE_DEVICES':'','OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1','AIRK_SELECTED_CONTEXT_MODEL':packet['model']}.items()):raise ValueError('CPU-only actual selected model/threading differs')
    for field in ('AIRK_SELECTED_CONTEXT_PREFLIGHT','AIRK_SELECTED_CONTEXT_PROFILES'):
        if pins.get(str(Path(env[field]).resolve()))!=env[field+'_SHA256']:raise ValueError('Native input descriptor is not pinned')
    preflight=json.loads(Path(env['AIRK_SELECTED_CONTEXT_PREFLIGHT']).read_bytes())
    if not(preflight.get('complete')is True and preflight.get('Focus_configuration')=='ON'and preflight.get('human_semantic_gold_count')==0 and preflight.get('qualified_profiles')==[] and preflight['corpus_sha256']==env['AIRK_SELECTED_CONTEXT_CORPUS_SHA256']):raise ValueError('Actual FocusON CAL reference preflight required')
    for role in ('capture','corpus','rubric','source_manifest'):
        row=packet['reference_inputs'][role]
        if row['sha256']!=preflight[role+'_sha256']or pins.get(str(Path(row['path']).resolve()))!=row['sha256']:
            raise ValueError('Independent reference input missing from source closure: '+role)
    capture_input=json.loads(Path(packet['reference_inputs']['capture']['path']).read_bytes())
    release_pin=packet['capture_release']
    if not(capture_input.get('complete')is True and capture_input.get('Focus_configuration')=='ON'and capture_input.get('Context_task_NN_invocations')==0 and capture_input.get('human_semantic_gold_count')==0 and capture_input.get('release_sha256')==release_pin['sha256']and pins.get(str(Path(release_pin['path']).resolve()))==release_pin['sha256']):
        raise ValueError('Actual separate FocusON native capture release/closure differs')
    chain=packet['capture_execution'];capture_execution(chain['path'],chain['sha256'],packet['reference_inputs']['capture'],release_pin)
    if pins.get(str(Path(chain['path']).resolve()))!=chain['sha256']:raise ValueError('Actual capture execution not in input closure')
    if packet['raw_header']!={'model':packet['model'],'profile':packet['profile_manifest'],'profile_source_sha256':packet['profile_source_sha256'],'profiles_sha256':env['AIRK_SELECTED_CONTEXT_PROFILES_SHA256'],'preflight_sha256':env['AIRK_SELECTED_CONTEXT_PREFLIGHT_SHA256'],'capture_sha256':preflight['capture_sha256'],'corpus_sha256':preflight['corpus_sha256'],'task_policy':preflight['task_policy']}:raise ValueError('Raw header scope differs from independent sealed inputs')
    work=Path(packet['output']).resolve()
    if work.exists()or not work.is_relative_to(ROOT/'artifacts'):raise ValueError('Fresh private output required')
    control=Path(env['AIRK_CONTEXT_ACTION_PRIVATE_CONTROL']).resolve()
    if control.exists()or not control.is_relative_to(work):raise ValueError('Fresh isolated native lease control required')
    listed=subprocess.run([str(binary),'--exact',SELECTOR,'--ignored','--list'],capture_output=True,text=True,timeout=min(10,remaining(deadline)),check=True)
    if listed.stdout.count(SELECTOR+': test')!=1:raise ValueError('Exact native selector must select one test')
    capture=work/'capture.json';env['AIRK_SELECTED_CONTEXT_RAW_OUTPUT']=str(capture)
    argv=[str(binary),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
    if not args.execute_reviewed:
        print(json.dumps({'preflight_passed':True,'selected':1,'model':packet['model'],'packet_sha256':args.packet_sha256,'task_NN_executed':False}));return
    if args.decision is None or not args.decision_sha256 or sha(args.decision)!=args.decision_sha256:raise ValueError('Actual separate Root CAL raw release required')
    decision=json.loads(args.decision.read_bytes())
    if not(decision.get('schema')=='airk.private-selected-context-cal-raw-decision/1'and decision.get('approved')is True and decision.get('packet_sha256')==args.packet_sha256 and decision.get('FocusON_CAL_raw_only')is True):raise ValueError('Foreign Root taskNN scope decision')
    pins[str(args.decision.resolve())]=args.decision_sha256
    spec=importlib.util.spec_from_file_location('unchanged_CAL_native_job_owner',OWNER);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
    work.mkdir();(work/'stdin.txt').write_bytes(b'');(work/'request.json').write_text(json.dumps({'packet_sha256':args.packet_sha256,'decision_sha256':args.decision_sha256,'command':argv,'environment':packet['environment'],'input_seals':pins,'absent_build_inputs':absences,'limits':packet['limits'],'selector_list':listed.stdout},indent=2)+'\n')
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)]or active!=1 or total!=1:raise ValueError('One actual suspended native member required')
        image=owner.image_proof(api,info.process,packet['binary'])
        if time.monotonic()>=deadline:raise ValueError('Whole-phase deadline expired before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    owned=None;failure=None;drift=[];complete=False
    try:
        with(work/'native.log').open('xb')as log,(work/'stdin.txt').open('rb')as stdin:
            owned=owner.run_windows_owned_training(argv,ROOT,log,remaining(deadline),env,stdin,scope['host'],before_resume=suspended)
        validate_raw_header(json.loads(capture.read_bytes()),packet,preflight)
        complete=natural(owned,packet['binary'])
    except BaseException as error:
        failure={'type':type(error).__name__,'message':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)}
        raise
    finally:
        try:
            drift=[path for path,digest in pins.items()if sha(path)!=digest]
            drift.extend(path for path,sentinel in absences.items()if sentinel!='missing'or os.path.lexists(path))
        except(OSError,ValueError)as error:drift=['post-pin-check-failed: '+str(error)]
        try:capture_digest,finished,expired=finalize_artifact(capture,deadline)
        except(OSError,ValueError)as error:
            capture_digest=None;finished=time.monotonic();expired=finished>=deadline;complete=False
            failure=failure or {'type':type(error).__name__,'message':'Final artifact hash failed: '+str(error)}
        complete=complete and not drift and not expired
        result={'schema':'airk.private-selected-context-cal-raw-owned-execution/1','complete':complete,'model':packet['model'],'packet_sha256':args.packet_sha256,'decision_sha256':args.decision_sha256,'owned':owned,'failure':failure,'input_seals':pins,'absent_build_inputs':absences,'post_input_drift':drift,'elapsed_seconds':finished-started,'whole_phase_expired':expired,'whole_phase_deadline_not_refreshed':True,'raw_header':packet['raw_header'],'actual_build_provenance':packet['actual_build_provenance'],'task_or_runtime_RAM_composed_A6_or_release_qualified':False,'human_semantic_gold_count':0,'no_automatic_retry':True}
        if capture_digest is not None:result['capture_sha256']=capture_digest
        (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'complete':complete,'model':packet['model'],'elapsed_seconds':result['elapsed_seconds'],'capture_sha256':result.get('capture_sha256')}),flush=True)
    if not complete:raise SystemExit(1)

if __name__=='__main__':main()
