# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import io
from types import SimpleNamespace
from pathlib import Path
import unittest
from unittest import mock
spec=importlib.util.spec_from_file_location('ort_projection',Path(__file__).with_name('project.py'))
owner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)

class Projection(unittest.TestCase):
    def test_stream_growth_and_truncation_refuse(self):
        class Stream(io.BytesIO):
            def fileno(self):return 123
        info=SimpleNamespace(st_dev=1,st_ino=2,st_size=3,st_mtime_ns=4)
        for raw in (b'ab',b'abcd'):
            path=mock.Mock();path.stat.return_value=info;path.open.return_value=Stream(raw)
            with mock.patch.object(owner,'input_path',return_value=path),mock.patch.object(owner.os,'fstat',return_value=info):
                with self.assertRaises(ValueError):owner.file_pin('artifacts/input')
    def test_known_git_blob_identity(self):
        self.assertEqual(owner.blob_oid(b'hello\n'),'ce013625030ba8dba906f756967f9e9ca394464a')
    def test_normalized_observation_conflict_refuses(self):
        row={'path':'artifacts/a.py','source_sha256':'a'*64,'bytes':2,'git_blob_oid':'x','normalized_text_git_blob_oid':'one'}
        report={'schema':'airk.private.source-inventory/1','candidates':[row]}
        changed={'schema':report['schema'],'candidates':[dict(row,normalized_text_git_blob_oid='two')]}
        with self.assertRaises(ValueError):owner.observed_rows([report,changed])
    def test_archive_capture_rejects_replacement_bytes(self):
        class Stream(io.BytesIO):
            def fileno(self):return 123
        info=SimpleNamespace(st_dev=1,st_ino=2,st_size=8,st_mtime_ns=4)
        path=mock.Mock();path.stat.return_value=info;path.open.side_effect=[Stream(b'original'),Stream(b'changed!')]
        with mock.patch.object(owner,'input_path',return_value=path),mock.patch.object(owner.os,'fstat',return_value=info):
            with self.assertRaises(ValueError):owner.authenticated_bytes('artifacts/archive',owner.hashlib.sha256(b'original').hexdigest())
        self.assertEqual(path.open.call_count,2)
    def test_all_bundle_kinds_are_bounded(self):
        for filename in ('summary.json','upstream-tree.json','projection.json','source-disposition.json'):
            with mock.patch.object(owner,'MAX_FILE',4):
                with self.assertRaises(ValueError):owner.validate_bundle({'files':{filename:'12345'}})
    def test_exact_locked_protobuf_release_url(self):
        self.assertEqual(owner.upstream_from_url('https://github.com/protocolbuffers/protobuf/releases/download/v33.6/protobuf-33.6.tar.gz'),('https://github.com/protocolbuffers/protobuf','v33.6'))
        with self.assertRaises(ValueError):owner.upstream_from_url('https://github.com/protocolbuffers/protobuf/releases/download/v33.7/protobuf-33.7.tar.gz')
    def test_authenticated_tar_member_projection(self):
        archive=io.BytesIO();body=b'upstream source\n'
        with owner.tarfile.open(fileobj=archive,mode='w:gz') as tar:
            info=owner.tarfile.TarInfo('protobuf-33.6/src/example.cc');info.size=len(body);tar.addfile(info,io.BytesIO(body))
        rows=owner.archive_members(archive.getvalue(),{'src/example.cc'},True)
        self.assertEqual(rows,{'src/example.cc':{'sha256':owner.hashlib.sha256(body).hexdigest(),'bytes':len(body)}})
    def test_raw_requires_size_and_digest(self):
        row={'source_sha256':'a','bytes':2}
        self.assertTrue(owner.raw_match(row,{'sha256':'a','bytes':2}))
        self.assertFalse(owner.raw_match(row,{'sha256':'a','bytes':3}))
        self.assertFalse(owner.raw_match(row,{'sha256':'b','bytes':2}))
    def test_git_representation_is_explicit(self):
        row={'git_blob_oid':'raw','normalized_text_git_blob_oid':'lf'}
        self.assertEqual(owner.git_match(row,'raw'),'raw')
        self.assertEqual(owner.git_match(row,'lf'),'CRLF-to-LF')
        self.assertIsNone(owner.git_match(row,'different'))
        self.assertIsNone(owner.git_match(row,None))
    def test_source_escape_refused(self):
        for path in ('../x','/x','a/../x','a\\x','C:/x','a//x'):
            with self.assertRaises(ValueError):owner.safe_relative(path)
    def test_upstream_revision_from_pinned_url(self):
        self.assertEqual(owner.upstream_from_url('https://github.com/google/flatbuffers/archive/refs/tags/v23.5.26.zip'),('https://github.com/google/flatbuffers','refs/tags/v23.5.26'))
        self.assertEqual(owner.upstream_from_url('https://github.com/example/repo/archive/abcdef/archive.zip'),('https://github.com/example/repo','abcdef'))
        with self.assertRaises(ValueError):owner.upstream_from_url('https://other.invalid/file.zip')
    def test_conflicting_observations_refuse(self):
        row={'path':'artifacts/a.py','source_sha256':'a'*64,'bytes':2,'git_blob_oid':'x'}
        one={'schema':'airk.private.source-inventory/1','candidates':[row]}
        two={'schema':one['schema'],'candidates':[dict(row,source_sha256='b'*64)]}
        with self.assertRaises(ValueError):owner.observed_rows([one,two])
        self.assertEqual(len(owner.observed_rows([one,one])),1)
    def test_duplicate_json_keys_refuse(self):
        with self.assertRaises(ValueError):owner.unique_pairs([('x',1),('x',2)])
    def test_binding_has_exact_proof_pin(self):
        row={'source':'artifacts/a.py','source_sha256':'a'*64,'source_bytes':2,'evidence':[],'upstream':{'path':'a.py'}}
        result=owner.bundle_files('tree',{'bindings':[{'source':'artifacts/a.py'}]},[row],'artifacts/proof')
        proof=result['files']['projection.json'].encode()
        mapping=json.loads(result['files']['source-disposition.json'])['files'][0]
        self.assertEqual(mapping['projection']['binding_source'],'artifacts/a.py')
        self.assertEqual(mapping['projection']['sha256'],owner.hashlib.sha256(proof).hexdigest())
        self.assertEqual(mapping['evidence'][0]['bytes'],len(proof))
    def test_oversized_group_refuses(self):
        with mock.patch.object(owner,'MAX_FILE',10):
            with self.assertRaises(ValueError):owner.bundle_files('x',{'long':'value'},[],'artifacts/proof')
if __name__=='__main__':unittest.main()
