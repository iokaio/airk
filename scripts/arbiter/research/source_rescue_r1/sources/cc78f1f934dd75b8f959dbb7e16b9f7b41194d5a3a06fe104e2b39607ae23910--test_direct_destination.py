# SPDX-License-Identifier: Apache-2.0
import importlib.util,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('preserver',Path(__file__).with_name('preserve_first_fp32_images.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
class Controls(unittest.TestCase):
 def test_exact_direct_inference_and_separate_retrieval_destinations(self):
  actual=p.destination('airk-inference',Path('original.exe'),Path('artifact/images'))
  self.assertEqual(actual,Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-first-fp32-nn0-preparation-r1/runtime/airk_inference-fp32-approved-r1.exe'))
  self.assertEqual(p.destination('airk-retrieval',Path('original.exe'),Path('artifact/images')),Path('artifact/images/original.exe'))
 def test_native_payload_policy_preserved_and_foreign_write_refused(self):
  p.payload_owner({'owner_sid':p.SID,'rules':[{'type':'Allow','sid':'S-1-5-18','rights':2032127},{'type':'Allow','sid':'S-1-5-32-544','rights':2032127},{'type':'Allow','sid':'foreign-read','rights':0x1200a9}]})
  with self.assertRaises(ValueError):p.payload_owner({'owner_sid':p.SID,'rules':[{'type':'Allow','sid':'foreign-write','rights':2032127}]})
  with self.assertRaises(ValueError):p.private_owner({'owner_sid':p.SID,'rules':[{'type':'Allow','sid':'S-1-5-18','rights':2032127}]})
if __name__=='__main__':unittest.main()
