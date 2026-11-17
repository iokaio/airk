# SPDX-License-Identifier: Apache-2.0
"""Pure in-memory archive/path controls; no network, filesystem outputs or tools."""
import io
import hashlib
import json
from pathlib import Path
from unittest.mock import patch
import unittest
import zipfile
import project_archives as p

class ArchiveControls(unittest.TestCase):
    def test_consumed_pin_and_duplicate_json_refuse(self):
        with self.assertRaisesRegex(ValueError,'duplicate'):p.strict_json(b'{"x":1,"x":2}')
        with patch.object(p,'checked',return_value=Path('authority')),patch.object(p,'consumed',return_value=(b'replaced',{'sha256':'different','bytes':8})):
            with self.assertRaisesRegex(ValueError,'consumed evidence pin changed'):p.pinned_bytes(Path('.'),{'path':'authority','sha256':'expected','bytes':8})

    def test_post_read_archive_drift_discards_claims(self):
        root=p.REPOSITORY;data=io.BytesIO()
        with zipfile.ZipFile(data,'w') as z:z.writestr('root/tool.py',b'abc')
        def pin(raw):return {'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}
        archived=data.getvalue();authority=json.dumps({'digest':pin(archived)['sha256']}).encode()
        spec={'name':'test','roots':['vendor'],'archive_prefix':'root/','format':'zip','repository':'https://example.test/upstream','revision':'fixed','archive':dict(path='archive.zip',**pin(archived)),'authority':dict(path='authority.json',**pin(authority)),'authority_selector':['digest']}
        catalog=json.dumps({'schema':'airk.upstream-tool-archives/1','archives':[spec]}).encode()
        delta=json.dumps({'lanes':{'test':[{'path':'vendor/tool.py','source_sha256':pin(b'abc')['sha256'],'bytes':3}]}}).encode()
        blobs={root/'catalog.json':catalog,root/'delta.json':delta,root/'authority.json':authority,root/'archive.zip':archived}
        def consumed(path,*args):return blobs[path],pin(blobs[path])
        def measure(path,*args):
            if path==root/'archive.zip':return pin(b'drift')
            if path==root/'vendor/tool.py':return pin(b'abc')
            return pin(b'projector')
        with patch.object(p,'checked',side_effect=lambda r,path:r/path),patch.object(p,'consumed',side_effect=consumed),patch.object(p,'measure',side_effect=measure):
            result=p.project(root/'delta.json',root/'catalog.json',root)
        self.assertEqual(result['selected_count'],1)
        self.assertEqual(result['files'],[])
        self.assertFalse(result['complete_within_selected_scope'])
        self.assertIn('archive changed',result['errors'][0]['error'])

    def test_relative_paths_refuse_escape_and_alias_spellings(self):
        for value in ('../x','/x','a/../b','a//b','C:x','a\\b'):
            with self.subTest(value=value),self.assertRaises(ValueError):p.safe_relative(value)
        self.assertEqual(p.safe_relative('share/cmake/Modules/Test.cmake'),'share/cmake/Modules/Test.cmake')

    def test_archive_members_are_read_without_extraction(self):
        data=io.BytesIO()
        with zipfile.ZipFile(data,'w') as z:z.writestr('root/tool.py',b'abc')
        with zipfile.ZipFile(io.BytesIO(data.getvalue())) as z:
            rows=p.archive_roster(z,'zip');actual=p.member_digest(z,'zip',rows['root/tool.py'])
            self.assertEqual(actual['bytes'],3)
            self.assertEqual(actual['sha256'],'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')

    def test_duplicate_case_member_refuses(self):
        data=io.BytesIO()
        with zipfile.ZipFile(data,'w') as z:
            z.writestr('A.py',b'a');z.writestr('a.py',b'b')
        with zipfile.ZipFile(io.BytesIO(data.getvalue())) as z:
            with self.assertRaisesRegex(ValueError,'case-colliding'):p.archive_roster(z,'zip')

    def test_askpass_selection_only_maps_known_basename(self):
        spec=dict(roots=[],archive_prefix='',askpass_member_prefix='stock/git/')
        self.assertEqual(p.member_for('artifacts/profile/User/globalStorage/vscode.git/askpass/abcd/askpass.sh',spec),'stock/git/askpass.sh')
        self.assertIsNone(p.member_for('artifacts/profile/User/globalStorage/vscode.git/askpass/abcd/credentials.json',spec))
        self.assertIsNone(p.member_for('artifacts/askpass.sh',spec))

if __name__=='__main__':unittest.main(verbosity=2)
