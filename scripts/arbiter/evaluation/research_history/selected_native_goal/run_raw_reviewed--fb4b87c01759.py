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
SELECTOR='inference_owner::context_action_tests::selected::selected_context_cal_complete_readout_raw_scores'
SCOPES={'arbiter-context-qwen3-17b':{'cpu':4,'reservation':16*1024**3,'host':32*1024**3,'seconds':1200},'arbiter-context-qwen3-4b':{'cpu':8,'reservation':40*1024**3,'host':48*1024**3,'seconds':1800}}

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def natural(value):
    return bool(value is not None and value['exit_code']==0 and not value['timed_out'] and not value['log_oversized'] and value['owned_descendants_settled'] and not value['owned_job_forced_termination'] and not value['unexpected_third_process'] and value['owned_total_processes']==1 and not value['owned_job_final_member_pids'] and value['owned_job_final_active_processes']==0 and len(value['retained_member_exits'])==1 and all(x['exit_observed']and x['exit_code']==0 for x in value['retained_member_exits']))

def remaining(deadline):
    value=deadline-time.monotonic()
    if value<=0:raise TimeoutError('Original whole-phase deadline expired')
    return value

def validate_raw_header(value,packet):
    expected=packet['raw_header']
    if not (value.get('schema')=='airk.arbiter.context-action.selected-cal-raw/1'and value.get('complete')is True
            and value.get('task_qualified')is False and value.get('release_qualified')is False
            and value.get('human_semantic_gold_count')==0 and len(value.get('rows',[]))==100
            and value.get('test_binary_sha256')==packet['binary']['sha256']
            and all(value.get(key)==pin for key,pin in expected.items())):
        raise ValueError('Exact complete raw100/source/profile/policy/image header differs')

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
    if sha(OWNER)!=OWNER_SHA or packet['runner_sha256']!=sha(__file__):raise ValueError('Frozen original job owner/runner differs')
    pins=dict(packet['pins']);binary=Path(packet['binary']['path']).resolve()
    for row in packet['actual_build_provenance']:
        if pins.get(str(Path(row['path']).resolve()))!=row['sha256']:raise ValueError('Build provenance is not in exact input closure')
    if not packet['actual_build_provenance']:raise ValueError('Actual retained image/build provenance required')
    pins[str(OWNER)]=OWNER_SHA;pins[str(binary)]=packet['binary']['sha256'];pins[str(Path(__file__).resolve())]=packet['runner_sha256'];pins[str(args.packet.resolve())]=args.packet_sha256
    def verify():
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
    work.mkdir();(work/'stdin.txt').write_bytes(b'');(work/'request.json').write_text(json.dumps({'packet_sha256':args.packet_sha256,'decision_sha256':args.decision_sha256,'command':argv,'environment':packet['environment'],'input_seals':pins,'limits':packet['limits'],'selector_list':listed.stdout},indent=2)+'\n')
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
        validate_raw_header(json.loads(capture.read_bytes()),packet)
        complete=natural(owned)
    except BaseException as error:
        failure={'type':type(error).__name__,'message':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)}
        raise
    finally:
        try:drift=[path for path,digest in pins.items()if sha(path)!=digest]
        except(OSError,ValueError)as error:drift=['post-pin-check-failed: '+str(error)]
        expired=time.monotonic()>=deadline;complete=complete and not drift and not expired
        result={'schema':'airk.private-selected-context-cal-raw-owned-execution/1','complete':complete,'model':packet['model'],'packet_sha256':args.packet_sha256,'decision_sha256':args.decision_sha256,'owned':owned,'failure':failure,'input_seals':pins,'post_input_drift':drift,'elapsed_seconds':time.monotonic()-started,'whole_phase_expired':expired,'whole_phase_deadline_not_refreshed':True,'raw_header':packet['raw_header'],'actual_build_provenance':packet['actual_build_provenance'],'task_or_runtime_RAM_composed_A6_or_release_qualified':False,'human_semantic_gold_count':0,'no_automatic_retry':True}
        if capture.is_file():result['capture_sha256']=sha(capture)
        (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'complete':complete,'model':packet['model'],'elapsed_seconds':result['elapsed_seconds'],'capture_sha256':result.get('capture_sha256')}),flush=True)
    if not complete:raise SystemExit(1)

if __name__=='__main__':main()
