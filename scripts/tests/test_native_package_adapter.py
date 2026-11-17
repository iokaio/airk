# SPDX-License-Identifier: Apache-2.0
"""Cheap literal/refusal and copy plumbing only; no real native ownership fixture."""
import importlib.util,unittest,tempfile,hashlib
from types import SimpleNamespace
from pathlib import Path
from unittest.mock import patch
HERE=Path(__file__).resolve().parents[2]
def load(name):
 s=importlib.util.spec_from_file_location(name,HERE/'scripts'/(name+'.py'));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
n=load('cuda-native-execution');p=load('cuda-package-provenance')
class Controls(unittest.TestCase):
 def proof_paths(self):
  return {'projection':HERE/'literal-projection.json','native_emission':HERE/'literal-emission.json','stdout':HERE/'literal-stdout.log'}
 def proof_row(self,path):
  return {'path':str(path),'bytes':1,'sha256':'a'*64}

 def fixture(self):
  a={'path':'input','bytes':1,'sha256':'a'*64};d={'path':'descriptor','bytes':2,'sha256':'b'*64};o={'path':'output','bytes':3,'sha256':'c'*64};plan={'native_input':a,'descriptor':d};owned={'owned_leader_pid':123};v={'schema':'airk.private.cuda-runtime-package-projection-emission/1','input':a,'descriptor':d,'projection':o,'pid':123,'execution_user':n.SID,'elapsed_us':1,'model_requests_dispatched':False,'runtime_admission_dispatched':False,'forward_count_observed':None,'activation':False,'qualification':False};return v,plan,o,owned
 def test_only_actual_emission_identity_fields_match(self):
  v,plan,o,owned=self.fixture();n.emission_facts(v,plan,o,owned)
  for key,value in [('pid',124),('execution_user','other'),('projection',{'path':'output','sha256':'0'*64}),('elapsed_us',120000000)]:
   changed=dict(v);changed[key]=value
   with self.assertRaises(ValueError):n.emission_facts(changed,plan,o,owned)
 def test_dispatch_or_observed_forward_claim_refuses(self):
  v,plan,o,owned=self.fixture()
  for key,value in [('model_requests_dispatched',True),('runtime_admission_dispatched',True),('activation',True),('qualification',True),('forward_count_observed',0)]:
   changed=dict(v);changed[key]=value
   with self.assertRaises(ValueError):n.emission_facts(changed,plan,o,owned)
 def test_non_nn0_plan_refused_before_any_dispatch(self):
  for schema in ['airk.private.independent-torch-plan/1','airk.private.software-v2-owned-native-plan/1']:
   with self.assertRaises(ValueError):n.preflight({'schema':schema})

 def test_final_clock_follows_last_stdout_hash(self):
  events=[];proofs=self.proof_paths()
  def hash_file(path):events.append(path.name);return self.proof_row(path)
  value={'complete':True,'failure':None}
  n.finish_observation(value,0,10,proofs,clock=lambda:(events.append('clock') or 9),hash_file=hash_file)
  self.assertEqual(events,['literal-projection.json','literal-emission.json','literal-stdout.log','clock'])
  self.assertTrue(value['complete']);self.assertEqual(value['final_proof_success'],['projection','native_emission','stdout'])

 def test_exact_cutoff_refuses_completion(self):
  value={'complete':True,'failure':None}
  n.finish_observation(value,0,10,self.proof_paths(),clock=lambda:10,hash_file=self.proof_row)
  self.assertFalse(value['complete']);self.assertTrue(value['whole_phase_expired']);self.assertEqual(value['elapsed_seconds'],10)

 def test_original_error_survives_retention_error(self):
  original={'type':'NativeError','message':'actual original'};value={'complete':False,'failure':original};events=[]
  def failure(path):events.append(path.name);raise OSError('actual retention error')
  n.finish_observation(value,0,10,self.proof_paths(),clock=lambda:(events.append('clock') or 9),hash_file=failure)
  self.assertEqual(value['failure'],original);self.assertEqual(value['retention_failure']['type'],'OSError')
  self.assertEqual(len(value['final_proof_errors']),3);self.assertEqual(events[-1],'clock')
 def test_arbitrary_projection_without_parent_custody_refused(self):
  with self.assertRaises(p.NativeGpuProjectionUnavailable):p.require_native_activation_projection()
  with self.assertRaises(ValueError):p.require_native_activation_projection('x','a'*64,{},native_execution_receipt='not-tuple',repo_root=HERE)
 def test_stage_creates_only_exact_selected_bytes(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);source=root/'source';source.mkdir();dest=root/'dest';dest.mkdir();(source/'one.dll').write_bytes(b'one');(source/'airk-cuda-notice-cuda-license.txt').write_bytes(b'license');files={name:hashlib.sha256((source/name).read_bytes()).hexdigest() for name in ['one.dll','airk-cuda-notice-cuda-license.txt']};value={'files':files,'source_root':source}
   with patch.object(p,'reverify_native_projection',side_effect=lambda v,h:v),patch.object(p,'_execution_owner',return_value=n):self.assertEqual(p.stage_native_projection(value,dest,{}),files)
   self.assertEqual(sorted(x.relative_to(dest).as_posix() for x in dest.rglob('*') if x.is_file()),sorted(files))
 def test_stage_preserves_existing_entry_without_partial_copy(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);dest=root/'dest';dest.mkdir();(dest/'one.dll').write_bytes(b'existing');value={'files':{'one.dll':'a'*64,'two.dll':'b'*64},'source_root':root}
   with patch.object(p,'reverify_native_projection',side_effect=lambda v,h:v),patch.object(p,'_execution_owner',return_value=n):
    with self.assertRaises(ValueError):p.stage_native_projection(value,dest,{})
   self.assertEqual((dest/'one.dll').read_bytes(),b'existing');self.assertFalse((dest/'two.dll').exists())

class MandatoryProofControls(unittest.TestCase):
 proof_paths=Controls.proof_paths
 proof_row=Controls.proof_row
 def test_skipped_missing_proof_cannot_pass_and_all_callbacks_finish_before_clock(self):
  for missing in ['projection','native_emission','stdout']:
   with self.subTest(missing=missing):
    value={'complete':True,'failure':None};proofs=self.proof_paths();events=[]
    def observed(path):
     events.append(path.name)
     return None if path==proofs[missing] else self.proof_row(path)
    n.finish_observation(value,0,10,proofs,clock=lambda:(events.append('clock') or 9),hash_file=observed)
    self.assertFalse(value['complete']);self.assertNotIn(missing,value['final_proof_success'])
    self.assertEqual(len(events),4);self.assertEqual(events[-1],'clock')
 def test_missing_label_and_malformed_sha_row_cannot_pass(self):
  for edit in ['label','sha','path','size']:
   with self.subTest(edit=edit):
    value={'complete':True,'failure':None};proofs=self.proof_paths()
    if edit=='label':proofs.pop('projection')
    def observed(path):
     row=self.proof_row(path)
     if edit=='sha':row.pop('sha256')
     if edit=='path':row['path']=str(HERE/'wrong-output')
     if edit=='size':row['bytes']=(64<<20)+1
     return row
    n.finish_observation(value,0,10,proofs,clock=lambda:9,hash_file=observed)
    self.assertFalse(value['complete']);self.assertTrue(value['final_proof_errors'])
 def test_final_drift_keeps_original_native_pin_and_failure(self):
  original={'type':'OriginalNativeFailure'};value={'complete':False,'failure':original};proofs=self.proof_paths()
  value['projection']={'path':str(proofs['projection']),'bytes':1,'sha256':'b'*64}
  n.finish_observation(value,0,10,proofs,clock=lambda:9,hash_file=self.proof_row)
  self.assertEqual(value['failure'],original);self.assertEqual(value['projection']['sha256'],'b'*64)
  self.assertEqual(value['final_proof_drift']['projection']['observed']['sha256'],'a'*64)
 def test_established_no_site_parent_roster_is_exact(self):
  expected={'reference_owned.py':'2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1','reference_parent_no_site_r2.py':'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d','reference_worker.py':'9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'}
  self.assertEqual(n.HELPERS,expected)
  parent=SimpleNamespace(worker=SimpleNamespace(__file__=HERE/'reference_worker.py'));loaded=[]
  def load(path,name):loaded.append(Path(path).name);return parent if 'parent' in Path(path).name else object()
  with patch.object(n.sys,'path',list(n.sys.path)),patch.object(n,'regular',side_effect=Path),patch.object(n,'digest',side_effect=lambda p:expected[Path(p).name]),patch.object(n,'load',side_effect=load):
   n.helpers({'ownership_helper_root':str(HERE)})
  self.assertEqual(loaded,['reference_parent_no_site_r2.py','reference_owned.py'])
 def test_wrong_parent_pin_refuses_before_any_load(self):
  with patch.object(n,'regular',side_effect=Path),patch.object(n,'digest',return_value='0'*64),patch.object(n,'load') as loader:
   with self.assertRaisesRegex(ValueError,'ownership primitive'):n.helpers({'ownership_helper_root':str(HERE)})
   loader.assert_not_called()


class NativeProjectionLegalBoundary(unittest.TestCase):
 # Independent literal shape from Rust windows::observe_package_projection.
 # Native execution authenticity is injected; this is never an actual NN0 pass.
 def literal(self,root):
  runtime=root/'runtime';runtime.mkdir()
  payload={'airk-cuda-runtime.json':b'literal descriptor','airk-cuda-build-provenance.json':b'literal public provenance','onnxruntime_providers_shared.dll':b'inert shared provider','onnxruntime_providers_cuda.dll':b'inert cuda provider','airk-cuda-notice-onnxruntime-license.txt':b'literal ORT license','airk-cuda-notice-cuda-license.txt':b'literal CUDA license','airk-cuda-notice-cudnn-license.txt':b'literal cuDNN license'}
  for name,raw in payload.items():(runtime/name).write_bytes(raw)
  pins=[n.pin(runtime/name) for name in sorted(payload)]
  rows=[{'name':Path(row['path']).name,'bytes':row['bytes'],'sha256':row['sha256']} for row in pins]
  value={'schema':'airk.native.cuda-package-projection/1','descriptor':next(row for row in rows if row['name']=='airk-cuda-runtime.json'),'static_core_build':'9'*64,'files':rows,'scope':'literal native-shaped descriptive fixture','activation':False,'qualification':False}
  projection=root/'projection.json';execution=root/'execution.json';execution.write_bytes(b'inert injected custody receipt')
  plan={'runtime_root':str(runtime),'runtime_files':pins,'descriptor':n.pin(runtime/'airk-cuda-runtime.json')}
  owner=SimpleNamespace(pin=n.pin,regular=n.regular,read=n.read,digest=n.digest,verify_execution=lambda *a,**kw:(plan,{}))
  handoff={'activation_pins_available':False,'gpu_build':{'schema':'airk.cuda-static-build-proof/2','build_receipt':{'sha256':'9'*64}}}
  return payload,value,projection,execution,plan,owner,handoff
 def consume(self,value,projection,execution,owner,handoff):
  import json
  projection.write_text(json.dumps(value),encoding='utf-8')
  with patch.object(p,'_execution_owner',return_value=owner):
   return p.require_native_activation_projection(projection,n.digest(projection),handoff,native_execution_receipt=(execution,n.digest(execution)),repo_root=HERE)
 def test_complete_native_shaped_legal_roster_is_accepted_and_staged_without_renaming(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);payload,value,projection,execution,plan,owner,handoff=self.literal(root)
   observed=self.consume(value,projection,execution,owner,handoff)
   self.assertEqual(set(observed['files']),{'airk-cuda-runtime.json','airk-cuda-build-provenance.json','onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.dll','airk-cuda-notice-onnxruntime-license.txt','airk-cuda-notice-cuda-license.txt','airk-cuda-notice-cudnn-license.txt'})
   destination=root/'staged';destination.mkdir()
   with patch.object(p,'_execution_owner',return_value=owner):
    p.stage_native_projection(observed,destination,handoff)
   self.assertEqual({file.name:file.read_bytes() for file in destination.iterdir()},payload)
 def test_mutated_legal_names_are_refused_at_the_real_projection_boundary(self):
  import copy
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);payload,value,projection,execution,plan,owner,handoff=self.literal(root)
   for name in ['notices/cuda-runtime/license.txt','airk-cuda-notice-cuda-license.dll','AIRK-CUDA-NOTICE-cuda-license.txt','../airk-cuda-notice-cuda-license.txt','cuda-license.txt']:
    changed=copy.deepcopy(value);next(row for row in changed['files'] if row['name']=='airk-cuda-notice-cuda-license.txt')['name']=name
    changed['files'].sort(key=lambda row:row['name'])
    with self.subTest(name=name),self.assertRaises(ValueError):self.consume(changed,projection,execution,owner,handoff)
 def test_omitted_extra_duplicate_and_tampered_legal_rows_cannot_replace_native_census(self):
  import copy
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);payload,value,projection,execution,plan,owner,handoff=self.literal(root)
   for kind in ['omitted','extra','duplicate','hash']:
    changed=copy.deepcopy(value);notice=next(row for row in changed['files'] if row['name']=='airk-cuda-notice-cuda-license.txt')
    if kind=='omitted':changed['files'].remove(notice)
    elif kind=='duplicate':changed['files'].append(dict(notice))
    elif kind=='hash':notice['sha256']='0'*64
    else:
     extra=Path(plan['runtime_root'])/'airk-cuda-notice-extra-license.txt';extra.write_bytes(b'inert extra outside original census');row=n.pin(extra);changed['files'].append({'name':extra.name,'bytes':row['bytes'],'sha256':row['sha256']})
    changed['files'].sort(key=lambda row:row['name'])
    with self.subTest(kind=kind),self.assertRaises(ValueError):self.consume(changed,projection,execution,owner,handoff)

if __name__=='__main__':unittest.main()
