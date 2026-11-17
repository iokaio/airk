# SPDX-License-Identifier: Apache-2.0
"""Frozen-owner additive and refusal controls, stdlib only."""
import copy,json,tempfile,unittest,sys
from unittest.mock import patch
import qwen3_tiny_worker_r3 as worker
from pathlib import Path
import prepare_qwen3_tiny_r3 as subject
class OwnedTests(unittest.TestCase):
 def actual(self):return json.loads((subject.PREP/'owned-train-r3/execution-receipt.json').read_bytes())
 def test_exact_old_provenance(self):
  self.assertEqual(subject.sha(subject.PREP/'execution-seal-train-r3.json'),subject.BASE_SHA)
  self.assertEqual(subject.sha(subject.PREP/'owned-train-r3/execution-receipt.json'),subject.OWNED_SHA)
  self.assertTrue(subject.natural(self.actual()))
 def test_forced_remaining_nonzero_unobserved_refused(self):
  for name,value in [('owned_job_forced_termination',True),('timed_out',True),('owned_job_final_active_processes',1),('owned_job_final_member_pids',[123]),('owned_total_processes',3),('unexpected_third_process',True),('owned_descendants_settled',False),('exit_code',1)]:
   r=self.actual();r['owned'][name]=value;self.assertFalse(subject.natural(r),name)
  r=self.actual();r['owned']['retained_member_exits'][0]['exit_observed']=False;self.assertFalse(subject.natural(r))
 def test_append_preserves_every_old_row(self):
  base=json.loads((subject.PREP/'execution-seal-train-r3.json').read_bytes())
  new=subject.append_only(base,[Path(__file__)])
  self.assertEqual(len(base['files']),49277);self.assertEqual(new['files'][:49277],base['files']);self.assertEqual(len(new['files']),49278)
 def test_inherited_conflict_refused(self):
  p=Path(__file__).resolve();base={'files':[{'path':str(p),'sha256':'0'*64,'bytes':p.stat().st_size}]}
  with self.assertRaisesRegex(ValueError,'conflicting'):subject.append_only(base,[p])
 def test_false_release_refuses_before_owner_or_torch(self):
  with tempfile.TemporaryDirectory() as d:
   release=Path(d)/'release.json';release.write_text(json.dumps({'execution_authorized':False,'authorized_stage':'qwen3-tiny-export','model_id':None}),encoding='utf-8')
   seal=Path(d)/'seal.json';seal.write_text(json.dumps({'qwen_tiny_stage':{'release_path':str(release),'release_sha256':subject.sha(release)}}),encoding='utf-8')
   with patch.object(sys,'argv',['worker','--seal',str(seal),'--seal-sha256',subject.sha(seal)]):
    with self.assertRaisesRegex(ValueError,'tiny-only release'):worker.main()
   self.assertNotIn('torch',sys.modules)
 def test_no_unreviewed_fresh_third_script_included(self):
  self.assertEqual(set(subject.SCRIPTS),{'qwen3_reference_r2.py','qwen3_pilot_r2.py','qwen3_tiny_worker_r3.py','launch_qwen3_tiny_r3.py','prepare_qwen3_tiny_r3.py','test_qwen3_preparation_r2.py','test_qwen3_owned_r3.py'})
  self.assertEqual(subject.LIMITS,{'wall_seconds':600,'host_committed_bytes':8589934592,'cuda_allocator_bytes':4294967296,'minimum_gpu_free_mib':8192,'owned_process_slots':2})
if __name__=='__main__':unittest.main(verbosity=2)
