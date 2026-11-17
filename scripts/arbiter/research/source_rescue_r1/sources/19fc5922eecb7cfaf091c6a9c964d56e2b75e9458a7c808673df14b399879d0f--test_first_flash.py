# SPDX-License-Identifier: Apache-2.0
import datetime,importlib.util,unittest
from pathlib import Path
p=Path(__file__).with_name('run_checkpoint_section.py');s=importlib.util.spec_from_file_location('first_flash',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Controls(unittest.TestCase):
 def test_new_fixed_budget_is_clipped_to_campaign(self):
  self.assertEqual(m.section_seconds(m.CAMPAIGN-datetime.timedelta(days=1)),10800)
  self.assertEqual(m.section_seconds(m.CAMPAIGN-datetime.timedelta(seconds=90)),90)
 def test_expired_campaign_refuses(self):
  with self.assertRaises(ValueError):m.section_seconds(m.CAMPAIGN)
 def test_changed_warm_output_needs_new_same_command_success_record(self):
  old={'sha256':'old','ninja_success_record':{'start_ms':0,'end_ms':1,'output_mtime':'1','command_hash':'same'}}
  m.new_log_required(old,'old',None)
  for record in [None,old['ninja_success_record'],{'start_ms':2,'end_ms':3,'output_mtime':'2','command_hash':'foreign'}]:
   with self.assertRaises(ValueError):m.new_log_required(old,'new',record)
  m.new_log_required(old,'new',{'start_ms':2,'end_ms':3,'output_mtime':'2','command_hash':'same'})
if __name__=='__main__':unittest.main()
