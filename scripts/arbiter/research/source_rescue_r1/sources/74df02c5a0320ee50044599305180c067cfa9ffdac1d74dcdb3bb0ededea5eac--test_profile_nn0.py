# SPDX-License-Identifier: Apache-2.0
"""Literal NN0 whitelist and native metadata-link controls; no actual model/image calls."""
import importlib.util,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('profile_nn0',Path(__file__).with_name('run_profile_nn0.py'));n=importlib.util.module_from_spec(s);s.loader.exec_module(n)
class Fake:
 def __init__(self,docs,paths):self.docs=docs;self.paths=paths
 def pin(self,p):return {'path':str(p),'bytes':1,'sha256':'a'*64}
 def read(self,p):return self.docs[str(p)]
 def census(self,d):return [self.pin(p) for p in self.paths]
 @staticmethod
 def core_pin_matches(a,b):return isinstance(a,dict) and a.get('path')==b['path'] and a.get('sha256')==b['sha256']
class Controls(unittest.TestCase):
 def test_exact_two_selectors(self):
  prefix='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::';self.assertEqual(set(n.SELECTORS.values()),{prefix+'emit_cuda_component_probe_request',prefix+'emit_cuda_fp32_probe_metadata'})
 def test_model_or_assembly_selector_refused(self):
  for value in ['cuda_original_context_fp32_t17_component','emit_cuda_runtime_manifest_from_held_native_closure','emit_cuda_runtime_package_projection_from_held_native_closure']:
   with self.assertRaises(ValueError):n.output_paths({'selector':value,'environment':{}},before=True)
 def test_only_prepare_kind_allowed(self):
  for kind in ['assembly','package_projection']:
   with self.assertRaises(ValueError):n.output_paths({'selector':n.SELECTORS['request'],'environment':{'AIRK_CUDA_REQUEST_KIND':kind}},before=True)
 def fixture(self):
  paths=[Path('out')/x for x in ['profile.json','native-input.json','probe-request.json','commands.json','metadata-receipt.json']];cal=Path('out/native-cal.json');docs={str(paths[0]):{'manifest':'manifest-id','assets':[{'role':'calibration','name':'native-cal.json','bytes':1,'sha256':'a'*64}]},str(paths[1]):{'profile':{'path':str(paths[0]),'sha256':'a'*64}},str(paths[2]):{'kind':'probe','schema':'airk.private.cuda-component-probe-request/1','gpu_input':{'path':str(paths[1]),'sha256':'a'*64}},str(paths[4]):{'schema':'airk.private.cuda-probe-metadata/1','qualified':False,'model_requests_dispatched':False,'forward_count_observed':None,'request':{'path':'prepare.json','sha256':'b'*64},'probe_request':{'path':str(paths[2]),'sha256':'a'*64},'profile':'manifest-id','elapsed_us':1}};plan={'environment':{'AIRK_CUDA_QUALIFICATION_INPUT':'prepare.json','AIRK_CUDA_QUALIFICATION_INPUT_SHA256':'b'*64}};return paths,docs,plan,Fake(docs,paths+[cal])
 def test_native_six_file_roster_derived_without_constructor(self):
  paths,docs,plan,b=self.fixture();self.assertEqual(len(n.native_output('metadata',paths,plan,{},b)),6);b.paths.append(Path('out/unexpected.json'))
  with self.assertRaises(ValueError):n.native_output('metadata',paths,plan,{},b)
 def test_unsupported_forward_and_wrong_probe_pin_refused(self):
  for field,value in [('forward_count_observed',0),('probe_request',{'path':'wrong','sha256':'a'*64})]:
   paths,docs,plan,b=self.fixture();docs[str(paths[-1])][field]=value
   with self.assertRaises(ValueError):n.native_output('metadata',paths,plan,{},b)
if __name__=='__main__':unittest.main()
