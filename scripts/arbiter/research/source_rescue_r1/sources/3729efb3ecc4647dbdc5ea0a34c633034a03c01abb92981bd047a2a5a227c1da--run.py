# SPDX-License-Identifier: Apache-2.0
import pathlib,sys,importlib.util,json,hashlib,ctypes,time,traceback,os,shutil
from ctypes import wintypes as w
root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('actual_workbench_gate',root/'scripts/check-workbench-window-scope.py');gate=importlib.util.module_from_spec(spec);spec.loader.exec_module(gate)
runtime=pathlib.Path('D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-runtime-debug-r1');node=pathlib.Path('D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe')
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

import subprocess
clients=wrapped_module('workbench_native_clients',root/'scripts/check-service-clients.py')
def seal(path):return {'bytes':path.stat().st_size,'sha256':gate.sha256(path)}
inputs={str(p.relative_to(root)):seal(p) for directory in ['packages/admin/src','crates/airk-cli/admin-assets','packages/ui/src','packages/contracts-gen'] for p in (root/directory).rglob('*') if p.is_file()}
for p in [root/'scripts/check-service-clients.py',root/'scripts/check-workbench-window-scope.py',root/'packages/ui/test/browserHarness.mjs']:inputs[str(p.relative_to(root))]=seal(p)
artifacts={p.name:seal(p) for p in runtime.iterdir() if p.is_file()}
result={'schema':'airk.private.embedded-admin-execution/1','argv':[sys.executable,'-B',str(__file__)],'candidate_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_inputs':inputs,'runtime':str(runtime),'runtime_inputs':artifacts,'private_sources':{p.name:seal(p) for p in out.iterdir() if p.suffix in ['.mjs','.py']},'passed':False};started=time.monotonic()
try:
 metadata=json.loads((runtime/'airk-runtime.json').read_bytes());build=json.loads((runtime/'airk-build.json').read_bytes());package=original_module('admin_runtime_package',root/'scripts/package-native.py');package.validate_runtime(runtime,metadata['version'],'x86_64-pc-windows-msvc',build['source'])
 with clients.owned_client_fixture() as directory:
  owner=pathlib.Path(directory).resolve(strict=True)
  with clients.Fixture(runtime,owner) as fixture:
   endpoint=fixture.http_endpoint()
   launch=json.loads(fixture.run('--output','json','admin','url','--runtime-dir',fixture.control).stdout)
   request={'url':launch['url'],'endpoint':endpoint,'instance':fixture.managed_health['instance'],'assets':{p.name:seal(p) for p in (root/'crates/airk-cli/admin-assets').iterdir() if p.is_file()}}
   request_path=out/'request.json';request_path.write_text(json.dumps(request)+'\n',encoding='utf-8')
   temp=out/'browser-temp';temp.mkdir();env={**fixture.environment,'TEMP':str(temp),'TMP':str(temp),'AIRK_BROWSER':r'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'}
   command=[str(node),str(out/'admin-live.mjs')];result['browser_argv']=command
   completed=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True,timeout=240,creationflags=subprocess.CREATE_NO_WINDOW)
   (out/'browser.stdout.log').write_text(completed.stdout,encoding='utf-8');(out/'browser.stderr.log').write_text(completed.stderr,encoding='utf-8');print(completed.stdout,end='')
   assert completed.returncode==0,completed.stderr[-8192:]
   result['browser_receipt']=json.loads((out/'browser-result.json').read_bytes())
   assert result['browser_receipt']['passed']
   # One-use native launch code no longer needed; retain only route-safe metadata.
   request_path.unlink()
 assert inputs=={name:seal(root/name) for name in inputs},'Source inputs changed during run'
 assert artifacts=={name:seal(runtime/name) for name in artifacts},'Immutable native pair changed during run'
 result['passed']=True
except BaseException as error:
 result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:
 result.update(elapsed_seconds=time.monotonic()-started,native_processes=witness);(out/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
