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
  self.assertEqual(q.NUMERIC,{'raw_max_absolute_error':0.25,'normalized_mass_max_error':0.05,'synthetic_argmax_must_match':True,'all_live_mask_control':True,'fp32_original_tolerance_unchanged':0.001,'all_live_against_original_trained_rows':True,'projection_recipe':'MatMul-only 7/layer, embedding/gather-head FP32','single_thread_head_and_ORT':True,'full_graph_ranges_hash_before_after':True,'native_external_storage_normalized_exactly':True})
 def test_false_release_refuses_before_torch_or_owner(self):
  with tempfile.TemporaryDirectory() as d:
   release=Path(d)/'r.json';release.write_text(json.dumps({'execution_authorized':False}),encoding='utf-8');seal=Path(d)/'s.json';seal.write_text(json.dumps({'qwen_full_stage':{'release_path':str(release),'release_sha256':q.sha(release),'model_id':'Qwen/Qwen3-1.7B'}}),encoding='utf-8')
   with patch.object(sys,'argv',['worker','--seal',str(seal),'--seal-sha256',q.sha(seal)]):
    with self.assertRaisesRegex(ValueError,'separate exact'):worker.main()
   self.assertNotIn('torch',sys.modules)
 def test_all_preparation_sources_parse_without_importing_nn(self):
  for name in q.SOURCES:
   if name.endswith('.py'):ast.parse((q.HERE/name).read_text(encoding='utf-8-sig'))
  self.assertNotIn('torch',sys.modules)
 def test_projection_census_requires_all_seven_per_layer_and_no_fp32_leftover(self):
  from types import SimpleNamespace as S
  from projection_census import census
  embedding=S(name='embedding',dims=[151936,2048],data_type=1)
  weights=[S(name='w%d'%i,dims=[1,1],data_type=1) for i in range(196)]
  nodes=[S(name='m%d'%i,op_type='MatMul',input=['activation',w.name],attribute=[]) for i,w in enumerate(weights)]
  graph=S(graph=S(initializer=[embedding,*weights],node=nodes));self.assertEqual(census(graph,28,False)['constant_fp32_count'],196)
  graph.graph.node.pop()
  with self.assertRaisesRegex(ValueError,'census'):census(graph,28,False)
 def test_packed_projection_domain_symmetric_inputs_and_accuracy_are_exact(self):
  from types import SimpleNamespace as S
  from projection_census import census
  embedding=S(name='embedding',dims=[151936,2048],data_type=1)
  weights=[S(name='w%d'%i,dims=[1,1],data_type=2) for i in range(196)]
  nodes=[S(name='m%d'%i,domain='com.microsoft',op_type='MatMulNBits',input=['activation',w.name,'scale'],attribute=[S(name='bits',i=8),S(name='block_size',i=32)]) for i,w in enumerate(weights)]
  graph=S(graph=S(initializer=[embedding,*weights],node=nodes));self.assertEqual(census(graph,28,True)['packed_weight8_count'],196)
  nodes[0].domain=''
  with self.assertRaisesRegex(ValueError,'packed projection'):census(graph,28,True)
  nodes[0].domain='com.microsoft';nodes[0].input.append('zero_point')
  with self.assertRaisesRegex(ValueError,'packed projection'):census(graph,28,True)
  nodes[0].input.pop();nodes[0].attribute.append(S(name='accuracy_level',i=1))
  with self.assertRaisesRegex(ValueError,'packed projection'):census(graph,28,True)
 def test_two_file_full_range_hash_closure_and_malformed_range_refusals(self):
  from graph_closure import bind_references
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);(root/'model.onnx').write_bytes(b'graph');(root/'model.onnx.data').write_bytes(b'abcdefgh')
   refs=[('weight',[('location','model.onnx.data'),('offset','0'),('length','8')])]
   before=bind_references(root,refs);self.assertEqual([x['name'] for x in before['files']],['model.onnx','model.onnx.data'])
   (root/'model.onnx.data').write_bytes(b'ABCDEFGH');self.assertNotEqual(before,bind_references(root,refs))
   with self.assertRaises(ValueError):bind_references(root,[('weight',[('location','model.onnx.data'),('length','9')])])
   with self.assertRaises(ValueError):bind_references(root,[('weight',[('location','../escape'),('length','8')])])
   (root/'stray').write_bytes(b'x')
   with self.assertRaisesRegex(ValueError,'Unreferenced'):bind_references(root,refs)
if __name__=='__main__':unittest.main(verbosity=2)
