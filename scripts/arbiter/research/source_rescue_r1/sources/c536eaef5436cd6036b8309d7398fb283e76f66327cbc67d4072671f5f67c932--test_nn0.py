# SPDX-License-Identifier: Apache-2.0
"""Literal metadata selection/refusal controls only; no native image calls or copies."""
import importlib.util,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('nn0',Path(__file__).with_name('run_metadata_nn0.py'));n=importlib.util.module_from_spec(s);s.loader.exec_module(n)
class Fake:
 SID='human'
 def __init__(self,value):self.value=value
 def pin(self,path):return {'path':str(path),'bytes':1,'sha256':'a'*64}
 def read(self,path):return self.value
 @staticmethod
 def core_pin_matches(a,b):return isinstance(a,dict) and a.get('path')==b['path'] and a.get('sha256')==b['sha256']
class Controls(unittest.TestCase):
 def test_three_selectors_only_independent_literal_roster(self):
  prefix='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::'
  self.assertEqual(set(n.SELECTORS.values()),{prefix+'emit_cuda_metadata_custody_inputs',prefix+'emit_cuda_component_probe_request',prefix+'emit_cuda_runtime_manifest_from_held_native_closure'})
 def test_component_or_package_selector_not_admitted(self):
  for selector in ['emit_cuda_fp32_probe_metadata','emit_cuda_runtime_package_projection_from_held_native_closure','selected_cuda_probe']:
   with self.assertRaises(ValueError):n.output_paths({'selector':selector,'environment':{},'output_pins_expected':[]})
 def test_request_must_be_assembly(self):
  for mode in ['prepare','package_projection']:
   with self.assertRaises(ValueError):n.output_paths({'selector':n.SELECTORS['request'],'environment':{'AIRK_CUDA_REQUEST_KIND':mode},'output_pins_expected':[]})
 def test_opaque_native_request_output_kind_refusal(self):
  for value in [{'kind':'prepare','schema':'airk.private.cuda-component-probe-request/1'},{'kind':'assembly','schema':'wrong'}]:
   with self.assertRaises(ValueError):n.native_output('request',[Path('output')],{}, {},Fake(value))
  self.assertEqual(len(n.native_output('request',[Path('output')],{}, {},Fake({'kind':'assembly','schema':'airk.private.cuda-component-probe-request/1'}))),1)
 def frame(self):
  v={'schema':'airk.private.cuda-runtime-assembly-emission/1','pid':123,'execution_user':'human','model_requests_dispatched':False,'runtime_admission_dispatched':False,'forward_count_observed':None,'elapsed_us':1,'approved':False,'fits_empty':True,'input':{'path':'input','sha256':'b'*64},'manifest':{'path':'manifest','sha256':'a'*64}};plan={'environment':{'AIRK_CUDA_QUALIFICATION_INPUT':'input','AIRK_CUDA_QUALIFICATION_INPUT_SHA256':'b'*64}};receipt={'owned':{'owned_leader_pid':123}};return v,plan,receipt
 def test_actual_inner_pid_sid_scope_flags_required(self):
  v,plan,receipt=self.frame()
  for key,value in [('pid',124),('execution_user','other'),('forward_count_observed',0),('runtime_admission_dispatched',True),('approved',True),('fits_empty',False)]:
   changed=dict(v);changed[key]=value
   with self.assertRaises(ValueError):n.native_output('assembly',[Path('manifest'),Path('receipt')],plan,receipt,Fake(changed))
 def test_inner_opaque_input_and_output_pin_must_match(self):
  v,plan,receipt=self.frame();self.assertEqual(2,len(n.native_output('assembly',[Path('manifest'),Path('receipt')],plan,receipt,Fake(v))))
  v['input']={'path':'input','sha256':'0'*64}
  with self.assertRaises(ValueError):n.native_output('assembly',[Path('manifest'),Path('receipt')],plan,receipt,Fake(v))
if __name__=='__main__':unittest.main()
