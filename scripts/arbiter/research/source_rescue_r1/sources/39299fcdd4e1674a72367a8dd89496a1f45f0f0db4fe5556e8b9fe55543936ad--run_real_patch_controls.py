# SPDX-License-Identifier: Apache-2.0
"""Actual GNU patch controls only. No CMake/C++/DLL/model execution."""
from pathlib import Path
import json,importlib.util,tarfile,zipfile,time
ROOT=Path('D:/code/Github/airk');HERE=Path(__file__).resolve().parent;BASE=HERE.parent
CPU=ROOT/'artifacts/advanced-split/ort130-x64-owned-r5'
INPUT=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-runtime-inputs-r1')
OUT=INPUT.parent/'gpu-patch-controls-r6'
def main():
 lock=json.loads((HERE/'cuda-runtime-producer-lock.json').read_text(encoding='utf8'))
 spec=importlib.util.spec_from_file_location('cpu_owner',ROOT/'scripts/provision-static-onnx-runtime.py');o=importlib.util.module_from_spec(spec);spec.loader.exec_module(o)
 spec=importlib.util.spec_from_file_location('candidate',HERE/'provision_cuda_runtime.py');g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)
 if OUT.exists():raise ValueError('fresh controls root required')
 if o.digest(ROOT/'scripts/provision-static-onnx-runtime.py')!=lock['cpu_owner_sha256']:raise ValueError('owner drift')
 o.verify_file(CPU/'downloads/source.tar.gz',o.LOCK['source']);dep=next(r for r in lock['dependencies'] if r['name']=='cudnn_frontend');o.verify_file(INPUT/'cuda-source-dependencies/cudnn_frontend.zip',dep)
 program=Path('C:/Program Files/Git/usr/bin/patch.exe')
 if o.digest(program)!='5f02c3c167f715247a19007dac691cd90b5752b55d0269d7b82c6e5aa0ffee6f':raise ValueError('patch tool drift')
 o.plain(OUT);OUT.mkdir();source=OUT/'source';frontend=OUT/'frontend';source.mkdir();frontend.mkdir();start=time.monotonic();deadline=start+120;completed=[];status='RUNNING';failure=None
 paths=['onnxruntime/core/platform/windows/env.cc','onnxruntime/core/providers/cuda/cudnn_loader.cc','onnxruntime/core/providers/cuda/cufft_loader.cc','cmake/external/helper_functions.cmake','cmake/patches/cudnn_frontend/cudnn_frontend_win_arm64_compat.patch','onnxruntime/core/session/onnxruntime_c_api.cc']
 with tarfile.open(CPU/'downloads/source.tar.gz','r:gz') as t:
  for rel in paths:
   member=t.getmember(o.LOCK['source']['archive_root']+'/'+rel)
   if not member.isfile() or member.size>2*1024**2:raise ValueError('invalid control source member')
   target=o.child(source,rel);target.parent.mkdir(parents=True,exist_ok=True)
   with t.extractfile(member) as f,target.open('xb') as out:out.write(f.read())
 with zipfile.ZipFile(INPUT/'cuda-source-dependencies/cudnn_frontend.zip') as z:
  for rel in ['include/cudnn_frontend_Tensor.h','include/cudnn_frontend_shim.h']:
   target=o.child(frontend,rel);target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(z.read('cudnn-frontend-1.27.0/'+rel))
 cases=[('loader',BASE/'ort-windows-loader-proposal-r2.patch',source,lock['loader_files'],lock['loader_patch_sha256']),('offline',HERE/'offline-helper.patch',source,[lock['offline_helper']],lock['offline_helper']['patch_sha256']),('final-placement',HERE/'final-placement.patch',source,[lock['final_placement']],lock['final_placement']['patch_sha256']),('arm64',source/'cmake/patches/cudnn_frontend/cudnn_frontend_win_arm64_compat.patch',frontend,[lock['frontend_upstream_arm64']],lock['frontend_upstream_arm64']['patch_sha256']),('frontend',HERE/'frontend-authenticated-imports.patch',frontend,[lock['frontend_final']],lock['frontend_final']['patch_sha256'])]
 try:
  for name,patch,cwd,targets,pin in cases:
   if o.digest(patch)!=pin:raise ValueError('input patch drift')
   snapshot=OUT/'patch-inputs'/(name+'.patch');snapshot.parent.mkdir(exist_ok=True);snapshot.write_bytes(patch.read_bytes())
   if o.digest(snapshot)!=pin:raise ValueError('protected patch copy drift')
   before=[{'path':r['path'],'sha256':o.digest(o.child(cwd,r['path']))} for r in targets]
   argv=[str(program),'--batch','--forward','--fuzz=0','--binary','--ignore-whitespace','-p1','--input='+str(snapshot)];result=[]
   def dispatch():
    remaining=int(deadline-time.monotonic())
    if remaining<=0:raise ValueError('original control wall budget exhausted')
    observed=o.run_owned(argv,cwd,OUT/'logs'/(name+'.log'),min(30,remaining));result.append(observed)
    if not o.successful(observed):raise ValueError('actual patch failed: '+name)
   g.after_patch_target_checks(o,cwd,targets,dispatch);after=[]
   for row in targets:
    actual=o.digest(o.child(cwd,row['path']));expected=row.get('after_sha256',row.get('proposed_sha256'))
    if actual!=expected:raise ValueError('after target differs: '+row['path'])
    after.append({'path':row['path'],'sha256':actual,'expected_sha256':expected})
   completed.append({'target_set':name,'before':before,'after':after,'patch_sha256':pin,'owned_result':result[0],'log_sha256':o.digest(OUT/'logs'/(name+'.log'))});print('actual GNU patch PASS',name,flush=True)
  status='PASS_ALL_FIVE_REAL_GNU_PATCH_TARGET_SETS'
 except BaseException as e:
  status='FAILED_PRESERVED_NO_RETRY';failure=type(e).__name__+': '+str(e);raise
 finally:
  receipt={'schema':'airk.private.gpu-real-patch-controls/2','status':status,'failure':failure,'elapsed_seconds':time.monotonic()-start,'completed':completed,'source_archive_sha256':o.LOCK['source']['sha256'],'frontend_archive_sha256':dep['sha256'],'producer_sha256':o.digest(HERE/'provision_cuda_runtime.py'),'lock_sha256':o.digest(HERE/'cuda-runtime-producer-lock.json'),'control_script_sha256':o.digest(Path(__file__)),'patch_program_sha256':o.digest(program),'cpp_compile':False,'cmake':False,'gpu_dll_load':False,'model_calls':0}
  (OUT/'receipt.json').write_bytes((json.dumps(receipt,indent=2)+'\n').encode());print(status,o.digest(OUT/'receipt.json'),flush=True)
if __name__=='__main__':main()
