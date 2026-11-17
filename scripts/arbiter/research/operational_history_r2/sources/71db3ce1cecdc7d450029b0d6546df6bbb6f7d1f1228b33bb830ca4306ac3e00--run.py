# SPDX-License-Identifier: Apache-2.0
"""Root-only exact configure diagnostic. No ORT/Ninja build or public mutation."""
from pathlib import Path
import importlib.util,inspect,json,os,subprocess,sys,hashlib
root=Path.cwd();out=root/'artifacts/advanced-split/msvc-configure-owner-context-r10'
assert not (out/'receipt.json').exists() and not (out/'ninja-build').exists()
source=root/'scripts/provision-static-onnx-runtime.py'
sp=importlib.util.spec_from_file_location('owned_diag',source);m=importlib.util.module_from_spec(sp);sp.loader.exec_module(m)
a=json.loads((root/'artifacts/advanced-split/ort130-x64-owned-r4/tools/attempt.json').read_bytes());argv=a['configure_argv'];argv[argv.index('-B')+1]=str(out/'ninja-build');argv=[('-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT=' if x.startswith('-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT=') else x) for x in argv]
vc=a['windows_tools'];cmd=out/'configure.cmd'
cmd.write_text('@echo off\r\ncall "'+vc['vcvars']['path']+'" '+vc['windows_sdk']+' -vcvars_ver='+vc['msvc_toolset']+' > "'+str(out/'toolchain.log')+'" 2>&1\r\nif errorlevel 1 exit /b %errorlevel%\r\n'+subprocess.list2cmdline(argv)+'\r\nexit /b %errorlevel%\r\n',encoding='utf-8',newline='')
environment=dict(os.environ)
for key in list(environment):
 if key.startswith('CMAKE_') or key in {'CC','CXX','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','PYTHONPATH','PYTHONHOME'}:environment.pop(key)
environment['PYTHONNOUSERSITE']='1';environment['PATH']=str(Path(sys.executable).resolve().parent)+os.pathsep+environment.get('PATH','')
r=m.run_owned([str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(cmd)],out,out/'configure.log',30,environment)
receipt={'diagnostic_only':True,'production_helper_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'configure_argv':argv,'environment_policy':'Identical tools.provision_tools sanitizer; no ambient values emitted','environment_presence':{k:k in environment for k in ['CL','_CL_','VSCMD_SKIP_SENDTELEMETRY','_MSPDBSRV_ENDPOINT_']},'result':r}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps({'receipt':str(out/'receipt.json'),'result':r}))
