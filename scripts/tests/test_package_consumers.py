# SPDX-License-Identifier: Apache-2.0
"""Explicit optional-context plumbing controls. No signing/native/model operations."""
import argparse,importlib.util,json,tempfile,unittest,hashlib
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock,patch
HERE=Path(__file__).resolve().parents[2]
def load(name):
 s=importlib.util.spec_from_file_location(name,HERE/'scripts'/(name+'.py'));m=importlib.util.module_from_spec(s)
 if name=='sign-native':
  # Isolated import fixture only; no existing policy/credential configuration copy.
  fixture={'schema':'airk-signing-identity/1','note':'inert fixture','windows':{'publisher':'fixture','identity_eku':'fixture','artifact_signing':{k:[] if k=='excluded_credentials' else 'inert' for k in ['endpoint','account','certificate_profile','file_digest','timestamp_url','timestamp_digest','credential','excluded_credentials']},'local_client':{'package':'inert','version':'0','nupkg_sha256':'0'*64,'dlib':'inert','dlib_sha256':'0'*64,'dlib_publisher':'inert'}},'macos':{'team_id':'AAAAAAAAAA','identity':'inert','certificate_sha256':'0'*64,'local':{'keychain':'inert','notary_profile':'inert','unlock_item':'inert'}}}
  original=Path.read_text
  with patch.object(Path,'read_text',lambda path,*a,**kw:json.dumps(fixture) if path.name=='signing-identity.json' else original(path,*a,**kw)):s.loader.exec_module(m)
 else:s.loader.exec_module(m)
 return m
p=load('package-native');sign=load('sign-native');local=load('build-local-service-installer')
class Controls(unittest.TestCase):
 def args(self,values=()):
  parser=argparse.ArgumentParser();p.add_cuda_arguments(parser);return parser.parse_args(values)
 def test_cpu_default_does_not_consult_gpu_owner(self):
  b=Mock();self.assertIsNone(p.cuda_from_args(self.args(),b));b.prepare_cuda_inputs.assert_not_called()
 def test_incomplete_explicit_gpu_pins_refuse(self):
  with self.assertRaises(ValueError):p.cuda_from_args(self.args(['--cuda-environment','only']),Mock())
 def test_exact_six_pins_use_existing_builder_owner(self):
  b=Mock();args=self.args(['--cuda-environment','env','--cuda-environment-sha256','a','--cuda-projection','projection','--cuda-projection-sha256','b','--cuda-native-execution-receipt','receipt','--cuda-native-execution-receipt-sha256','c']);p.cuda_from_args(args,b);b.prepare_cuda_inputs.assert_called_once_with(Path('env'),'a',Path('projection'),'b',Path('receipt'),'c')
  args.target='x86_64-unknown-linux-gnu'
  with self.assertRaises(ValueError):p.cuda_from_args(args,b)
 def test_payload_requires_same_static_source_and_exact_rows(self):
  b=Mock();b.reverify_cuda_inputs.return_value={'identity':'same'};cuda={'projection':{'files':{'provider.dll':'a'}}};build={'static_ort':{'identity':'same'}};p.validate_cuda_payload(b,cuda,HERE,'x86_64-pc-windows-msvc',build,{'provider.dll':'a'})
  with self.assertRaises(ValueError):p.validate_cuda_payload(b,cuda,HERE,'x86_64-pc-windows-msvc',build,{})
  with self.assertRaises(ValueError):p.validate_cuda_payload(b,cuda,HERE,'x86_64-pc-windows-msvc',{'static_ort':{}},{'provider.dll':'a'})
 def test_sign_prepare_cpu_default_and_explicit_context(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);source=root/'source';source.mkdir();(source/'airk.exe').write_bytes(b'fixture');(source/'airk-install.json').write_text(json.dumps({'version':'1.8.207'}));(source/'airk-build.json').write_text(json.dumps({'source':{'source':'fixture'}}));files=p.inventory(source);fake=Mock();fake.validate_runtime.return_value=(files,('airk.exe',));fake.inventory=p.inventory
   with patch.object(sign,'ROOT',root),patch.object(sign,'pair_package',return_value=fake):
    sign.prepare_pair(source,root/'artifacts/default','x86_64-pc-windows-msvc');fake.validate_runtime.assert_called_with(source,'1.8.207','x86_64-pc-windows-msvc',{'source':'fixture'})
    context={'opaque':'native'};sign.prepare_pair(source,root/'artifacts/gpu','x86_64-pc-windows-msvc',cuda=context);fake.validate_runtime.assert_called_with(source,'1.8.207','x86_64-pc-windows-msvc',{'source':'fixture'},cuda=context)
 def test_local_consume_validator_preserves_default_and_forwards_same_context(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);fake=Mock();fake.validate_runtime.return_value=({},('airk.exe','airkctl.exe'));fake.bounded_json.return_value={'static_ort':{'static':'same'},'admin_assets':{'admin':'same'}}
   with patch.object(local,'load_script',return_value=fake):
    local.verified_runtime(root,'1.8.207',{}, {'static':'same'},{'admin':'same'});fake.validate_runtime.assert_called_with(root,'1.8.207',local.TARGET,{})
    context={'native':'same'};local.verified_runtime(root,'1.8.207',{}, {'static':'same'},{'admin':'same'},cuda=context);fake.validate_runtime.assert_called_with(root,'1.8.207',local.TARGET,{},cuda=context)
 def test_installer_places_native_roster_beside_actual_service_image(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);src=root/'source';src.mkdir();airk=src/'airk.exe';ctl=src/'airkctl.exe';airk.write_bytes(b'fixture service');ctl.write_bytes(b'fixture cli');notices=src/'notice';notices.mkdir();(notices/'LICENSE').write_bytes(b'fixture notice')
   for name in ['LICENSE','NOTICE','THIRD_PARTY_NOTICES.md']:(root/name).write_bytes(b'fixture legal')
   owner=Mock();context={'projection':{'opaque':'native'},'handoff':{'opaque':'same'}}
   with patch.object(local,'ROOT',root),patch.object(local,'admin_digests',return_value={}),patch.object(local,'load_script',return_value=owner):local.stage(root/'layout',airk,ctl,'1.8.207',None,verified_notices=notices,cuda=context)
   owner.stage_native_projection.assert_called_once_with(context['projection'],root/'layout/service',context['handoff'])

 def test_installer_preserves_actual_direct_child_notice_names_and_bytes(self):
  native=load('cuda-package-provenance');owner=load('cuda-native-execution')
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);src=root/'source';src.mkdir();airk=src/'airk.exe';ctl=src/'airkctl.exe';airk.write_bytes(b'inert service');ctl.write_bytes(b'inert ctl');notices=src/'notices';notices.mkdir();(notices/'LICENSE').write_bytes(b'inert notice')
   for name in ['LICENSE','NOTICE','THIRD_PARTY_NOTICES.md']:(root/name).write_bytes(b'inert legal')
   payload={'onnxruntime_providers_shared.dll':b'inert shared','onnxruntime_providers_cuda.dll':b'inert cuda','airk-cuda-runtime.json':b'inert descriptor','airk-cuda-build-provenance.json':b'inert provenance','airk-cuda-notice-onnxruntime-license.txt':b'inert ORT license','airk-cuda-notice-cuda-license.txt':b'inert CUDA license','airk-cuda-notice-cudnn-license.txt':b'inert cuDNN license'}
   for name,raw in payload.items():(src/name).write_bytes(raw)
   value={'source_root':src,'files':{name:hashlib.sha256(raw).hexdigest() for name,raw in payload.items()}}
   context={'projection':value,'handoff':{}}
   with patch.object(local,'ROOT',root),patch.object(local,'admin_digests',return_value={}),patch.object(local,'load_script',return_value=native),patch.object(native,'reverify_native_projection',side_effect=lambda value,handoff:value),patch.object(native,'_execution_owner',return_value=owner):
    inventory=local.stage(root/'layout',airk,ctl,'1.8.207',None,verified_notices=notices,cuda=context)
   for name,raw in payload.items():
    self.assertEqual((root/'layout/service'/name).read_bytes(),raw)
    self.assertEqual(inventory['files']['service/'+name]['sha256'],hashlib.sha256(raw).hexdigest())
   self.assertFalse((root/'layout/service/notices/cuda-runtime').exists())

if __name__=='__main__':unittest.main()
