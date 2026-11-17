# SPDX-License-Identifier: Apache-2.0
import unittest
from patch_replay import sections,apply
class Replay(unittest.TestCase):
    PATCH=b'--- a/a\n+++ b/a\n@@ -1,2 +1,2 @@\n one\n-two\n+three\n'
    def test_exact(self):self.assertEqual(apply(b'one\ntwo\n',sections(self.PATCH)['a']),b'one\nthree\n')
    def test_context_refuses(self):
        with self.assertRaises(ValueError):apply(b'wrong\ntwo\n',sections(self.PATCH)['a'])
    def test_count_refuses(self):
        with self.assertRaises(ValueError):sections(self.PATCH.replace(b'+1,2',b'+1,3'))
    def test_path_refuses(self):
        with self.assertRaises(ValueError):sections(self.PATCH.replace(b'a/a',b'a/../a').replace(b'b/a',b'b/../a'))
    def test_duplicate_refuses(self):
        with self.assertRaises(ValueError):sections(self.PATCH+self.PATCH)
    def test_crlf_patch_only(self):self.assertEqual(apply(b'one\ntwo\n',sections(self.PATCH.replace(b'\n',b'\r\n'))['a']),b'one\nthree\n')
    def test_new_position_refuses(self):
        with self.assertRaises(ValueError):apply(b'one\ntwo\n',sections(self.PATCH.replace(b'+1,2',b'+999,2'))['a'])
    def test_extra_body_refuses(self):
        with self.assertRaises(ValueError):sections(self.PATCH+b'+extra\n')
if __name__=='__main__':unittest.main()
