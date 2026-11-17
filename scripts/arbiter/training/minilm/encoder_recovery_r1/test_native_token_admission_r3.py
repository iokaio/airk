# SPDX-License-Identifier: Apache-2.0
import copy,unittest
from pathlib import Path
from tokenizers import Tokenizer
from native_token_admission_r3 import native_unit_admission
H=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models')))/'encoder-recovery-preparation-r1';TOKENIZER=H.parent/'minilm-training-source/tokenizer.json'
class AdmissionControls(unittest.TestCase):
 def setUp(self):
  self.t=Tokenizer.from_file(str(TOKENIZER));self.t.no_padding();self.t.no_truncation();self.native={'input_ids':[[101,7592,102]+[0]*29],'attention_mask':[[1,1,1]+[0]*29],'token_type_ids':[[0]*32]}
 def test_literal_hello_ids_padding_mask_and_type_rows(self):
  before=copy.deepcopy(self.native);w=native_unit_admission(self.t,'hello',self.native,'embeddable');self.assertEqual((w['raw_tokens'],w['native_width'],w['padding_tokens']),(3,32,29));self.assertEqual(self.native,before)
 def test_equal_width_needs_no_padding(self):
  e=self.t.encode('hello');native={'input_ids':[e.ids],'attention_mask':[e.attention_mask],'token_type_ids':[e.type_ids]};self.assertEqual(native_unit_admission(self.t,'hello',native,'embeddable')['padding_tokens'],0)
 def test_changed_real_id_refuses(self):
  self.native['input_ids'][0][1]=7593
  with self.assertRaisesRegex(ValueError,'whole native tensor'):native_unit_admission(self.t,'hello',self.native,'embeddable')
 def test_wrong_padding_id_mask_or_type_refuses(self):
  for name in ['input_ids','attention_mask','token_type_ids']:
   bad=copy.deepcopy(self.native);bad[name][0][-1]=1
   with self.assertRaisesRegex(ValueError,'whole native tensor'):native_unit_admission(self.t,'hello',bad,'embeddable')
 def test_overlong_raw_input_never_crops(self):
  with self.assertRaisesRegex(ValueError,'uncropped'):native_unit_admission(self.t,'hello '*300,self.native,'embeddable')
 def test_malformed_width_or_extra_tensor_refuses(self):
  for bad in [{**self.native,'future':[[]]}, {**self.native,'input_ids':[[101,102]]},{**self.native,'input_ids':[[0]*257]}]:
   with self.assertRaises(ValueError):native_unit_admission(self.t,'hello',bad,'embeddable')
 def test_blank_admission_or_boolean_tensors_refuse(self):
  with self.assertRaises(ValueError):native_unit_admission(self.t,'hello',self.native,'not_embeddable_blank')
  self.native['input_ids'][0][0]=True
  with self.assertRaises(ValueError):native_unit_admission(self.t,'hello',self.native,'embeddable')
if __name__=='__main__':unittest.main()
