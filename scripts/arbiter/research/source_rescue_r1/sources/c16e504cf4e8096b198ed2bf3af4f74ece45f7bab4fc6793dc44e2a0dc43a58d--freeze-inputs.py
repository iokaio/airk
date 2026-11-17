# SPDX-License-Identifier: Apache-2.0
"""Read-only proposal freeze generator. Never invokes a compiler or writes files."""
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json, os, pathlib, subprocess
U=pathlib.Path(r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z')
OLD=U/'cu8'; B=OLD/'b/Release'; OUT=U/'gpu-finalgraph-core-producer-r1'
NINJA=pathlib.Path(r'D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r3\tools\ninja-build\ninja.exe')
VC=pathlib.Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat')
TOOLS=pathlib.Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64')
VSWHERE=pathlib.Path(r'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe')
def pin(p):
 p=pathlib.Path(p).resolve(); h=hashlib.sha256()
 with p.open('rb') as f:
  while chunk:=f.read(4<<20): h.update(chunk)
 return dict(path=str(p),bytes=p.stat().st_size,sha256=h.hexdigest())
def query(*args):
 return subprocess.run([str(NINJA),'-C',str(B),'-t',*args],capture_output=True,text=True,timeout=30,check=True).stdout
def argv_of(command):
 shell=c.WinDLL(r'C:\Windows\System32\shell32.dll'); kernel=c.WinDLL(r'C:\Windows\System32\kernel32.dll')
 shell.CommandLineToArgvW.argtypes=[w.LPCWSTR,c.POINTER(c.c_int)]; shell.CommandLineToArgvW.restype=c.POINTER(w.LPWSTR)
 kernel.LocalFree.argtypes=[c.c_void_p]; kernel.LocalFree.restype=c.c_void_p
 count=c.c_int(); data=shell.CommandLineToArgvW(command,c.byref(count)); result=[data[i] for i in range(count.value)]; kernel.LocalFree(data); return result
line=next(x for x in (B/'build.ninja').read_text().splitlines() if x.startswith('build ') and 'onnxruntime_c_api.cc.obj:' in x)
obj=line[6:].split(': ',1)[0].replace('\\','/'); source=OLD/'source/onnxruntime/core/session/onnxruntime_c_api.cc'
deps=query('deps',obj).splitlines(); assert '#deps 482,' in deps[0] and '(VALID)' in deps[0]
headers=[(B/pathlib.Path(s.strip())).resolve() for s in deps[1:] if s.strip()]; assert len(headers)==482
q=query('query','onnxruntime_session.lib'); objects=[(B/p.strip()).resolve() for p in q.split('  input:',1)[1].split('  outputs:',1)[0].splitlines()[1:] if p.strip().endswith('.obj')]; assert len(objects)==40
command=next(x for x in query('commands',obj).splitlines() if 'onnxruntime_c_api.cc' in x); argv=argv_of(command)
newsource=OUT/'source/onnxruntime_c_api.cc'; newobj=OUT/'objects/onnxruntime_c_api.cc.obj'; newpdb=OUT/'objects/onnxruntime_session.pdb'; session=OUT/'session/onnxruntime_session.lib'; aggregate=OUT/'aggregate/onnxruntime.lib'
assert sum(x.startswith('/Fo') for x in argv)==sum(x.startswith('/Fd') for x in argv)==1
newargv=[str(TOOLS/'cl.exe')]+[('/Fo'+str(newobj) if x.startswith('/Fo') else '/Fd'+str(newpdb) if x.startswith('/Fd') else str(newsource) if pathlib.Path(x)==source else x) for x in argv[1:]]; assert str(newsource) in newargv
newargv.insert(next(i for i,x in enumerate(newargv) if x.startswith(('-I','/I'))),'/I'+str(source.parent))
cmd='"C:\\Windows\\System32\\cmd.exe" /d /s /c "call "'+str(VC)+'" 10.0.26100.0 -vcvars_ver=14.44.35207 >nul && set"'
seed=os.environ.copy(); seed['PATH']=str(VSWHERE.parent)+';'+seed.get('PATH','')
r=subprocess.run(cmd,env=seed,capture_output=True,text=True,timeout=30,check=True); assert not r.stderr,r.stderr
allenv={k.upper():v for line in r.stdout.splitlines() if '=' in line for k,v in [line.split('=',1)]}; assert not allenv.get('CL') and not allenv.get('_CL_')
keys=['PATH','INCLUDE','LIB','LIBPATH','VCTOOLSINSTALLDIR','VCTOOLSVERSION','WINDOWSSDKDIR','WINDOWSSDKVERSION','WINDOWSSDKLIBVERSION','UCRTVERSION','UNIVERSALCRTSDKDIR','VSINSTALLDIR','VCINSTALLDIR','VSCMD_ARG_HOST_ARCH','VSCMD_ARG_TGT_ARCH','SYSTEMROOT','WINDIR','COMSPEC','PROCESSOR_ARCHITECTURE']
env={k:allenv[k] for k in keys if k in allenv}; env.update(TEMP=str(OUT/'tmp'),TMP=str(OUT/'tmp'),VSLANG='1033',CL='',_CL_='')
producer=pin(U/'gpu-first-fp32-nn0-preparation-r1/runtime/gpu-checkpointed-build-receipt.json'); assert producer['sha256']=='f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178'
a=json.loads((OLD/'gpu-static-aggregate-receipt.json').read_bytes()); libs=[pin(B/x) for x in a['plan']['graph']['static_inputs']]; assert len(libs)==96
oldrows={str((OLD/x['path']).resolve()):x for x in json.loads(pathlib.Path(producer['path']).read_bytes())['libraries']}; assert all(x['sha256']==oldrows[x['path']]['sha256'] and x['bytes']==oldrows[x['path']]['bytes'] for x in libs)
sessioninputs=[str(newobj) if p==(B/obj).resolve() else str(p) for p in objects]; assert sessioninputs.count(str(newobj))==1
aggregateinputs=[str(session) if pathlib.Path(x['path'])==B/'onnxruntime_session.lib' else x['path'] for x in libs]; assert aggregateinputs.count(str(session))==1
compilerfiles=sorted({p for p in TOOLS.glob('*') if p.suffix.lower() in ['.exe','.dll']}|{p for p in (TOOLS/'1033').glob('*.dll')})
result={'schema':'airk.private.finalgraph-incremental-recipe-proposal/1','execution_authorized':False,'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'fresh_output':str(OUT),'source_patch':None,'new_source_sha256':None,'new_source_bytes':None,'approved_new_dependencies':[],
'old_producer':producer,'old_aggregate':pin(OLD/'gpu-aggregate/onnxruntime.lib'),'old_aggregate_receipt':pin(OLD/'gpu-static-aggregate-receipt.json'),'base_source':pin(source),'ninja':pin(NINJA),'ninja_graph':pin(B/'build.ninja'),'ninja_rules':pin(B/'CMakeFiles/rules.ninja'),'cmake_cache':pin(B/'CMakeCache.txt'),'vcvars':pin(VC),'vswhere':pin(VSWHERE),'environment_observation_command':cmd,'environment_query_stderr':'','original_compile_command':command,'original_compile_argv':argv,'compiler_dependency_state':deps[0],
'compiler_dependencies':[pin(p) for p in headers],'session_objects':[pin(p) for p in objects],'aggregate_libraries':libs,'provider_dlls':[pin(B/n) for n in ['onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']],'compiler_tool_files':[pin(p) for p in compilerfiles],'environment':env,
'commands':{'compile':newargv,'session':[str(TOOLS/'lib.exe'),'/nologo','/machine:x64','/OUT:'+str(session),'@'+str(OUT/'session-inputs.rsp')],'aggregate':[str(TOOLS/'lib.exe'),'/NOLOGO','/OUT:'+str(aggregate),'@'+str(OUT/'aggregate-inputs.rsp')],'object_symbols':[str(TOOLS/'dumpbin.exe'),'/symbols',str(newobj)],'aggregate_directives':[str(TOOLS/'dumpbin.exe'),'/directives',str(aggregate)]},
'session_input_paths':sessioninputs,'aggregate_input_paths':aggregateinputs,'showincludes':{'cwd':str(OUT),'language':'1033','prefix':'Note: including file:','required_dependency_roster':'compiler_dependencies minus original TU, plus approved_new_dependencies','relocated_quote_search_parent':str(source.parent),'before_original_include_roots':True,'unlisted_dependencies_refuse':True},
'required_exports':['airk_ort_collect_final_placement_v1','airk_ort_collect_final_graph_metadata_v1'],
'required_actual_receipt_fields':['exact_commands','effective_user_sid','default_owner_sid','interactive_user_sid','started_utc','finished_utc','elapsed_seconds','process_tree_settlement','new_source_pin','new_object_pin','new_session_pin','new_aggregate_pin','actual_showincludes','input_pre_post_pins','output_owner_acl_audits','symbol_export_checks','qualification_false']}
print(json.dumps(result))
