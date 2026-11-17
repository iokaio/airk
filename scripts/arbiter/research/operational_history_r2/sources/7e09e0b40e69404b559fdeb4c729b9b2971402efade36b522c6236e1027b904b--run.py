# SPDX-License-Identifier: Apache-2.0
"""Root-only exact configure diagnostic. No ORT/Ninja build or public mutation."""
from pathlib import Path
import importlib.util,inspect,json,os,subprocess,sys,hashlib
root=Path.cwd();out=root/'artifacts/advanced-split/msvc-configure-owner-context-r4'
assert not (out/'receipt.json').exists() and not (out/'ninja-build').exists()
source=root/'scripts/provision-static-onnx-runtime.py'
sp=importlib.util.spec_from_file_location('owned_diag',source);m=importlib.util.module_from_spec(sp);sp.loader.exec_module(m)
src=(root/'artifacts/advanced-split/msvc-configure-owned-diagnostic-r1/instrumented-run-windows-owned.py').read_text(encoding='utf-8')
# Instrumentation only queries images on held handles after exact job membership;
# same5s cleanup / forced flags / finalempty proof as production helper.
base=inspect.getsource(m.run_windows_owned)
expected=base.replace('emergency_members = []','emergency_members = []\n    observed_members = []\n    api.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)]\n    api.QueryFullProcessImageNameW.restype=w.BOOL')
expected=expected.replace('retained.append(handle)','''image=ctypes.create_unicode_buffer(32768);image_size=w.DWORD(len(image))
                        image_ok=api.QueryFullProcessImageNameW(handle,0,image,ctypes.byref(image_size))
                        observed_members.append({"pid":pid,"image":image.value if image_ok else None,"image_error":0 if image_ok else ctypes.get_last_error(),"cleanup_remaining_seconds":round(max(0,cleanup_end-time.monotonic()),6)})
                        retained.append(handle)''')
expected=expected.replace('"owned_job_final_active_processes": active,','"owned_job_observed_member_images": observed_members,\n                  "owned_job_final_active_processes": active,')
assert src==expected,'Production helper changed since exact diagnostic instrumentation; review before running'
exec(compile(src,str(out/'instrumented.py'),'exec'),m.__dict__)
a=json.loads((root/'artifacts/advanced-split/ort130-x64-owned-r2/tools/attempt.json').read_bytes());argv=a['configure_argv'];argv[argv.index('-B')+1]=str(out/'ninja-build')
vc=a['windows_tools'];cmd=out/'configure.cmd'
cmd.write_text('@echo off\r\ncall "'+vc['vcvars']['path']+'" '+vc['windows_sdk']+' -vcvars_ver='+vc['msvc_toolset']+' > "'+str(out/'toolchain.log')+'" 2>&1\r\nif errorlevel 1 exit /b %errorlevel%\r\n'+subprocess.list2cmdline(argv)+'\r\nexit /b %errorlevel%\r\n',encoding='utf-8',newline='')
environment=dict(os.environ)
for key in list(environment):
 if key.startswith('CMAKE_') or key in {'CC','CXX','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','PYTHONPATH','PYTHONHOME'}:environment.pop(key)
environment['PYTHONNOUSERSITE']='1';environment['PATH']=str(Path(sys.executable).resolve().parent)+os.pathsep+environment.get('PATH','')
r=m.run_owned([str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(cmd)],out,out/'configure.log',30,environment)
receipt={'diagnostic_only':True,'production_helper_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'observer_sha256':hashlib.sha256(src.encode()).hexdigest(),'configure_argv':argv,'environment_policy':'Identical tools.provision_tools sanitizer; no ambient values emitted','environment_presence':{k:k in environment for k in ['CL','_CL_','VSCMD_SKIP_SENDTELEMETRY','_MSPDBSRV_ENDPOINT_']},'result':r}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps({'receipt':str(out/'receipt.json'),'result':r}))
