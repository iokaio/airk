# SPDX-License-Identifier: Apache-2.0
"""Pure analyzer seams; no generated files, historical imports or subprocesses."""
import hashlib
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('js_analyzer',Path(__file__).with_name('analyze.py'))
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)

class Analysis(unittest.TestCase):
    def test_nonfinite_and_boolean_deadlines_refuse(self):
        for value in [float('inf'),float('-inf'),float('nan'),True]:
            with self.subTest(value=value),self.assertRaises(ValueError):a.check(value)
    def test_expiry_is_original_cutoff(self):
        with patch.object(a.time,'monotonic',return_value=10):
            with self.assertRaises(ValueError):a.check(10)
            a.check(11)
    def test_exact_bytes_length_and_hash(self):
        raw=b'exact\r\n';digest=hashlib.sha256(raw).hexdigest()
        self.assertEqual(a.exact(raw,digest,7),raw)
        for other,h,n in [(raw,digest,6),(b'exact\n',digest,6),(raw,'0'*64,7),(raw,digest,True)]:
            with self.assertRaises(ValueError):a.exact(other,h,n)
    def test_only_explicit_js_partition(self):
        rows=[{'path':'a.js'},{'path':'a.cjs'},{'path':'a.mjs'},{'path':'a.js.in'}]
        self.assertEqual(a.select(rows),[{'path':'a.js'}])
    def test_external_and_traversal_paths_refuse(self):
        for path in ['../outside.js','D:/other/file.js','D:file.js',r'\\host\share\x.js','']:
            with self.subTest(path=path),self.assertRaises(ValueError):a.relative_name(path)
        self.assertEqual(a.relative_name('artifacts/a.js'),'artifacts/a.js')
    def test_document_parsed_from_single_authenticated_read(self):
        with patch.object(a,'read',side_effect=[b'{"one":1}',b'{"two":2}']) as reader:
            raw,value=a.json_snapshot(Path('map.json'),100)
            self.assertEqual(raw,b'{"one":1}');self.assertEqual(value,{'one':1})
            self.assertEqual(reader.call_count,1)
    def test_final_recheck_refuses_same_length_drift(self):
        with patch.object(a,'read',return_value=b'abd'):
            with self.assertRaisesRegex(ValueError,'input drift'):a.unchanged(Path('input'),b'abc',100)
        with patch.object(a,'read',return_value=b'abc'):a.unchanged(Path('input'),b'abc',100)
    def test_duplicate_path_refuses(self):
        with self.assertRaises(ValueError):a.select([{'path':'a.js'},{'path':'a.js'}])
    def test_expired_child_never_starts(self):
        with patch.object(a.time,'monotonic',return_value=10),patch.object(a.subprocess,'check_output',side_effect=AssertionError('spawn')):
            with self.assertRaises(ValueError):a.command(['node'],9)
    def test_child_late_return_refuses(self):
        with patch.object(a.time,'monotonic',side_effect=[1,2,11]),patch.object(a.subprocess,'check_output',return_value=b'{}') as child:
            with self.assertRaises(ValueError):a.command(['node'],10)
            self.assertEqual(child.call_args.kwargs['timeout'],8)

if __name__=='__main__':unittest.main(verbosity=2)
