# SPDX-License-Identifier: Apache-2.0
import hashlib,json,tempfile,unittest
from pathlib import Path
import stage_profiles as s
class StagingTests(unittest.TestCase):
 def master(self,root,count=3):
  rows=[]
  for i in range(count):
   parent=root/('source%d'%i);parent.mkdir();asset=parent/'numeric.json';asset.write_bytes(b'numeric control');pin=s.digest(asset)
   profile=parent/'profile.json';profile.write_text(json.dumps({'model':'synthetic%d'%i,'assets':[{'name':'numeric.json','role':'fixture','bytes':asset.stat().st_size,'sha256':pin}]}),encoding='utf-8')
   rows.append({'profile':str(profile),'profile_sha256':s.digest(profile),'files':{'numeric.json':str(asset)}})
  master=root/'master.json';master.write_text(json.dumps({'schema':'airk.private.selected-native-profile-master/1','profiles':rows}),encoding='utf-8');return master
 def test_exact_existing_four_field_inputs_and_unchanged_profile_bytes(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);master=self.master(root);inputs=s.stage([(master,s.digest(master))],root/'staged',root)
   self.assertEqual(len(inputs),3)
   for i,row in enumerate(inputs):
    self.assertEqual(set(row),{'profile','profile_sha256','assets','fixtures'});self.assertEqual(Path(row['profile']).read_bytes(),(root/('source%d/profile.json'%i)).read_bytes());self.assertEqual(s.digest(Path(row['fixtures'])),s.digest(root/('source%d/numeric.json'%i)))
   with self.assertRaisesRegex(ValueError,'fresh'):s.stage([(master,s.digest(master))],root/'staged',root)
 def test_mutated_asset_refuses_without_overwriting_source(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);master=self.master(root);asset=root/'source0/numeric.json';asset.write_bytes(b'changed control')
   with self.assertRaises(ValueError):s.stage([(master,s.digest(master))],root/'staged',root)
   self.assertEqual(asset.read_bytes(),b'changed control')
 def test_wrong_master_pin_and_missing_third_profile_refuse(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);master=self.master(root,count=2)
   with self.assertRaisesRegex(ValueError,'master changed'):s.stage([(master,'0'*64)],root/'staged',root)
   with self.assertRaisesRegex(ValueError,'exactly three'):s.stage([(master,s.digest(master))],root/'staged',root)
 def test_lexical_parent_escape_refuses_before_read(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d)
   with self.assertRaisesRegex(ValueError,'outside'):s.no_alias(root/'..'/'unrelated',root)
if __name__=='__main__':unittest.main(verbosity=2)
