# SPDX-License-Identifier: Apache-2.0
"""One reviewed native Focus parity/resource process, reusing the existing job owner."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time

ROOT=Path.cwd().resolve()
OWNER=ROOT/'tools/retrieval-train/training_execution.py'
OWNER_SHA='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
SELECTORS={
    'parity':'inference_engine::pool::arbiter::qualification::focus::focus_native_weight8_static512_matches_publisher_and_compiled_model_views',
    'resource':'inference_engine::pool::arbiter::qualification::focus::focus_native_cpu4_tier_records_maximum512_pair_resources',
}
def digest(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def remaining_seconds(deadline,now):
    remaining=deadline-now
    if remaining<=0:raise TimeoutError('Original whole-phase deadline expired')
    return min(600,remaining)
def natural(value):
    return (value is not None and value['exit_code']==0 and not value['timed_out'] and not value['log_oversized']
        and value['owned_descendants_settled'] and not value['owned_job_forced_termination']
        and not value['unexpected_third_process'] and value['owned_total_processes']==1
        and not value['owned_job_final_member_pids'] and value['owned_job_final_active_processes']==0
        and len(value['retained_member_exits'])==1
        and all(row['exit_observed']and row['exit_code']==0 for row in value['retained_member_exits']))
def main():
    started=time.monotonic();deadline=started+600
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet',type=Path,required=True);parser.add_argument('--packet-sha256',required=True)
    parser.add_argument('--execute-reviewed',action='store_true')
    args=parser.parse_args()
    if os.name!='nt':raise ValueError('This existing process owner is Windows-only; other platforms require their own qualification')
    if digest(args.packet)!=args.packet_sha256:raise ValueError('Root-reviewed native packet differs')
    packet=json.loads(args.packet.read_bytes());stage=packet['stage'];selector=SELECTORS[stage]
    if packet['schema']!='airk.private-focus-native-execution/1' or packet['cpu_fit']!={'minimum_physical_cores':4,'intra_threads':4,'inter_threads':1,'sequential':True,'spinning':False}:raise ValueError('Frozen native CPU4 packet required')
    if packet['limits']!={'wall_seconds':600,'host_committed_bytes':16*1024**3}:raise ValueError('Native phase resource caps differ')
    if digest(OWNER)!=OWNER_SHA:raise ValueError('Existing reviewed process owner changed')
    pins=dict(packet['pins']);pins[str(OWNER)]=OWNER_SHA
    binary=Path(packet['binary']['path']).resolve();pins[str(binary)]=packet['binary']['sha256']
    pins[str(Path(__file__).resolve())]=packet['runner_sha256']
    def verify():
        if any(digest(path)!=expected for path,expected in pins.items()):raise ValueError('Native image/source/profile/asset input drift')
    verify()
    listed=subprocess.run([str(binary),'--exact',selector,'--ignored','--list'],capture_output=True,text=True,timeout=min(10,remaining_seconds(deadline,time.monotonic())),check=True)
    if listed.stdout.count(selector+': test')!=1:raise ValueError('Exact native selector must select one test')
    work=Path(packet['output']).resolve()
    if work.exists() or not work.is_relative_to(ROOT/'artifacts'):raise ValueError('Fresh reviewed private native output required')
    environment=dict(os.environ);environment.update(packet['environment'])
    if any(environment.get(name)!=value for name,value in {'CUDA_VISIBLE_DEVICES':'','OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1'}.items()):raise ValueError('Native CPU-only/session-owned threading environment differs')
    inputs=Path(environment['AIRK_FOCUS_QUALIFICATION_INPUT']).resolve()
    if pins.get(str(inputs))!=environment['AIRK_FOCUS_QUALIFICATION_INPUT_SHA256']:raise ValueError('Actual native descriptor must be pinned before execution')
    capture=work/'capture.json'
    environment['AIRK_FOCUS_NATIVE_PARITY_OUTPUT' if stage=='parity'else'AIRK_FOCUS_NATIVE_RESOURCE_OUTPUT']=str(capture)
    argv=[str(binary),'--exact',selector,'--ignored','--nocapture','--test-threads=1']
    if not args.execute_reviewed:
        remaining_seconds(deadline,time.monotonic())
        print(json.dumps({'preflight_passed':True,'selected':1,'stage':stage,'command':argv,'packet_sha256':args.packet_sha256,'model_executed':False}));return
    if packet['status']!='ROOT_RELEASED':raise ValueError('Separate Root release absent')
    spec=importlib.util.spec_from_file_location('existing_native_owner',OWNER);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
    work.mkdir(parents=True);(work/'stdin.txt').write_bytes(b'')
    (work/'request.json').write_text(json.dumps({'packet_sha256':args.packet_sha256,'command':argv,'environment':packet['environment'],'pins':pins,'limits':packet['limits'],'selector_list':listed.stdout},indent=2)+'\n',encoding='utf8')
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)]or active!=1 or total!=1:raise ValueError('One exact suspended native member required')
        image=owner.image_proof(api,info.process,packet['binary'])
        if time.monotonic()>=deadline:raise ValueError('Original phase deadline expired before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    owned=None;failure=None;drift=[];complete=False
    try:
        with (work/'native.log').open('xb')as log,(work/'stdin.txt').open('rb')as stdin:
            owned=owner.run_windows_owned_training(argv,ROOT,log,remaining_seconds(deadline,time.monotonic()),environment,stdin,16*1024**3,before_resume=suspended)
        value=json.loads(capture.read_bytes())
        if value.get('complete')is not True or value.get('evaluation')is not True or value.get('release_qualified')is not False:raise ValueError('Complete scoped native capture missing')
        if stage=='parity'and (len(value.get('rows',[]))!=12 or sum(row['native_model_view']for row in value['rows'])!=10):raise ValueError('Incomplete native twelve-case capture')
        if stage=='resource'and len(value.get('samples_us',[]))!=20:raise ValueError('Incomplete native twenty-sample observation')
        complete=natural(owned)
    except BaseException as error:
        failure={'type':type(error).__name__,'message':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)}
        raise
    finally:
        try:drift=[path for path,expected in pins.items()if digest(path)!=expected]
        except (OSError,ValueError)as error:
            drift=['post-pin-verification-failed: '+str(error)]
            complete=False
        whole_phase_expired=time.monotonic()>=deadline
        complete=complete and not drift and not whole_phase_expired
        value={'schema':'airk.private-focus-native-execution-receipt/1','complete':complete,'stage':stage,'packet_sha256':args.packet_sha256,'owned':owned,'failure':failure,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,'quality_or_release_qualified':False,'no_automatic_retry':True,'other_processes_touched':False}
        if capture.is_file():value['capture_sha256']=digest(capture)
        value['whole_phase_deadline_not_refreshed']=True
        value['whole_phase_expired']=whole_phase_expired or time.monotonic()>=deadline
        if value['whole_phase_expired']:complete=False;value['complete']=False
        (work/'execution.json').write_text(json.dumps(value,indent=2)+'\n',encoding='utf8')
        print(json.dumps({'complete':complete,'stage':stage,'elapsed_seconds':value['elapsed_seconds'],'capture_sha256':value.get('capture_sha256')}),flush=True)
    if not complete:raise SystemExit(1)
if __name__=='__main__':main()
