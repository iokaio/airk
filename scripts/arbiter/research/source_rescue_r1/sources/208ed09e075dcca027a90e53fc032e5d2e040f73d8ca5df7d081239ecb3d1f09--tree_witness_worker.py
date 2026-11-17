# SPDX-License-Identifier: Apache-2.0
"""Only Root-owned event-gated process-tree witness; no sleep/foreign PID actions."""
import argparse,ctypes,json,os,re,subprocess,sys
from pathlib import Path
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--request',type=Path,required=True);ap.add_argument('--child',action='store_true');a=ap.parse_args();r=json.loads(a.request.read_bytes())
 k=ctypes.WinDLL('kernel32',use_last_error=True);k.OpenEventW.argtypes=[ctypes.c_uint32,ctypes.c_bool,ctypes.c_wchar_p];k.OpenEventW.restype=ctypes.c_void_p;k.SetEvent.argtypes=[ctypes.c_void_p];k.WaitForSingleObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32];k.CloseHandle.argtypes=[ctypes.c_void_p]
 def event(name):
  h=k.OpenEventW(0x100002,False,name)
  if not h:raise ctypes.WinError(ctypes.get_last_error())
  return h
 actual=subprocess.check_output(['C:/Windows/System32/whoami.exe','/user','/fo','csv','/nh'],text=True);sid=re.search(r'S-1-5-21-[0-9-]+',actual).group()
 if sid!='S-1-5-21-3678370870-3858642417-826968105-1002':raise ValueError('wrong witness execution SID')
 if a.child:
  ready=event(r['events']['ready']);release=event(r['events']['release'])
  try:
   with Path(r['child_marker']).open('xb') as f:f.write(json.dumps({'pid':os.getpid(),'sid':sid}).encode());f.flush();os.fsync(f.fileno())
   if not k.SetEvent(ready):raise ctypes.WinError(ctypes.get_last_error())
   result=k.WaitForSingleObject(release,10000)
   if result!=0:raise TimeoutError('event-gated child not released within original witness limit')
  finally:k.CloseHandle(ready);k.CloseHandle(release)
  return
 with Path(r['parent_marker']).open('xb') as f:f.write(json.dumps({'pid':os.getpid(),'sid':sid}).encode());f.flush();os.fsync(f.fileno())
 if r['mode']=='normal':return
 child=subprocess.Popen([sys.executable,'-S','-B',str(Path(__file__).resolve()),'--request',str(a.request),'--child'])
 gate=event(r['events']['parent_exit'])
 try:
  if k.WaitForSingleObject(gate,10000)!=0:raise TimeoutError('parent exit gate expired')
 finally:k.CloseHandle(gate)
 # Intentionally return without waiting for the still-owned child.
if __name__=='__main__':main()
