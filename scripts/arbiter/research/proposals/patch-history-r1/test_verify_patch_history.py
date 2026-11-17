# SPDX-License-Identifier: Apache-2.0
"""Read-only refusal controls; no patch application or fixture writes."""
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('patch_history_owner',HERE/'verify_patch_history.py')
owner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)

class PatchHistoryControls(unittest.TestCase):
    def refused_map(self, change):
        value=json.loads((HERE/'rescue-map.json').read_bytes())
        change(value)
        altered=json.dumps(value).encode()
        original=Path.read_bytes
        def read(path):
            return altered if path==HERE/'rescue-map.json' else original(path)
        with patch.object(Path,'read_bytes',read):
            with self.assertRaises(ValueError):
                owner.verify(False)

    def test_missing_source_row_refuses(self):
        self.refused_map(lambda value:value['files'].pop())

    def test_duplicate_source_with_preserved_count_refuses(self):
        self.refused_map(lambda value:value['files'].__setitem__(1,value['files'][0]))

    def test_changed_disposition_refuses(self):
        self.refused_map(lambda value:value['files'][0].__setitem__('disposition','unreviewed_skip'))

    def test_build_role_representation_is_exact(self):
        self.assertEqual(owner.build_identity(b'hello\n','raw_bytes'),('raw',{'source_sha256':'5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03'}))
        self.assertEqual(owner.build_identity(b'hello\r\n','CRLF_to_LF_only'),('CRLF-to-LF',{'git_blob_oid':'ce013625030ba8dba906f756967f9e9ca394464a'}))
        for raw,kind in ((b'a\0b','CRLF_to_LF_only'),(b'\xff','CRLF_to_LF_only'),(b'a','invented')):
            with self.assertRaises((ValueError,UnicodeError)):owner.build_identity(raw,kind)

    def test_only_line_endings_may_normalize_for_upstream(self):
        self.assertEqual(owner.matched_member(b'a\r\nb\r\n',b'a\nb\n'),'CRLF_to_LF_only')
        self.assertEqual(owner.matched_member(b'a\n',b'a\n'),'raw_bytes')
        for changed in (b'a \n',b'b\n',b'a\r'):
            with self.assertRaises(ValueError):
                owner.matched_member(changed,b'a\n')

if __name__=='__main__':
    unittest.main(verbosity=2)
