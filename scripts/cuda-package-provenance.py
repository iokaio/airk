# SPDX-License-Identifier: Apache-2.0
"""Exact optional payload merge only. Native Rust owns runtime manifests/PE approval.
This module deliberately cannot manufacture/approve a RuntimeManifest.
"""
from pathlib import Path,PurePosixPath
import re
class NativeGpuProjectionUnavailable(ValueError):pass

def _path(name):
 if not isinstance(name,str) or not name or '\\' in name or ':' in name or any(ord(c)<32 for c in name) or any(x in ('','.','..') for x in name.split('/')):raise ValueError('unsafe package path')
 p=PurePosixPath(name)
 if p.is_absolute():raise ValueError('absolute package path')
 return p

def _payload_path(name,legal):
 path=_path(name)
 if len(path.parts)!=1:raise ValueError('native optional payload must remain a direct child')
 if legal:
  # Role/component/source validation belongs to the authenticated Rust emitter.
  # Its projection flattens Notice assets to these unchanged safe leaf names.
  if len(name)>128 or '..' in name or not re.fullmatch(r'airk-cuda-notice-[a-z0-9_.+-]+-[a-z0-9_.+-]+',name) or not name.endswith(('.txt','.html','.md')):raise ValueError('native legal notice leaf differs')
 else:
  if name.lower() in ('nvcuda.dll','onnxruntime.dll'):raise ValueError('driver/second-core GPU payload refused')
  if not (name.lower().endswith('.dll') or name in ('airk-cuda-runtime.json','airk-cuda-build-provenance.json')):raise ValueError('private/developer/model GPU payload refused')
 return path

def exact_optional_payload(base_files,native_runtime_files,notice_files):
 # Native owner must have validated/emitted native_runtime_files. This is not a manifest decoder.
 if not native_runtime_files:raise NativeGpuProjectionUnavailable('actual native GPU file projection unavailable')
 combined=dict(base_files);seen={n.casefold() for n in combined}
 required={'onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.dll','airk-cuda-runtime.json'}
 if not required.issubset(native_runtime_files):raise NativeGpuProjectionUnavailable('native same-core runtime projection incomplete')
 for mapping,legal in [(native_runtime_files,False),(notice_files,True)]:
  for name,digest in mapping.items():
   _payload_path(name,legal)
   if name.casefold() in seen:raise ValueError('duplicate/case-colliding optional payload entry')
   if not re.fullmatch('[0-9a-f]{64}',digest):raise ValueError('payload SHA256 differs')
   seen.add(name.casefold());combined[name]=digest
 if not notice_files:raise NativeGpuProjectionUnavailable('actual exact GPU legal projection unavailable')
 return combined

def _execution_owner():
 import importlib.util
 p=Path(__file__).with_name('cuda-native-execution.py');spec=importlib.util.spec_from_file_location('cuda_package_native_custody',p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def require_native_activation_projection(projection_path=None,projection_sha256=None,verified_handoff=None,*,native_execution_receipt=None,repo_root=None,source_root=None):
 # This consumes only the descriptive Rust-emitted projection, never RuntimeManifest.
 import hashlib,json
 if projection_path is None or verified_handoff is None or native_execution_receipt is None or repo_root is None:raise NativeGpuProjectionUnavailable('actual native projection and owned NN0 execution pins required')
 if not isinstance(native_execution_receipt,tuple) or len(native_execution_receipt)!=2:raise ValueError('explicit parent receipt path/SHA tuple required')
 owner=_execution_owner();projection=owner.pin(projection_path)
 if projection['sha256']!=projection_sha256:raise ValueError('reviewed native projection SHA differs')
 plan,execution=owner.verify_execution(*native_execution_receipt,projection_pin=projection)
 root=Path(plan['runtime_root'])
 if source_root is not None and Path(source_root)!=root:raise ValueError('caller source root differs from actual native custody')
 value=owner.read(projection_path,1<<20)
 if set(value)!={'schema','descriptor','static_core_build','files','scope','activation','qualification'} or value['schema']!='airk.native.cuda-package-projection/1' or value['activation'] is not False or value['qualification'] is not False:raise ValueError('exact descriptive native package projection required')
 build=verified_handoff.get('gpu_build',{})
 if verified_handoff.get('activation_pins_available') is not False or build.get('schema') not in ('airk.cuda-static-build-proof/1','airk.cuda-static-build-proof/2','airk.cuda-static-build-proof/3') or value['static_core_build']!=build.get('build_receipt',{}).get('sha256'):raise ValueError('native projection/static-core producer mismatch')
 descriptor=value['descriptor']
 if not isinstance(descriptor,dict) or set(descriptor)!={'name','bytes','sha256'} or descriptor['name']!='airk-cuda-runtime.json' or (descriptor['bytes'],descriptor['sha256'])!=(plan['descriptor']['bytes'],plan['descriptor']['sha256']):raise ValueError('actual native descriptor pin differs')
 rows=value['files']
 if not isinstance(rows,list) or not 4<=len(rows)<=128 or rows!=sorted(rows,key=lambda r:r.get('name','')):raise ValueError('bounded sorted native exact file roster required')
 files={};legal={};native={};total=0
 for row in rows:
  if set(row)!={'name','bytes','sha256'} or type(row['bytes']) is not int or not 0<row['bytes']<=2<<30:raise ValueError('native file row bound/shape differs')
  name=row['name'];is_legal=isinstance(name,str) and name.startswith('airk-cuda-notice-');_payload_path(name,is_legal)
  if name in files:raise ValueError('duplicate native file row')
  if is_legal:legal[name]=row['sha256']
  else:native[name]=row['sha256']
  source=owner.regular(root.joinpath(*PurePosixPath(name).parts));actual=owner.pin(source)
  if (actual['bytes'],actual['sha256'])!=(row['bytes'],row['sha256']):raise ValueError('actual exact native payload byte drift')
  total+=row['bytes'];files[name]=row['sha256']
 if total>4<<30:raise ValueError('native optional payload exceeds4GiB')
 exact_optional_payload({},native,legal)
 if {str(root.joinpath(*PurePosixPath(name).parts)):sha for name,sha in files.items()}!={r['path']:r['sha256'] for r in plan['runtime_files']}:raise ValueError('projection omits/invents an original runtime custody file')
 if files.get(descriptor['name'])!=descriptor['sha256']:raise ValueError('native descriptor omitted from exact files')
 return {'files':files,'source_root':root,'native_projection':projection,'native_execution_receipt':owner.pin(native_execution_receipt[0]),'static_core_build':value['static_core_build'],'descriptor':descriptor,'activation':False,'qualification':False,'_repo_root':str(Path(repo_root))}

def reverify_native_projection(value,verified_handoff):
 observed=require_native_activation_projection(value['native_projection']['path'],value['native_projection']['sha256'],verified_handoff,native_execution_receipt=(value['native_execution_receipt']['path'],value['native_execution_receipt']['sha256']),repo_root=value['_repo_root'],source_root=value['source_root'])
 if observed!=value:raise ValueError('native package custody/projection drift')
 return observed

def stage_native_projection(value,destination,verified_handoff):
 import shutil
 value=reverify_native_projection(value,verified_handoff);owner=_execution_owner();destination=Path(destination)
 if not destination.is_absolute() or destination.resolve(strict=True)!=destination or not destination.is_dir():raise ValueError('existing canonical package staging directory required')
 for ancestor in [destination,*destination.parents]:
  st=ancestor.lstat()
  if ancestor.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('package destination alias refused')
 # All names already passed the exact native/legal allowlist; never overwrite base images.
 for name in value['files']:
  target=destination.joinpath(*PurePosixPath(name).parts)
  if target.exists():raise ValueError('existing package entry retained: '+name)
  for parent in target.parents:
   if parent==destination:break
   if parent.exists() and (parent.is_symlink() or getattr(parent.lstat(),'st_file_attributes',0)&0x400 or not parent.is_dir()):raise ValueError('package nested destination redirected')
 for name,sha in value['files'].items():
  source=owner.regular(value['source_root'].joinpath(*PurePosixPath(name).parts));target=destination.joinpath(*PurePosixPath(name).parts);target.parent.mkdir(parents=True,exist_ok=True)
  with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1<<20)
  if owner.digest(owner.regular(target))!=sha:raise ValueError('native package copy hash differs')
 reverify_native_projection(value,verified_handoff)
 return dict(value['files'])
