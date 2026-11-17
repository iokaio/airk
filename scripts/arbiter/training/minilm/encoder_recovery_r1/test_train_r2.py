# SPDX-License-Identifier: Apache-2.0
import copy,unittest
from prepare_train_r2 import HERE,LIMITS,RESOURCE_PINS,PROTOCOL_SHA,validate_resource
class TrainPrerequisiteTests(unittest.TestCase):
 def setUp(self):
  self.sha='a'*64;self.seal={'limits':dict(LIMITS),'file_roles':{'trainer':{'sha256':RESOURCE_PINS['resource_pilot.py']}},'command':['python','resource_pilot.py','--output',str(HERE/'resource-pilot-r2.json'),'--execution-seal',str(HERE/'execution-seal-resource-r2.json'),'--execution-seal-sha256','<SEAL_SHA256>']};self.receipt={'seal_sha256':self.sha,'limits':dict(LIMITS),'failure':None,'command':[v.replace('<SEAL_SHA256>',self.sha) for v in self.seal['command']],'owned':{'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'owned_total_processes':2,'unexpected_third_process':False,'retained_member_exits':[{'exit_observed':True,'exit_code':0},{'exit_observed':True,'exit_code':0}]}};self.proof={'complete':True,'protocol_sha256':PROTOCOL_SHA,'script_sha256':RESOURCE_PINS['resource_pilot.py'],'cuda_equivalence_sha256':RESOURCE_PINS['cuda-equivalence-r2.json'],'synthetic_only':True,'pretrained_weights_or_task_text_decoded':False,'shape':{'queries':8,'units':27,'all_sequence_widths':256,'unit_microbatch':8},'steps':2,'optimizer':'AdamW fp32','peak_cuda_allocated_bytes':123456}
 def check(self):return validate_resource(self.seal,self.proof,self.receipt,self.sha)
 def test_exact_same_stage_prerequisite(self):self.check()
 def test_foreign_natural_receipt_not_sufficient(self):
  self.receipt['seal_sha256']='b'*64
  with self.assertRaises(ValueError):self.check()
 def test_foreign_actual_command_refused(self):
  self.receipt['command'][0]='foreign'
  with self.assertRaises(ValueError):self.check()
 def test_wrong_worst_shape_refused(self):
  self.proof['shape']['units']=26
  with self.assertRaises(ValueError):self.check()
 def test_missing_proof_or_widened_gpu_cap_refused(self):
  for key,value in [('complete',False),('peak_cuda_allocated_bytes',4*1024**3+1),('steps',1),('pretrained_weights_or_task_text_decoded',True)]:
   prior=self.proof[key];self.proof[key]=value
   with self.assertRaises(ValueError):self.check()
   self.proof[key]=prior
 def test_non_natural_owner_refused(self):
  self.receipt['owned']['owned_job_forced_termination']=True
  with self.assertRaises(ValueError):self.check()
 def test_wrong_protocol_or_script_refused(self):
  for key in ['protocol_sha256','script_sha256','cuda_equivalence_sha256']:
   prior=self.proof[key];self.proof[key]='b'*64
   with self.assertRaises(ValueError):self.check()
   self.proof[key]=prior
if __name__=='__main__':unittest.main()
