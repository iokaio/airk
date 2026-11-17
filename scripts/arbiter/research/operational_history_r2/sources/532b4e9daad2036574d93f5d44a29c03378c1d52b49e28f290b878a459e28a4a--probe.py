# SPDX-License-Identifier: Apache-2.0
import ctypes,errno,json,os,platform,socket,subprocess,time
from pathlib import Path
assert os.getuid()==501 and platform.machine()=='arm64' and socket.gethostname()=='Tylers-MacBook-Neo.local'
lib=ctypes.CDLL('/usr/lib/libSystem.B.dylib',use_errno=True);proc=ctypes.CDLL('/usr/lib/libproc.dylib',use_errno=True)
selfport=ctypes.c_uint32.in_dll(lib,'mach_task_self_').value
lib.task_name_for_pid.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.POINTER(ctypes.c_uint32)];lib.task_name_for_pid.restype=ctypes.c_int
lib.task_info.argtypes=[ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint32)];lib.task_info.restype=ctypes.c_int
lib.mach_port_deallocate.argtypes=[ctypes.c_uint32,ctypes.c_uint32];lib.mach_port_deallocate.restype=ctypes.c_int
proc.proc_pidpath_audittoken.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_uint32];proc.proc_pidpath_audittoken.restype=ctypes.c_int
proc.proc_signal_with_audittoken.argtypes=[ctypes.c_void_p,ctypes.c_int];proc.proc_signal_with_audittoken.restype=ctypes.c_int

report={'schema':1,'host':'192.168.68.61','purpose':'samePID exec oldtoken rejection','existing_AIRK_processes_touched':False,'compiler_executed':False}
child=subprocess.Popen(['/bin/sh','-c','read ready; exec /bin/sleep 60'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);port=ctypes.c_uint32();replacement_port=ctypes.c_uint32();token=(ctypes.c_uint32*8)();count=ctypes.c_uint32(8)
try:
 assert lib.task_name_for_pid(selfport,child.pid,ctypes.byref(port))==0
 assert lib.task_info(port.value,15,token,ctypes.byref(count))==0 and count.value==8
 path=ctypes.create_string_buffer(4096);assert proc.proc_pidpath_audittoken(token,path,len(path))>0 and path.value==b'/bin/sh'
 report['original']={'pid':child.pid,'pidversion':token[7],'path':path.value.decode()}
 child.stdin.write(b'ready\n');child.stdin.flush();child.stdin.close()
 end=time.monotonic()+3
 while True:
  ctypes.set_errno(0);n=proc.proc_pidpath_audittoken(token,path,len(path));error=ctypes.get_errno()
  if n==0 and error==errno.ESRCH:break
  assert time.monotonic()<end,'Oldtoken remained valid across samePIDexec'
  time.sleep(.01)
 assert child.poll() is None
 assert lib.task_name_for_pid(selfport,child.pid,ctypes.byref(replacement_port))==0
 current=(ctypes.c_uint32*8)();count=ctypes.c_uint32(8);assert lib.task_info(replacement_port.value,15,current,ctypes.byref(count))==0
 assert current[5]==token[5] and current[7]!=token[7]
 assert proc.proc_pidpath_audittoken(current,path,len(path))>0 and path.value==b'/bin/sleep'
 report['replacement']={'pid':current[5],'pidversion':current[7],'path':path.value.decode()}
 r=proc.proc_signal_with_audittoken(token,15);report['old_token_signal_status']=r;report['replacement_alive_after_old_token_signal']=child.poll() is None
 assert r==errno.ESRCH and child.poll() is None
 report['result']='passed'
except BaseException as error:
 report['result']='failed';report['error']=repr(error)
finally:
 for held in [port,replacement_port]:
  if held.value:assert lib.mach_port_deallocate(selfport,held.value)==0
 if child.poll() is None:
  child.terminate()
  try:child.wait(timeout=3)
  except subprocess.TimeoutExpired:child.kill();child.wait(timeout=3)
 report['owned_child_settled']=child.poll() is not None
 report['cleanup_exit']=child.returncode
print(json.dumps(report))
if report['result']!='passed':raise SystemExit(1)
