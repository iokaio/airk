# SPDX-License-Identifier: Apache-2.0
"""Literal metadata refusal controls; no actual build or producer success fixture."""
import copy,importlib.util,json,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('composer',Path(__file__).with_name('compose_checkpointed_build.py'));c=importlib.util.module_from_spec(s);s.loader.exec_module(c)
class Owner:
 def __init__(self,rows):self.rows=rows
 def plain(self,p,**kw):return p
 def verify_file(self,p,row):
  if p not in self.rows:raise ValueError('unavailable pin')
 def read_json(self,p):return self.rows[p]
class Helper:
 @staticmethod
 def verify_section(body,index,o,sha):
  if body['section_index']!=index:raise ValueError('order')
def fixture():
 pins=[{'path':str(i),'bytes':1,'sha256':str(i)*64} for i in range(3)];rows={};bodies=[]
 for i in range(3):
  row={'section_index':i,'cold_build_qualified':False,'elapsed_seconds':1,'runner':{'sha256':'r'},'input_contract':{'sha256':'i'},'predecessor':None if i==0 else pins[i-1],'warm_checkpoint_before':{'path':'w'+str(i)},'warm_checkpoint_after':{'path':'w'+str(i+1)},'source_manifest_after':{'path':'s'+str(i)}};rows[str(i)]=row;rows['s'+str(i)]={'source':'unchanged'};bodies.append(row)
 return pins,rows,bodies
class Controls(unittest.TestCase):
 def check(self,mutate):
  pins,rows,bodies=fixture();mutate(pins,rows,bodies)
  with self.assertRaises(ValueError):c.validate_sections(Owner(rows),Helper(),pins,'r','i','plan')
 def test_metadata_chain_only(self):
  pins,rows,_=fixture();self.assertEqual(3,len(c.validate_sections(Owner(rows),Helper(),pins,'r','i','plan')))
 def test_missing_section(self):self.check(lambda p,r,b:p.pop())
 def test_bad_order(self):self.check(lambda p,r,b:b[1].update(section_index=2))
 def test_source_authority(self):self.check(lambda p,r,b:b[1].update(runner={'sha256':'old'}))
 def test_predecessor_identity(self):self.check(lambda p,r,b:b[2].update(predecessor=p[0]))
 def test_changed_warm_chain(self):self.check(lambda p,r,b:b[2].update(warm_checkpoint_before={'path':'wrong'}))
 def test_changed_source(self):self.check(lambda p,r,b:r['s2'].update(source='changed'))
 def test_cold_claim(self):self.check(lambda p,r,b:b[0].update(cold_build_qualified=True))
 def test_invalid_time(self):self.check(lambda p,r,b:b[1].update(elapsed_seconds=float('nan')))
 def test_missing_exact_custody(self):
  with self.assertRaises(ValueError):c.normalized_custody(Owner({}),1,{'ninja_log_before':{'path':'missing'},'ninja_deps_before':{'path':'missing'},'ninja_log_after':{'path':'missing'},'ninja_deps_after':{'path':'missing'}},[])
if __name__=='__main__':unittest.main()
