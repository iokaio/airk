# SPDX-License-Identifier: Apache-2.0
"""Root-released no-NN certificate/native-fit phase; original bounded job owner."""
import argparse,importlib.util,json,os,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'native-cal-owned-r1'))
from run_raw_reviewed import ROOT,OWNER,OWNER_SHA,sha,verify_absences,finalize_artifact

def natural(value,phase,image):
    if not isinstance(value,dict):return False
    if phase not in ('certificate','fit'):return False
    assignment=value.get('suspended_assignment')or {};actual=assignment.get('image')or {}
    if not(assignment.get('members')==[value.get('owned_leader_pid')]and assignment.get('active')==1 and assignment.get('total')==1 and actual.get('sha256')==image['sha256']and actual.get('path')and Path(actual['path']).resolve()==Path(image['path']).resolve()):return False
    exits=value.get('retained_member_exits',[]);close=value.get('handle_close_proof')or {}
    return bool(value.get('exit_code')==0 and value.get('timed_out')is False and value.get('log_oversized')is False
        and value.get('owned_descendants_settled')is True and value.get('owned_job_forced_termination')is False
        and value.get('owned_job_final_member_pids')==[]and value.get('owned_job_final_active_processes')==0
        and value.get('owned_total_processes')in ((1,)if phase=='fit'else (1,2))and len(exits)==value['owned_total_processes']
        and all(row.get('exit_observed')is True and row.get('exit_code')==0 for row in exits)
        and value.get('child_assigned_before_resume')is True and value.get('parent_assigned_to_job')is False
        and value.get('whole_process_deadline_not_refreshed')is True and close.get('forced')is False
        and close.get('settled')is True and close.get('final_member_pids')==[]and close.get('final_active')==0
        and len(close.get('retained_exits',[]))==len(exits)
        and all(row.get('exit_observed')is True and row.get('exit_code')==0 for row in close['retained_exits']))

def main():
    start=time.monotonic();p=argparse.ArgumentParser()
    for name in ('packet','decision'):p.add_argument('--'+name,type=Path,required=True);p.add_argument('--'+name+'-sha256',required=True)
    a=p.parse_args()
    if os.name!='nt'or sha(a.packet)!=a.packet_sha256 or sha(a.decision)!=a.decision_sha256:raise ValueError('Exact Windows phase release required')
    packet=json.loads(a.packet.read_bytes());decision=json.loads(a.decision.read_bytes())
    if not(packet['schema']=='airk.private-selected-cal-completion-packet/1'and packet['phase']in ('certificate','fit')and packet['limits']=={'wall_seconds':60,'host_committed_bytes':256*1024**2}and decision.get('schema')=='airk.private-selected-cal-completion-decision/1'and decision.get('approved')is True and decision.get('packet_sha256')==a.packet_sha256 and decision.get('neural_execution')is False):raise ValueError('Exact no-NN phase/resources decision required')
    deadline=start+60;pins=dict(packet['pins']);pins[str(a.packet.resolve())]=a.packet_sha256;pins[str(a.decision.resolve())]=a.decision_sha256
    def verify():
        verify_absences(packet.get('absent_build_inputs',{}))
        for path,digest in pins.items():
            if sha(path)!=digest:raise ValueError('Phase source/data/image closure differs: '+path)
    verify()
    if sha(OWNER)!=OWNER_SHA or pins.get(str(Path(__file__).resolve()))!=sha(__file__):raise ValueError('Original owner/runner not pinned')
    work=Path(packet['output']).resolve()
    if work.exists()or not work.is_relative_to(ROOT/'artifacts'):raise ValueError('Fresh private output required')
    work.mkdir();(work/'stdin.txt').write_bytes(b'')
    environment=dict(os.environ);environment.update(packet['environment'])
    spec=importlib.util.spec_from_file_location('unchanged_selected_phase_owner',OWNER);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)]or active!=1 or total!=1:raise ValueError('One suspended initial member required')
        image=owner.image_proof(api,info.process,packet['image'])
        if time.monotonic()>=deadline:raise TimeoutError('Original phase deadline before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    receipt={'schema':'airk.private-selected-cal-completion-owned-execution/1','complete':False,'phase':packet['phase'],'packet_sha256':a.packet_sha256,'decision_sha256':a.decision_sha256,'input_seals':pins,'absent_build_inputs':packet.get('absent_build_inputs',{}),'neural_execution':False,'qualification_granted':False,'command':packet['argv']}
    try:
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('Original whole phase deadline before launch')
        with(work/'native.log').open('xb')as log,(work/'stdin.txt').open('rb')as stdin:
            receipt['owned']=owner.run_windows_owned_training(packet['argv'],ROOT,log,remaining,environment,stdin,256*1024**2,before_resume=suspended)
        if not natural(receipt['owned'],packet['phase'],packet['image']):raise ValueError('Phase did not naturally settle')
        artifact=Path(packet['artifact']);report=json.loads(artifact.read_bytes())
        if not(report.get('schema')==packet['artifact_schema']and report.get('complete')is True and report.get('human_semantic_gold_count')==0):raise ValueError('Exact no-human-gold complete artifact required')
        if packet['phase']=='certificate':
            if len(report.get('rows',[]))!=100 or len({row['id']for row in report['rows']})!=100 or report.get('qualified_profiles')!=[]:raise ValueError('Complete unqualified CAL100 certificate required')
        verify()
        artifact_digest,finished,expired=finalize_artifact(artifact,deadline)
        if expired:raise TimeoutError('Original phase deadline expired after artifact hashing')
        receipt['artifact']={'path':str(artifact),'sha256':artifact_digest};receipt['post_input_drift']=[];receipt['complete']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)};raise
    finally:
        receipt['elapsed_seconds']=time.monotonic()-start
        (work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
