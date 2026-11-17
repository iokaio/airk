# SPDX-License-Identifier: Apache-2.0
"""PRIVATE cheap custody controls only; no AIRK, model or Cargo execution."""
import ctypes
from ctypes import wintypes as w
import json
from pathlib import Path
import subprocess
import sys
import threading
import custody

SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
WORKER_SHA = '9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'

def say(value):
    print(json.dumps(value),flush=True)

def child(scenario,index):
    actual_sid=custody.load_worker(WORKER_SHA).windows_sid()
    custody.need(actual_sid==SID,'Synthetic child did not retain the verified human SID')
    observed=custody.require_transport_job()
    memory=None
    if scenario=='memory' and index==0:
        api=ctypes.WinDLL('kernel32',use_last_error=True)
        api.VirtualAlloc.argtypes=[ctypes.c_void_p,ctypes.c_size_t,w.DWORD,w.DWORD]
        api.VirtualAlloc.restype=ctypes.c_void_p
        api.VirtualFree.argtypes=[ctypes.c_void_p,ctypes.c_size_t,w.DWORD]
        api.VirtualFree.restype=w.BOOL
        request=(8<<30)+(64<<20)
        allocation=api.VirtualAlloc(None,request,0x1000|0x2000,0x04)
        error=ctypes.get_last_error()
        if allocation:
            released=bool(api.VirtualFree(allocation,0,0x8000))
            say({'unexpected_over_cap_allocation':True,'released_without_touching':released})
            raise SystemExit(42)
        custody.need(error in (8,1455),'Native over-cap commit failed for an unexpected reason')
        memory={'requested_commit_bytes':request,'committed_pages_touched':0,'allocation_returned_null':True,'native_error':error}
    say({'ready':True,'index':index,'pid':__import__('os').getpid(),'sid':actual_sid,'actual_job':observed,'native_memory_refusal':memory})
    custody.need(sys.stdin.readline().strip()=='finish','Original control stream did not release child')
    raise SystemExit(23 if scenario=='nonzero' and index==2 else 0)

def leader(scenario):
    children=[]
    for index in range(3):
        children.append(subprocess.Popen([sys.executable,'-B','-S',__file__,'child',scenario,str(index)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NO_WINDOW))
    ready=[]
    for process in children:
        line=process.stdout.readline()
        custody.need(line,'Synthetic child did not reach actual ready event')
        ready.append(json.loads(line))
    custody.need(all(process.poll() is None for process in children),'Three original child handles were not simultaneously live')
    say({'all_three_child_handles_live':True,'children':ready})
    for process in children:
        process.stdin.write('finish\n')
        process.stdin.flush()
    exits=[]
    for process in children:
        stdout,stderr=process.communicate(timeout=5)
        exits.append(process.returncode)
        custody.need(not stderr,'Unexpected child failure: '+stderr)
    say({'children_exited':exits})
    raise SystemExit(23 if scenario=='nonzero' and exits==[0,0,23] else (0 if exits==[0,0,0] else 43))

if __name__=='__main__':
    role=sys.argv[1]
    if role=='child':child(sys.argv[2],int(sys.argv[3]))
    elif role=='leader':leader(sys.argv[2])
    elif role=='marker':
        Path(sys.argv[2]).write_text('child actually resumed',encoding='utf-8')
    elif role=='wait':
        say({'waiting_on_unsignaled_event':True,'pid':__import__('os').getpid(),'actual_job':custody.require_transport_job()})
        threading.Event().wait()
    else:raise ValueError('Unknown cheap custody control')
