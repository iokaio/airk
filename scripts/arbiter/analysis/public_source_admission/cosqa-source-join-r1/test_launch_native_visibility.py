# SPDX-License-Identifier: Apache-2.0
import copy,importlib.util,json,tempfile,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('visibility_runner',Path(__file__).with_name('launch_native_visibility.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Controls(unittest.TestCase):
 def test_original_limits_are_exact(self):
  plan={'schema':'airk.private.public-human-visibility-owned-plan/1','limits':dict(m.LIMITS),'selector':m.SELECTOR,'original_denominator':1955,'human_sid':'S-1-5-21-3678370870-3858642417-826968105-1002','runner':{'path':m.__file__,'sha256':m.digest(m.__file__)}}
  m.validate_plan(plan)
  for key in m.LIMITS:
   bad=copy.deepcopy(plan);bad['limits'][key]+=1
   with self.assertRaises(ValueError):m.validate_plan(bad)
 def test_complete_original_rows_and_unobserved_forwards(self):
  rows=[{'id':str(x)} for x in range(1955)];plan={'native_input':{'path':'original','sha256':'p'},'image':{'sha256':'binary'}};body={'schema':'airk.private.public-human-native-visibility-result/1','complete':True,'input':plan['native_input'],'native_binary_sha256':'binary','original_denominator':1955,'rows':[{'original':x} for x in rows],'actual_units':[],'embedding_forwards':None,'focus_forwards':None}
  for key in ['gold_admission','rights_admission','published','training_gradients','nonblank_embedding_request_dispatched','focus_scorer_request_dispatched']:body[key]=False
  m.validate_body(body,plan,{'rows':rows})
  for change in ['drop','reorder','gold','forward','dispatch']:
   bad=copy.deepcopy(body)
   if change=='drop':bad['rows'].pop()
   elif change=='reorder':bad['rows'][0],bad['rows'][1]=bad['rows'][1],bad['rows'][0]
   elif change=='gold':bad['gold_admission']=True
   elif change=='forward':bad['embedding_forwards']=0
   else:bad['nonblank_embedding_request_dispatched']=True
   with self.assertRaises(ValueError):m.validate_body(bad,plan,{'rows':rows})
 def test_expired_final_phase_cannot_complete(self):
  receipt={'complete':True,'failure':None};m.final_state(receipt,5,lambda:5);self.assertFalse(receipt['complete']);self.assertEqual(receipt['failure']['type'],'TimeoutError')
 def test_original_deadline_not_renewed(self):
  with self.assertRaises(TimeoutError):m.check_deadline(5,lambda:5)
 def test_full_directory_census_detects_extra_and_changed_files(self):
  with tempfile.TemporaryDirectory() as temp:
   root=Path(temp);(root/'a.txt').write_text('original');before=m.census(root);(root/'extra.txt').write_text('extra');self.assertNotEqual(before,m.census(root));(root/'extra.txt').unlink();(root/'a.txt').write_text('changed');self.assertNotEqual(before,m.census(root))
 def test_report_cap_checked_before_native_event_wait(self):
  class UnexpectedWait:
   def WaitForSingleObject(*args):raise AssertionError('cap must refuse before wait')
  class Info:process=1
  with tempfile.TemporaryDirectory() as temp:
   raw=Path(temp)/'report';log=Path(temp)/'log';raw.touch();log.touch()
   with raw.open('r+b') as f:f.truncate(m.LIMITS['report_bytes']+1)
   with self.assertRaises(ValueError):m.monitor(UnexpectedWait(),Info(),10,raw,log,lambda:1)
if __name__=='__main__':unittest.main()
