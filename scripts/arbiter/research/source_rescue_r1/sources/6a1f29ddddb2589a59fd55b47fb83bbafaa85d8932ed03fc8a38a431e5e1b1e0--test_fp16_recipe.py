# SPDX-License-Identifier: Apache-2.0
import importlib.util,struct,unittest
from pathlib import Path
p=Path(__file__).with_name('fp16_recipe.py');s=importlib.util.spec_from_file_location('g5_recipe',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Controls(unittest.TestCase):
 def test_half_round_ties_even_and_signed_zero(self):
  values=[0.,-0.,1.,1.+2**-11,1.+3*2**-11,65504.]
  self.assertEqual(m.half_bytes_from_f32(struct.pack('<6f',*values)),bytes.fromhex('00000080003c003c023cff7b'))
 def test_conversion_refuses_nonfinite_overflow_unaligned(self):
  for raw in [b'x',struct.pack('<f',float('inf')),struct.pack('<f',float('nan')),struct.pack('<f',65536.)]:
   with self.assertRaises(ValueError):m.half_bytes_from_f32(raw)
 def test_context_causal_bags_at_boundaries(self):
  for t in [1,17,128,8192]:
   r=m.mask_recipe('compact',[1]*t,list(range(t)))
   self.assertEqual(r['seqlens_k'],[t-1])
   for q in {0,t//2,t-1}:self.assertEqual(m.source_key_for_query(r,q),tuple(k for k in range(t) if k<=q))
 def test_focus_leftpad_causal_bags_and_packed_offsets(self):
  for p in [0,1,17,384,511]:
   r=m.mask_recipe('focus',[0]*p+[1]*(512-p),list(range(512)))
   self.assertEqual(r['packed_mask'],[512-p,p,512,p,512])
   for q in {p,511}:self.assertEqual(m.source_key_for_query(r,q),tuple(k for k in range(512) if k>=p and k<=q))
 def test_mask_hole_allmasked_wrongposition_and_width_refuse(self):
  cases=[('focus',[0]*512,[0]*512),('focus',[1]*511+[0],list(range(512))),('focus',[1]*512,[1]*512),('compact',[1,0,1],[0,1,2]),('large',[1]*8193,list(range(8193)))]
  for values in cases:
   with self.assertRaises(ValueError):m.mask_recipe(*values)
 def test_kv_expansion_preserves_original_head_groups(self):
  self.assertEqual([m.grouped_kv_head(h,16,8) for h in range(16)],[0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7])
  with self.assertRaises(ValueError):m.grouped_kv_head(0,15,8)
 def test_dispatch_missing_math_duplicate_foreign_refuse(self):
  m.require_dispatch([('layer0','fp16','FLASH_ATTENTION')],{'layer0'},'FLASH_ATTENTION')
  for records in [[],[('layer0','fp16','MATH')],[('layer0','fp16','FLASH_ATTENTION')]*2,[('other','fp16','FLASH_ATTENTION')],[('layer0','fp32','FLASH_ATTENTION')]]:
   with self.assertRaises(ValueError):m.require_dispatch(records,{'layer0'},'FLASH_ATTENTION')
if __name__=='__main__':unittest.main()
