# SPDX-License-Identifier: Apache-2.0
"""Campaign-private CUDA static-core producer. No Cargo/model/install/publication.
Reviewed --execute-build only. Existing CPU recipe and receipts are never changed.
"""
import argparse,hashlib,importlib.util,json,os,re,shutil,stat,subprocess,sys,time,zipfile
from pathlib import Path
ROOT=Path('D:/code/Github/airk')
HERE=Path(__file__).resolve().parent
BASE=HERE.parent
INPUT=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-runtime-inputs-r1')
PARENT=INPUT.parent
CPU=ROOT/'artifacts/advanced-split/ort130-x64-owned-r5'
CUDA=Path('C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8')
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
LOCK=HERE/'cuda-runtime-producer-lock.json'

def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for b in iter(lambda:f.read(4*1024*1024),b''):h.update(b)
 return h.hexdigest()

def write(p,value):
 p.parent.mkdir(parents=True,exist_ok=True)
 with p.open('x',encoding='utf8') as f:json.dump(value,f,indent=2);f.write('\n')

def cpu_owner(lock):
 if digest(ROOT/'scripts/provision-static-onnx-runtime.py')!=lock['cpu_owner_sha256'] or digest(ROOT/'scripts/static-onnx-runtime-lock.json')!=lock['cpu_lock_sha256']:raise ValueError('canonical CPU owner drift')
 spec=importlib.util.spec_from_file_location('airk_cpu_owner',ROOT/'scripts/provision-static-onnx-runtime.py');o=importlib.util.module_from_spec(spec);spec.loader.exec_module(o);o.validate_lock();return o

def identity_and_parent(o,destination):
 # The caller additionally verifies active logged-in human before this process.
 result=subprocess.run(['C:/Windows/System32/whoami.exe','/user','/fo','csv','/nh'],check=True,capture_output=True,text=True,timeout=15)
 if SID not in result.stdout:raise ValueError('execution SID differs from reviewed human')
 o.plain(PARENT);o.plain(destination)
 if destination.parent!=PARENT or destination.exists():raise ValueError('fresh direct child of approved protected parent required')
 # Absolute constant parent enters fixed PowerShell code, never caller-built shell text.
 script="$a=Get-Acl -LiteralPath 'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z'; @{owner=$a.Owner;sddl=$a.Sddl}|ConvertTo-Json -Compress"
 r=subprocess.run(['pwsh','-NoProfile','-Command',script],check=True,capture_output=True,text=True,timeout=15);policy=json.loads(r.stdout)
 if policy['owner']!=('TYAI'+chr(92)+'tdotn'):raise ValueError('protected parent owner differs')
 # Exact reviewed policy, not a vague owner-only check. No ACL is changed.
 expected='O:'+SID+'G:'+SID+'D:AI(A;OICIID;0x1200a9;;;S-1-5-21-3678370870-3858642417-826968105-1011)(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)(A;OICIID;FA;;;'+SID+')'
 if policy['sddl']!=expected:raise ValueError('protected output-parent access policy drift')
 return policy

def gpu_command(o,dest,tools,frontend):
 cmd=o.configure_argv(dest,'x86_64-pc-windows-msvc',tools)
 if cmd.count('-Donnxruntime_USE_CUDA=OFF')!=1:raise ValueError('CPU configure contract drift')
 cmd[cmd.index('-Donnxruntime_USE_CUDA=OFF')]='-Donnxruntime_USE_CUDA=ON'
 cmd += ['-Donnxruntime_BUILD_CUDA_EP_AS_PLUGIN=OFF','-DCMAKE_CUDA_ARCHITECTURES=86','-DCMAKE_OBJECT_PATH_MAX=240','-DCMAKE_CUDA_COMPILER='+str(CUDA/'bin/nvcc.exe'),'-Donnxruntime_CUDA_HOME='+str(CUDA),'-DCUDAToolkit_ROOT='+str(CUDA),'-Donnxruntime_CUDNN_HOME='+str(INPUT/'cudnn-dev'),'-DCUDNN_PATH='+str(INPUT/'cudnn-dev'),'-DFETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND='+str(frontend),'-DFETCHCONTENT_UPDATES_DISCONNECTED=ON','-Donnxruntime_NVCC_THREADS=2','-Donnxruntime_USE_FLASH_ATTENTION=ON','-Donnxruntime_FLASH_NVCC_THREADS=1','-Donnxruntime_USE_MEMORY_EFFICIENT_ATTENTION=ON','-Donnxruntime_USE_TRT_FUSED_ATTENTION=OFF','-Donnxruntime_USE_LEAN_ATTENTION=OFF','-Donnxruntime_USE_FP4_QMOE=OFF','-Donnxruntime_USE_FP8_QMOE=OFF','-Donnxruntime_USE_FPA_INTB_GEMM=OFF','-Donnxruntime_USE_CUDA_NHWC_OPS=OFF','-Donnxruntime_CUDA_MINIMAL=OFF']
 return cmd

def safe_frontend(o,archive,output,lock):
 if output.exists():raise ValueError('fresh frontend source required')
 row=next(r for r in lock['dependencies'] if r['name']=='cudnn_frontend');o.verify_file(archive,row)
 entries=[];seen=set();expanded=0;skipped=[]
 with zipfile.ZipFile(archive) as z:
  for i in z.infolist():
   parts=o.relative_name(i.filename.rstrip('/')).parts
   if parts[0]!='cudnn-frontend-1.27.0':raise ValueError('foreign frontend root')
   rel='/'.join(parts[1:]);expanded+=i.file_size
   if i.filename.casefold() in seen or i.flag_bits&1:raise ValueError('duplicate/encrypted frontend member')
   seen.add(i.filename.casefold());mode=(i.external_attr>>16)&0o170000
   if mode==stat.S_IFLNK:
    b=z.read(i);ex=lock['excluded_link']
    if rel!=ex['path'] or len(b)!=ex['bytes'] or hashlib.sha256(b).hexdigest()!=ex['target_sha256']:raise ValueError('unreviewed frontend link')
    skipped.append(rel);continue
   if mode not in (0,stat.S_IFREG,stat.S_IFDIR):raise ValueError('frontend special member')
   entries.append((i,rel))
  if len(z.infolist())!=row['entries'] or expanded!=row['expanded_bytes'] or skipped!=['.claude/skills']:raise ValueError('frontend expansion census differs')
  output.mkdir(parents=True)
  for i,rel in entries:
   if not rel:continue
   p=o.child(output,rel)
   if i.is_dir():p.mkdir(parents=True,exist_ok=True);continue
   p.parent.mkdir(parents=True,exist_ok=True)
   with z.open(i) as f,p.open('xb') as g:shutil.copyfileobj(f,g,1024*1024)
   o.plain(p,regular=True)
 return {'excluded_inert_link':skipped,'raw_archive':row,'files_before_patches':o.source_inventory(output)}

def verify_input_closure(o,rows):
 seen=set()
 for row in rows:
  path=o.plain(row['path'],regular=True);key=os.path.normcase(str(path))
  if key in seen:raise ValueError('duplicate frozen input path')
  seen.add(key);o.verify_file(path,row)
 return len(seen)

def verify_mirror_closure(o,root,expected):
 if o.source_inventory(root)!=sorted(expected,key=lambda r:r['path']):raise ValueError('exact copied mirror closure drift')

def after_patch_target_checks(o,root,targets,dispatch):
 if not targets:raise ValueError('patch needs declared before targets')
 for row in targets:
  target=o.plain(o.child(root,row['path']),regular=True)
  if digest(target)!=row['before_sha256']:raise ValueError('patch target before SHA256 differs: '+row['path'])
 return dispatch()

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--destination',type=Path,required=True);ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--approved-lock-sha256',required=True);ap.add_argument('--execute-build',action='store_true');ap.add_argument('--timeout-seconds',type=int,default=10800);a=ap.parse_args()
 if digest(Path(__file__))!=a.approved_script_sha256 or digest(LOCK)!=a.approved_lock_sha256:raise ValueError('reviewed producer/lock bytes differ')
 if not a.execute_build:raise ValueError('no mutation/build without reviewed --execute-build')
 if os.name!='nt' or not 1<=a.timeout_seconds<=10800:raise ValueError('Windows and bounded shared wall limit required')
 lock=json.loads(LOCK.read_text(encoding='utf8'));o=cpu_owner(lock);dest=Path(os.path.abspath(a.destination));policy=identity_and_parent(o,dest)
 if shutil.disk_usage(PARENT).free<40*1024**3:raise ValueError('GPU producer needs40GiB free storage')
 o.verify_source(CPU,'x86_64-pc-windows-msvc');cudnn=json.loads((INPUT/'extraction.json').read_text(encoding='utf8'))
 if digest(INPUT/'extraction.json')!=lock['cudnn']['extraction_receipt_sha256']:raise ValueError('cuDNN extraction receipt changed')
 for r in cudnn['files']:o.verify_file(o.child(INPUT/'cudnn-dev',r['path']),r)
 for r in lock['dependencies']:
  o.verify_file(INPUT/'cuda-source-dependencies'/(r['name']+'.zip'),r)
  with (INPUT/'cuda-source-dependencies'/(r['name']+'.zip')).open('rb') as f:
   if o.stream_digest(f,'sha1').hexdigest()!=r['upstream_sha1']:raise ValueError('upstream CUDA dependency SHA1 differs')
 if digest(BASE/'ort-windows-loader-proposal-r2.patch')!=lock['loader_patch_sha256'] or digest(HERE/'offline-helper.patch')!=lock['offline_helper']['patch_sha256'] or digest(HERE/'frontend-authenticated-imports.patch')!=lock['frontend_final']['patch_sha256']:raise ValueError('reviewed input patch drift')
 original_inputs=[{'path':str((CPU/'downloads/source.tar.gz').absolute()),'bytes':o.LOCK['source']['bytes'],'sha256':o.LOCK['source']['sha256']}]
 for row in o.selected_dependencies('x86_64-pc-windows-msvc'):
  original_inputs.append({'path':str((CPU/'downloads'/(row['name']+'.zip')).absolute()),'bytes':row['bytes'],'sha256':row['sha256']})
 for row in lock['dependencies']:
  original_inputs.append({'path':str((INPUT/'cuda-source-dependencies'/(row['name']+'.zip')).absolute()),'bytes':row['bytes'],'sha256':row['sha256']})
 original_inputs.append({'path':str(INPUT/'cudnn-windows-x86_64-9.10.2.21_cuda12-archive.zip'),'bytes':lock['cudnn']['bytes'],'sha256':lock['cudnn']['sha256']})
 verify_input_closure(o,original_inputs)
 dest.mkdir();write(dest/'owner.json',{'schema':'airk.private.cuda-runtime-producer/1','sid':SID,'cpu_source_owner':str(CPU),'script_sha256':a.approved_script_sha256,'lock_sha256':a.approved_lock_sha256,'output_parent_policy':policy,'qualification':False})
 start=time.monotonic();deadline=start+a.timeout_seconds;receipt={'status':'RUNNING','commands':[],'policy':{'one_static_core':True,'optional_shared_cuda_provider':True,'runtime_version':'1.30.0','SM':86,'compiled_flash_attention':True,'compiled_memory_efficient_attention':True,'first_model_gate':'FP32-unfused-TF32off','fp16_model_admission':'separate-later-G5-unqualified','cpu_qualification_transferred':False},'source_inputs':[]}
 try:
  # Canonical extraction verifies complete original source census; never copy a warm build tree.
  o.verify_file(CPU/'downloads/source.tar.gz',o.LOCK['source']);o.extract_source(CPU/'downloads/source.tar.gz',dest,o.LOCK['source'])
  mirrors=[]
  for r in o.selected_dependencies('x86_64-pc-windows-msvc')+lock['dependencies']:
   source=CPU/'downloads'/(r['name']+'.zip') if r['name'] not in ('cutlass','cudnn_frontend') else INPUT/'cuda-source-dependencies'/(r['name']+'.zip')
   o.verify_file(source,r);target=o.child(dest/'source/mirror',o.mirror_relative(r));target.parent.mkdir(parents=True,exist_ok=True)
   with source.open('rb') as f,target.open('xb') as g:shutil.copyfileobj(f,g,1024*1024)
   o.verify_file(target,r);receipt['source_inputs'].append({'name':r['name'],'bytes':r['bytes'],'sha256':r['sha256']})
   mirrors.append({'path':target.relative_to(dest/'source/mirror').as_posix(),'bytes':r['bytes'],'sha256':r['sha256']})
  # Hash complete toolkit build inputs to prevent ambient CUDA/NVCC selection or drift.
  cuda_rows=[]
  for name in ('bin','include','lib','nvvm'):
   for r in o.source_inventory(CUDA/name):cuda_rows.append({'path':name+'/'+r['path'],'bytes':r['bytes'],'sha256':r['sha256']})
  write(dest/'cuda-toolkit-inputs.json',cuda_rows)
  if json.loads((CUDA/'version.json').read_text())['cuda']['version']!='12.8.1':raise ValueError('CUDA toolkit version differs')
  tools_args=argparse.Namespace(vs_root=Path('C:/Program Files/Microsoft Visual Studio/2022/Community'),cmake=Path('D:/code/Github/airk/artifacts/advanced-split/ort130-x64-owned-r3/tools/cmake/bin/cmake.exe'),ninja=Path('D:/code/Github/airk/artifacts/advanced-split/ort130-x64-owned-r3/tools/ninja-build/ninja.exe'),patch=Path('C:/Program Files/Git/usr/bin/patch.exe'),msvc_toolset='14.44.35207',windows_sdk='10.0.26100.0')
  tools=o.toolchain(tools_args,dest,'x86_64-pc-windows-msvc');receipt['toolchain']=tools
  tool_inputs=[{key:tools[name][key] for key in ('path','bytes','sha256')} for name in ('compiler','cmake','ninja','patch','python','vcvars')]
  verify_input_closure(o,tool_inputs)
  environment=dict(os.environ)
  forbidden={'CC','CXX','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','CUDAFLAGS','CUDACXX','CUDAHOSTCXX','CUDAARCHS','NVCC_PREPEND_FLAGS','NVCC_APPEND_FLAGS','CL','_CL_','LINK','LIB','LIBPATH','INCLUDE','PYTHONPATH','PYTHONHOME','CUDNN_FRONTEND_CUDART_LIB_NAME'}
  for key in list(environment):
   if key.upper() in forbidden or key.upper().startswith(('CMAKE_','ORT_','CUDNN_','CUDA_')):environment.pop(key)
  private_temp=dest/'t';private_temp.mkdir()
  environment.update(PATH='C:/Windows/System32;C:/Windows;'+str(Path(sys.executable).parent)+';'+str(CUDA/'bin'),TEMP=str(private_temp),TMP=str(private_temp),TMPDIR=str(private_temp),PYTHONNOUSERSITE='1',CUDA_CACHE_PATH=str(dest/'cc'))
  def run(argv,cwd,stage,wrapped=False):
   remaining=int(deadline-time.monotonic())
   if remaining<=0:raise ValueError('original GPU producer wall budget exhausted')
   if wrapped:argv=o.windows_command(argv,tools,dest,stage)
   result=o.run_owned(argv,cwd,dest/'logs'/(stage+'.log'),remaining,environment);receipt['commands'].append(result)
   if not o.successful(result):raise ValueError('owned GPU producer stage failed: '+stage)
  pinned_patches={
   str((BASE/'ort-windows-loader-proposal-r2.patch').absolute()):lock['loader_patch_sha256'],
   str((HERE/'offline-helper.patch').absolute()):lock['offline_helper']['patch_sha256'],
   str((HERE/'frontend-authenticated-imports.patch').absolute()):lock['frontend_final']['patch_sha256'],
   str((dest/'source/cmake/patches/cudnn_frontend/cudnn_frontend_win_arm64_compat.patch').absolute()):lock['frontend_upstream_arm64']['patch_sha256']}
  def patch(path,cwd,name,targets):
   def dispatch():
    o.plain(path,regular=True);expected=pinned_patches[str(path.absolute())]
    target=dest/'patch-inputs'/(name+'.patch');target.parent.mkdir(exist_ok=True)
    with path.open('rb') as f,target.open('xb') as g:shutil.copyfileobj(f,g,1024*1024)
    if digest(target)!=expected:raise ValueError('protected patch input snapshot differs')
    run([tools['patch']['path'],'--batch','--forward','--fuzz=0','--binary','--ignore-whitespace','-p1','--input='+str(target.absolute())],cwd,name)
   return after_patch_target_checks(o,cwd,targets,dispatch)
  patch(BASE/'ort-windows-loader-proposal-r2.patch',dest/'source','loader-patch',lock['loader_files'])
  patch(HERE/'offline-helper.patch',dest/'source','offline-patch',[lock['offline_helper']])
  for r in lock['loader_files']:
   if digest(dest/'source'/r['path'])!=r['proposed_sha256']:raise ValueError('loader output source differs')
  if digest(dest/'source'/lock['offline_helper']['path'])!=lock['offline_helper']['after_sha256']:raise ValueError('offline helper output differs')
  frontend=dest/'frontend';frontend_proof=safe_frontend(o,INPUT/'cuda-source-dependencies/cudnn_frontend.zip',frontend,lock);write(dest/'frontend-source-before.json',frontend_proof)
  patch(dest/'source/cmake/patches/cudnn_frontend/cudnn_frontend_win_arm64_compat.patch',frontend,'frontend-arm64',[lock['frontend_upstream_arm64']])
  patch(HERE/'frontend-authenticated-imports.patch',frontend,'frontend-auth-imports',[lock['frontend_final']])
  for key in ('frontend_upstream_arm64','frontend_final'):
   r=lock[key]
   if digest(frontend/r['path'])!=r['after_sha256']:raise ValueError('exact frontend transformed source differs')
  source_rows=o.source_inventory(dest/'source',exclude_mirror=True);frontend_rows=o.source_inventory(frontend);write(dest/'gpu-source-files.json',source_rows);write(dest/'frontend-source-files.json',frontend_rows)
  # Fail cheaply on frontend/header/toolkit ABI incompatibility before full CMake/native build.
  syntax=dest/'frontend-header-probe.cc'
  syntax.write_text('#include <cudnn_frontend.h>\nstatic_assert(CUDNN_MAJOR == 9 && CUDNN_MINOR == 10 && CUDNN_PATCHLEVEL == 2);\n',encoding='utf8')
  syntax_command=[tools['compiler']['path'],'/nologo','/Zs','/std:c++17','/EHsc','/MD','/DNV_CUDNN_FRONTEND_USE_DYNAMIC_LOADING','/I'+str(frontend/'include'),'/I'+str(frontend/'include/cudnn_frontend/thirdparty'),'/I'+str(CUDA/'include'),'/I'+str(INPUT/'cudnn-dev/include'),str(syntax)]
  receipt['header_probe_source_sha256']=digest(syntax);receipt['header_probe_argv']=syntax_command;run(syntax_command,dest,'frontend-header-probe',True)
  closure=original_inputs+tool_inputs
  for row in mirrors:closure.append({'path':str((dest/'source/mirror'/row['path']).absolute()),'bytes':row['bytes'],'sha256':row['sha256']})
  for path in [ROOT/'scripts/provision-static-onnx-runtime.py',ROOT/'scripts/static-onnx-runtime-lock.json',INPUT/'extraction.json',CUDA/'version.json']:
   closure.append({'path':str(path.absolute()),'bytes':path.stat().st_size,'sha256':digest(path)})
  verify_input_closure(o,closure);verify_mirror_closure(o,dest/'source/mirror',mirrors)
  write(dest/'frozen-input-closure.json',closure);receipt['frozen_input_closure_sha256']=digest(dest/'frozen-input-closure.json')
  command=gpu_command(o,dest,tools,frontend);receipt['configure_argv']=command;run(command,dest,'configure',True)
  o.verify_cmake_cache(dest/'b/Release/CMakeCache.txt',command)
  build=[tools['cmake']['path'],'--build',str(dest/'b/Release'),'--config','Release','--target','all','re2','--parallel','2'];receipt['build_argv']=build;run(build,dest,'build',True)
  if o.source_inventory(dest/'source',exclude_mirror=True)!=source_rows or o.source_inventory(frontend)!=frontend_rows:raise ValueError('compiled source drift')
  for r in cuda_rows:o.verify_file(o.child(CUDA,r['path']),r)
  for r in cudnn['files']:o.verify_file(o.child(INPUT/'cudnn-dev',r['path']),r)
  libraries=[]
  for p in sorted((dest/'b').rglob('*.lib')):
   o.plain(p,regular=True)
   with p.open('rb') as f:
    if f.read(8)!=b'!<arch>\n':raise ValueError('invalid archive/import library output')
   libraries.append({'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':digest(p)})
  dlls=[]
  for name in ('onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.dll'):
   found=list((dest/'b').rglob(name))
   if len(found)!=1:raise ValueError('exact same-build provider output missing/ambiguous')
   p=o.plain(found[0],regular=True);dlls.append({'path':p.relative_to(dest).as_posix(),'bytes':p.stat().st_size,'sha256':digest(p)})
  if list((dest/'b').rglob('onnxruntime.dll')):raise ValueError('second dynamic ORT core output refused')
  receipt['reopened_frozen_input_count']=verify_input_closure(o,closure)
  verify_mirror_closure(o,dest/'source/mirror',mirrors)
  if digest(dest/'frozen-input-closure.json')!=receipt['frozen_input_closure_sha256']:raise ValueError('frozen closure record drift')
  if digest(Path(__file__))!=a.approved_script_sha256 or digest(LOCK)!=a.approved_lock_sha256:raise ValueError('producer or lock changed during build')
  receipt.update(status='PASS_PRODUCER_ONLY_UNQUALIFIED',libraries=libraries,provider_dlls=dlls,source_inventory_sha256=digest(dest/'gpu-source-files.json'),frontend_inventory_sha256=digest(dest/'frontend-source-files.json'),cuda_inventory_sha256=digest(dest/'cuda-toolkit-inputs.json'))
 except BaseException as e:
  receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(e).__name__+': '+str(e));raise
 finally:
  receipt['elapsed_seconds']=time.monotonic()-start;write(dest/'gpu-build-receipt.json',receipt);print(json.dumps({'status':receipt['status'],'destination':str(dest),'elapsed_seconds':receipt['elapsed_seconds']}),flush=True)
if __name__=='__main__':main()
