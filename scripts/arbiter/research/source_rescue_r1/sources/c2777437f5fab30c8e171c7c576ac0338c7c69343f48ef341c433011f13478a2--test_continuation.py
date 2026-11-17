# SPDX-License-Identifier: Apache-2.0
import importlib.util,unittest,datetime
from pathlib import Path
p=Path(__file__).with_name('continue_cuda_runtime.py');s=importlib.util.spec_from_file_location('continuation',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Owner:
 @staticmethod
 def successful(c):return c.get('exit_code')==0 and c.get('owned_descendants_settled') is True
class Controls(unittest.TestCase):
 def test_exact_single_source_delta_retains_all_other_rows(self):
  rows=[{'path':'other.cc','bytes':3,'sha256':'x'},{'path':'onnxruntime/core/session/onnxruntime_c_api.cc','bytes':4,'sha256':'old'}]
  self.assertEqual(m.changed_rows(rows,'old','new',5),[{'path':'other.cc','bytes':3,'sha256':'x'},{'path':'onnxruntime/core/session/onnxruntime_c_api.cc','bytes':5,'sha256':'new'}]);self.assertEqual(rows[1]['sha256'],'old')
 def test_wrong_before_refuses(self):
  with self.assertRaises(ValueError):m.changed_rows([{'path':'onnxruntime/core/session/onnxruntime_c_api.cc','sha256':'wrong'}],'old','new',5)
 def test_missing_duplicate_target_refuses(self):
  for rows in [[],[{'path':'onnxruntime/core/session/onnxruntime_c_api.cc','sha256':'old'}]*2]:
   with self.assertRaises(ValueError):m.changed_rows(rows,'old','new',5)
 def parent(self):return {'status':'FAILED_PRESERVED_NO_RETRY','commands':[{'exit_code':0,'owned_descendants_settled':True} for _ in range(7)]+[{'exit_code':1,'owned_descendants_settled':True,'owned_job_forced_termination':False,'timed_out':False,'log_oversized':False}]}
 def test_exact_natural_parent_failure_accepted_only_as_ancestry(self):m.verify_parent(self.parent(),Owner)
 def test_partial_or_successful_parent_refuses(self):
  for key,value in [('status','PASS_PRODUCER_ONLY_UNQUALIFIED'),('commands',[])]:
   p=self.parent();p[key]=value
   with self.assertRaises(ValueError):m.verify_parent(p,Owner)
 def test_unsettled_forced_timeout_or_noncompile_error_refuses(self):
  for key,value in [('owned_descendants_settled',False),('owned_job_forced_termination',True),('timed_out',True),('log_oversized',True),('exit_code',2)]:
   p=self.parent();p['commands'][-1][key]=value
   with self.assertRaises(ValueError):m.verify_parent(p,Owner)
 def test_interattempt_wall_gap_is_charged(self):
  t=datetime.datetime(2026,11,17,5,27,tzinfo=datetime.timezone.utc)
  self.assertEqual(m.wall_remaining(t,t+datetime.timedelta(seconds=2700),800),8100)
  self.assertEqual(m.wall_remaining(t,t+datetime.timedelta(seconds=500),900),9900)
 def test_expired_wall_refuses_even_when_active_budget_remains(self):
  t=datetime.datetime(2026,11,17,5,27,tzinfo=datetime.timezone.utc)
  self.assertLessEqual(m.wall_remaining(t,t+datetime.timedelta(seconds=10800),800),0)
  with self.assertRaises(ValueError):m.wall_remaining(t,t-datetime.timedelta(seconds=1),0)
if __name__=='__main__':unittest.main()
