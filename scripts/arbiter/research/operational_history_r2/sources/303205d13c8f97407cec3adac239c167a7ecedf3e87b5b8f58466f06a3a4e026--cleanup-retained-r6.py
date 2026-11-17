# SPDX-License-Identifier: Apache-2.0
import ctypes as c, ctypes.wintypes as w, datetime as dt, hashlib, json, pathlib, subprocess
root=pathlib.Path(r'D:\code\Github\airk\artifacts\local\one-airk-1.8.14\user-private-fixtures-6c0f255fb113480ea4938e25f563cdf7\.tmpVnrMQ9')
exe=root/'install/runtimes/fd3e5417a715f721bb6b554845adffae34771fb4c6973fdf371e86093bccfa24/airk.exe'
expected_sha='55469651bb2675f5042a0d1696ebc776558d31df63bfa27db9762b1f6a3ba9b0'
k=c.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD]; k.OpenProcess.restype=w.HANDLE
k.GetProcessTimes.argtypes=[w.HANDLE,c.POINTER(w.FILETIME),c.POINTER(w.FILETIME),c.POINTER(w.FILETIME),c.POINTER(w.FILETIME)]
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,c.POINTER(w.DWORD)]
k.TerminateProcess.argtypes=[w.HANDLE,w.UINT]; k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD]; k.CloseHandle.argtypes=[w.HANDLE]
def require(ok):
 if not ok: raise c.WinError(c.get_last_error())
def norm(p): return str(p).removeprefix('\\\\?\\').lower()
held=[]; receipt={'schema':1,'fixture':str(root),'native_stop_receipt':'cleanup-r8-stop.stderr','processes':[]}
try:
 assert hashlib.sha256(exe.read_bytes()).hexdigest()==expected_sha
 for pid,start in [(252128,'2026-11-07T21:33:10.409545-07:00'),(92032,'2026-11-07T21:33:12.022748-07:00')]:
  h=k.OpenProcess(0x100000|0x1000|1,False,pid); require(h); held.append((pid,h))
  buf=c.create_unicode_buffer(32768); size=w.DWORD(len(buf)); require(k.QueryFullProcessImageNameW(h,0,buf,c.byref(size)))
  assert norm(buf.value)==norm(exe), (pid,'image changed')
  values=[w.FILETIME() for _ in range(4)]; require(k.GetProcessTimes(h,*[c.byref(v) for v in values]))
  ticks=(values[0].dwHighDateTime<<32)|values[0].dwLowDateTime
  epoch=dt.datetime(1601,1,1,tzinfo=dt.timezone.utc); delta=dt.datetime.fromisoformat(start)-epoch
  expected=(delta.days*86400+delta.seconds)*10000000+delta.microseconds*10
  assert abs(ticks-expected)<10,(pid,'creation changed',ticks,expected)
  assert k.WaitForSingleObject(h,0)==258,(pid,'already exited')
  receipt['processes'].append({'pid':pid,'creation_filetime':ticks,'executable':buf.value,'sha256':expected_sha})
 observed=subprocess.run([str(exe),'runtime','inspect','--install-dir',str(root/'install')],capture_output=True,text=True,timeout=20,check=True)
 state=json.loads(observed.stdout); life=state['lifecycle']; pending=life['pending']
 assert state['payload_verified'] and state['registry_namespace']=='ceeb9c029207ab8708c63d12a1fcdd75'
 assert state['selection']['active']=='fd3e5417a715f721bb6b554845adffae34771fb4c6973fdf371e86093bccfa24'
 assert pending['id']=='a96c7d478862e198416bc50a830e65fb707a3f8f861b99db31a59fb4713800b8' and pending['request']['expected_instance']=='e5b998f648c59e3ff7f4ba9aad6ff1da'
 assert pending['request']['action']=='stop' and pending['request']['manager'] is False and pending['phase']=='draining'
 receipt['native_installation']=state
 (root/'cleanup-r8-held-handles-before.json').write_text(json.dumps(receipt,indent=2))
 for pid,h in held:
  if k.WaitForSingleObject(h,0)==258: require(k.TerminateProcess(h,1))
 for pid,h in held:
  assert k.WaitForSingleObject(h,10000)==0,(pid,'exit not observed')
  next(x for x in receipt['processes'] if x['pid']==pid)['exit_observed_on_held_handle']=True
 receipt['result']='exact_owned_fixture_handles_exited'
 (root/'cleanup-r8-held-handles-complete.json').write_text(json.dumps(receipt,indent=2))
 print(json.dumps({'result':receipt['result'],'pids':[pid for pid,_ in held]}))
finally:
 for _,h in held: k.CloseHandle(h)

