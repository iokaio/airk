# SPDX-License-Identifier: Apache-2.0
"""Cheap source-coordinate controls; no corpus materialization/model use."""
import importlib.util
from pathlib import Path
import unittest
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('train_sources',HERE/'prepare_sources.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
class SourceSpanTests(unittest.TestCase):
    def test_unicode_byte_offsets_and_original_indent(self):
        raw='# Café 🚀\nclass C:\n    def f(self):\n        return "日本語"\n'.encode()
        text='    def f(self):\n        return "日本語"\n';start,end=owner.function_span(raw,{'text':text,'source_line':3})
        self.assertEqual(raw[start:end],text.encode());self.assertEqual(start,len('# Café 🚀\nclass C:\n'.encode()))
    def test_ast_firstline_column_offset_is_exact_not_whole_body_dedent(self):
        raw=b'class C:\n    def f(self):\n        return 1\n';text='def f(self):\n        return 1\n'
        start,end=owner.function_span(raw,{'text':text,'source_line':2});self.assertEqual(raw[start:end],text.encode())
        with self.assertRaises(ValueError):owner.function_span(raw,{'text':'def f(self):\n    return 1\n','source_line':2})
    def test_wrong_line_and_nonidentical_body_refuse(self):
        for document in ({'text':'','source_line':1},{'text':'def f(): pass','source_line':0},{'text':'def f(): pass','source_line':2},{'text':'def f(): return 1','source_line':1}):
            with self.subTest(document=document),self.assertRaises(ValueError):owner.function_span(b'def f(): pass\n',document)
    def test_original_paths_cannot_escape_or_alias_windows_syntax(self):
        self.assertEqual(owner.relative('requests/api.py').as_posix(),'requests/api.py')
        for path in ('../outside.py','/absolute.py','C:/outside.py','requests\\api.py'):
            with self.subTest(path=path),self.assertRaises(ValueError):owner.relative(path)
if __name__=='__main__':unittest.main()
