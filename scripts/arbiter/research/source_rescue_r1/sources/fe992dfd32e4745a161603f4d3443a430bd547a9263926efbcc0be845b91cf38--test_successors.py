# SPDX-License-Identifier: Apache-2.0
import datetime,importlib.util,tempfile,unittest
from pathlib import Path
p=Path(__file__).with_name('run_checkpoint_sections.py');s=importlib.util.spec_from_file_location('successors',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Controls(unittest.TestCase):
 def test_section_budget_keeps_fixed_original_cap_and_campaign(self):
  self.assertEqual(m.section_seconds(m.CAMPAIGN-datetime.timedelta(days=1)),10800);self.assertEqual(m.section_seconds(m.CAMPAIGN-datetime.timedelta(seconds=5)),5)
 def test_campaign_expired_refuses(self):
  with self.assertRaises(ValueError):m.section_seconds(m.CAMPAIGN)
 def test_changed_warm_bytes_without_new_same_command_record_refuse(self):
  old={'sha256':'a','ninja_success_record':{'command_hash':'same','start_ms':1,'end_ms':2,'output_mtime':'1'}}
  with self.assertRaises(ValueError):m.new_log_required(old,'b',None)
  with self.assertRaises(ValueError):m.new_log_required(old,'b',{'command_hash':'other','start_ms':3,'end_ms':4,'output_mtime':'2'})
 def test_state_copy_requires_exact_recorded_bytes_and_never_rewrites(self):
  with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as t:
   source=Path(t)/'source';source.write_bytes(b'exact');target=Path(t)/'copy';row=m.pin(source);copy=m.copy_state(source,target,row);self.assertEqual(copy['sha256'],row['sha256'])
   with self.assertRaises(FileExistsError):m.copy_state(source,target,row)
   source.write_bytes(b'drift')
   with self.assertRaises(ValueError):m.copy_state(source,Path(t)/'wrong',row)
 def test_missing_nonzero_or_direct_only_tree_result_is_refused(self):
  good={'actual_tree_result':{'schema':'airk.private.process-tree-result/1','exit_code':0,'wait':'Start-Process-Wait-process-and-descendants'},'owned_job_final_active_processes':0,'owned_job_final_member_pids':[]};m.require_complete_tree(good)
  for field,value in [('schema','other'),('exit_code',1),('wait','direct-process-only')]:
   c={**good,'actual_tree_result':dict(good['actual_tree_result'])};c['actual_tree_result'][field]=value
   with self.assertRaises(ValueError):m.require_complete_tree(c)
  for c in [{},{**good,'owned_job_final_active_processes':1},{**good,'owned_job_final_member_pids':[99]}]:
   with self.assertRaises(ValueError):m.require_complete_tree(c)
if __name__=='__main__':unittest.main()

