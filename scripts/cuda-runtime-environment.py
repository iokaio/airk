# SPDX-License-Identifier: Apache-2.0
"""Explicit GPU-only static Cargo handoff; no CPU-validator fallback or GPU activation."""
from pathlib import Path
import importlib.util,json,os,subprocess
STATIC_KEYS={'LIBONNXRUNTIME_NO_PKG_CONFIG','ORT_SKIP_DOWNLOAD','ORT_PREFER_DYNAMIC_LINK','ORT_LIB_LOCATION','ORT_LIB_PROFILE'}
def _load(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def environment(destination,build,aggregate):
 runtime=_load('cuda-static-runtime')
 projection=runtime.static_projection(build,aggregate)
 if projection['target']!=runtime.TARGET:raise ValueError('GPU static environment target differs')
 return {'LIBONNXRUNTIME_NO_PKG_CONFIG':'1','ORT_SKIP_DOWNLOAD':'1','ORT_PREFER_DYNAMIC_LINK':'0','ORT_LIB_LOCATION':str(Path(destination)/'gpu-aggregate'),'ORT_LIB_PROFILE':'Release'}

def verify_handoff(path,expected_handoff_sha256,repo_root):
 runtime=_load('cuda-static-runtime');aggregate_owner=_load('cuda-static-aggregate');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_for_gpu_environment')
 path=core.plain(path,regular=True)
 if core.digest(path)!=expected_handoff_sha256:raise runtime.GpuProofUnavailable('reviewed GPU environment pin differs')
 value=core.read_json(path)
 if set(value)!={'schema','request','build','aggregate','environment','activation_pins_available'} or value['schema'] not in ('airk.cuda-static-environment/1','airk.cuda-static-environment/2','airk.cuda-static-environment/3','airk.cuda-static-environment/4') or value['activation_pins_available'] is not False:raise runtime.GpuProofUnavailable('GPU environment format/activation claim differs')
 request=value['request']
 destination=core.plain(request['destination'])
 if value['schema']=='airk.cuda-static-environment/1':
  if set(request)!={'destination','producer_file','lock_file','receipt_sha256'}:raise runtime.GpuProofUnavailable('GPU verifier request fields differ')
  build=runtime.verify_build(destination,request['producer_file'],request['lock_file'],request['receipt_sha256'],repo_root)
 elif value['schema']=='airk.cuda-static-environment/4':
  if set(request)!={'destination','composite_root','composer_proposal','receipt_sha256'}:raise runtime.GpuProofUnavailable('exact checkpoint handoff request fields differ')
  build=_load('cuda-static-checkpointed').verify_checkpointed(destination,request['composite_root'],request['composer_proposal'],request['receipt_sha256'],repo_root)
 else:
  if set(request)!={'destination','continuation_root','continuation_proposal','receipt_sha256'}:raise runtime.GpuProofUnavailable('exact continuation handoff request fields differ')
  owner=_load('cuda-static-continuation-r12' if value['schema']=='airk.cuda-static-environment/3' else 'cuda-static-continuation');build=owner.verify_continuation(destination,request['continuation_root'],request['continuation_proposal'],request['receipt_sha256'],repo_root)
 aggregate=aggregate_owner.verify_aggregate(destination,build,core)
 expected=environment(destination,build,aggregate)
 if value['build']!=build or value['aggregate']!=aggregate or value['environment']!=expected or set(expected)!=STATIC_KEYS:raise runtime.GpuProofUnavailable('GPU static handoff no longer matches actual build/aggregate')
 return {'environment':expected,'static_ort':runtime.static_projection(build,aggregate),'gpu_build':build,'gpu_aggregate':aggregate,'activation_pins_available':False}

def handoff_value(destination,producer_file,lock_file,receipt_sha256,build,aggregate):
 return {'schema':'airk.cuda-static-environment/1','request':{'destination':str(destination),'producer_file':str(producer_file),'lock_file':str(lock_file),'receipt_sha256':receipt_sha256},'build':build,'aggregate':aggregate,'environment':environment(destination,build,aggregate),'activation_pins_available':False}

def continuation_handoff_value(destination,continuation,proposal,receipt_sha256,build,aggregate):
 return {'schema':'airk.cuda-static-environment/2','request':{'destination':str(destination),'continuation_root':str(continuation),'continuation_proposal':str(proposal),'receipt_sha256':receipt_sha256},'build':build,'aggregate':aggregate,'environment':environment(destination,build,aggregate),'activation_pins_available':False}

def r12_handoff_value(destination,continuation,proposal,receipt_sha256,build,aggregate):
 value=continuation_handoff_value(destination,continuation,proposal,receipt_sha256,build,aggregate);value['schema']='airk.cuda-static-environment/3';return value

def checkpoint_handoff_value(destination,composite_root,proposal,receipt_sha256,build,aggregate):
 return {"schema":"airk.cuda-static-environment/4","request":{"destination":str(destination),"composite_root":str(composite_root),"composer_proposal":str(proposal),"receipt_sha256":receipt_sha256},"build":build,"aggregate":aggregate,"environment":environment(destination,build,aggregate),"activation_pins_available":False}
