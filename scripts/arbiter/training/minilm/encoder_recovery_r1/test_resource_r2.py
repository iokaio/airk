# SPDX-License-Identifier: Apache-2.0
import copy,json,unittest
from prepare_resource_r2 import HERE,PINS,validate_cuda
class ResourceR2Tests(unittest.TestCase):
 def setUp(self):
  self.seal=json.loads((HERE/'execution-seal-cuda-r2.json').read_bytes());self.proof=json.loads((HERE/'cuda-equivalence-r2.json').read_bytes());self.receipt=json.loads((HERE/'owned-cuda-r2/execution-receipt.json').read_bytes())
 def check(self):return validate_cuda(self.seal,self.proof,self.receipt,PINS['execution-seal-cuda-r2.json'],HERE/'cuda-equivalence-r2.json',HERE/'execution-seal-cuda-r2.json')
 def test_actual_r2_proof_command_and_natural_closure_bind(self):self.check()
 def test_arbitrary_natural_receipt_does_not_suffice(self):
  self.receipt['seal_sha256']='0'*64
  with self.assertRaisesRegex(ValueError,'seal/receipt'):self.check()
 def test_command_field_identity_preserved(self):
  self.receipt['command'][0]='foreign.exe'
  with self.assertRaisesRegex(ValueError,'command differs'):self.check()
 def test_foreign_output_path_refused_even_when_receipt_matches(self):
  at=self.seal['command'].index('--output')+1;self.seal['command'][at]=str(HERE/'foreign.json');self.receipt['command'][at]=str(HERE/'foreign.json')
  with self.assertRaisesRegex(ValueError,'output/seal path'):self.check()
 def test_wrong_protocol_or_script_proof_refused(self):
  for field in ['protocol_sha256','script_sha256']:
   original=self.proof[field];self.proof[field]='0'*64
   with self.assertRaisesRegex(ValueError,'actual CUDA proof'):self.check()
   self.proof[field]=original
 def test_incomplete_cpu_or_missing_cases_refused(self):
  for field,value in [('complete',False),('device','cpu'),('selected',5),('passed',5)]:
   original=self.proof[field];self.proof[field]=value
   with self.assertRaisesRegex(ValueError,'actual CUDA proof'):self.check()
   self.proof[field]=original
 def test_forced_or_unsettled_owner_refused(self):
  for field,value in [('owned_job_forced_termination',True),('owned_job_final_active_processes',1),('owned_job_final_member_pids',[1]),('owned_total_processes',1)]:
   original=self.receipt['owned'][field];self.receipt['owned'][field]=value
   with self.assertRaisesRegex(ValueError,'natural CUDA'):self.check()
   self.receipt['owned'][field]=original
 def test_widened_limits_refused(self):
  self.receipt['limits']['host_committed_bytes']+=1
  with self.assertRaisesRegex(ValueError,'natural CUDA'):self.check()
 def test_original_failed_r1_receipt_cannot_be_used(self):
  self.receipt=json.loads((HERE/'owned-cuda/execution-receipt.json').read_bytes())
  with self.assertRaises(ValueError):self.check()
if __name__=='__main__':unittest.main()
