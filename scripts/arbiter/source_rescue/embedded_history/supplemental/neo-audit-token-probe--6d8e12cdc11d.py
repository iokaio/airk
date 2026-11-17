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
sdk=Path(subprocess.check_output(['/usr/bin/xcrun','--sdk','macosx','--show-sdk-path'],text=True).strip())
lines=(sdk/'usr/include/libproc.h').read_text(errors='replace').splitlines()
report={'schema':1,'host':'192.168.68.61','uid':501,'mach_control_port_requested':False,'existing_AIRK_processes_touched':False,'compiler_executed':False,'sdk_header_context':[str(i+1)+':'+v for i,v in enumerate(lines) if 112<=i<=138]}
child=subprocess.Popen(['/usr/bin/sleep','60'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);port=ctypes.c_uint32();token=(ctypes.c_uint32*8)();count=ctypes.c_uint32(8)
try:
 r=lib.task_name_for_pid(selfport,child.pid,ctypes.byref(port));report['task_name_status']=r;assert r==0 and port.value
 r=lib.task_info(port.value,15,token,ctypes.byref(count));report['audit_token_status']=r;report['audit_token_count']=count.value;assert r==0 and count.value==8
 assert token[1]==501 and token[3]==501 and token[5]==child.pid and token[7]!=0
 report['identity']={'pid':token[5],'effective_uid':token[1],'real_uid':token[3],'pidversion':token[7]}
 path=ctypes.create_string_buffer(4096);ctypes.set_errno(0);n=proc.proc_pidpath_audittoken(token,path,len(path));assert n>0 and path.value==b'/usr/bin/sleep';report['atomic_path']=path.value.decode()
 wrong=(ctypes.c_uint32*8)(*token);wrong[7]^=1
 r=proc.proc_signal_with_audittoken(wrong,15);report['crossed_version_signal_status']=r;report['child_alive_after_crossed_token']=child.poll() is None;assert r==errno.ESRCH and child.poll() is None
 r=proc.proc_signal_with_audittoken(token,15);report['exact_token_signal_status']=r;assert r==0
 report['owned_child_exit']=child.wait(timeout=5);assert report['owned_child_exit']==-15
 ctypes.set_errno(0);n=proc.proc_pidpath_audittoken(token,path,len(path));report['old_token_path_bytes_after_exit']=n;report['old_token_path_errno_after_exit']=ctypes.get_errno();assert n==0 and ctypes.get_errno()==errno.ESRCH
 report['result']='passed'
except BaseException as error:
 report['result']='failed';report['error']=repr(error)
finally:
 if port.value:report['mach_name_deallocate_status']=lib.mach_port_deallocate(selfport,port.value)
 if child.poll() is None:
  report['probe_cleanup_fallback']=True;child.terminate()
  try:child.wait(timeout=3)
  except subprocess.TimeoutExpired:child.kill();child.wait(timeout=3)
 report['owned_child_settled']=child.poll() is not None
print(json.dumps(report))
if report['result']!='passed':raise SystemExit(1)
