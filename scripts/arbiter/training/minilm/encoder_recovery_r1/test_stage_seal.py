# SPDX-License-Identifier: Apache-2.0
import copy,unittest
from prepare_stage_seal import natural_owned
class ClosureTests(unittest.TestCase):
 def setUp(self):self.good={'failure':None,'owned':{'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'owned_total_processes':2,'unexpected_third_process':False,'retained_member_exits':[{'exit_observed':True,'exit_code':0},{'exit_observed':True,'exit_code':0}]}}
 def test_natural_two_process_owner(self):self.assertTrue(natural_owned(self.good))
 def test_unknown_owner(self):self.assertFalse(natural_owned({}))
 def test_failures_cannot_be_complete(self):
  for key,value in [('exit_code',1),('timed_out',True),('log_oversized',True),('owned_descendants_settled',False),('owned_job_forced_termination',True),('owned_job_final_active_processes',1),('owned_job_final_member_pids',[42]),('owned_total_processes',3),('unexpected_third_process',True),('retained_member_exits',[{'exit_observed':True,'exit_code':0}])]:
   case=copy.deepcopy(self.good);case['owned'][key]=value;self.assertFalse(natural_owned(case),key)
 def test_unobserved_or_failed_retained_member_refused(self):
  for replacement in [{'exit_observed':False,'exit_code':0},{'exit_observed':True,'exit_code':1}]:
   case=copy.deepcopy(self.good);case['owned']['retained_member_exits'][1]=replacement;self.assertFalse(natural_owned(case))
 def test_outer_failure_refused(self):
  self.good['failure']={'reason':'unknown'};self.assertFalse(natural_owned(self.good))
if __name__=='__main__':unittest.main()
