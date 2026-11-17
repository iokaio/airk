# SPDX-License-Identifier: Apache-2.0
"""One orderly stop of the pinned original R7 fixture; never kills or retries."""
from pathlib import Path
import json,subprocess,sys,time,datetime,ctypes
from ctypes import wintypes
base=Path(__file__).resolve().parent;repo=base.parents[2]
sys.path.insert(0,str(repo/'tools/retrieval-train'))
from native_goal_client import windows_process_identity,digest
work=base/'native-goal-private-anchor-r7/work';ready_path=work/'ready.json';ready=json.loads(ready_path.read_bytes());expected=ready['health'];pair=repo/'artifacts/advanced-split/runtime-debug-r2';out=base/'native-a6-r7-shutdown-r1';out.mkdir()
assert digest(ready_path)=='c78c7a28a8e1dbf69d43724458876be4cfe3c21a17b590ce428de402ee542810'
assert digest(pair/'airkctl.exe')=='53fb8f49f2190ba6b497ab58a50d9fccc8b38d1905768723457c48d4bce5552a'
start=time.monotonic();report={'complete':False,'started_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'ready_sha256':digest(ready_path),'actual_process_before':windows_process_identity(expected['process_id'])}
save=lambda:(out/'shutdown-observation.json').write_text(json.dumps(report,indent=2)+'\n')
save();assert report['actual_process_before']['executable_sha256']==expected['executable_sha256']=='3d56d2afe6cf1afd30a0db7c7ce8f8d6a0205464d027bc9fbd99a0d7fb8adaac'
command=[str(pair/'airkctl.exe'),'--service-runtime-dir',ready['control_root'],'service','health']
try:
 p=subprocess.run(command,capture_output=True,timeout=30);(out/'health-before-stdout.bin').write_bytes(p.stdout);(out/'health-before-stderr.bin').write_bytes(p.stderr);p.check_returncode();before=json.loads(p.stdout);a=before['observation'];assert all(a[k]==expected[k] for k in ('instance','process_id','executable_sha256'));assert a['catalog_clock']['namespace']==expected['catalog_clock']['namespace'];report['actual_health_before']=before;save()
 k=ctypes.WinDLL('kernel32',use_last_error=True);k.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD];k.OpenProcess.restype=wintypes.HANDLE;k.WaitForSingleObject.argtypes=[wintypes.HANDLE,wintypes.DWORD];k.CloseHandle.argtypes=[wintypes.HANDLE];h=k.OpenProcess(0x1000|0x100000,False,expected['process_id']);assert h
 try:
  command[-1]='shutdown';report['command']=command;save();p=subprocess.run(command,capture_output=True,timeout=30);(out/'shutdown-stdout.bin').write_bytes(p.stdout);(out/'shutdown-stderr.bin').write_bytes(p.stderr);report['shutdown_returncode']=p.returncode;save();p.check_returncode();report['actual_shutdown_response']=json.loads(p.stdout);save();report['process_exit_wait_result']=k.WaitForSingleObject(h,60000);save();assert report['process_exit_wait_result']==0
 finally:k.CloseHandle(h)
 closed=work/'closed.json';report['native_closed']=json.loads(closed.read_bytes());report['native_closed_sha256']=digest(closed);save();assert report['native_closed']['instance']==expected['instance'] and report['native_closed']['shutdown_completed'] is True and report['native_closed']['service_lease_reacquired'] is True
 report['complete']=True
except Exception as error:report['error']=str(error);raise
finally:report['elapsed_seconds']=time.monotonic()-start;report['finished_at_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps({'complete':True,'closed_sha256':report['native_closed_sha256'],'elapsed_seconds':report['elapsed_seconds']}))
