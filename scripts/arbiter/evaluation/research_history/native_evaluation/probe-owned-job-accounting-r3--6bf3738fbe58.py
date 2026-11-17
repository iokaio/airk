# SPDX-License-Identifier: Apache-2.0
"""Private synthetic Job-accounting diagnostic. Never reads FINAL payload/content."""
import ctypes,hashlib,importlib.util,inspect,json,os,sys,time
from ctypes import wintypes as w
from pathlib import Path
ROOT=Path.cwd(); BASE=ROOT/'artifacts/advanced-split/models/qasper-owned-job-diagnostic-r3';BASE.mkdir(exist_ok=False)
source_path=ROOT/'scripts/provision-static-onnx-runtime.py';raw=source_path.read_bytes();assert hashlib.sha256(raw).hexdigest()=='304ff3392aec7a97cd57802e8a91bb2b0d7b1102e0452f08662627b0d1f667a4'
spec=importlib.util.spec_from_file_location('owned_process_source',source_path);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
def census(api,job,info,accounting):
 class Members(ctypes.Structure):_fields_=[('assigned',w.DWORD),('listed',w.DWORD),('pids',ctypes.c_size_t*64)]
 members=Members();size=w.DWORD();ok=api.QueryInformationJobObject(job,3,ctypes.byref(members),ctypes.sizeof(members),ctypes.byref(size))
 if not ok:raise ctypes.WinError(ctypes.get_last_error())
 assert members.listed<=64
 api.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];api.OpenProcess.restype=w.HANDLE
 api.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];api.QueryFullProcessImageNameW.restype=w.BOOL
 observations=[]
 for index in range(members.listed):
  pid=int(members.pids[index]);handle=api.OpenProcess(0x100000|0x1000,False,pid)
  if not handle:observations.append({'pid':pid,'open_error':ctypes.get_last_error()});continue
  try:
   capacity=w.DWORD(32768);image=ctypes.create_unicode_buffer(capacity.value);ok=api.QueryFullProcessImageNameW(handle,0,image,ctypes.byref(capacity));code=w.DWORD();api.GetExitCodeProcess(handle,ctypes.byref(code))
   observations.append({'pid':pid,'wait_status':int(api.WaitForSingleObject(handle,0)),'exit_code':int(code.value),'image':image.value if ok else None})
  finally:api.CloseHandle(handle)
 return {'accounting_active_at_original_decision':int(accounting.active),'leader_pid':int(info.pid),'leader_wait_status':int(api.WaitForSingleObject(info.process,0)),'job_assigned_processes':int(members.assigned),'job_member_pids':[int(members.pids[i]) for i in range(members.listed)],'members_observed':observations}
def event_drain(api,job,accounting):
 # Private diagnostic only: native exact-handle waits share one250ms deadline.
 class Members(ctypes.Structure):_fields_=[('assigned',w.DWORD),('listed',w.DWORD),('pids',ctypes.c_size_t*64)]
 started=time.monotonic();deadline=started+0.25;observations=[]
 while True:
  m=Members();size=w.DWORD();assert api.QueryInformationJobObject(job,3,ctypes.byref(m),ctypes.sizeof(m),ctypes.byref(size));assert m.listed<=64
  if m.listed==0:break
  for index in range(m.listed):
   pid=int(m.pids[index]);handle=api.OpenProcess(0x100000|0x1000,False,pid)
   if not handle:observations.append({'pid':pid,'already_exited_or_open_error':ctypes.get_last_error()});continue
   try:
    remaining=max(0,deadline-time.monotonic());waited=api.WaitForSingleObject(handle,max(0,int(remaining*1000)));observations.append({'pid':pid,'event_wait_status':int(waited)})
   finally:api.CloseHandle(handle)
  if time.monotonic()>=deadline:break
 assert api.QueryInformationJobObject(job,1,ctypes.byref(accounting),ctypes.sizeof(accounting),None)
 return {'native_event_waits':observations,'elapsed_seconds':time.monotonic()-started,'remaining_active':int(accounting.active),'private_diagnostic_limit_seconds':0.25}

source=inspect.getsource(owner.run_windows_owned);assert source.count('forced = accounting.active != 0')==1
source=source.replace('forced = accounting.active != 0','diagnostic = census(api, job, info, accounting)\n        diagnostic[\"event_drain\"] = event_drain(api,job,accounting)\n        forced = accounting.active != 0')
source=source.replace('"owned_job_forced_termination": forced,','"owned_job_forced_termination": forced, "decision_census": diagnostic,')
(BASE/'instrumented-function.py').write_text(source,encoding='utf-8',newline='\n');ns=dict(owner.__dict__);ns['census']=census;ns['event_drain']=event_drain;exec(compile(source,str(BASE/'instrumented-function.py'),'exec'),ns)
cases=[('bare_python_exit',"print('synthetic bare child complete',flush=True)"),('waited_pwsh_exit',"import subprocess; r=subprocess.run(['pwsh','-NoProfile','-NonInteractive','-Command',\"Write-Output 'synthetic observer complete'\"],capture_output=True,check=True); print('synthetic waited observer returned',flush=True)"),('actual_live_descendant',"import subprocess,sys; child=subprocess.Popen([sys.executable,'-I','-B','-c',\"import threading;print('ready',flush=True);threading.Event().wait()\"],stdout=subprocess.PIPE); assert child.stdout.readline().strip()==b'ready'; print('synthetic parent exits leaving owned descendant',flush=True)")]
rows=[]
for name,code in cases:
 start=time.monotonic()
 with (BASE/(name+'.log')).open('wb') as output:result=ns['run_windows_owned']([sys.executable,'-I','-B','-c',code],ROOT,output,10,None)
 rows.append({'name':name,'result':result,'elapsed_seconds':time.monotonic()-start})
assert source_path.read_bytes()==raw
report={'schema':'airk.private-owned-job-accounting-diagnostic/1','source_sha256':hashlib.sha256(raw).hexdigest(),'instrumented_function_sha256':hashlib.sha256(source.encode()).hexdigest(),'instrumentation':'Private revised-diagnosis probe adds bounded250ms exact-native-member-handle event drain before refreshed forced decision. No product/default deadline changes; historical runs retain failures.','cases':rows,'actual_gold_or_alignment_rerun':False,'public_source_changed':False}
(BASE/'receipt.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
