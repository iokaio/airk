# SPDX-License-Identifier: Apache-2.0
"""Private reviewed no-gradient feature owner. Full runtime verification precedes model execution."""
from pathlib import Path
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
import time
import types
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[3]

def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def require(value,message):
    if not value:raise ValueError(message)

def natural(result):
    return (result is not None and result.get('exit_code')==0 and result.get('timed_out') is False
        and result.get('log_oversized') is False and result.get('owned_descendants_settled') is True
        and result.get('owned_job_forced_termination') is False and not result.get('unexpected_third_process')
        and result.get('owned_job_final_member_pids')==[] and result.get('owned_job_final_active_processes')==0
        and result.get('owned_total_processes')==2 and len(result.get('retained_member_exits',[]))==2
        and all(row['exit_observed'] and row['exit_code']==0 for row in result['retained_member_exits']))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seal-sha256',required=True);parser.add_argument('--runner-sha256',required=True)
    parser.add_argument('--execute-reviewed',action='store_true')
    args=parser.parse_args();seal_path=HERE/'pilot-seal.json'
    require(digest(seal_path)==args.seal_sha256 and digest(__file__)==args.runner_sha256,'reviewed feature freeze differs')
    seal=json.loads(seal_path.read_bytes())
    for path,expected in seal['pins'].items():require(digest(path)==expected,'source/input identity differs: '+path)
    owner_path=Path(seal['owned_process_source']);owner=types.ModuleType('_feature_owned_process');owner.__file__=str(owner_path)
    exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__)
    runtime=json.loads(Path(seal['runtime_reference']).read_bytes());started=time.monotonic()
    indexed={os.path.normcase(str(Path(row['path']).resolve())):row['sha256'] for row in runtime['files']}
    for tree in runtime['runtime_trees']:
        root=Path(tree).resolve();expected={p for p in indexed if Path(p).is_relative_to(root)}
        require(owner.runtime_paths(root)==expected,'runtime source census differs: '+str(root))
    for row in runtime['files']:
        path=Path(row['path']);stat=path.stat(follow_symlinks=False)
        require(path.is_file() and not path.is_symlink() and not getattr(stat,'st_file_attributes',0)&0x400
                and stat.st_size==row['bytes'] and digest(path)==row['sha256'],'actual runtime file differs: '+str(path))
    proof={'runtime_files':len(runtime['files']),'runtime_bytes':sum(row['bytes'] for row in runtime['files']),
           'elapsed_seconds':time.monotonic()-started,'runtime_reference_sha256':seal['pins'][seal['runtime_reference']],
           'current_full_verification':True,'model_or_neural_execution':False}
    if not args.execute_reviewed:
        print(json.dumps(proof));return
    require(os.name=='nt','reviewed Windows feature owner required')
    baseline=owner.gpu_observation(seal['nvidia_smi'])
    for key in ['index','uuid','name','driver_version','compute_capability']:
        require(baseline[key]==runtime['gpu'][key],'actual GPU/driver identity differs: '+key)
    require(baseline['free_mib']*1024**2>=seal['limits']['cuda_allocator_bytes']+seal['limits']['cuda_context_headroom_bytes'],
            'declared GPU allocator+context headroom unavailable; preserve other users')
    output=HERE/'execution-r1';require(not output.exists() and not Path(seal['output']).exists(),'fresh feature operation required; no retry/overwrite')
    require(not any((HERE/'tmp').iterdir()),'fresh private TEMP required')
    environment=dict(os.environ);environment.update(seal['environment']);environment['AIRK_FEATURE_START_GATE']='1'
    argv=[runtime['venv_launcher']['path'],'-B','-X','pycache_prefix='+seal['environment']['PYTHONPYCACHEPREFIX'],
          str(HERE/'extract_pilot.py'),'--seal-sha256',args.seal_sha256]
    output.mkdir();read_fd,write_fd=os.pipe();result=None;failure=None;begin=time.monotonic()
    def before_resume(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        require(members==[int(info.pid)] and active==total==1,'one suspended venv shim required')
        image=owner.image_proof(api,info.process,runtime['venv_launcher'])
        return {'members':members,'active':active,'total':total,'shim_image':image}
    with os.fdopen(read_fd,'rb',buffering=0) as stdin,os.fdopen(write_fd,'wb',buffering=0) as writer,(output/'stdout.log').open('xb') as log:
        def before_go(api,job,info,deadline):
            gate_deadline=min(deadline,time.monotonic()+30);worker_pid=None
            while time.monotonic()<gate_deadline:
                lines=(output/'stdout.log').read_text(encoding='utf-8',errors='replace').splitlines()
                ready=[line for line in lines if line.startswith('AIRK_FEATURE_GATE_READY:')]
                if ready:
                    require(len(ready)==1,'ambiguous feature worker readiness');worker_pid=int(ready[0].split(':',1)[1]);break
                if api.WaitForSingleObject(info.process,max(1,min(50,int((gate_deadline-time.monotonic())*1000))))==0:
                    raise ValueError('owned interpreter exited before feature gate')
            require(worker_pid is not None and worker_pid!=info.pid,'exact separate base interpreter must reach pre-Torch gate')
            handle=api.OpenProcess(0x100000|0x1000,False,worker_pid);require(handle,'base interpreter handle unavailable');transferred=False
            try:
                member=wintypes.BOOL();require(api.IsProcessInJob(handle,job,ctypes.byref(member)) and member.value,'base interpreter escaped owned Job')
                image=owner.image_proof(api,handle,runtime['base_interpreter']);members,active,total=owner.job_snapshot(api,job)
                require(members==sorted([int(info.pid),worker_pid]) and active==total==2,'exact shim/base two-process roster required')
                require(time.monotonic()<deadline,'original deadline expired before GO')
                writer.write(('GO '+args.seal_sha256+'\n').encode());writer.flush();transferred=True
                return {'worker_pid':worker_pid,'members':members,'active':active,'total':total,'worker_image':image},[handle]
            finally:
                if not transferred:api.CloseHandle(handle)
        try:
            result=owner.run_windows_owned_training(argv,REPO,log,seal['limits']['wall_seconds'],environment,
                stdin,seal['limits']['host_committed_bytes'],before_resume=before_resume,after_resume=before_go)
        except BaseException as error:
            failure={'exception_type':type(error).__name__,'reason':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)}
        finally:
            drift=[path for path,expected in seal['pins'].items() if digest(path)!=expected]
            receipt={'schema':'airk.private-bounded-feature-pilot/1','seal_sha256':args.seal_sha256,
                'runner_sha256':args.runner_sha256,'command':argv,'environment':seal['environment'],
                'runtime_verification':proof,'baseline_gpu':baseline,'limits':seal['limits'],'owned':result,'failure':failure,
                'elapsed_seconds':time.monotonic()-begin,'post_input_drift':drift,'complete':False,'gradients':False,
                'FINAL':False,'task_qualified':False,'other_user_processes_touched':False}
            manifest=Path(seal['output'])/'features.json'
            if natural(result) and not drift and manifest.is_file():
                feature=json.loads(manifest.read_bytes());receipt['feature_manifest_sha256']=digest(manifest)
                receipt['complete']=(feature['complete'] is True and feature['seal_sha256']==args.seal_sha256
                    and feature['model']=='smallintern' and len(feature['examples'])==44 and feature['gradients'] is False)
            (output/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(receipt))
    if not receipt['complete']:raise SystemExit(1)
if __name__=='__main__':main()
