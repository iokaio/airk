# SPDX-License-Identifier: Apache-2.0
"""Cheap resource-owner controls only; no child/model/native/OS smoke."""
import ast,copy,importlib.util,json,subprocess,sys,unittest
from pathlib import Path
from unittest.mock import patch
BASE=Path(__file__).resolve().parent
NATIVE=BASE.parent/'software-v2-native-execution-preparation-r3'
CF=BASE.parent/'compact-native-diagnostic-owner-r1'
sys.path.insert(0,str(NATIVE))
import capacity_resource20 as capacity
import launch_resource20_current as launcher

def ast_function(path,name):return next(v for v in ast.walk(ast.parse(path.read_text())) if isinstance(v,ast.FunctionDef) and v.name==name)
def native_body():
 return {'schema':'airk.arbiter.native-resource/2','candidate_resource_passed':True,'numeric_status':'unavailable_in_resource_only_run','input':{'input_digest':'native-literal-digest','unknown':{'retained':True}},'resource':{'complete':True,'attempted_samples':20,'samples':[{'elapsed_us':123,'peak_process_rss_bytes':456}]*20,'failure':None,'reservation_bytes':16<<30,'nearest_rank_p95_us':123,'maximum_us':123}}
class Controls(unittest.TestCase):
 def test_full_hour_retention_is_bounded_and_every_observation_preserved_in_counter(self):
  clock=[0.];c=capacity.Capacity(clock=lambda:clock[0])
  for i in range(36001):
   clock[0]=i/10;c.observe({}, {'peak_rss':100+i%7,'peak_job_commit':200+i%11})
  self.assertEqual(c.observations,36001);self.assertLessEqual(len(c.samples),3601);self.assertLess(len(c.samples),4096)
  self.assertEqual(c.proof()['final_observation']['memory']['peak_rss'],100+36000%7)
 def test_between_snapshot_rss_and_commit_refuse_immediately_without_refunding_peaks(self):
  for field in ['peak_rss','peak_job_commit']:
   clock=[0.];c=capacity.Capacity(clock=lambda:clock[0]);c.observe({}, {'peak_rss':1,'peak_job_commit':1});clock[0]=.01
   value={'peak_rss':1,'peak_job_commit':1};value[field]=(16<<30)+1
   with self.subTest(field=field),self.assertRaises(ValueError):c.observe({},value)
   self.assertEqual(c.observations,2);self.assertEqual(len(c.samples),1)
   self.assertEqual(c.proof()['final_observation']['memory'][field],(16<<30)+1)
 def test_actual_reader_abi_and_all_limits_equal_verified_cf_source(self):
  old=ast.parse((CF/'capacity.py').read_text());new=ast.parse((NATIVE/'capacity_resource20.py').read_text())
  for name in ['WindowsRead','initial_free']:
   def node(t):return next(v for v in ast.walk(t) if isinstance(v,(ast.FunctionDef,ast.ClassDef)) and v.name==name)
   self.assertEqual(ast.dump(node(old)),ast.dump(node(new)))
  for name in ['GIB','GRAPH','WEIGHTS','RESERVATION','CONTAINMENT','FREE']:
   def assign(t):return next(v for v in ast.walk(t) if isinstance(v,ast.Assign) and any(isinstance(n,ast.Name) and n.id==name for n in v.targets))
   self.assertEqual(ast.dump(assign(old)),ast.dump(assign(new)))
  original=ast_function(CF/'capacity.py','observe');changed=ast_function(NATIVE/'capacity_resource20.py','observe')
  for node in ast.walk(changed):
   if isinstance(node,ast.Constant) and type(node.value) is float and node.value==1.:node.value=.1
  self.assertEqual(ast.dump(original),ast.dump(changed))
 def test_live_watcher_clock_cadence_and_factory_equal_verified_cf_source(self):
  old=ast.parse((CF/'watch.py').read_text());new=ast.parse((NATIVE/'watch_resource20.py').read_text())
  for node in ast.walk(new):
   if isinstance(node,ast.ImportFrom) and node.module=='capacity_resource20':node.module='capacity'
   if isinstance(node,ast.Constant) and node.value=='Original3600s resource phase expired':node.value='Original360s phase expired'
   if isinstance(node,ast.Constant) and node.value=='Original22operation population incomplete':node.value='Original2operation population incomplete'
  self.assertEqual(ast.dump(old),ast.dump(new))
 def test_full_resource_denominator_and_scope_are_independent_literals(self):
  b=native_body();plan={'reservation_bytes':16<<30,'expected_resource_input':copy.deepcopy(b['input'])};launcher.resource_result(b,plan)
  for field,value in [('complete',False),('attempted_samples',19),('samples',b['resource']['samples'][:19]),('failure','native_failure'),('reservation_bytes',8<<30)]:
   bad=copy.deepcopy(b);bad['resource'][field]=value
   with self.subTest(field=field),self.assertRaises(ValueError):launcher.resource_result(bad,plan)
  for field,value in [('candidate_resource_passed',False),('numeric_status','passed'),('schema','old')]:
   bad=copy.deepcopy(b);bad[field]=value
   with self.subTest(field=field),self.assertRaises(ValueError):launcher.resource_result(bad,plan)
 def test_whole_native_compiled_input_including_unknown_fields_must_match(self):
  b=native_body();plan={'reservation_bytes':16<<30,'expected_resource_input':copy.deepcopy(b['input'])};bad=copy.deepcopy(b);del bad['input']['unknown']
  with self.assertRaises(ValueError):launcher.resource_result(bad,plan)
 def test_original_absolute_owner_call_auth_and_final_clock_are_preserved(self):
  old=ast.parse((NATIVE/'launch_native.py').read_text());new=ast.parse((NATIVE/'launch_resource20_current.py').read_text())
  for name in ['finish_observations','check_combined_source_bridge','new_image']:
   def fn(t):return next(v for v in ast.walk(t) if isinstance(v,ast.FunctionDef) and v.name==name)
   self.assertEqual(ast.dump(fn(old)),ast.dump(fn(new)),name)
  for attr in ['run_windows_owned_training','suspended_authentication']:
   def call(t):return next(v for v in ast.walk(t) if isinstance(v,ast.Call) and isinstance(v.func,ast.Attribute) and v.func.attr==attr)
   self.assertEqual(ast.dump(call(old)),ast.dump(call(new)))
 def test_late_final_stdout_hash_cannot_pass_original_whole_deadline(self):
  r={'complete':True,'measurement_passed':True,'failure':None,'post_input_drift':[]};clock=[9.]
  def hashed(path):clock[0]=11.;return 'actual-control-hash'
  launcher.finish_observations(r,0.,10.,NATIVE/'launch_native.py',clock=lambda:clock[0],hash_file=hashed)
  self.assertFalse(r['complete']);self.assertTrue(r['whole_phase_expired'])
if __name__=='__main__':
 called=[]
 def nochild(*a,**k):called.append(1);raise AssertionError('No child in resource-owner controls')
 with patch.object(subprocess,'Popen',nochild):result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Controls))
 out={'selected':result.testsRun,'passed':result.testsRun-len(result.errors)-len(result.failures),'errors':len(result.errors),'failures':len(result.failures),'child_calls':len(called),'native_model_Cargo_runs':0}
 with (BASE/'controls-result.json').open('x',encoding='utf-8') as f:json.dump(out,f,indent=2);f.write('\n')
 raise SystemExit(0 if result.wasSuccessful() and not called else 1)
