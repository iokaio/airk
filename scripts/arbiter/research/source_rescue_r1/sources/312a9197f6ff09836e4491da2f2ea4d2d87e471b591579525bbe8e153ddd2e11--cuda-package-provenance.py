# SPDX-License-Identifier: Apache-2.0
"""Exact optional payload merge only. Native Rust owns runtime manifests/PE approval.
This module deliberately cannot manufacture/approve a RuntimeManifest.
"""
from pathlib import PurePosixPath
import re
class NativeGpuProjectionUnavailable(ValueError):pass

def _path(name):
 if not isinstance(name,str) or not name or '\\' in name or ':' in name or any(ord(c)<32 for c in name) or any(x in ('','.','..') for x in name.split('/')):raise ValueError('unsafe package path')
 p=PurePosixPath(name)
 if p.is_absolute():raise ValueError('absolute package path')
 return p

def exact_optional_payload(base_files,native_runtime_files,notice_files):
 # Native owner must have validated/emitted native_runtime_files. This is not a manifest decoder.
 if not native_runtime_files:raise NativeGpuProjectionUnavailable('actual native GPU file projection unavailable')
 combined=dict(base_files);seen={n.casefold() for n in combined}
 required={'onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.dll','airk-cuda-runtime.json'}
 if not required.issubset(native_runtime_files):raise NativeGpuProjectionUnavailable('native same-core runtime projection incomplete')
 for mapping,legal in [(native_runtime_files,False),(notice_files,True)]:
  for name,digest in mapping.items():
   path=_path(name)
   if name.casefold() in seen:raise ValueError('duplicate/case-colliding optional payload entry')
   if not re.fullmatch('[0-9a-f]{64}',digest):raise ValueError('payload SHA256 differs')
   if legal:
    if len(path.parts)<3 or path.parts[0]!='notices' or path.parts[1]!='cuda-runtime':raise ValueError('GPU legal file outside exact notice root')
   else:
    if len(path.parts)!=1 or name.lower()=='nvcuda.dll' or name.lower()=='onnxruntime.dll':raise ValueError('driver/second-core/nested GPU payload refused')
    if not (name.lower().endswith('.dll') or name in ('airk-cuda-runtime.json','airk-cuda-build-provenance.json')):raise ValueError('private/developer/model GPU payload refused')
   seen.add(name.casefold());combined[name]=digest
 if not notice_files:raise NativeGpuProjectionUnavailable('actual exact GPU legal projection unavailable')
 return combined

def require_native_activation_projection(*args,**kwargs):
 # No native projection/approval protocol has been supplied yet. Do not guess it in Python.
 raise NativeGpuProjectionUnavailable('native Rust validated distributable projection and actual producer/PE proof required')
