# SPDX-License-Identifier: Apache-2.0
import json,unittest
from run_native_context import remaining_seconds,natural,PROFILES
from prepare_native import cargo_image
class Controls(unittest.TestCase):
 def test_deadline_is_not_renewed(self):
  self.assertEqual(remaining_seconds(1600,1000),600);self.assertEqual(remaining_seconds(1600,1599),1)
  with self.assertRaises(TimeoutError):remaining_seconds(1600,1600)
 def test_exact_caps(self):
  self.assertEqual([(p['cpu'],p['reservation']//1024**3,p['host']//1024**3,p['seconds']) for p in PROFILES.values()],[(4,16,32,1200),(16,40,48,1800)])
 def test_natural_one_process_only(self):
  good=dict(exit_code=0,timed_out=False,log_oversized=False,owned_descendants_settled=True,owned_job_forced_termination=False,unexpected_third_process=False,owned_total_processes=1,owned_job_final_member_pids=[],owned_job_final_active_processes=0,retained_member_exits=[dict(exit_observed=True,exit_code=0)])
  self.assertTrue(natural(good))
  for field,value in [('exit_code',1),('timed_out',True),('owned_job_forced_termination',True),('owned_total_processes',2),('owned_job_final_active_processes',1),('owned_job_final_member_pids',[999])]:
   with self.subTest(field=field):self.assertFalse(natural({**good,field:value}))
 def test_exact_cargo_image_provenance(self):
  row=dict(reason='compiler-artifact',target={'name':'airk_inference'},profile={'test':True},executable='actual-test-image.exe');self.assertEqual(cargo_image(json.dumps(row).encode()).name,'actual-test-image.exe')
  for raw in [b'',json.dumps({**row,'profile':{'test':False}}).encode(),(json.dumps(row)+'\n'+json.dumps({**row,'executable':'other.exe'})).encode()]:
   with self.assertRaises(ValueError):cargo_image(raw)
if __name__=='__main__':unittest.main()
