# SPDX-License-Identifier: Apache-2.0
"""Root-only exact configure diagnostic. No ORT/Ninja build or public mutation."""
from pathlib import Path
import importlib.util,inspect,json,os,subprocess,sys,hashlib
root=Path.cwd();out=root/'artifacts/advanced-split/msvc-configure-owner-context-r8'
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
a=json.loads((root/'artifacts/advanced-split/ort130-x64-owned-r2/tools/attempt.json').read_bytes());argv=a['configure_argv'];argv[argv.index('-B')+1]=str(out/'ninja-build');argv=[('-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT=' if x.startswith('-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT=') else x) for x in argv];argv.append('-DCMAKE_TRY_COMPILE_CONFIGURATION=Release')
vc=a['windows_tools'];cmd=out/'configure.cmd'
cmd.write_text('@echo off\r\ncall "'+vc['vcvars']['path']+'" '+vc['windows_sdk']+' -vcvars_ver='+vc['msvc_toolset']+' > "'+str(out/'toolchain.log')+'" 2>&1\r\nif errorlevel 1 exit /b %errorlevel%\r\n'+subprocess.list2cmdline(argv)+'\r\nexit /b %errorlevel%\r\n',encoding='utf-8',newline='')
environment=dict(os.environ)
for key in list(environment):
 if key.startswith('CMAKE_') or key in {'CC','CXX','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','PYTHONPATH','PYTHONHOME'}:environment.pop(key)
environment['PYTHONNOUSERSITE']='1';environment['PATH']=str(Path(sys.executable).resolve().parent)+os.pathsep+environment.get('PATH','')
r=m.run_owned([str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(cmd)],out,out/'configure.log',30,environment)
receipt={'diagnostic_only':True,'production_helper_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'observer_sha256':hashlib.sha256(src.encode()).hexdigest(),'configure_argv':argv,'environment_policy':'Identical tools.provision_tools sanitizer; no ambient values emitted','environment_presence':{k:k in environment for k in ['CL','_CL_','VSCMD_SKIP_SENDTELEMETRY','_MSPDBSRV_ENDPOINT_']},'result':r}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps({'receipt':str(out/'receipt.json'),'result':r}))

# Comparative diagnostic: .debug$S can contain minimal compiler metadata without
# /Z7 or /Zi. Do not infer embedded source/type data from that section name.
assert r['exit_code']==0 and r['owned_descendants_settled'] and not r['owned_job_forced_termination']
import struct
witness=out/'coff-witness';witness.mkdir()
(witness/'CMakeLists.txt').write_text('cmake_minimum_required(VERSION 3.31)\nproject(coff_witness LANGUAGES CXX)\nadd_library(coff_witness OBJECT witness.cc)\n',encoding='utf-8')
(witness/'witness.cc').write_text('int airk_release_coff_witness(int x) { return x + 7; }\n',encoding='utf-8')
cmake=argv[0];flags=[x for x in argv if x.startswith(('-DCMAKE_POLICY_DEFAULT_CMP0141=','-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT=','-DCMAKE_POLICY_DEFAULT_CMP0137=','-DCMAKE_TRY_COMPILE_CONFIGURATION=','-DCMAKE_MAKE_PROGRAM='))]
receipt['coff_comparison']={}
for mode,value in [('empty',''),('embedded','Embedded')]:
 selected=[('-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT='+value if flag.startswith('-DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT=') else flag) for flag in flags]
 build=witness/mode
 commands=[[cmake,'-S',str(witness),'-B',str(build),'-G','NMake Makefiles','-DCMAKE_BUILD_TYPE=Release',*selected],[cmake,'--build',str(build),'--verbose']]
 proof=[]
 for i,command in enumerate(commands):
  script=out/(mode+'-'+str(i)+'.cmd');script.write_text('@echo off\r\ncall "'+vc['vcvars']['path']+'" '+vc['windows_sdk']+' -vcvars_ver='+vc['msvc_toolset']+' > nul 2>&1\r\nif errorlevel 1 exit /b %errorlevel%\r\n'+subprocess.list2cmdline(command)+'\r\nexit /b %errorlevel%\r\n',encoding='utf-8',newline='')
  result=m.run_owned([str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(script)],out,out/(mode+'-'+str(i)+'.log'),30,environment)
  proof.append({'argv':command,'result':result})
  assert result['exit_code']==0 and result['owned_descendants_settled'] and not result['owned_job_forced_termination']
 obj=next(build.rglob('*.obj'));raw=obj.read_bytes();machine,count,_,_,_,optional,_=struct.unpack_from('<HHIIIHH',raw)
 assert machine==0x8664 and optional==0
 sections=[]
 for i in range(count):
  header=20+i*40;name=raw[header:header+8].rstrip(b'\0').decode('ascii');size,offset=struct.unpack_from('<II',raw,header+16)
  row={'name':name,'bytes':size}
  if name=='.debug$S':
   data=raw[offset:offset+size];assert struct.unpack_from('<I',data)[0]==4
   position=4;subsections=[]
   while position+8<=len(data):
    kind,length=struct.unpack_from('<II',data,position);assert position+8+length<=len(data)
    subsections.append({'kind':hex(kind),'bytes':length});position=(position+8+length+3)&~3
   row['codeview_subsections']=subsections
  sections.append(row)
 receipt['coff_comparison'][mode]={'commands':proof,'object_sha256':hashlib.sha256(raw).hexdigest(),'object_bytes':len(raw),'sections':sections}
 (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
empty=receipt['coff_comparison']['empty'];embedded=receipt['coff_comparison']['embedded']
assert not any(s['name']=='.debug$T' for s in empty['sections']),empty
assert any(s['name']=='.debug$T' for s in embedded['sections']),embedded
assert empty['object_bytes']<embedded['object_bytes'],(empty,embedded)
log=(out/'empty-1.log').read_text(encoding='utf-8',errors='replace')
assert '/Z7' not in log and '/Zi' not in log and '/ZI' not in log and '/DEBUG' not in log.upper(),log
assert '/Z7' in (out/'embedded-1.log').read_text(encoding='utf-8',errors='replace')
receipt['comparison_assertions_passed']=True
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'receipt':str(out/'receipt.json'),'empty_bytes':empty['object_bytes'],'embedded_bytes':embedded['object_bytes']}))
