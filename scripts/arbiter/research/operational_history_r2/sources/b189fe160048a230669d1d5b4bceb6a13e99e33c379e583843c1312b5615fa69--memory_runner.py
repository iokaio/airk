# SPDX-License-Identifier: Apache-2.0
"""Private memory fence around the exact reviewed data-only producer."""
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import runpy
import sys
import time
sys.dont_write_bytecode = True
ROOT=Path.cwd();WORK=ROOT/'artifacts/advanced-split/context-action-facts-source-r1'
SOURCE=WORK/'context_action_facts.py';PROTOCOL=WORK/'protocol.private.json'
assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()=='0911be58cda856e741bec6dec8d1b5763337096b889765060c9acaa82ee40a24'
assert hashlib.sha256(PROTOCOL.read_bytes()).hexdigest()=='f7c20e6de1519fb05d6d06444b97a7911fc0b4aa5f31abd563fde8d21e26bc14'
helper=ROOT/'artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r3/audit.py'
spec=importlib.util.spec_from_file_location('existing_memory_guard_source_only',helper);guard=importlib.util.module_from_spec(spec);spec.loader.exec_module(guard)
started=time.monotonic();api=job=None;status='refused';failure=None;peak=None
try:
    if os.name!='nt':raise RuntimeError('reviewed memory fence is Windows-only')
    api,job=guard.enforce_budget()
    status='running'
    sys.argv=[str(SOURCE),'--protocol',str(PROTOCOL),'--protocol-sha256','f7c20e6de1519fb05d6d06444b97a7911fc0b4aa5f31abd563fde8d21e26bc14','--output',str(ROOT/'artifacts/advanced-split/models/context-action-publisher-facts-r1')]
    runpy.run_path(str(SOURCE),run_name='__main__')
    status='passed'
except BaseException as error:
    status='failed';failure=type(error).__name__
    raise
finally:
    if job:
        observed=guard.Extended();api.QueryInformationJobObject.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_void_p,wintypes.DWORD,ctypes.c_void_p]
        if api.QueryInformationJobObject(job,9,ctypes.byref(observed),ctypes.sizeof(observed),None):peak=int(observed.peak_process)
        api.CloseHandle.argtypes=[wintypes.HANDLE];api.CloseHandle(job)
    receipt={'schema':'airk.private-data-memory-observation/1','pid':os.getpid(),'status':status,'failure_class':failure,'hard_process_committed_bytes':268435456,'peak_committed_process_bytes':peak,'elapsed_seconds':time.monotonic()-started,'guard_source_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'model_execution':False}
    with (WORK/'preparation-memory-r1.json').open('x',encoding='utf-8') as stream:json.dump(receipt,stream,indent=2);stream.write('\n')
