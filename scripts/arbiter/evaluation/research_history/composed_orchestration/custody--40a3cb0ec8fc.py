# SPDX-License-Identifier: Apache-2.0
"""PRIVATE read-only native job and original service-process handle observations."""
import ctypes
from ctypes import wintypes as w
import hashlib
import importlib.util
import os
from pathlib import Path
import time

WORKER = Path(r'D:\code\Github\airk\artifacts\advanced-split\models\weights-v2\software-v2-independent-numeric-r1\reference_worker.py')

def need(value, message):
    if not value:
        raise ValueError(message)

def load_worker(expected_sha):
    need(hashlib.sha256(WORKER.read_bytes()).hexdigest() == expected_sha, 'Reviewed native SID reader changed')
    spec = importlib.util.spec_from_file_location('ann_custody_reviewed_sid', WORKER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class Basic(ctypes.Structure):
    _fields_ = [('process_time',ctypes.c_longlong),('job_time',ctypes.c_longlong),('flags',w.DWORD),('min_working',ctypes.c_size_t),('max_working',ctypes.c_size_t),('active_limit',w.DWORD),('affinity',ctypes.c_size_t),('priority',w.DWORD),('scheduling',w.DWORD)]
class IO(ctypes.Structure):
    _fields_ = [(name,ctypes.c_ulonglong) for name in ('read_ops','write_ops','other_ops','read_bytes','write_bytes','other_bytes')]
class Extended(ctypes.Structure):
    _fields_ = [('basic',Basic),('io',IO),('process_memory',ctypes.c_size_t),('job_memory',ctypes.c_size_t),('peak_process',ctypes.c_size_t),('peak_job',ctypes.c_size_t)]

def current_job():
    api = ctypes.WinDLL('kernel32',use_last_error=True)
    api.QueryInformationJobObject.argtypes = [w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.c_void_p]
    api.QueryInformationJobObject.restype = w.BOOL
    limits = Extended()
    need(api.QueryInformationJobObject(None,9,ctypes.byref(limits),ctypes.sizeof(limits),None), 'Current actual native Job policy is unobservable')
    return {'flags':int(limits.basic.flags),'active_limit':int(limits.basic.active_limit),'job_memory':int(limits.job_memory),'process_memory':int(limits.process_memory),'peak_job':int(limits.peak_job)}

def require_transport_job():
    observed = current_job()
    need(observed['flags'] & 0x2200 == 0x2200 and observed['job_memory'] == 8 << 30 and observed['active_limit'] == 0, 'Real whole-transport8GiB unrestricted multiprocess Job required; two-process training owner refused')
    return observed

class OriginalProcess:
    """Retain actual original process object; no terminate, PID substitution or fabricated exit."""
    def __init__(self, pid, expected_image, expected_sid, sid_reader):
        self.api = api = ctypes.WinDLL('kernel32',use_last_error=True)
        signatures = {
            'OpenProcess':([w.DWORD,w.BOOL,w.DWORD],w.HANDLE),
            'QueryFullProcessImageNameW':([w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)],w.BOOL),
            'GetProcessTimes':([w.HANDLE,ctypes.POINTER(w.FILETIME),ctypes.POINTER(w.FILETIME),ctypes.POINTER(w.FILETIME),ctypes.POINTER(w.FILETIME)],w.BOOL),
            'WaitForSingleObject':([w.HANDLE,w.DWORD],w.DWORD),
            'GetExitCodeProcess':([w.HANDLE,ctypes.POINTER(w.DWORD)],w.BOOL),
            'CloseHandle':([w.HANDLE],w.BOOL),
        }
        for name,(args,result) in signatures.items():
            getattr(api,name).argtypes=args
            getattr(api,name).restype=result
        self.handle = api.OpenProcess(0x100000 | 0x1000,False,pid)
        need(self.handle, 'Original service process handle unavailable after actual startup')
        try:
            actual_sid = sid_reader(self.handle)
            need(actual_sid == expected_sid, 'Actual original service process SID differs')
            text=ctypes.create_unicode_buffer(32768)
            size=w.DWORD(len(text))
            need(api.QueryFullProcessImageNameW(self.handle,0,text,ctypes.byref(size)), 'Actual service image unavailable')
            actual=Path(text.value).resolve(strict=True)
            need(actual.samefile(Path(expected_image['path'])),'Actual service image path differs from native active fixture')
            with actual.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
            need(digest==expected_image['sha256'],'Actual service image hash differs')
            created,exited,kernel,user=(w.FILETIME() for _ in range(4))
            need(api.GetProcessTimes(self.handle,ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)), 'Original service creation time unavailable')
            self.proof={'pid':int(pid),'process_created_filetime':(int(created.dwHighDateTime)<<32)|int(created.dwLowDateTime),'execution_user_sid':actual_sid,'image':{'path':str(actual),'sha256':digest},'retained_original_handle':True,'exit_observed':False,'handle_closed':False}
        except BaseException:
            api.CloseHandle(self.handle)
            self.handle=None
            raise
    def after_canonical_stop(self, stop, original_deadline):
        need(stop is not None and stop.get('status')=='lifecycle' and stop['record']['phase']=='complete','Actual native StopComplete required before original process exit proof')
        return self.observe_original_exit(original_deadline)
    def observe_original_exit(self,original_deadline):
        remaining=original_deadline-time.monotonic()
        need(remaining>0,'Original transport phase expired before original service exit observation')
        waited=self.api.WaitForSingleObject(self.handle,max(1,int(remaining*1000)))
        need(waited==0,'Actual original service process exit unobserved within original phase deadline')
        code=w.DWORD()
        need(self.api.GetExitCodeProcess(self.handle,ctypes.byref(code)),'Actual original service exit code unavailable')
        self.proof.update(exit_observed=True,exit_code=int(code.value),observed_before_original_deadline=time.monotonic()<original_deadline)
        need(code.value==0 and self.proof['observed_before_original_deadline'],'Actual original service did not exit naturally within original phase')
        need(self.api.CloseHandle(self.handle),'Original service handle close failed')
        self.handle=None
        self.proof['handle_closed']=True
        return dict(self.proof)
