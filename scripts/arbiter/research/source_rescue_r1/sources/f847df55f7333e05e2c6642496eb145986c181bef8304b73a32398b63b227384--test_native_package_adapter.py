# SPDX-License-Identifier: Apache-2.0
"""Cheap literal/refusal and copy plumbing only; no real native ownership fixture."""
import importlib.util,unittest,tempfile,hashlib
from pathlib import Path
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
def load(name):
 s=importlib.util.spec_from_file_location(name,HERE/'scripts'/(name+'.py'));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
n=load('cuda-native-execution');p=load('cuda-package-provenance')
class Controls(unittest.TestCase):
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
  events=[]
  class Log:
   def exists(self):return True
  v={'complete':True,'failure':None};n.finish_observation(v,0,10,Log(),clock=lambda:(events.append('clock') or 9),hash_file=lambda p:(events.append('hash') or {'sha256':'a'}));self.assertEqual(events,['hash','clock']);self.assertTrue(v['complete'])
 def test_exact_cutoff_refuses_completion(self):
  class Log:
   def exists(self):return False
  v={'complete':True,'failure':None};n.finish_observation(v,0,10,Log(),clock=lambda:10);self.assertFalse(v['complete']);self.assertTrue(v['whole_phase_expired'])
 def test_original_error_survives_retention_error(self):
  class Log:
   def exists(self):return True
  def failure(p):raise OSError('actual retention error')
  original={'type':'NativeError','message':'actual original'};v={'complete':False,'failure':original};n.finish_observation(v,0,10,Log(),clock=lambda:9,hash_file=failure);self.assertEqual(v['failure'],original);self.assertEqual(v['retention_failure']['type'],'OSError')
 def test_arbitrary_projection_without_parent_custody_refused(self):
  with self.assertRaises(p.NativeGpuProjectionUnavailable):p.require_native_activation_projection()
  with self.assertRaises(ValueError):p.require_native_activation_projection('x','a'*64,{},native_execution_receipt='not-tuple',repo_root=HERE)
 def test_stage_creates_only_exact_selected_bytes(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);source=root/'source';source.mkdir();dest=root/'dest';dest.mkdir();(source/'one.dll').write_bytes(b'one');(source/'notices').mkdir();(source/'notices/license.txt').write_bytes(b'license');files={name:hashlib.sha256((source/name).read_bytes()).hexdigest() for name in ['one.dll','notices/license.txt']};value={'files':files,'source_root':source}
   with patch.object(p,'reverify_native_projection',side_effect=lambda v,h:v),patch.object(p,'_execution_owner',return_value=n):self.assertEqual(p.stage_native_projection(value,dest,{}),files)
   self.assertEqual(sorted(x.relative_to(dest).as_posix() for x in dest.rglob('*') if x.is_file()),sorted(files))
 def test_stage_preserves_existing_entry_without_partial_copy(self):
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   root=Path(t);dest=root/'dest';dest.mkdir();(dest/'one.dll').write_bytes(b'existing');value={'files':{'one.dll':'a'*64,'two.dll':'b'*64},'source_root':root}
   with patch.object(p,'reverify_native_projection',side_effect=lambda v,h:v),patch.object(p,'_execution_owner',return_value=n):
    with self.assertRaises(ValueError):p.stage_native_projection(value,dest,{})
   self.assertEqual((dest/'one.dll').read_bytes(),b'existing');self.assertFalse((dest/'two.dll').exists())
if __name__=='__main__':unittest.main()
