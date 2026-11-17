# SPDX-License-Identifier: Apache-2.0
import json,unittest,tempfile
from prepare_packet import validate_build_proof
from pathlib import Path
from run_native import PhaseDeadline,FAMILIES,CONTROLS
class Controls(unittest.TestCase):
 def test_single_original_family_not_six_or_old205(self):
  self.assertEqual(FAMILIES,{'original-shipped-s8-ort130-evaluation'});self.assertEqual(CONTROLS,set());rows=json.loads((Path(__file__).parent/'one-family-input.json').read_bytes());self.assertEqual([x['family'] for x in rows],['original-shipped-s8-ort130-evaluation']);self.assertEqual(rows[0]['oracle_result'],'original-shipped-s8-ort130-eval')
 def test_existing_deadline_never_renews(self):
  current=[100.0];phase=PhaseDeadline(clock=lambda:current[0]);self.assertEqual(phase.remaining(),1800);current[0]=1899;self.assertEqual(phase.remaining(),1);current[0]=1900
  with self.assertRaises(TimeoutError):phase.remaining()
  with self.assertRaises(ValueError):PhaseDeadline(seconds=1801)
class ReceiptControls(unittest.TestCase):
 def test_relative_and_absolute_log_resolution_and_foreign_refusal(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);receipt=root/'nested/build.json';actual=root/'nested/image.log';good=dict(exit_code=0,inputs_unchanged=True,runner_unchanged=True,runtime_unchanged=True,command_inputs_unchanged=True,log='image.log')
   validate_build_proof(receipt,good,actual);validate_build_proof(receipt,{**good,'log':str(actual)},actual)
   with self.assertRaises(ValueError):validate_build_proof(receipt,{**good,'log':'../foreign.log'},actual)
   for flag in ['inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged']:
    with self.subTest(flag=flag),self.assertRaises(ValueError):validate_build_proof(receipt,{**good,flag:False},actual)
if __name__=='__main__':unittest.main()
