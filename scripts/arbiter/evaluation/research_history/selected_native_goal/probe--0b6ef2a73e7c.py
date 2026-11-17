# SPDX-License-Identifier: Apache-2.0
"""Actual-host no-content memory seam. No dataset parsing or model/native work."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
import types

class Memory(ctypes.Structure):
    _fields_=[('cb',wintypes.DWORD),('PageFaultCount',wintypes.DWORD)]+[(name,ctypes.c_size_t) for name in ('PeakWorkingSetSize','WorkingSetSize','QuotaPeakPagedPoolUsage','QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage','QuotaNonPagedPoolUsage','PagefileUsage','PeakPagefileUsage','PrivateUsage')]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=('legacy_eager_dataset','one_thread_eager_dataset','phase2_import_order'),required=True)
    parser.add_argument('--resource-owner',type=Path,required=True)
    parser.add_argument('--role',nargs=2,action='append',required=True)
    args=parser.parse_args()
    raw=args.resource_owner.read_bytes()
    if hashlib.sha256(raw).hexdigest()!='9c540c08c2e56d868059ab8425193e52f0d9141f75d16df01f622c679032425b':raise ValueError('resource owner changed')
    owner=types.ModuleType('memory_resource_owner');owner.__file__=str(args.resource_owner);exec(compile(raw,str(args.resource_owner),'exec'),owner.__dict__)
    owner.JOB_STATE=owner.enforce_budget()
    done=threading.Event()
    def expire():
        if not done.wait(60):os._exit(124)
    threading.Thread(target=expire,daemon=True).start()
    psapi=ctypes.WinDLL('psapi',use_last_error=True)
    psapi.GetProcessMemoryInfo.argtypes=[wintypes.HANDLE,ctypes.POINTER(Memory),wintypes.DWORD]
    psapi.GetProcessMemoryInfo.restype=wintypes.BOOL
    kernel=ctypes.WinDLL('kernel32',use_last_error=True);kernel.GetCurrentProcess.restype=wintypes.HANDLE
    stage='initialized'
    def observe(role,byte_count=0):
        memory=Memory();memory.cb=ctypes.sizeof(memory)
        if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(),ctypes.byref(memory),memory.cb):raise OSError(ctypes.get_last_error())
        print(json.dumps({'mode':args.mode,'stage':role,'role_bytes':byte_count,'PrivateUsage':memory.PrivateUsage,'PeakPagefileUsage':memory.PeakPagefileUsage,'WorkingSetSize':memory.WorkingSetSize,'job_peak_process_memory':owner.peak_memory(),'numpy_imported':'numpy' in sys.modules,'pandas_imported':'pandas' in sys.modules,'dataset_imported':'pyarrow.dataset' in sys.modules}),flush=True)
    observe('before_imports')
    thread_keys=('OMP_NUM_THREADS','OPENBLAS_NUM_THREADS','MKL_NUM_THREADS','NUMEXPR_NUM_THREADS','VECLIB_MAXIMUM_THREADS')
    print(json.dumps({'mode':args.mode,'inherited_thread_environment':{key:os.environ.get(key) for key in thread_keys}}),flush=True)
    if args.mode!='legacy_eager_dataset':
        for key in thread_keys:os.environ[key]='1'
    try:
        stage='import_pyarrow';observe('before_'+stage)
        import pyarrow
        observe('after_'+stage)
        if args.mode=='phase2_import_order':
            pyarrow.set_cpu_count(1);pyarrow.set_io_thread_count(1)
            stage='import_parquet';observe('before_'+stage)
            import pyarrow.parquet
            observe('after_'+stage)
        stage='import_dataset';observe('before_'+stage)
        import pyarrow.dataset
        observe('after_'+stage)
        retained={}
        for role,path in args.role:
            stage='read_'+role;path=Path(path);size=path.stat().st_size
            observe('before_'+stage,size)
            retained[role]=path.read_bytes()
            observe('after_'+stage,len(retained[role]))
        print(json.dumps({'mode':args.mode,'complete':True,'roles_retained':len(retained),'task_gold_artifact_parsed':False}),flush=True)
        observe('completed')
    except MemoryError:
        observe('MemoryError_'+stage)
        print(json.dumps({'mode':args.mode,'complete':False,'failure_class':'MemoryError','failed_stage':stage,'task_gold_artifact_parsed':False}),flush=True)
        raise
    finally:done.set()

if __name__=='__main__':main()
