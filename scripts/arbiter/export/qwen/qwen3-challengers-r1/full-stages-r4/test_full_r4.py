# SPDX-License-Identifier: Apache-2.0
"""Cheap full-profile admission: no Torch, model weights or NN execution."""
import ast,copy,json,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
import prepare_full_r4 as s
import full_worker_r4 as worker
class FullTests(unittest.TestCase):
 def tiny_review(self):return {'model_id':'Qwen/Qwen3-1.7B','authorized_stage':'qwen3-full-reference','prerequisite':{k:{'path':str((s.PARENT/n).resolve()),'sha256':pin} for k,(n,pin) in s.TINY.items()}}
 def test_exact_successful_tiny_chain(self):
  self.assertEqual(len(s.admit_prerequisite(self.tiny_review())['files']),49294)
 def test_raw_profile_only_four_approved_edits(self):
  raw=(s.HERE.parents[4]/'tools/retrieval-train/training_execution.py').read_bytes().decode('utf-8')
  self.assertEqual(s.sha(s.HERE.parents[4]/'tools/retrieval-train/training_execution.py'),'d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975')
  for name,wall,host,gpu in [('owned_profile_17.py',900,32,16),('owned_profile_4.py',1200,48,24)]:
   expected=raw.replace("Path(__file__).resolve().parents[2] / 'artifacts'","Path(__file__).resolve().parents[5] / 'artifacts'")
   expected=expected.replace("if seal['limits']['wall_seconds'] > 600 or seal['limits']['host_committed_bytes'] > 8 * 1024**3 or seal['limits']['cuda_allocator_bytes'] > 4 * 1024**3:",f"if seal['limits']['wall_seconds'] > {wall} or seal['limits']['host_committed_bytes'] > {host} * 1024**3 or seal['limits']['cuda_allocator_bytes'] > {gpu} * 1024**3:")
   self.assertEqual((s.HERE/name).read_bytes(),expected.encode('utf-8'))
 def test_whole_process_algorithm_ast_identical(self):
  raw=(s.HERE.parents[4]/'tools/retrieval-train/training_execution.py').read_text(encoding='utf-8')
  functions={x.name:ast.dump(x) for x in ast.parse(raw).body if isinstance(x,(ast.FunctionDef,ast.AsyncFunctionDef)) and x.name!='launch'}
  for name in ['owned_profile_17.py','owned_profile_4.py']:
   actual={x.name:ast.dump(x) for x in ast.parse((s.HERE/name).read_text(encoding='utf-8')).body if isinstance(x,(ast.FunctionDef,ast.AsyncFunctionDef)) and x.name!='launch'}
   self.assertEqual(actual,functions)
 def test_append_keeps_49294_old_pins_and_refuses_conflict(self):
  base=s.admit_prerequisite(self.tiny_review());p=Path(__file__).resolve();row={'path':str(p),'sha256':s.sha(p),'bytes':p.stat().st_size}
  result=s.append_rows(base,[row]);self.assertEqual(result['files'][:49294],base['files']);self.assertEqual(len(result['files']),49295)
  bad=copy.deepcopy(base['files'][0]);bad['sha256']='0'*64
  with self.assertRaisesRegex(ValueError,'conflict'):s.append_rows(base,[bad])
 def test_false_full_release_refuses_before_torch(self):
  with tempfile.TemporaryDirectory() as d:
   release=Path(d)/'release.json';release.write_text(json.dumps({'execution_authorized':False}),encoding='utf-8')
   seal=Path(d)/'seal.json';seal.write_text(json.dumps({'qwen_full_stage':{'release_path':str(release),'release_sha256':s.sha(release),'model_id':'Qwen/Qwen3-1.7B'}}),encoding='utf-8')
   with patch.object(sys,'argv',['worker','--seal',str(seal),'--seal-sha256',s.sha(seal)]):
    with self.assertRaisesRegex(ValueError,'separate exact'):worker.main()
   self.assertNotIn('torch',sys.modules)
 def test_exact_bounds_and_no_cross_model_fallback(self):
  self.assertEqual(s.LIMITS['Qwen/Qwen3-1.7B'],{'wall_seconds':900,'host_committed_bytes':34359738368,'cuda_allocator_bytes':17179869184,'minimum_gpu_free_mib':22528,'owned_process_slots':2})
  self.assertEqual(s.LIMITS['Qwen/Qwen3-4B'],{'wall_seconds':1200,'host_committed_bytes':51539607552,'cuda_allocator_bytes':25769803776,'minimum_gpu_free_mib':30720,'owned_process_slots':2})
  self.assertEqual(set(s.LIMITS),{'Qwen/Qwen3-1.7B','Qwen/Qwen3-4B'})
if __name__=='__main__':unittest.main(verbosity=2)
