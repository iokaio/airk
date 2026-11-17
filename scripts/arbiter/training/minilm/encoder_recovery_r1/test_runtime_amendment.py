# SPDX-License-Identifier: Apache-2.0
import copy,unittest
from pathlib import Path
from prepare_runtime_amendment import amend_files,key
ROOT=str(Path('imaginary-runtime').resolve());P=lambda x:str(Path(ROOT)/x)
class AmendmentTests(unittest.TestCase):
 def setUp(self):
  self.old=[{'path':P('keep.py'),'sha256':'keep','bytes':1},{'path':P('change.py'),'sha256':'old','bytes':2}];self.a={'root':ROOT,'expected_before_count':2,'expected_after_count':3,'added':[{'path':P('new.py'),'sha256':'new','bytes':3}],'changed':[{'path':P('change.py'),'expected':'old','actual':'changed'}],'removed':[]};self.actual={key(P('keep.py')):{'sha256':'keep','bytes':1},key(P('change.py')):{'sha256':'changed','bytes':4},key(P('new.py')):{'sha256':'new','bytes':3}}
 def test_exact_additive_review_preserves_every_old_path_and_old_input(self):
  before=copy.deepcopy(self.old);r=amend_files(self.old,self.a,self.actual);self.assertEqual(self.old,before);self.assertEqual(len(r),3);self.assertTrue({key(f['path']) for f in before}<={key(f['path']) for f in r});self.assertEqual(r[0],before[0])
 def test_missing_old_file_refuses(self):
  del self.actual[key(P('keep.py'))]
  with self.assertRaisesRegex(ValueError,'missing'):amend_files(self.old,self.a,self.actual)
 def test_unreviewed_added_file_refuses(self):
  self.actual[key(P('extra.py'))]={'sha256':'extra','bytes':1}
  with self.assertRaisesRegex(ValueError,'unreviewed added'):amend_files(self.old,self.a,self.actual)
 def test_unreviewed_changed_file_refuses(self):
  self.actual[key(P('keep.py'))]['sha256']='bad'
  with self.assertRaisesRegex(ValueError,'unreviewed changed'):amend_files(self.old,self.a,self.actual)
 def test_wrong_old_changed_hash_refuses(self):
  self.a['changed'][0]['expected']='foreign'
  with self.assertRaisesRegex(ValueError,'reviewed changed hash'):amend_files(self.old,self.a,self.actual)
 def test_wrong_new_file_hash_refuses(self):
  self.actual[key(P('new.py'))]['sha256']='foreign'
  with self.assertRaisesRegex(ValueError,'reviewed added file hash'):amend_files(self.old,self.a,self.actual)
 def test_explicit_removal_cannot_be_reviewed_into_this_helper(self):
  self.a['removed']=['keep.py']
  with self.assertRaisesRegex(ValueError,'removal'):amend_files(self.old,self.a,self.actual)
if __name__=='__main__':unittest.main()
