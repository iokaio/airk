# SPDX-License-Identifier: Apache-2.0
import copy
import unittest
from recovery import create,recover
class Recovery(unittest.TestCase):
    def test_preserves_mixed_endings_and_no_final_newline(self):
        a=b'a\r\nb\nc';b=b'a\r\nb2\r\nc\n';self.assertEqual(recover(a,create(a,b)),b)
    def test_insert_delete_empty(self):
        for a,b in [(b'',b'new'),(b'old',b''),(b'a\nb\nc\n',b'first\na\nc\nlast')]:self.assertEqual(recover(a,create(a,b)),b)
    def test_base_refuses(self):
        with self.assertRaises(ValueError):recover(b'bad',create(b'old',b'new'))
    def test_target_refuses(self):
        r=create(b'old',b'new');r['target_sha256']='0'*64
        with self.assertRaises(ValueError):recover(b'old',r)
    def test_overlap_refuses(self):
        r=create(b'old',b'new');r['edits']*=2
        with self.assertRaises(ValueError):recover(b'old',r)
    def test_deleted_identity_refuses(self):
        r=create(b'old',b'new');r['edits'][0]['deleted_sha256']='0'*64
        with self.assertRaises(ValueError):recover(b'old',r)
    def test_unknown_field_refuses(self):
        r=create(b'a',b'b');r['extra']=True
        with self.assertRaises(ValueError):recover(b'a',r)
if __name__=='__main__':unittest.main()
