# SPDX-License-Identifier: Apache-2.0
import unittest
from generated_roles import marker,template_match
class Roles(unittest.TestCase):
    def test_path_name_alone_is_not_evidence(self):self.assertIsNone(marker(b'custom program'))
    def test_explicit_generated_marker(self):self.assertEqual(marker(b'# CMAKE generated file: DO NOT EDIT!\n'), 'cmake_generated_file')
    def test_full_template_literals_required(self):
        template=b'set(CMAKE_CXX_COMPILER "@CMAKE_CXX_COMPILER@")\n'
        self.assertTrue(template_match(b'set(CMAKE_CXX_COMPILER "compiler")\r\n',template))
        self.assertFalse(template_match(b'unrelated',template))
    def test_literal_without_substitution_is_not_generic_template(self):self.assertFalse(template_match(b'a',b'a'))
    def test_repeated_substitution_tail_refuses_without_backtracking(self):
        self.assertFalse(template_match(b'x'*50+b'BAD',(b'@VALUE@x'*25)+b'END'))
if __name__=='__main__':unittest.main()
