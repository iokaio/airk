# SPDX-License-Identifier: Apache-2.0
"""PRIVATE actual bounded owners for retained transport and separately reviewed native phases.
No model/service execution occurs on import or --help. Root review precedes real phases.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time
import custody

STATE=Path(__file__).parents[1]
ROOT=Path(r'D:\code\Github\airk')
REF=ROOT/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
OWNER_SHA='2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1'
PARENT_SHA='fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d'
WORKER_SHA='9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'

def need(value,message):
    if not value:raise ValueError(message)

def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module

def reviewed_reference():
    need(digest(REF/'reference_owned.py')==OWNER_SHA,'Reviewed native owner changed')
    need(digest(REF/'reference_parent_no_site_r2.py')==PARENT_SHA,'Reviewed suspended authentication/settlement predicate changed')
    worker=custody.load_worker(WORKER_SHA)
    need(worker.windows_sid()==SID,'Root parent is not the verified human SID')
    sys.path.insert(0,str(REF))
    return load(REF/'reference_owned.py','ann_reviewed_reference_owner'),load(REF/'reference_parent_no_site_r2.py','ann_reviewed_reference_parent'),worker

def write_new(path,value):
    with Path(path).open('x',encoding='utf-8',newline='\n') as stream:
        json.dump(value,stream,indent=2);stream.write('\n');stream.flush();os.fsync(stream.fileno())

def native_environment_paths(input_path,output_path,producer):
    """Native strict canonical spelling uses observed handles, separate from Python views."""
    need(Path(input_path).is_absolute(),'Native input must be absolute')
    native_input=producer.native_path(input_path)
    output=Path(output_path)
    need(output.is_absolute() and output.name not in ('','.','..'),'Absolute fresh native output required')
    parent=producer.unaliased(output.parent)
    need(parent==producer.unaliased(STATE/'evidence') and not output.exists(),'Fresh owned native evidence output required')
    native_output=Path(producer.native_path(parent))/output.name
    return {'AIRK_ANN_QUERY_INPUT':native_input,'AIRK_ANN_QUERY_OUTPUT':str(native_output)}

def final_phase_observation(receipt,log,started,deadline,*,clock=time.monotonic,hash_file=digest):
    """Hash every remaining file proof before ONE final monotonic cutoff sample."""
    if log.exists():
        receipt['stdout']={'path':str(log),'bytes':log.stat().st_size,'sha256':hash_file(log)}
    final_now=clock()
    receipt['elapsed_seconds']=final_now-started
    receipt['original_phase_expired']=final_now>=deadline
    late_failure=receipt['original_phase_expired'] and receipt['failure'] is None
    if receipt['original_phase_expired']:
        receipt['complete']=False
        if late_failure:
            receipt['failure']={'type':'TimeoutError','message':'Original whole phase expired during final file observation'}
    return late_failure

def natural_multi(result):
    return (result.get('exit_code')==0 and result.get('timed_out') is False and result.get('log_oversized') is False and result.get('owned_descendants_settled') is True and result.get('owned_job_forced_termination') is False and result.get('owned_job_final_active_processes')==0 and result.get('owned_job_final_member_pids')==[] and result.get('child_assigned_before_resume') is True and result.get('parent_assigned_to_job') is False)

def phase_output_paths(plan,phase):
    name=plan['name']+'-'+phase
    log=STATE/'evidence'/(name+'-owned.log')
    receipt=STATE/'evidence'/(name+'-owner.json')
    need(not log.exists() and not receipt.exists(),'Fresh phase outputs required; no unchanged replay')
    return log,receipt

def run(args):
    started=time.monotonic()
    path=Path(args.plan)
    need(digest(path)==args.plan_sha256,'Actual prepared plan SHA differs')
    plan=json.loads(path.read_bytes())
    budget=json.loads(Path(plan['pins']['read_budget_fixture']['path']).read_bytes())
    phase_seconds=plan['execution_policy']['transport_whole_seconds'] if args.phase=='transport' else budget['operation_deadline_ms']/1000
    deadline=started+phase_seconds
    need(plan['execution_policy']['host_committed_bytes']==8<<30,'Frozen original host policy differs')
    need(plan['execution_policy']['native_phase_deadline_ms']==budget['operation_deadline_ms'],'Native phase budget differs from Rust emitter')
    owner,reference,worker=reviewed_reference()
    producer=load(STATE/'producer.py','actual_ann_producer')
    actor=producer.identity()
    for value in plan['pins'].values():producer.check_pin(value)
    quality=producer.load_script(producer.python_source_workspace(plan['source'])/'scripts/check-retrieval-quality.py','ann_owned_frozen_quality')
    need(quality.source_identity()==plan['source_identity'],'Actual consumed candidate full/dirty source changed')
    need(quality.clients.runtime_builder().source_identity()==plan['candidate_consumed_c7'],'Immutable runtime C7 source changed')
    if args.phase=='transport':producer.preflight_paths(plan,quality)
    need(time.monotonic()<deadline,'Original phase expired during actual preflight')
    log,receipt_path=phase_output_paths(plan,args.phase)
    environment=dict(os.environ,PYTHONDONTWRITEBYTECODE='1')
    environment['TEMP']=str(STATE/'temp')
    environment['TMP']=str(STATE/'temp')
    need(Path(environment['TEMP']).is_dir(),'Approved same-human private child TEMP missing')
    receipt={'schema':'airk.private.fresh-ann-owned-phase/1','phase':args.phase,'actor':actor,'plan_sha256':args.plan_sha256,'started_monotonic':started,'original_deadline_monotonic':deadline,'host_committed_bytes':8<<30,'native_phase_budget_owner':plan['pins']['read_budget_fixture'],'complete':False,'failure':None,'source_identity':plan['source_identity']}
    try:
        with log.open('xb') as output:
            if args.phase=='transport':
                transport=load(STATE/'execution-owner-r1/windows_job_8g.py','actual_ann_transport_owner')
                environment['AIRK_FRESH_ANN_ABSOLUTE_DEADLINE']=repr(deadline)
                command=[sys.executable,'-B','-S',str(STATE/'producer.py'),'execute','--plan',str(path),'--plan-sha256',args.plan_sha256]
                remaining=deadline-time.monotonic();need(remaining>0,'Original transport phase expired before launch')
                actual=transport.run_windows_owned(command,STATE,output,remaining,environment,absolute_deadline=deadline)
                need(natural_multi(actual),'Actual whole-transport job did not naturally settle')
                capture=STATE/'evidence'/(plan['name']+'-transport.json')
                need(capture.is_file(),'Actual original transport report missing')
                native=json.loads(capture.read_bytes())
                need(native.get('capture_complete') is True,'Actual original mode/process-handle/source proof incomplete')
                receipt['native_result']={'path':str(capture),'sha256':digest(capture)}
            else:
                need(args.input is not None and args.input_sha256 is not None,'Actual frozen native input and SHA required')
                need(digest(args.input)==args.input_sha256,'Actual native phase input bytes differ')
                selector=('inference_owner::evaluation_tests::ann_query::emit_fresh_stopped_ann_query_typed_pins' if args.phase=='native-pins' else 'inference_owner::evaluation_tests::ann_query::fresh_queries_compare_exact_all_live_to_diskann_with_same_embedding')
                # Python image proof compares lexical resolved paths; retain native
                # serialized FilePin unchanged and normalize only this local view.
                image={**plan['pins']['diagnostic_image'],'path':str(producer.unaliased(plan['pins']['diagnostic_image']['path']))}
                command=[image['path'],'--exact',selector,'--ignored','--nocapture']
                result_path=STATE/'evidence'/(plan['name']+'-'+args.phase+'.jsonl')
                environment.update(native_environment_paths(args.input,result_path,producer),AIRK_ANN_QUERY_INPUT_SHA256=args.input_sha256)
                def authenticate(api,job,info,child_deadline):
                    return reference.suspended_authentication(api,info,deadline,child_deadline,SID,image,owner.image_proof,sid_reader=worker.windows_sid)
                remaining=deadline-time.monotonic();need(remaining>0,'Original native phase expired before launch')
                with open(os.devnull,'rb') as null_input:
                    actual=owner.run_windows_owned_training(command,STATE,output,remaining,environment,null_input,8<<30,before_resume=authenticate,absolute_deadline=deadline)
                need(reference.settled(actual),'Actual original native child/handle/job proof was not a natural single-child completion')
                events=[json.loads(line) for line in result_path.read_text(encoding='utf-8').splitlines()]
                need(events and events[-1].get('event')=='finish' and events[-1].get('complete') is True,'Actual native phase output incomplete; preserve all original slots')
                need(events[-1]['original_task_denominator']==9 and events[-1]['original_mode_denominator']==27,'Original native task/mode denominator changed')
                if args.phase=='native-compare':need(events[-1].get('compared')==9,'Original same-vector comparison denominator incomplete')
                receipt['native_result']={'path':str(result_path),'sha256':digest(result_path),'finish':events[-1]}
            receipt['actual_owner_result']=actual
        for value in plan['pins'].values():producer.check_pin(value)
        need(quality.source_identity()==plan['source_identity'],'Candidate full/dirty source changed during actual phase')
        need(quality.clients.runtime_builder().source_identity()==plan['candidate_consumed_c7'],'Runtime C7 source changed during actual phase')
        need(time.monotonic()<deadline,'Original whole phase expired during final observations')
        receipt['complete']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error)}
        if 'actual' in locals():receipt['actual_owner_result']=actual
        if hasattr(error,'owned_cleanup'):receipt['actual_cleanup_failure']=error.owned_cleanup
        raise
    finally:
        late_failure=final_phase_observation(receipt,log,started,deadline)
        write_new(receipt_path,receipt)
        print(json.dumps({'receipt':str(receipt_path),'complete':receipt['complete'],'phase':args.phase,'failure':receipt['failure']}),flush=True)
        if late_failure:
            raise TimeoutError('Original whole phase expired during final file observation')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase',choices=['transport','native-pins','native-compare'])
    parser.add_argument('--plan',type=Path,required=True)
    parser.add_argument('--plan-sha256',required=True)
    parser.add_argument('--input',type=Path)
    parser.add_argument('--input-sha256')
    run(parser.parse_args())

if __name__=='__main__':main()
