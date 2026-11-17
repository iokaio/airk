# SPDX-License-Identifier: Apache-2.0
"""Cheap byte/provenance/authorization controls; no Torch/NN or model-data reads."""
import ast,json,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
import prepare_quant as q
import quant_worker as worker
class QuantTests(unittest.TestCase):
 def test_exact_actual_both_fp32_proofs_and_source_ownership(self):
  for model,label in [('Qwen/Qwen3-1.7B','17'),('Qwen/Qwen3-4B','4')]:
   root=q.R4;proof=root/(label+'-export-r4-result/receipt.json');owned=root/('owned-'+label+'-export-r4/execution-receipt.json');seal=root/('execution-seal-'+label+'-export-r4.json')
   self.assertEqual(q.sha(proof),q.EXPECTED[model]['proof']);self.assertEqual(q.sha(owned),q.EXPECTED[model]['owned'])
   r=json.loads(owned.read_bytes());s=json.loads(seal.read_bytes());self.assertTrue(q.natural(r));self.assertEqual(r['seal_sha256'],q.sha(seal));self.assertEqual(r['command'],[x.replace('<SEAL_SHA256>',q.sha(seal)) for x in s['command']])
 def test_unchanged_reviewed_resource_profiles_and_distinct_precision_protocol(self):
  self.assertEqual(q.LIMITS['Qwen/Qwen3-1.7B']['wall_seconds'],900);self.assertEqual(q.LIMITS['Qwen/Qwen3-4B']['wall_seconds'],1200)
  self.assertEqual(q.LIMITS['Qwen/Qwen3-1.7B']['host_committed_bytes'],32*1024**3);self.assertEqual(q.LIMITS['Qwen/Qwen3-4B']['host_committed_bytes'],48*1024**3)
  self.assertEqual(q.NUMERIC,{'raw_max_absolute_error':0.25,'normalized_mass_max_error':0.05,'synthetic_argmax_must_match':True,'all_live_mask_control':True,'fp32_original_tolerance_unchanged':0.001,'all_live_against_original_trained_rows':True})
 def test_false_release_refuses_before_torch_or_owner(self):
  with tempfile.TemporaryDirectory() as d:
   release=Path(d)/'r.json';release.write_text(json.dumps({'execution_authorized':False}),encoding='utf-8');seal=Path(d)/'s.json';seal.write_text(json.dumps({'qwen_full_stage':{'release_path':str(release),'release_sha256':q.sha(release),'model_id':'Qwen/Qwen3-1.7B'}}),encoding='utf-8')
   with patch.object(sys,'argv',['worker','--seal',str(seal),'--seal-sha256',q.sha(seal)]):
    with self.assertRaisesRegex(ValueError,'separate exact'):worker.main()
   self.assertNotIn('torch',sys.modules)
 def test_all_preparation_sources_parse_without_importing_nn(self):
  for name in q.SOURCES:ast.parse((q.HERE/name).read_text(encoding='utf-8-sig'))
  self.assertNotIn('torch',sys.modules)
if __name__=='__main__':unittest.main(verbosity=2)
