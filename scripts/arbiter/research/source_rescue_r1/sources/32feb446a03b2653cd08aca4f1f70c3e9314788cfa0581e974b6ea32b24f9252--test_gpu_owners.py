# SPDX-License-Identifier: Apache-2.0
"""Small structural/refusal tests only. No genuine GPU proof fabricated."""
from pathlib import Path
import importlib.util,struct,tempfile,unittest,hashlib
HERE=Path(__file__).resolve().parent

def load(name):
 p=HERE/'scripts'/(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
a=load('cuda-static-archive');runtime=load('cuda-static-runtime');package=load('cuda-package-provenance')

def coff(section=b'.text',body=b'X',machine=0x8664,big=False):
 if big:head=struct.pack('<HHHHI',0,0xffff,2,machine,0)+a.BIGOBJ+struct.pack('<IIIIIII',0,0,0,0,1,0,0)
 else:head=struct.pack('<HHIIIHH',machine,1,0,0,0,0,0)
 row=section.ljust(8,b'\0')+struct.pack('<IIIIIIHHI',0,0,len(body),len(head)+40,0,0,0,0,0)
 return head+row+body

def library(*members):
 data=b'!<arch>\n'
 for i,member in enumerate(members):
  name=('object'+str(i)+'.obj/').encode().ljust(16);header=name+b'0'.ljust(12)+b'0'.ljust(6)+b'0'.ljust(6)+b'100644'.ljust(8)+str(len(member)).encode().ljust(10)+b'`\n';data+=header+member+(b'\n' if len(member)%2 else b'')
 return data

NINJA='''
build onnxruntime_session.lib: CXX_STATIC_LIBRARY_LINKER__session a.obj || onnxruntime_common.lib onnxruntime_providers_shared.lib
build onnxruntime_common.lib: CXX_STATIC_LIBRARY_LINKER__common b.obj
build onnxruntime_providers_shared.dll onnxruntime_providers_shared.lib: CXX_SHARED_LIBRARY_LINKER__shared c.obj
build onnxruntime_providers_cuda.dll: CUDA_MODULE_LIBRARY_LINKER__cuda d.obj
'''
class GpuOwnerControls(unittest.TestCase):
 def inspect(self,data):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   p=Path(t)/'archive.lib';p.write_bytes(data);return a.inspect_archive(p)
 def test_regular_bigobj_and_export_directive(self):
  result=self.inspect(library(coff(),coff(big=True),coff(b'.drectve',b' /EXPORT:airk_ort_collect_final_placement_v1 ')))
  self.assertEqual(result['kind'],'static');self.assertEqual(result['members'],3);self.assertIn('airk_ort_collect_final_placement_v1',result['export_directives'])
 def test_short_import_and_idata_not_static(self):
  body=b'F\0onnxruntime_providers_shared.dll\0';short=struct.pack('<HHHHIIHH',0,0xffff,0,0x8664,0,len(body),0,0)+body
  result=self.inspect(library(short));self.assertEqual(result['kind'],'import_or_mixed');self.assertEqual(result['imported_dlls'],['onnxruntime_providers_shared.dll'])
  self.assertEqual(self.inspect(library(coff(b'.idata$2')))['kind'],'import_or_mixed')
 def test_x86_corrupt_and_empty_library_refusals(self):
  for data in [library(coff(machine=0x14c)),b'!<arch>\n',library(coff())[:-1],b'MZ'+b'X'*100]:
   with self.subTest(data=data[:8]):
    with self.assertRaises(ValueError):self.inspect(data)
 def test_exact_object_bytes_no_foreign_aggregate_members(self):
  original=self.inspect(library(coff()));same=self.inspect(library(coff()));a.verify_member_closure([original],same)
  with self.assertRaises(ValueError):a.verify_member_closure([original],self.inspect(library(coff(body=b'Y'))))
 def test_identical_input_objects_may_coalesce(self):
  original=self.inspect(library(coff()));a.verify_member_closure([original,original],self.inspect(library(coff())))
 def test_ninja_static_roots_exclude_shared_import_and_no_fake_cuda_lib(self):
  result=a.runtime_closure(NINJA,['onnxruntime_session.lib']);self.assertEqual(result['static_inputs'],['onnxruntime_common.lib','onnxruntime_session.lib']);self.assertEqual(result['provider_modules']['onnxruntime_providers_cuda.dll']['import_library'],None);self.assertEqual(result['excluded_ordered_provider_imports'],[{'path':'onnxruntime_providers_shared.lib','module':'onnxruntime_providers_shared.dll'}])
 def test_direct_import_cycle_unknown_and_second_core_refuse(self):
  changes=[NINJA.replace('a.obj ||','a.obj onnxruntime_providers_shared.lib ||'),NINJA.replace('b.obj','b.obj || onnxruntime_session.lib'),NINJA.replace('|| onnxruntime_common.lib','|| foreign.lib',1),NINJA+'build onnxruntime.dll onnxruntime.lib: CXX_SHARED_LIBRARY_LINKER__other z.obj\n']
  for text in changes:
   with self.subTest(text=text[:80]):
    with self.assertRaises(ValueError):a.runtime_closure(text,['onnxruntime_session.lib'])
 def test_missing_actual_gpu_receipt_is_unavailable(self):
  repo=Path('D:/code/Github/airk');snapshot=repo/'artifacts/v190/final-plan/exec-20261116T052942Z/gpu-runtime-strategy-proposal-r1/gpu-provision-owner-proposal-r8'
  with self.assertRaises(runtime.GpuProofUnavailable):runtime.verify_build(HERE/'missing-never-created',snapshot/'provision_cuda_runtime.py',snapshot/'cuda-runtime-producer-lock.json','0'*64,repo)
  self.assertFalse((HERE/'missing-never-created').exists())
 def test_public_projection_absence_and_private_payload_refusals(self):
  with self.assertRaises(package.NativeGpuProjectionUnavailable):package.require_native_activation_projection()
  native={'onnxruntime_providers_shared.dll':'a'*64,'onnxruntime_providers_cuda.dll':'b'*64,'airk-cuda-runtime.json':'c'*64};legal={'notices/cuda-runtime/LICENSE.txt':'d'*64}
  for name in ['nvcuda.dll','onnxruntime.dll','source/model.safetensors','fixture.json','C:/private/file.dll']:
   with self.subTest(name=name):
    with self.assertRaises(ValueError):package.exact_optional_payload({},dict(native,**{name:'e'*64}),legal)
 def test_cpu_and_gpu_static_proof_kinds_cannot_mix(self):
  with self.assertRaises(runtime.GpuProofUnavailable):runtime.static_projection({'schema':'cpu-proof'}, {'schema':'airk.cuda-static-aggregate-proof/1'})
 def test_unreviewed_continuation_source_bridge_refuses_before_proof(self):
  owner=load('cuda-static-continuation')
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   proposal=Path(t);(proposal/'continue_cuda_runtime.py').write_bytes(b'not the reviewed producer')
   with self.assertRaisesRegex(ValueError,'reviewed source/input bridge'):
    owner.verify_continuation(HERE,HERE,proposal,'0'*64,Path('D:/code/Github/airk'))
 def test_r12_refuses_old_schema_or_faked_completed_header(self):
  owner=load('cuda-static-continuation-r12')
  for value in [{'schema':'airk.private.cuda-runtime-continuation/1','status':'PASS_PRODUCER_ONLY_UNQUALIFIED'},{'schema':'airk.private.cuda-runtime-continuation/2','status':'PASS_PRODUCER_ONLY_UNQUALIFIED','continuation_script_sha256':'0'*64}]:
   with self.assertRaisesRegex(ValueError,'actual reviewed R12 PASS'):owner.require_r12_header(value)
 def test_r12_refuses_older_source_bridge_before_any_receipt(self):
  owner=load('cuda-static-continuation-r12')
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   proposal=Path(t);(proposal/'continue_cuda_runtime.py').write_bytes(b'old R9/R11 source is not R12')
   with self.assertRaisesRegex(ValueError,'reviewed source/input bridge'):owner.verify_continuation(HERE,HERE,proposal,'0'*64,Path('D:/code/Github/airk'))
 def test_r12_old_input_omission_or_substitution_refuses(self):
  owner=load('cuda-static-continuation-r12');old=[{'path':str(i),'sha256':'a'*64,'bytes':1} for i in range(44)]
  for current in [old[:-1],old[:20]+[{'path':'foreign','sha256':'b'*64,'bytes':1}]+old[21:]]:
   with self.assertRaises(ValueError):owner.require_original_input_prefix(old,current)
 def test_r12_original_input_bytes_are_reopened(self):
  repo=Path('D:/code/Github/airk');snapshot=repo/'artifacts/v190/final-plan/exec-20261116T052942Z/gpu-runtime-strategy-proposal-r1/gpu-provision-owner-proposal-r8'
  o=runtime.load(repo/'scripts/provision-static-onnx-runtime.py','cpu_old_input_control');recipe=runtime.load(snapshot/'provision_cuda_runtime.py','actual_r8_old_input_control')
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   q=Path(t)/'source';q.write_bytes(b'original');row={'path':str(q),'bytes':8,'sha256':hashlib.sha256(b'original').hexdigest()};q.write_bytes(b'changed!')
   with self.assertRaises(ValueError):recipe.verify_input_closure(o,[row])
class CheckpointRefusals(unittest.TestCase):
 def test_failed_r12_cannot_be_checkpointed_pass(self):
  checkpoint=load('cuda-static-checkpointed')
  for value in [{}, {'schema':'airk.private.cuda-runtime-continuation/2','status':'PASS_PRODUCER_ONLY_UNQUALIFIED'}, {'schema':'airk.private.cuda-checkpointed-build/1','status':'FAILED_PRESERVED_NO_RETRY'}]:
   with self.assertRaisesRegex(ValueError,'exact successful checkpointed'):checkpoint.require_header(value)
 def test_checkpoint_header_requires_every_exact_authority(self):
  checkpoint=load('cuda-static-checkpointed');v={'schema':'airk.private.cuda-checkpointed-build/1','status':'PASS_CHECKPOINTED_PRODUCER_ONLY_UNQUALIFIED','mode':'checkpointed-warm','qualification':False,'cold_build_qualified':False,'producer_source':{'sha256':checkpoint.COMPOSER},'input_contract':{'sha256':checkpoint.INPUT},'plan':{'sha256':checkpoint.PLAN},'original_failed_build':{'sha256':checkpoint.R12}}
  # Header-only control, never a build/owner/source proof.
  checkpoint.require_header(v)
  for key in ['producer_source','input_contract','plan','original_failed_build']:
   changed=dict(v);changed[key]={'sha256':'0'*64}
   with self.assertRaises(ValueError):checkpoint.require_header(changed)
  for key in ['qualification','cold_build_qualified']:
   changed=dict(v);changed[key]=True
   with self.assertRaises(ValueError):checkpoint.require_header(changed)
 def test_checkpoint_missing_composite_refuses_without_outputs(self):
  checkpoint=load('cuda-static-checkpointed');repo=Path('D:/code/Github/airk');E=repo/'artifacts/v190/final-plan/exec-20261116T052942Z'
  with self.assertRaises((ValueError,OSError)):checkpoint.verify_checkpointed(HERE,HERE/'missing-composite-never-created',E/'gpu-checkpointed-build-composer-r1','0'*64,repo)
  self.assertFalse((HERE/'missing-composite-never-created').exists())
 def test_checkpoint_execution_context_cannot_consume_old_proof(self):
  with self.assertRaisesRegex(ValueError,'proof/2'):load('cuda-static-checkpointed').execution_context({'schema':'airk.cuda-static-build-proof/1'},None)

class CanonicalPinControl(unittest.TestCase):
 def test_actual_tool_probe_metadata_stays_outside_canonical_proof(self):
  import json
  actual=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/cu8-continuation-r12/gpu-build-continuation-receipt.json')
  tools=json.loads(actual.read_bytes())['toolchain'];pins=load('cuda-static-checkpointed').toolchain_pins(tools)
  self.assertEqual(len(pins),6)
  for row in pins:
   self.assertEqual(set(row),{'path','bytes','sha256'});self.assertIs(type(row['bytes']),int);self.assertIsInstance(row['path'],str);self.assertEqual(len(row['sha256']),64)
  self.assertNotIn('probe',str(pins));runtime.canonical(pins)

if __name__=='__main__':unittest.main()


