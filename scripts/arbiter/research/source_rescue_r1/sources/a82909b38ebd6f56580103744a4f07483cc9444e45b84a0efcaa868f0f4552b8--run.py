# SPDX-License-Identifier: Apache-2.0
import pathlib,sys,importlib.util,json,hashlib,ctypes,time,traceback,os,shutil
from ctypes import wintypes as w
root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent
from frozen_binding import verify,settle
binding_before=verify()
assert not (out/"execution.json").exists(), "Fresh run required"
spec=importlib.util.spec_from_file_location('actual_workbench_gate',root/'scripts/check-workbench-window-scope.py');gate=importlib.util.module_from_spec(spec);spec.loader.exec_module(gate)
runtime=pathlib.Path('D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-runtime-debug-r2');node=pathlib.Path('D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe')
assert gate.subprocess.check_output([str(node),'--version'],text=True).strip()=='v24.15.0'
k=ctypes.WinDLL('kernel32',use_last_error=True);k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE;k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.GetExitCodeProcess.argtypes=[w.HANDLE,ctypes.POINTER(w.DWORD)];k.CloseHandle.argtypes=[w.HANDLE];k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)]
witness=[];original_module=gate.module
class_marker={}
def wrapped_module(name,path):
 m=original_module(name,path)
 if name=='workbench_native_clients':
  base=m.Fixture
  class ObservedFixture(base):
   def __init__(self,*args,**kwargs):
    self.observed_handle=None;self.observation=None;super().__init__(*args,**kwargs)
   def __enter__(self):
    value=super().__enter__();health=self.managed_health or self.rpc('health')['observation'];pid=health['process_id'];handle=k.OpenProcess(0x100000|0x1000,False,pid)
    if not handle:raise ctypes.WinError(ctypes.get_last_error())
    self.observed_handle=handle
    buf=ctypes.create_unicode_buffer(32768);size=w.DWORD(len(buf))
    if not k.QueryFullProcessImageNameW(handle,0,buf,ctypes.byref(size)):raise ctypes.WinError(ctypes.get_last_error())
    assert pathlib.Path(buf.value).samefile(self.binary),'actual native PID image differs'
    self.observation={'fixture':str(self.owner),'health':health,'pid':pid,'actual_image':buf.value,'image_sha256':gate.sha256(self.binary),'original_handle_retained_before_reads':True,'native_stop_complete':False,'kernel_exit_observed':False}
    witness.append(self.observation)
    return value
   def close(self):
    try:
     result=super().close()
     if self.observed_handle is not None:
      status=k.WaitForSingleObject(self.observed_handle,0);code=w.DWORD()
      if not k.GetExitCodeProcess(self.observed_handle,ctypes.byref(code)):raise ctypes.WinError(ctypes.get_last_error())
      self.observation.update(native_stop=self.managed_stop,native_stop_complete=self.managed_stop is not None and self.managed_stop['record']['phase']=='complete',kernel_exit_observed=status==0,actual_exit_code=code.value)
      for name in ['managed-start.json','managed-stop-request.json','managed-stop-result.json','workbench-observations.json','workbench-observations.json.failure.json']:
       source=self.owner/name
       if source.is_file():shutil.copyfile(source,out/name)
      assert status==0 and code.value==0,'original native process did not exit naturally with zero status'
     return result
    finally:
     if self.observed_handle is not None:k.CloseHandle(self.observed_handle);self.observed_handle=None
     if self.observation is not None:(out/'native-settlement.json').write_text(json.dumps(witness,indent=2)+'\n',encoding='utf-8')
  m.Fixture=ObservedFixture
 return m
gate.module=wrapped_module
started=time.monotonic();result={'argv':[sys.executable,'-B',str(__file__)],'gate':str(root/'scripts/check-workbench-window-scope.py'),'gate_sha256':gate.sha256(root/'scripts/check-workbench-window-scope.py'),'wrapper_sha256':gate.sha256(__file__),'runtime':str(runtime),'node':str(node),'candidate_head':gate.subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'passed':False}
try:
 gate.qualify(runtime,node,out/'workbench-receipt.json','x86_64-pc-windows-msvc');result['passed']=True
except BaseException as error:
 result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:
 binding_ok=settle(result);result.update(binding_before=binding_before,elapsed_seconds=time.monotonic()-started,native_processes=witness);(out/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
assert binding_ok, "Frozen Workbench inputs changed; execution receipt remains nonpassing"
