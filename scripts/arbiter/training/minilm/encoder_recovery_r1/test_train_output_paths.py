# SPDX-License-Identifier: Apache-2.0
import copy,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
from train_output_paths import validate_acl_observation,validate_training_output,FULL_CONTROL
class OutputAdmissionTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name).resolve();self.anchor=self.root/'anchor';self.anchor.mkdir();self.output=self.anchor/'result';self.frozen=self.root/'frozen';self.frozen.mkdir();self.good={'current_sid':'current','owner_sid':'current','protected':True,'aces':[{'type':0,'flags':3,'mask':FULL_CONTROL,'sid':'current'}]}
 def check(self):
  with patch('train_output_paths.private_acl',return_value=self.good):return validate_training_output(self.anchor,self.output,[self.frozen])
 def test_exact_empty_disjoint_geometry_and_private_acl(self):self.assertEqual(self.check(),self.good);self.assertEqual(validate_acl_observation(self.good),self.good)
 def test_missing_anchor_refused(self):
  self.anchor.rmdir()
  with self.assertRaisesRegex(ValueError,'precreated'):self.check()
 def test_wrong_output_child_refused(self):
  self.output=self.anchor/'other'
  with self.assertRaisesRegex(ValueError,'anchor/result'):self.check()
 def test_preexisting_result_refused(self):
  self.output.mkdir()
  with self.assertRaisesRegex(ValueError,'absent result'):self.check()
 def test_unknown_anchor_entry_refused(self):
  (self.anchor/'foreign').write_text('kept')
  with self.assertRaisesRegex(ValueError,'empty anchor'):self.check()
 def test_equal_frozen_root_refused(self):
  self.frozen=self.anchor
  with self.assertRaisesRegex(ValueError,'overlaps'):self.check()
 def test_anchor_inside_frozen_root_refused(self):
  self.frozen=self.root
  with self.assertRaisesRegex(ValueError,'overlaps'):self.check()
 def test_frozen_root_inside_anchor_refused(self):
  self.frozen=self.anchor/'frozen-child'
  with self.assertRaisesRegex(ValueError,'overlaps'):self.check()
 def test_wrong_owner_or_unprotected_acl_refused(self):
  for k,v in [('owner_sid','other'),('protected',False)]:
   row=copy.deepcopy(self.good);row[k]=v
   with self.assertRaises(ValueError):validate_acl_observation(row)
 def test_extra_ace_refused(self):
  row=copy.deepcopy(self.good);row['aces'].append({'type':0,'flags':3,'mask':FULL_CONTROL,'sid':'other'})
  with self.assertRaises(ValueError):validate_acl_observation(row)
 def test_weakened_or_unsafe_ace_refused(self):
  for k,v in [('type',1),('flags',0),('flags',7),('flags',19),('mask',1),('sid','other')]:
   row=copy.deepcopy(self.good);row['aces'][0][k]=v
   with self.assertRaises(ValueError):validate_acl_observation(row)
if __name__=='__main__':unittest.main()
