# SPDX-License-Identifier: Apache-2.0
import copy,importlib.util,unittest
from pathlib import Path
BASE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('dense_run',BASE/'run_tiny.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
spec=importlib.util.spec_from_file_location('dense_stage',BASE/'stage_cards.py');f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
class Controls(unittest.TestCase):
 def body(self):
  return {'kind':'large_dense_synthetic_attention_diagnostic','complete':True,'attention_mode':'explicit_mask_dense','tokens':17,'model_weights_loaded':False,'qualification_granted':False,'controls':[{'mask_control':v,'tokens':17,'attention_mode':'explicit_mask_dense','graph_outputs':['context'],'model_weights_loaded':False,'maximum_error':.00001,'profiling_path':'p'+str(i),'optimized_graph_path':'o'+str(i),'masked_key_one_value_spike':.25} for i,v in enumerate(m.CONTROLS)]}
 def test_complete(self):
  retained=[];m.validate_body(self.body(),17,lambda *v:retained.append(v));self.assertEqual(len(retained),6)
 def test_missing_duplicate_or_unmasked_refusal(self):
  for edit in [lambda b:b['controls'].pop(),lambda b:b['controls'].__setitem__(1,copy.deepcopy(b['controls'][0])),lambda b:b.__setitem__('attention_mode','flash')]:
   b=self.body();edit(b)
   with self.assertRaises(ValueError):m.validate_body(b,17,lambda *v:None)
 def test_spike_value_or_nonfinite_refusal(self):
  for key,value in [('masked_key_one_value_spike',0),('maximum_error',float('nan')),('maximum_error',.001)]:
   b=self.body();b['controls'][2][key]=value
   with self.assertRaises(ValueError):m.validate_body(b,17,lambda *v:None)
 def test_only_exact_causal_change(self):
  original=f.MARK+b'x'*(617-len(f.MARK));dense=original.replace(f.MARK,f.MARK.replace(b'\x18\x01',b'\x18\x00'));f.exact_dense_fixture(original,dense)
  for bad in [original,dense[:-1]+b'y']:
   with self.assertRaises(ValueError):f.exact_dense_fixture(original,bad)
if __name__=='__main__':unittest.main()
