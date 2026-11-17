# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import copy,hashlib,importlib.util,json,tempfile,unittest
p=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('proposed_worker',p/'reference_worker.py');w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)
class Binding(unittest.TestCase):
 def plan(self):return {'kind':'same_artifact','upstream':'Qwen/Qwen3-1.7B','native_template':{'sha256':w.GQA_TEMPLATE_SHA},'compact_gqa_binding':{'source_graph':{'sha256':w.GQA_SOURCE_SHA},'shape_proof':{'sha256':w.GQA_PROOF_SHA}}}
 def test_exact_case_only_and_legacy_still_closed(self):
  plan=self.plan();profile={'model':'arbiter-context-qwen3-17b'}
  self.assertTrue(w.exact_gqa_context(plan,profile,w.GQA_GRAPH_SHA))
  legacy=copy.deepcopy(plan);del legacy['compact_gqa_binding'];self.assertFalse(w.exact_gqa_context(legacy,profile,w.GQA_SOURCE_SHA))
  with self.assertRaises(ValueError):w.exact_gqa_context(legacy,profile,w.GQA_GRAPH_SHA)
  for key,value in [('upstream','Qwen/Qwen3-4B'),('kind','original_fp32')]:
   changed=copy.deepcopy(plan);changed[key]=value
   with self.assertRaises(ValueError):w.exact_gqa_context(changed,profile,w.GQA_GRAPH_SHA)
  for key in ['source_graph','shape_proof']:
   changed=copy.deepcopy(plan);changed['compact_gqa_binding'][key]['sha256']='0'*64
   with self.assertRaises(ValueError):w.exact_gqa_context(changed,profile,w.GQA_GRAPH_SHA)
  with self.assertRaises(ValueError):w.exact_gqa_context(plan,profile,'0'*64)
  changed=copy.deepcopy(plan);changed['native_template']['sha256']='0'*64
  with self.assertRaises(ValueError):w.exact_gqa_context(changed,profile,w.GQA_GRAPH_SHA)
 def test_real_separate_directory_native_filemap_resolution(self):
  from types import SimpleNamespace as N
  with tempfile.TemporaryDirectory(dir=p) as tmp:
   root=Path(tmp);assets=root/'assets';graph=root/'graph';assets.mkdir();graph.mkdir();path=(assets/'model.onnx.data').resolve();path.write_bytes(b'not-neural-fixture')
   tensor=N(external_data=[N(key='location',value='model.onnx.data')]);asset={'name':'model.onnx.data','role':'external_weights','bytes':18,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
   request={'files':{'model.onnx.data':str(path)}};profile={'assets':[asset]};plan={'input_seals':{str(path):asset['sha256']}}
   self.assertEqual(len(path.read_bytes()),18);self.assertEqual(w.mapped_parameter_directory(tensor,request,profile,plan),assets.resolve());self.assertNotEqual(assets,graph)
   for changed in [{'assets':[dict(asset,role='model')]},{'assets':[asset,asset]}]:
    with self.assertRaises(ValueError):w.mapped_parameter_directory(tensor,request,changed,plan)
   with self.assertRaises(ValueError):w.mapped_parameter_directory(tensor,{'files':{}},profile,plan)
   with self.assertRaises(ValueError):w.mapped_parameter_directory(tensor,request,profile,{'input_seals':{str(path):'0'*64}})
   for value in ['../model.onnx.data','other.bin']:
    with self.assertRaises(ValueError):w.mapped_parameter_directory(N(external_data=[N(key='location',value=value)]),request,profile,plan)
if __name__=='__main__':unittest.main()