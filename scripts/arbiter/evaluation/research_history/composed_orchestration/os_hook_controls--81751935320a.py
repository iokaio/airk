# SPDX-License-Identifier: Apache-2.0
"""Actual CPU-only Win32 controls for the reviewed pre-resume authentication hook."""
import ctypes,hashlib,json,os,sys,time
from ctypes import wintypes as w
from pathlib import Path
BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(BASE/'owned'))
import windows_job_8g,custody,reference_owned,reference_parent_no_site_r2,reference_worker
SID='S-1-5-21-3678370870-3858642417-826968105-1002'

def digest(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def retain(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def need(ok,message):
    if not ok:raise ValueError(message)
def api():
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    signatures={
        'GetCurrentProcess':([],w.HANDLE),
        'DuplicateHandle':([w.HANDLE,w.HANDLE,w.HANDLE,ctypes.POINTER(w.HANDLE),w.DWORD,w.BOOL,w.DWORD],w.BOOL),
        'QueryInformationJobObject':([w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.c_void_p],w.BOOL),
        'WaitForSingleObject':([w.HANDLE,w.DWORD],w.DWORD),
        'GetExitCodeProcess':([w.HANDLE,ctypes.POINTER(w.DWORD)],w.BOOL),
        'CloseHandle':([w.HANDLE],w.BOOL)}
    for name,(args,result) in signatures.items():getattr(k,name).argtypes=args;getattr(k,name).restype=result
    return k
class Account(ctypes.Structure):
    _fields_=[(name,ctypes.c_longlong) for name in ('user','kernel','period_user','period_kernel')]+[(name,w.DWORD) for name in ('faults','total','active','terminated')]
class Members(ctypes.Structure):
    _fields_=[('assigned',w.DWORD),('listed',w.DWORD),('pids',ctypes.c_size_t*16)]

def run_case(name,root,python,source_seals,campaign_deadline):
    started=time.monotonic();deadline=min(campaign_deadline,started+15)
    folder=root/name;folder.mkdir();marker=folder/'user-code.marker'
    argv=[python['path'],'-S','-B','-c',
        'from pathlib import Path;import sys;Path(sys.argv[1]).write_text("actual-user-code-ran");print("actual-control-exit0",flush=True)',str(marker)]
    result={'case':name,'command':argv,'original_started':started,'original_deadline':deadline,
        'per_case_seconds':15,'service_or_model_execution':False,'passed':False}
    k=api();job_observer=None;process=None;hook=[]
    def before_resume(owner_api,job,info,child_deadline):
        nonlocal job_observer,process
        need(child_deadline==deadline,'Hook original deadline differs')
        duplicate=w.HANDLE();current=k.GetCurrentProcess()
        need(k.DuplicateHandle(current,job,current,ctypes.byref(duplicate),0,False,2),'Actual Job observer handle retention failed')
        job_observer=duplicate
        process=custody.OriginalProcess(info.pid,python,SID,reference_worker.windows_sid)
        hook.append({'actual_process_before_resume':dict(process.proof),'deadline':child_deadline})
        expected_sid=SID if name!='wrong-sid' else SID+'-WRONG-EXPECTED-SID'
        expected_image=python if name!='wrong-image' else dict(python,sha256='0'*64)
        proof=reference_parent_no_site_r2.suspended_authentication(owner_api,info,deadline,child_deadline,
            expected_sid,expected_image,reference_owned.image_proof)
        hook[-1]['authentication_returned']=True
        return proof
    error=None
    try:
        remaining=deadline-time.monotonic();need(remaining>0,'Control expired before actual child launch')
        with (folder/'stdout.log').open('xb') as log:
            try:result['owned']=windows_job_8g.run_windows_owned(argv,BASE,log,remaining,dict(os.environ),
                absolute_deadline=deadline,before_resume=before_resume)
            except BaseException as caught:error=caught;result['actual_refusal']={'type':type(caught).__name__,'message':str(caught)}
        result['hook']=hook;need(len(hook)==1 and process is not None and job_observer is not None,'Actual before-resume hook was not observed once')
        remaining=deadline-time.monotonic();need(remaining>0,'Original control expired before actual retained-handle observation')
        observed=k.WaitForSingleObject(process.handle,max(1,int(remaining*1000)))
        code=w.DWORD();need(k.GetExitCodeProcess(process.handle,ctypes.byref(code)),'Retained actual exit code unavailable')
        result['retained_original_process']={**process.proof,'exit_observed':observed==0,'exit_code':int(code.value)}
        need(observed==0,'Actual original child HANDLE exit unobserved')
        accounting=Account();members=Members();limits=custody.Extended()
        need(k.QueryInformationJobObject(job_observer,1,ctypes.byref(accounting),ctypes.sizeof(accounting),None),'Actual Job accounting unavailable')
        need(k.QueryInformationJobObject(job_observer,3,ctypes.byref(members),ctypes.sizeof(members),None),'Actual full Job PID census unavailable')
        need(members.listed<=16,'Actual Job PID census truncated')
        need(k.QueryInformationJobObject(job_observer,9,ctypes.byref(limits),ctypes.sizeof(limits),None),'Actual Job limits unavailable')
        result['actual_job']={'total_processes':int(accounting.total),'active_processes':int(accounting.active),
            'terminated_processes':int(accounting.terminated),'member_pids':[int(members.pids[i]) for i in range(members.listed)],
            'flags':int(limits.basic.flags),'memory_bytes':int(limits.job_memory),'active_limit':int(limits.basic.active_limit)}
        need(accounting.total==1 and accounting.active==0 and members.listed==0,'Actual owned Job did not settle as one expected child')
        need(limits.basic.flags&0x2200==0x2200 and limits.job_memory==8<<30 and limits.basic.active_limit==0,'Original8GiB unrestricted multiprocess Job policy changed')
        result['marker_exists']=marker.exists()
        if name=='correct':
            need(error is None and code.value==0 and marker.is_file(),'Correct suspended identity/image did not reach actual user code and natural exit0')
            owned=result['owned']
            need(owned['exit_code']==0 and owned['owned_descendants_settled'] is True
                and owned['owned_job_forced_termination'] is False and owned['timed_out'] is False,'Correct hook owner did not report natural settlement')
        else:
            need(error is not None and not marker.exists() and code.value==77,'Wrong expected identity/image did not refuse before user code with actual forced cleanup')
            expected='suspended child identity' if name=='wrong-sid' else 'owned interpreter image differs'
            need(expected in str(error),'Different failure hid expected pre-resume refusal')
            need(hook[0].get('authentication_returned') is not True,'Rejected authentication incorrectly returned')
        for path,expected in source_seals.items():need(digest(path)==expected,'Control source/image changed after actual child')
        result['passed']=True
    except BaseException as caught:
        result['control_failure']={'type':type(caught).__name__,'message':str(caught)};result['passed']=False
    finally:
        if process is not None and process.handle is not None:
            closed=bool(k.CloseHandle(process.handle));process.handle=None
            result['original_process_observer_handle_closed']=closed
            if not closed:result['passed']=False
        if job_observer is not None:
            closed=bool(k.CloseHandle(job_observer));result['job_observer_handle_closed']=closed
            if not closed:result['passed']=False
        log=folder/'stdout.log'
        if log.is_file():result['stdout']={'path':str(log),'sha256':digest(log),'bytes':log.stat().st_size}
        now=time.monotonic();result['elapsed_seconds']=now-started;result['original_deadline_expired']=now>=deadline
        if result['original_deadline_expired']:result['passed']=False
        retain(folder/'result.json',result)
    return result

def main():
    need(os.name=='nt' and reference_worker.windows_sid()==SID,'Verified same-human Windows execution required')
    root=BASE/'os-controls-r1';need(not root.exists(),'Create-only OS control output required; no retry')
    python={'path':str(Path(sys.executable).resolve()),'sha256':digest(sys.executable)}
    paths=[Path(__file__).resolve(),*(BASE/'owned').glob('*.py'),Path(python['path'])]
    source_seals={str(path):digest(path) for path in paths}
    started=time.monotonic();deadline=started+60
    root.mkdir()
    plan={'schema':'airk.private.r154-auth-hook-controls/1','human_sid':SID,'python':python,
        'fixed_original_campaign_seconds':60,'fixed_original_per_case_seconds':15,
        'cases':['correct','wrong-sid','wrong-image'],'source_seals':source_seals,
        'service_or_model_execution':False,'cargo':False,'no_automatic_retry':True}
    retain(root/'plan.json',plan)
    results=[]
    for name in plan['cases']:
        result=run_case(name,root,python,source_seals,deadline);results.append(result)
        if not result['passed']:break
    now=time.monotonic();complete=len(results)==3 and all(row['passed'] for row in results) and now<deadline
    summary={'schema':'airk.private.r154-auth-hook-controls-result/1','complete':complete,
        'selected':3,'attempted':len(results),'passed':sum(row['passed'] for row in results),
        'elapsed_seconds':now-started,'original_campaign_expired':now>=deadline,
        'source_seals':source_seals,'service_or_model_execution':False,
        'results':[{'case':row['case'],'passed':row['passed']} for row in results]}
    retain(root/'result.json',summary);print(json.dumps(summary))
    return 0 if complete else 1
if __name__=='__main__':sys.exit(main())
