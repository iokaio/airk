# SPDX-License-Identifier: Apache-2.0
"""Root-only reviewed-plan launcher using the maintained owned Windows Job owner."""
import argparse,ctypes,importlib.util,json,os,sys,time
from ctypes import wintypes
from pathlib import Path
from focus_worker import digest,load_module,pinned
from workflow import continuity_key

def disposition(owned,worker):
    settled=bool(owned and owned['owned_descendants_settled']and not owned['owned_job_forced_termination']and owned['owned_job_final_active_processes']==0 and not owned['owned_job_final_member_pids']and all(r['exit_observed']for r in owned['retained_member_exits']))
    passed=bool(settled and owned['exit_code']==0 and not owned['timed_out']and not owned['log_oversized']and owned['owned_total_processes']==1 and worker and worker['complete'])
    return {'naturally_settled':settled,'known_natural_nonzero':bool(settled and owned['exit_code']!=0),'passed':passed,'ownership_unknown_or_unsettled':not settled}

def finalize_receipt(owned,worker,plan,plan_path,plan_sha,started,command,identity,failure,deadline,*,clock=time.monotonic,hash_file=digest,flush_log=None):
    # The last clock sample follows every consumed hash and the receipt body.
    # Only the final timing-field replacement/serialization/write is excluded.
    state=disposition(owned,worker);errors=[];hashes={}
    if failure is not None:state['passed']=False
    def observe(label,path,expected=None):
        try:
            actual=hash_file(path);hashes[label]={'path':str(path),'sha256':actual}
            if expected is not None and actual!=expected:errors.append({'stage':label,'kind':'hash_mismatch','expected':expected,'observed':actual})
            return actual
        except BaseException as error:
            errors.append({'stage':label,'kind':'hash_error','type':type(error).__name__,'message':str(error)});return None
    if flush_log is not None:
        try:flush_log()
        except BaseException as error:errors.append({'stage':'child_stdout_flush','kind':'flush_error','type':type(error).__name__,'message':str(error)})
    result_path=Path(plan['worker_output'])/'result.json'
    result_sha=observe('worker_result',result_path)if worker is not None else None
    observe('child_stdout_log',Path(plan['log_dir'])/'stdout.log')
    observe('actual_execution_plan',plan_path,plan_sha)
    for name,pin in sorted(plan['execution_sources'].items()):observe('consumed_source:'+name,Path(pin['path']),pin['sha256'])
    if errors:state['passed']=False
    if owned and owned.get('peak_host_committed_bytes',0)>plan['limits']['host_committed_bytes']:state['passed']=False
    receipt={'schema':'airk.private.F1-F2-owned-execution/1','plan_sha256':plan_sha,'argv':command,'identity':identity,'elapsed_seconds':None,'original_deadline':deadline,'limits':plan['limits'],'owned':owned,'failure':failure,'worker_result_path':str(result_path),'worker_result_sha256':result_sha,'disposition':state,'final_hashes':hashes,'final_hash_failures':errors,'qualification':False,'promotion':False,'no_retry':True,'timing_scope':'Includes launch/actual owned settlement, worker finally retention, every final result/log/plan/consumed-source hash and receipt-body construction/validation. Excludes only final elapsed/disposition insertion, final execution.json serialization/write and terminal reporting.'}
    # Validate/construct the entire potentially large body before final sampling.
    json.dumps(receipt)
    elapsed=clock()-started;receipt['elapsed_seconds']=elapsed
    if not 0<=elapsed<plan['limits']['wall_seconds']:state['passed']=False
    return receipt

def launch(path,sha):
    path=Path(path).resolve()
    if digest(path)!=sha:raise ValueError('reviewed plan hash changed')
    plan=json.loads(Path(path).read_bytes());started=time.monotonic();deadline=started+plan['limits']['wall_seconds']
    if plan['continuity_key']!=continuity_key(plan):raise ValueError('immutable continuation key changed')
    identity_module=load_module(plan['human_identity_helper'],'actual_human_custody');identity=identity_module.identity()
    if identity['sid']!=plan['human_sid']:raise ValueError('logged-in human identity differs')
    out=Path(plan['log_dir']).resolve();root=Path(plan['repository_root']).resolve()
    if out.exists()or not out.is_relative_to(root/'artifacts/v190/final-plan'):raise ValueError('fresh approved private logs required')
    for filename,pin in plan['execution_sources'].items():pinned(pin)
    owner=load_module(plan['ownership_module'],'reviewed_unchanged_owned_job')
    gpu=owner.gpu_observation(plan['nvidia_smi'])
    if any(gpu[k]!=plan['gpu'][k]for k in('uuid','name','driver_version','compute_capability'))or gpu['free_mib']*1024**2<plan['limits']['minimum_cuda_free_bytes']:raise ValueError('exact GPU/device reserve changed')
    if time.monotonic()>=deadline:raise TimeoutError('original allocation consumed before launch')
    out.mkdir();command=[plan['base_python']['path'],'-B',plan['worker']['path'],'--plan',str(path),'--plan-sha256',sha]
    start={'schema':'airk.private.F1-F2-owned-start/1','plan':{'path':str(path),'sha256':sha},'identity':identity,'argv':command,'monotonic_start':started,'monotonic_deadline':deadline,'gpu_observed':gpu,'sources':plan['execution_sources'],'limits':plan['limits']}
    (out/'start.json').write_text(json.dumps(start,indent=2)+'\n',encoding='utf8')
    environment=dict(os.environ);environment.update(plan['environment']);environment['AIRK_F2_START_GATE']='1'
    read_fd,write_fd=os.pipe();owned=None;failure=None;worker=None
    def before_resume(api,job,info,cutoff):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)]or active!=1 or total!=1:raise ValueError('single suspended base worker required')
        parent_member=wintypes.BOOL()
        if not api.IsProcessInJob(api.GetCurrentProcess(),job,ctypes.byref(parent_member))or parent_member.value:raise ValueError('parent entered child Job')
        image=owner.image_proof(api,info.process,plan['base_python'])
        return{'single_suspended_worker':int(info.pid),'image':image,'parent_in_child_job':False}
    with os.fdopen(read_fd,'rb',buffering=0)as stdin,os.fdopen(write_fd,'wb',buffering=0)as writer,(out/'stdout.log').open('wb')as log:
        def after_resume(api,job,info,cutoff):
            while time.monotonic()<cutoff:
                raw=(out/'stdout.log').read_bytes()
                if len(raw)>1024*1024:raise ValueError('pre-Torch gate log over cap')
                found=[l for l in raw.decode('utf8',errors='strict').splitlines()if l.startswith('AIRK_F2_GATE_READY:')]
                if found:
                    if found!=['AIRK_F2_GATE_READY:'+str(info.pid)]:raise ValueError('actual worker gate PID changed')
                    members,active,total=owner.job_snapshot(api,job)
                    if members!=[int(info.pid)]or active!=1 or total!=1:raise ValueError('pre-Torch child roster changed')
                    image=owner.image_proof(api,info.process,plan['base_python']);writer.write(f'GO {sha} {cutoff!r}\n'.encode());writer.flush()
                    return{'same_actual_leader_gate':int(info.pid),'image':image,'members':members},[]
                remaining=max(1,min(50,int((cutoff-time.monotonic())*1000)))
                if api.WaitForSingleObject(info.process,remaining)==0:raise ValueError('worker exited before GO')
            raise TimeoutError('original deadline before GO')
        try:
            owned=owner.run_windows_owned_training(command,root,log,plan['limits']['wall_seconds'],environment,stdin,plan['limits']['host_committed_bytes'],before_resume=before_resume,after_resume=after_resume,absolute_deadline=deadline)
            result=Path(plan['worker_output'])/'result.json'
            if result.exists():worker=json.loads(result.read_bytes())
        except BaseException as error:
            failure={'type':type(error).__name__,'message':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)}
            raise
        finally:
            receipt=finalize_receipt(owned,worker,plan,path,sha,started,command,identity,failure,deadline,flush_log=log.flush)
            (out/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8')
    if not receipt['disposition']['passed']:raise RuntimeError('F1/F2 owned execution nonpass; original evidence retained')
    print(json.dumps(receipt))
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--plan',type=Path,required=True);p.add_argument('--plan-sha256',required=True);v=p.parse_args();launch(v.plan,v.plan_sha256)
