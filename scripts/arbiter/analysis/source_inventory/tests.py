# SPDX-License-Identifier: Apache-2.0
"""Pure inventory controls; no fixture files, Git mutations, or candidate execution."""
from collections import Counter
from contextlib import nullcontext
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import stat
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('source_inventory', Path(__file__).with_name('inventory.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def info(size=6, mode=stat.S_IFREG, attributes=0):
    return SimpleNamespace(st_dev=1, st_ino=2, st_size=size, st_mtime_ns=3, st_mode=mode, st_file_attributes=attributes)


class MemoryFile(io.BytesIO):
    def fileno(self):
        return 42


class Hashing(unittest.TestCase):
    def file(self, data=b'hello\n', observations=None):
        before = info(len(data))
        path = SimpleNamespace(lstat=lambda: before, open=lambda mode: MemoryFile(data))
        if observations is not None:
            path.lstat = observations
        return path, before

    def test_literal_git_sha1_and_raw_sha256(self):
        path, before = self.file()
        with patch.object(m.os, 'fstat', return_value=before):
            result = m.source_hashes(path, 'sha1')
        self.assertEqual(result['git_blob_oid'], 'ce013625030ba8dba906f756967f9e9ca394464a')
        self.assertEqual(result['source_sha256'], '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03')

    def test_archive_evidence_stream_and_cap(self):
        path,before=self.file()
        with patch.object(m.os,'fstat',return_value=before):
            self.assertEqual(m.evidence_hashes(path,'sha1')['source_sha256'],'5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03')
        with self.assertRaises(ValueError):m.evidence_hashes(path,'sha1',max_bytes=5)

    def test_sha256_git_format_uses_blob_header(self):
        path, before = self.file()
        with patch.object(m.os, 'fstat', return_value=before):
            result = m.source_hashes(path, 'sha256')
        self.assertEqual(result['git_blob_oid'], hashlib.sha256(b'blob 6\0hello\n').hexdigest())
        self.assertNotEqual(result['git_blob_oid'], result['source_sha256'])

    def test_crlf_is_not_normalized(self):
        path, before = self.file(b'hello\r\n')
        with patch.object(m.os, 'fstat', return_value=before):
            result = m.source_hashes(path, 'sha1')
        self.assertNotEqual(result['git_blob_oid'], 'ce013625030ba8dba906f756967f9e9ca394464a')
        self.assertEqual(result['bytes'], 7)

    def test_changed_file_and_size_cap_refuse(self):
        path, before = self.file()
        changed = info(); changed.st_mtime_ns = 4
        with patch.object(m.os, 'fstat', side_effect=[before, changed]), self.assertRaises(ValueError):
            m.source_hashes(path, 'sha1')
        with self.assertRaises(ValueError):
            m.source_hashes(path, 'sha1', max_bytes=5)

    def test_reparse_symlink_and_unknown_object_format_refuse(self):
        path, _ = self.file()
        for observed in [info(attributes=0x400), info(mode=stat.S_IFLNK)]:
            path.lstat = lambda observed=observed: observed
            with self.assertRaises(ValueError):
                m.source_hashes(path, 'sha1')
        path, _ = self.file()
        with self.assertRaises(ValueError):
            m.source_hashes(path, 'sha512')


class Classification(unittest.TestCase):
    def setUp(self):
        self.hashes = {'source_sha256':'a'*64, 'git_blob_oid':'b'*40}

    def test_committed_identity_precedes_worktree_match(self):
        value = m.classify(self.hashes, {'b'*40}, {'a'*64:['scripts/current.py']})
        self.assertEqual(value['classification'], 'committed_git_blob_match')
        self.assertEqual(value['tracked_worktree_matches'], ['scripts/current.py'])

    def test_uncommitted_tracked_match_is_disclosed(self):
        value = m.classify(self.hashes, set(), {'a'*64:['scripts/z.py','scripts/a.py']})
        self.assertEqual(value['classification'], 'tracked_worktree_match')
        self.assertEqual(value['tracked_worktree_matches'], ['scripts/a.py','scripts/z.py'])

    def test_remaining_is_candidate_not_purge_authority(self):
        value = m.classify(self.hashes, set(), {})
        self.assertEqual(value['classification'], 'remaining_implementation_candidate')
        self.assertIn('human review', value['reason'])

    def test_git_object_types_and_partial_observation(self):
        with patch.object(m, 'git', side_effect=[b'a tree\nb file.py\n', b'a tree\nb blob\n']):
            self.assertEqual(m.reachable_blobs(Path('repo'), 'HEAD'), {'b'})
        with patch.object(m, 'git', side_effect=[b'a tree\nb file.py\n', b'b blob\n']), self.assertRaises(ValueError):
            m.reachable_blobs(Path('repo'), 'HEAD')


class Traversal(unittest.TestCase):
    def test_permission_error_is_retained(self):
        root = Path.cwd()/'literal-repo/artifacts'
        errors, excluded, counts = [], [], Counter()
        def denied(path):
            raise PermissionError('access denied')
        with patch.object(Path, 'lstat', return_value=info(mode=stat.S_IFDIR)):
            self.assertEqual(list(m.walk_candidates(root, root.parent, errors, excluded, counts, denied)), [])
        self.assertEqual(errors[0]['type'], 'PermissionError')
        self.assertEqual(errors[0]['stage'], 'traversal')
        self.assertIn('access denied', errors[0]['message'])

    def test_vendor_and_reparse_are_not_walked_models_data_not_read(self):
        repo = Path.cwd()/'literal-repo'; root = repo/'artifacts'
        def entry(name, mode, attributes=0):
            return SimpleNamespace(name=name, stat=lambda **kwargs:info(mode=mode, attributes=attributes))
        entries=[entry('vendor',stat.S_IFDIR),entry('link',stat.S_IFDIR,0x400),entry('model.safetensors',stat.S_IFREG),entry('analyze.py',stat.S_IFREG)]
        calls=[]
        def scan(path):
            calls.append(path)
            return nullcontext(entries)
        errors, excluded, counts = [], [], Counter()
        with patch.object(Path, 'lstat', return_value=info(mode=stat.S_IFDIR)):
            self.assertEqual(list(m.walk_candidates(root,repo,errors,excluded,counts,scan)), [root/'analyze.py'])
        self.assertEqual(calls,[root]);self.assertEqual(len(excluded),2)
        self.assertEqual(counts['non_code_files_excluded'],1)
        self.assertEqual(errors,[])

    def test_source_selection_and_explicit_exclusions(self):
        for name in ['analyze.py','TRAIN.PS1','CMakeLists.txt','fit.ipynb','helper.rs','collector.patch','change.diff','owner.rs.in','script.py.template','header.hpp.tmpl','CMakeLists.txt.in','collector.cu','kernel.cuh','ui.svelte','layout.css','workflow.yml','Cargo.toml','api.proto']:
            self.assertTrue(m.candidate_name(name))
        for name in ['model.onnx','weights.bin','data.json','tokenizer.json','sample.parquet','data.json.in','weights.bin.template']:
            self.assertFalse(m.candidate_name(name))
        self.assertIsNone(m.exclusion('models'))
        self.assertIsNone(m.exclusion('weights-v2'))
        self.assertIsNotNone(m.exclusion('native-fixtures-r1'))

    def test_supplemental_extension_selection_excludes_prior_formats(self):
        with patch.object(m,'EXTENSIONS',frozenset({'.cu','.yaml'})),patch.object(m,'BASENAMES',frozenset()):
            self.assertTrue(m.candidate_name('kernel.cu'))
            self.assertTrue(m.candidate_name('kernel.cu.in'))
            self.assertTrue(m.candidate_name('workflow.yaml'))
            self.assertFalse(m.candidate_name('runner.py'))
            self.assertFalse(m.candidate_name('Makefile'))

    def test_root_escape_overlap_and_reparse_refuse(self):
        repo = Path.cwd()/'literal-repo'
        with self.assertRaises(ValueError):m.validate_roots(repo,[repo/'outside'])
        with patch.object(Path,'lstat',return_value=info(mode=stat.S_IFDIR)),patch.object(Path,'is_dir',return_value=True),self.assertRaises(ValueError):
            m.validate_roots(repo,[repo/'artifacts/a',repo/'artifacts/a/b'])
        with patch.object(Path,'lstat',return_value=info(mode=stat.S_IFDIR,attributes=0x400)),self.assertRaises(ValueError):
            m.validate_roots(repo,[repo/'artifacts/a'])


class NormalizationAndMigration(unittest.TestCase):
    def test_normalized_match_never_claims_byte_identity(self):
        oid=m.normalized_blob(b'hello\r\n','sha1')
        self.assertEqual(oid,'ce013625030ba8dba906f756967f9e9ca394464a')
        result=m.classify({'source_sha256':'a'*64,'git_blob_oid':'b'*40,'normalized_text_git_blob_oid':oid},{oid},{})
        self.assertEqual(result['classification'],'normalized_text_committed_blob_match')
        self.assertFalse(result['raw_byte_identity_match'])
        self.assertIn('NOT raw-byte',result['reason'])

    def test_binary_non_utf8_and_lone_cr_not_normalized(self):
        for data in [b'hello\n',b'hello\r',b'\x00hello\r\n',b'\xe9\r\n']:
            self.assertIsNone(m.normalized_blob(data,'sha1'))

    def row(self):
        return {'source':'artifacts/old/train.py','destination':'scripts/arbiter/training/train.py',
                'original_sha256':'a'*64,'maintained_sha256':'b'*64,'original_bytes':10,'maintained_bytes':20}

    def observe(self,path,fmt):
        return {'source_sha256':'a'*64,'bytes':10} if 'artifacts' in path.parts else {'source_sha256':'b'*64,'bytes':20}

    def test_migration_target_and_original_bytes_verified(self):
        repo=Path.cwd()/'literal-repo'
        with patch.object(Path,'lstat',return_value=info()):
            verified=m.verify_migration_row(self.row(),repo,'sha1',self.observe)
        self.assertEqual(verified['maintained'],{'path':'scripts/arbiter/training/train.py','sha256':'b'*64,'bytes':20})
        result=m.classify({'source_sha256':'a'*64,'git_blob_oid':'c'*40},set(),{}, {'a'*64:[verified]})
        self.assertEqual(result['classification'],'authenticated_migration_map_match')
        self.assertFalse(result['raw_byte_identity_match'])

    def test_declared_retrieval_helper_original_verified(self):
        row=self.row();row['source']='tools/retrieval-train/safe_int8.py'
        def observe(path,fmt):
            return {'source_sha256':'a'*64,'bytes':10} if 'tools' in path.parts else {'source_sha256':'b'*64,'bytes':20}
        with patch.object(Path,'lstat',return_value=info()):
            result=m.verify_migration_row(row,Path.cwd()/'literal-repo','sha1',observe)
        self.assertEqual(result['declared_source'],'tools/retrieval-train/safe_int8.py')
        row['source']='tools/other/helper.py'
        with patch.object(Path,'lstat',return_value=info()),self.assertRaises(ValueError):
            m.verify_migration_row(row,Path.cwd()/'literal-repo','sha1',observe)

    def test_native_current_owner_is_verified_not_copied(self):
        row=self.row();row['destination']='crates/airk-retrieval/src/owner.rs'
        with patch.object(Path,'lstat',return_value=info()):
            result=m.verify_migration_row(row,Path.cwd()/'literal-repo','sha1',self.observe)
        self.assertEqual(result['maintained']['path'],'crates/airk-retrieval/src/owner.rs')

    def test_map_tamper_and_target_drift_refuse(self):
        for key,bad in [('original_sha256','c'*64),('maintained_sha256','c'*64),('maintained_bytes',19),('original_bytes',True)]:
            row=self.row();row[key]=bad
            with self.subTest(key=key),patch.object(Path,'lstat',return_value=info()),self.assertRaises(ValueError):
                m.verify_migration_row(row,Path.cwd()/'literal-repo','sha1',self.observe)
        def drift(path,fmt):return {'source_sha256':'f'*64,'bytes':20}
        with patch.object(Path,'lstat',return_value=info()),self.assertRaises(ValueError):
            m.verify_migration_row(self.row(),Path.cwd()/'literal-repo','sha1',drift)

    def test_map_target_escape_and_alternate_source_scope_refuse(self):
        for key,bad in [('destination','scripts/arbiter/../outside.py'),('destination','artifacts/ignored.py'),('destination','C:/foreign.py'),
                        ('destination','scripts/arbiter/file.py:stream'),('source','scripts/old.py')]:
            row=self.row();row[key]=bad
            with self.subTest(key=key,bad=bad),patch.object(Path,'lstat',return_value=info()),self.assertRaises(ValueError):
                m.verify_migration_row(row,Path.cwd()/'literal-repo','sha1',self.observe)

    def test_range_inverse_indexes_extracted_body_not_whole_container(self):
        import copy
        raw=b'header\nprint(1)\nfooter';body=b'print(1)\n';prefix='# SPDX-License-Identifier: Apache-2.0\n';target=prefix.encode()+body
        row={'source':'artifacts/runbook.md','destination':'scripts/arbiter/probe.py.in','original_sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'maintained_sha256':hashlib.sha256(target).hexdigest(),'maintained_bytes':len(target),'disposition':'reviewed-diagnostic-byte-range',
             'changes':[{'kind':'byte-range-extraction','offset':7,'bytes':len(body),'extracted_sha256':hashlib.sha256(body).hexdigest()},{'kind':'spdx-prefix','prefix':prefix}]}
        def observe(path,fmt):
            data=raw if path.name=='runbook.md' else target
            return {'source_sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}
        with patch.object(Path,'lstat',return_value=info()),patch.object(Path,'read_bytes',return_value=raw):
            result=m.verify_migration_row(row,Path.cwd(),'sha1',observe)
            self.assertEqual(result['original_sha256'],hashlib.sha256(body).hexdigest());self.assertNotEqual(result['original_sha256'],row['original_sha256']);self.assertEqual(result['container_sha256'],row['original_sha256'])
            for key,value in [('offset',True),('offset',-1),('offset',len(raw)),('bytes',0),('bytes',True),('bytes',len(raw)),('extracted_sha256','f'*64),('kind','byte-range-unchecked')]:
                bad=copy.deepcopy(row);bad['changes'][0][key]=value
                with self.assertRaises(ValueError):m.verify_migration_row(bad,Path.cwd(),'sha1',observe)
            for changes in [row['changes'][::-1],row['changes'][:1],row['changes']+[{}],[row['changes'][0],{'kind':'spdx-prefix','prefix':prefix+'extra'}]]:
                with self.assertRaises(ValueError):m.verify_migration_row({**row,'changes':changes},Path.cwd(),'sha1',observe)
            with self.assertRaises(ValueError):m.verify_migration_row({**row,'bytes':len(raw)+1},Path.cwd(),'sha1',observe)
        with patch.object(Path,'lstat',return_value=info()),patch.object(Path,'read_bytes',return_value=raw+b'drift'),self.assertRaises(ValueError):m.verify_migration_row(row,Path.cwd(),'sha1',observe)

    def test_corpus_payload_requires_exact_declared_bytes(self):
        repo=Path.cwd()/'literal-repo';path='artifacts/data/source-tree/third_party.py'
        row={'path':path,'bytes':10,'source_sha256':'a'*64,'classification':'remaining_implementation_candidate'}
        declared={str(repo/path):{'bytes':10,'sha256':'a'*64,'reason':'documented corpus input'}}
        self.assertEqual(m.disposition(row,declared,repo)[0],'documented_corpus_input')
        row['source_sha256']='b'*64
        self.assertNotEqual(m.disposition(row,declared,repo)[0],'documented_corpus_input')

    def test_documented_corpus_root_is_not_opened(self):
        repo=Path.cwd()/'literal-repo';root=repo/'artifacts/corpus/source'
        errors,excluded,counts=[],[],Counter()
        def forbidden(path):self.fail('corpus data traversed')
        self.assertEqual(list(m.walk_candidates(root,repo,errors,excluded,counts,forbidden,
            corpus_roots={str(root):{'reason':'documented input','evidence':{'path':'receipt.json','sha256':'a'*64}}})),[])
        self.assertEqual(excluded[0]['reason'],'documented input')

    def test_snapshot_never_automatically_superseded(self):
        row={'path':'artifacts/exec-test/build/source/crates/owner.rs','classification':'remaining_implementation_candidate','source_sha256':'a'*64,'bytes':10}
        role,reason=m.disposition(row,{},Path.cwd())
        self.assertEqual(role,'repository_snapshot_or_proposal_review')
        self.assertIn('need review',reason)

    def test_gap_list_bound_and_unique_hash_counts(self):
        rows=[{'path':'artifacts/exec-test/'+name+'/runner.py','classification':'remaining_implementation_candidate',
               'source_sha256':sha*64,'disposition':'authored_operational_or_research_candidate'} for name,sha in [('a','a'),('b','b'),('c','a')]]
        value=m.review_summary(rows,1)
        self.assertEqual(value['migration_gap_distinct_hashes'],2)
        self.assertEqual(value['migration_gap_list_omitted'],1)
        self.assertEqual(value['migration_gap_review'][0]['path_count'],2)


class EmbeddedSource(unittest.TestCase):
    def test_decoded_pointer_and_target_body_are_independently_pinned(self):
        raw=b'print(1)\n';prefix='# SPDX-License-Identifier: Apache-2.0\n';maintained=prefix.encode()+raw
        row={'container':'artifacts/proposal.json','container_sha256':'a'*64,'container_bytes':100,'json_pointer':'/scripts/a~1b',
             'decoded_sha256':hashlib.sha256(raw).hexdigest(),'decoded_bytes':len(raw),'destination':'scripts/arbiter/history/tool.py',
             'maintained_sha256':hashlib.sha256(maintained).hexdigest(),'maintained_bytes':len(maintained),'prefix':prefix}
        with patch.object(Path,'lstat',return_value=info()),patch.object(m,'verified_json',return_value=({'scripts':{'a/b':raw.decode()}},{'source_sha256':'a'*64,'bytes':100})),patch.object(m,'source_hashes',return_value={'source_sha256':row['maintained_sha256'],'bytes':len(maintained)}):
            self.assertTrue(m.verify_embedded_row(row,Path.cwd(),'sha1')['embedded_source'])
            for destination in ['packages/vscode/src/owner.ts','crates/test/src/owner.rs','tools/test/owner.py']:
                self.assertTrue(m.verify_embedded_row({**row,'destination':destination},Path.cwd(),'sha1')['embedded_source'])
            for destination in ['artifacts/copy.py','packages/../artifacts/copy.py']:
                with self.assertRaises(ValueError):m.verify_embedded_row({**row,'destination':destination},Path.cwd(),'sha1')
            row['decoded_sha256']='f'*64
            with self.assertRaises(ValueError):m.verify_embedded_row(row,Path.cwd(),'sha1')


class EncodedSource(unittest.TestCase):
    def test_text_container_exact_pin_pointer_format_and_utf8_refusals(self):
        raw=b"trace b64decode('cHJpbnQoMSkK')";body=b'print(1)\n'
        row={'container':'artifacts/stderr.txt','container_format':'utf8-text','container_sha256':hashlib.sha256(raw).hexdigest(),'container_bytes':len(raw),'json_pointer':'',
             'decoded_sha256':hashlib.sha256(body).hexdigest(),'decoded_bytes':len(body),'destination':'scripts/arbiter/history/tool.py','maintained_sha256':hashlib.sha256(body).hexdigest(),'maintained_bytes':len(body),
             'encoded_field_sha256':hashlib.sha256(raw).hexdigest(),'encoded_field_bytes':len(raw),'decode_method':'single-python-b64decode-literal/1'}
        def observe(path,fmt):
            data=raw if path.name=='stderr.txt' else body
            return {'source_sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}
        with patch.object(Path,'lstat',return_value=info()),patch.object(Path,'read_bytes',new=lambda path:raw),patch.object(m,'source_hashes',side_effect=observe):
            self.assertTrue(m.verify_embedded_row(row,Path.cwd(),'sha1',encoded=True)['embedded_source'])
            for key,value in [('json_pointer','/stderr'),('container_format','arbitrary'),('container_sha256','f'*64),('container_bytes',len(raw)+1)]:
                bad={**row,key:value}
                with self.assertRaises(ValueError):m.verify_embedded_row(bad,Path.cwd(),'sha1',encoded=True)
            with self.assertRaises(ValueError):m.verify_embedded_row(row,Path.cwd(),'sha1',encoded=False)
            raw=b'\xff'
            with self.assertRaises(UnicodeDecodeError):m.verify_embedded_row(row,Path.cwd(),'sha1',encoded=True)

    def test_strict_single_literal_method_field_and_utf8_refusals(self):
        import base64
        body=b'print(1)\n';prefix='# SPDX-License-Identifier: Apache-2.0\n'
        def verify(field,method='single-python-b64decode-literal/1',override_hash=None,expected=body):
            maintained=prefix.encode()+expected
            row={'container':'artifacts/proposal.json','container_sha256':'a'*64,'container_bytes':100,'json_pointer':'/stderr',
                'decoded_sha256':hashlib.sha256(expected).hexdigest(),'decoded_bytes':len(expected),'destination':'scripts/arbiter/history/tool.py',
                'maintained_sha256':hashlib.sha256(maintained).hexdigest(),'maintained_bytes':len(maintained),'prefix':prefix,
                'encoded_field_sha256':override_hash or hashlib.sha256(field.encode()).hexdigest(),'encoded_field_bytes':len(field.encode()),'decode_method':method}
            with patch.object(Path,'lstat',return_value=info()),patch.object(m,'verified_json',return_value=({'stderr':field},{'source_sha256':'a'*64,'bytes':100})),patch.object(m,'source_hashes',return_value={'source_sha256':row['maintained_sha256'],'bytes':len(maintained)}):
                return m.verify_embedded_row(row,Path.cwd(),'sha1',encoded=True)
        field="b64decode('"+base64.b64encode(body).decode()+"')"
        self.assertTrue(verify(field)['embedded_source'])
        for bad in [field+' '+field,field+' b64decode("unsupported")',"b64decode('a===')"]:
            with self.assertRaises(ValueError):verify(bad)
        with self.assertRaises(ValueError):verify(field,method='arbitrary-decoder')
        with self.assertRaises(ValueError):verify(field,override_hash='f'*64)
        with self.assertRaises(UnicodeDecodeError):verify("b64decode('/w==')",expected=b'\xff')


class BroadRouting(unittest.TestCase):
    def test_patch_precedence_and_disjoint_review_lanes(self):
        spec=importlib.util.spec_from_file_location('delta',Path(__file__).with_name('delta.py'))
        d=importlib.util.module_from_spec(spec);spec.loader.exec_module(d)
        for path,expected in [
            ('artifacts/advanced-split/ort130-x64/source/fix.patch','fp16_readiness'),
            ('artifacts/advanced-split/ort130-x64/tools/cmake/run.py','artifact_code_audit'),
            ('artifacts/advanced-split/ort130-x64/source/session.cc','platform_inventory'),
            ('artifacts/advanced-split/models/example/Cargo.toml','focus_review'),
            ('artifacts/advanced-split/models/example/grade.py','graph_emitter'),
            ('artifacts/v190/runtime/run.py','component_review')]:
            self.assertEqual(d.broad_lane({'path':path}),expected)


class Routing(unittest.TestCase):
    def test_disjoint_complete_routing_never_proves_migration(self):
        spec=importlib.util.spec_from_file_location('routing',Path(__file__).with_name('disposition.py'))
        routing=importlib.util.module_from_spec(spec);spec.loader.exec_module(routing)
        cases=[('artifacts/v190/final-plan/exec-x/workspace-runner.py','component_review'),
               ('artifacts/v190/final-plan/exec-x/ui/run.mjs','fp16_readiness'),
               ('artifacts/advanced-split/models/actual-composed-service-r2/current-orchestration/goal.py','graph_emitter'),
               ('artifacts/advanced-split/models/large-gqa/converter.py','focus_review'),
               ('artifacts/advanced-split/models/backend-research/helper.py','artifact_code_audit')]
        rows=[]
        for path,expected in cases:
            row={'path':path,'source_sha256':'a'*64,'bytes':1,'classification':'remaining_implementation_candidate',
                 'disposition':'authored_operational_or_research_candidate','disposition_reason':'pending'}
            self.assertEqual(routing.lane(row),expected);rows.append(row)
        result=routing.disposition({'candidates':rows,'roots':['artifacts'],'complete_within_declared_scope':True},'b'*64)
        actual=[r['path'] for group in result['lanes'].values() for r in group]
        self.assertEqual(sorted(actual),sorted(p for p,_ in cases))
        self.assertEqual(len(actual),len(set(actual)))
        self.assertFalse(result['migration_complete'])
        self.assertTrue(result['routing_is_not_migration_proof'])


class EvidenceDisposition(unittest.TestCase):
    def setUp(self):
        spec=importlib.util.spec_from_file_location('routing',Path(__file__).with_name('disposition.py'))
        self.d=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.d)
        self.row={'source':'artifacts/upstream/code.py','source_sha256':'a'*64,'source_bytes':10,
                  'disposition':'upstream_research_input','reason':'pinned upstream input',
                  'evidence':[{'path':'artifacts/upstream/inventory.json','sha256':'b'*64}],
                  'upstream':{'repository':'owner/project','revision':'abcdef','path':'code.py'}}
    def observe(self,path,fmt):
        return {'source_sha256':('b' if path.name=='inventory.json' else 'a')*64,'bytes':10}
    def verify(self):
        with patch.object(Path,'lstat',return_value=info()):
            return self.d.verify_disposition_row(self.row,Path.cwd(),self.observe,m.checked_relative)
    def test_exact_source_and_evidence_required(self):
        self.assertFalse(self.verify()['qualification'])
        self.row['evidence'][0]['sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()
    def test_external_map_requires_exact_explicit_pin(self):
        path,digest=self.d.parse_map_pin('private/map.json='+'a'*64)
        self.assertTrue(path.is_absolute());self.assertEqual(digest,'a'*64)
        for value in ['private/map.json','private/map.json=passed','private/map.json='+'A'*64,'='+'a'*64]:
            with self.assertRaises(ValueError):self.d.parse_map_pin(value)

    def test_missing_or_boolean_source_size_refuses(self):
        for value in [None,True,-1]:
            self.row['source_bytes']=value
            with self.assertRaises(ValueError):self.verify()

    def test_unknown_provenance_and_source_tamper_remain_unresolved(self):
        self.row['upstream']['revision']=''
        with self.assertRaises(ValueError):self.verify()
        self.row['upstream']['revision']='abcdef';self.row['source_sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()
    def test_generated_typescript_requires_owner_and_matching_reproduced_hash(self):
        self.row['disposition']='generated_from_maintained_typescript'
        self.row['generated']={'compiler':{'path':'packages/admin/node_modules/typescript/lib/typescript.js','sha256':'a'*64,'bytes':10},
            'source_version':{'path':'artifacts/build/source.ts','sha256':'a'*64,'bytes':10,'maintained_path':'packages/admin/source.ts'},
            'options':{'target':'ES2022'},'method':'retained independent replay','reproduced_js_sha256':'a'*64}
        self.assertFalse(self.verify()['qualification'])
        self.row['generated']['reproduced_js_sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()
        self.row['generated']['reproduced_js_sha256']='a'*64
        del self.row['generated']['source_version']['maintained_path']
        with self.assertRaises(ValueError):self.verify()

    def test_fixed_commit_path_cache_rechecks_each_row_and_rejects_bad_locator(self):
        raw=b'const x = 1;\n';retained=raw.replace(b'\n',b'\r\n');oid=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
        self.row['disposition']='generated_from_maintained_typescript'
        version={'path':'artifacts/build/source.ts','sha256':hashlib.sha256(retained).hexdigest(),'bytes':len(retained),'git_commit':'c'*40,'git_path':'packages/source.ts','normalization':'CRLF to LF only','git_blob_oid':oid}
        self.row['generated']={'compiler':{'path':'packages/typescript.js','sha256':'a'*64,'bytes':10},'source_version':version,'options':{},'method':'exact retained replay','reproduced_js_sha256':'a'*64}
        from audit_memo import VerifiedMemo
        cache=VerifiedMemo();calls=[]
        def observe(path,fmt):
            return {'source_sha256':hashlib.sha256(retained).hexdigest(),'bytes':len(retained)} if path.name=='source.ts' else self.observe(path,fmt)
        oversized=False
        def git(args):
            calls.append(args)
            if 'rev-parse' in args:return (oid+'\n').encode()
            if '-s' in args:return str((16<<20)+1 if oversized else len(raw)).encode()
            return raw
        def verify():return self.d.verify_disposition_row(self.row,Path.cwd(),observe,m.checked_relative,git_cache=cache)
        with patch.object(Path,'lstat',return_value=info()),patch.object(Path,'read_bytes',new=lambda path:retained),patch.object(self.d.subprocess,'check_output',side_effect=git):
            verify();verify();self.assertEqual(len(calls),3)
            version['normalization']='none'
            with self.assertRaises(ValueError):verify()
            version['normalization']='CRLF to LF only';version['sha256']='f'*64
            with self.assertRaises(ValueError):verify()
            version['sha256']=hashlib.sha256(retained).hexdigest();version['git_blob_oid']='f'*40
            with self.assertRaises(ValueError):verify()
            version['git_blob_oid']=oid
            for key,bad in [('git_commit','HEAD'),('git_path','../escape.ts'),('git_path','packages/source.ts:stream')]:
                old=version[key];version[key]=bad
                with self.assertRaises(ValueError):verify()
                version[key]=old
            version['git_commit']='d'*40;verify();self.assertEqual(len(calls),6)
            version['git_path']='packages/other.ts';verify();self.assertEqual(len(calls),9)
            self.row['generated']['reproduced_js_sha256']='f'*64
            with self.assertRaises(ValueError):verify()

            self.row['generated']['reproduced_js_sha256']='a'*64;version['git_commit']='e'*40;oversized=True
            with self.assertRaises(ValueError):verify()
            self.assertEqual(len(calls),11);self.assertIn('-s',calls[-1])

    def test_typescript_shared_proof_cache_preserves_source_cap_and_fresh_output(self):
        self.row['disposition']='generated_from_maintained_typescript'
        self.row['generated']={'compiler':{'path':'packages/admin/node_modules/typescript/lib/typescript.js','sha256':'a'*64,'bytes':10},
            'source_version':{'path':'packages/admin/source.ts','sha256':'a'*64,'bytes':10},'options':{},'method':'exact replay','reproduced_js_sha256':'a'*64}
        seen=[];cache={}
        def fresh(path,fmt):seen.append(str(path));return self.observe(path,fmt)
        def shared(path,fmt):
            if str(path) not in cache:cache[str(path)]=fresh(path,fmt)
            return cache[str(path)]
        with patch.object(Path,'lstat',return_value=info()):
            for _ in range(2):self.d.verify_disposition_row(self.row,Path.cwd(),fresh,m.checked_relative,shared)
            self.assertEqual(sum(p.endswith('code.py') for p in seen),2)
            self.assertEqual(sum(p.endswith('typescript.js') for p in seen),1)
            self.assertEqual(sum(p.endswith('source.ts') for p in seen),1)
            for value in [True,-1,(16<<20)+1]:
                self.row['generated']['compiler']['bytes']=value
                with self.assertRaises(ValueError):self.d.verify_disposition_row(self.row,Path.cwd(),fresh,m.checked_relative,shared)

    def test_javascript_fixture_requires_exact_ast_owner_span_tag_and_output(self):
        import copy
        rows=[{'kind':'service_static_template','start':5,'end':12,'value':'body'}]
        fixture={'source':{'path':'scripts/check-extension-update-editor.cjs'},'literal':{'kind':'service_static_template','start_utf16':5,'end_utf16':12},'tag':'updated'}
        body=b'const TAG="updated";body';source={'sha256':hashlib.sha256(body).hexdigest(),'bytes':len(body)}
        verify=lambda f=fixture,r=rows,s=source:self.d.verify_javascript_fixture_literal(r,f,s)
        self.assertTrue(verify()['exact_output_reproduced']);self.assertFalse(verify()['generator_executed'])
        for field,value in [('start_utf16',True),('end_utf16',13),('kind','arbitrary')]:
            bad=copy.deepcopy(fixture);bad['literal'][field]=value
            with self.assertRaises(ValueError):verify(f=bad)
        for tag in ['arbitrary',None]:
            with self.assertRaises(ValueError):verify(f={**fixture,'tag':tag})
        with self.assertRaises(ValueError):verify(r=rows*2)
        with self.assertRaises(ValueError):verify(f={**fixture,'source':{'path':'artifacts/untrusted.js'}})
        with self.assertRaises(ValueError):verify(s={'sha256':'f'*64,'bytes':len(body)})
        direct={'source':{'path':'packages/vscode/src/test/extensionUpdateLoader.test.ts'},'literal':{'kind':'literal_output_property','start_utf16':2,'end_utf16':8,'output':'out/entry.js'},'tag':None}
        literal=[{'kind':'literal_output_property','start':2,'end':8,'output':'out/entry.js','value':'body'}]
        self.assertTrue(verify(f=direct,r=literal,s={'sha256':hashlib.sha256(b'body').hexdigest(),'bytes':4})['exact_output_reproduced'])
        with self.assertRaises(ValueError):verify(f={**direct,'tag':'updated'},r=literal)

    def test_recovery_exact_base_edits_target_and_archive_identity(self):
        import copy
        spec=importlib.util.spec_from_file_location('recovery_test_owner',Path(__file__).parents[1]/'upstream_ort/recovery.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
        base={'archive':{'path':'artifacts/base.zip','sha256':hashlib.sha256(b'PK\x03\x04raw').hexdigest(),'bytes':7},'member':'x.txt','sha256':hashlib.sha256(b'abc').hexdigest(),'bytes':3}
        target={'sha256':hashlib.sha256(b'aZc').hexdigest(),'bytes':3}
        record={'schema':'airk.exact-byte-recovery/1','base_sha256':base['sha256'],'base_bytes':3,'target_sha256':target['sha256'],'target_bytes':3,'edits':[{'offset':1,'delete_bytes':1,'deleted_sha256':hashlib.sha256(b'b').hexdigest(),'insert_base64':'Wg=='}]}
        select=lambda raw,names,tar:{'x.txt':b'abc'}
        def verify(b=base,t=target,r=record):return self.d.verify_recovery_payload(b'PK\x03\x04raw',json.dumps(r).encode(),b,t,select,owner.recover)
        self.assertTrue(verify()['exact_output_reproduced']);self.assertFalse(verify()['historical_application_verified']);self.assertFalse(verify()['airk_authorship'])
        for key,value in [('sha256','f'*64),('bytes',4),('member','../x.txt')]:
            bad=copy.deepcopy(base);bad[key]=value
            with self.assertRaises(ValueError):verify(b=bad)
        bad=copy.deepcopy(base);bad['archive']['sha256']='f'*64
        with self.assertRaises(ValueError):verify(b=bad)
        with self.assertRaises(ValueError):verify(t={'sha256':'f'*64,'bytes':3})
        bad=copy.deepcopy(record);bad['edits'][0]['deleted_sha256']='f'*64
        with self.assertRaises(ValueError):verify(r=bad)

    def test_upstream_build_raw_and_normalized_identity_distinct(self):
        self.row['disposition']='upstream_build_input';self.row['representation']='raw'
        self.row['upstream']['source_sha256']='a'*64
        self.assertFalse(self.verify()['qualification'])
        self.row['upstream']['source_sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()
        raw=b'hello\r\n';self.row['source_sha256']=hashlib.sha256(raw).hexdigest();self.row['source_bytes']=len(raw)
        self.row['representation']='CRLF-to-LF';self.row['upstream']['git_blob_oid']='ce013625030ba8dba906f756967f9e9ca394464a'
        def observe(path,fmt):
            return {'source_sha256':self.row['source_sha256'],'bytes':len(raw)} if path.name=='code.py' else {'source_sha256':'b'*64,'bytes':10}
        with patch.object(self,'observe',observe),patch.object(Path,'read_bytes',return_value=raw):
            self.assertFalse(self.verify()['qualification'])
            self.row['upstream']['git_blob_oid']='f'*40
            with self.assertRaises(ValueError):self.verify()

    def test_reference_document_requires_content_pin_and_explicit_gap(self):
        self.row['disposition']='upstream_reference_document'
        self.row['reference']={'url':'https://example.org/article','content_sha256':'a'*64,'provenance_gap':'Acquisition receipt unavailable; companion metadata retained.'}
        self.assertFalse(self.verify()['qualification'])
        del self.row['reference']['provenance_gap']
        with self.assertRaises(ValueError):self.verify()
        self.row['reference']['provenance_gap']=None;self.row['reference']['content_sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()

    def test_generated_literal_exact_argument_and_js_escape_subset(self):
        import subprocess
        def run(literal,body,anchor=None,actual_head='d'*40):
            anchor=anchor or "await fs.writeFile(path.join(a.root,'a.py'),"+literal+");"
            raw=(anchor+'\n').encode();oid='e'*40;head='d'*40
            self.row.update(source='artifacts/project-a/a.py',source_sha256=hashlib.sha256(body).hexdigest(),source_bytes=len(body),disposition='generated_literal')
            self.row['generated']={'git_generator':{'commit':'c'*40,'path':'packages/vscode/src/test/native.test.ts','blob_oid':oid,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'reachable_ref':'refs/heads/retained','reachable_head':head},
                'literal_lexeme':literal,'source_anchor':anchor,'reproduced_sha256':self.row['source_sha256']}
            def output(args):
                if '--verify' in args:return (actual_head+'\n').encode()
                if 'show' in args:return raw
                return (oid+'\n').encode()
            def observe(path,fmt):return {'source_sha256':self.row['source_sha256'],'bytes':len(body)} if path.name=='a.py' else {'source_sha256':'b'*64,'bytes':10}
            with patch.object(self,'observe',observe),patch.object(Path,'read_bytes',return_value=body),patch.object(subprocess,'run'),patch.object(subprocess,'check_output',side_effect=output):return self.verify()
        literal="'ok\\n'"
        self.assertFalse(run(literal,b'ok\n')['qualification'])
        unrelated="const nearby="+literal+"; await fs.writeFile(path.join(a.root,'a.py'),'different');"
        with self.assertRaises(ValueError):run(literal,b'ok\n',unrelated)
        with self.assertRaises(ValueError):run("'\\U00000041'",b'A')
        with self.assertRaises(ValueError):run(literal,b'ok\n',actual_head='f'*40)

    def test_observed_tool_input_requires_installed_identity_and_bound_private_home(self):
        repo=Path.cwd();profile=repo.parent/'human';relative='skills/.system/tool/a.py';private=repo/'artifacts/private'
        log=(str(private)+'\n').encode()
        self.row.update(source='artifacts/private/'+relative,disposition='observed_tool_input')
        self.row['tool_input']={'installed':{'absolute_path':str(profile/'.codex'/relative),'sha256':'a'*64,'bytes':10},
            'producer':{'path':'scripts/producer.py','sha256':'a'*64,'bytes':10},'private_home':'artifacts/private','relative_path':relative,
            'bindings':[{'path':'artifacts/log.txt','sha256':hashlib.sha256(log).hexdigest(),'bytes':len(log)}]}
        def observe(path,fmt):
            if path.name=='log.txt':return {'source_sha256':hashlib.sha256(log).hexdigest(),'bytes':len(log)}
            return {'source_sha256':('b' if path.name=='inventory.json' else 'a')*64,'bytes':10}
        with patch.object(self,'observe',observe),patch.object(Path,'home',return_value=profile),patch.object(Path,'read_text',return_value='CODEX_HOME'),patch.object(Path,'read_bytes',new=lambda path:log):
            self.assertFalse(self.verify()['upstream_release_verified'])
            self.row['tool_input']['installed']['absolute_path']=str(repo.parent/'another-user'/'.codex'/relative)
            with self.assertRaises(ValueError):self.verify()
            self.row['tool_input']['installed']['absolute_path']=str(profile/'.codex'/relative)
            log=b'no private home binding'
            self.row['tool_input']['bindings'][0].update(sha256=hashlib.sha256(log).hexdigest(),bytes=len(log))
            with self.assertRaises(ValueError):self.verify()

    def test_private_fixture_requires_concrete_origin_or_exact_historical_index(self):
        repo=Path.cwd();origin=repo.parent/'external-bookbuilder';documents={}
        def doc(name,value):
            raw=json.dumps(value).encode();documents[name]=raw
            return {'path':'artifacts/'+name,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}
        inputs=doc('input.json',{'workspaces':[{'label':'bookbuilder','root':str(origin)}]})
        summary=doc('summary.json',{'original_workspaces_unchanged':True,'runs':[{'workspace':'bookbuilder','isolation_and_citations_passed':True}]})
        self.row.update(source='artifacts/copies/bookbuilder/code.py',disposition='private_fixture_source',fixture={
            'producer':{'path':'scripts/benchmark-detail.py','sha256':'a'*64,'bytes':10},'input':inputs,'summary':summary,
            'copy_root':'artifacts/copies/bookbuilder','copy_relative_path':'code.py','label':'bookbuilder',
            'original':{'path':str(origin/'code.py'),'status':'matches','sha256':'a'*64,'bytes':10}})
        drift=False
        def observe(path,fmt):
            if path.name in documents:return {'source_sha256':hashlib.sha256(documents[path.name]).hexdigest(),'bytes':len(documents[path.name])}
            if path==origin/'code.py' and drift:return {'source_sha256':'c'*64,'bytes':10}
            return {'source_sha256':('b' if path.name=='inventory.json' else 'a')*64,'bytes':10}
        def read(path):return documents[path.name]
        with patch.object(self,'observe',observe),patch.object(Path,'read_bytes',read):
            self.assertEqual(self.verify()['ultimate_original']['status'],'matches')
            drift=True
            self.row['fixture']['original'].update(status='differs',sha256='c'*64)
            with self.assertRaises(ValueError):self.verify()
            self.row['fixture']['identity_gap']='No exact historical hash witness; source role established by pinned copy chain'
            self.assertFalse(self.verify()['historical_identity_complete'])
            del self.row['fixture']['identity_gap']
            inv=doc('native-inventory.json',{'root':str(origin),'files':{'code.py':{'state':{'source_hash':'a'*64},'source_bytes':10}}})
            manifest=doc('manifest.json',{'inventory_digest':inv['sha256']})
            self.row['fixture']['historical_index']={'inventory':inv,'manifest':manifest,'relative_path':'code.py','source_sha256':'a'*64,'source_bytes':10}
            self.assertTrue(self.verify()['historical_input_verified'])
            self.row['fixture']['historical_index']['source_sha256']='d'*64
            with self.assertRaises(ValueError):self.verify()

    def test_reachable_git_typescript_source_requires_actual_blob_content(self):
        raw=b'hello\n';sha=hashlib.sha256(raw).hexdigest();oid='ce013625030ba8dba906f756967f9e9ca394464a';head='c'*40
        self.row['disposition']='generated_from_maintained_typescript'
        self.row['generated']={'compiler':{'path':'packages/admin/typescript.js','sha256':'a'*64,'bytes':10},
            'source_version':{'git_blob_oid':oid,'reachable_head':head,'sha256':sha,'bytes':6},
            'options':{},'method':'exact observed emission','reproduced_js_sha256':'a'*64}
        import subprocess
        with patch.object(subprocess,'check_output',side_effect=[(oid+' label.ts\n').encode(),raw]):
            self.assertFalse(self.verify()['qualification'])
        self.row['generated']['source_version']['sha256']='f'*64
        with patch.object(subprocess,'check_output',side_effect=[(oid+' label.ts\n').encode(),raw]),self.assertRaises(ValueError):self.verify()

    def test_generated_fixture_requires_exact_generator_and_reproduced_hash(self):
        self.row['disposition']='generated_fixture'
        self.row['generated']={'generator':{'path':'tools/tests/generator.py','sha256':'a'*64,'bytes':10},
                               'method':'observed exact literal reproduction','reproduced_sha256':'a'*64}
        self.assertFalse(self.verify()['qualification'])
        self.row['generated']['generator']['sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()
        self.row['generated']['generator']['sha256']='a'*64
        self.row['generated']['reproduced_sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()

    def test_generated_snapshot_requires_exact_maintained_owner(self):
        self.row['disposition']='generated_source_snapshot'
        self.row['maintained']={'path':'scripts/history/code.py','sha256':'a'*64,'bytes':10}
        self.assertFalse(self.verify()['qualification'])
        self.row['maintained']['sha256']='c'*64
        with self.assertRaises(ValueError):self.verify()


class ParallelMemo(unittest.TestCase):
    def test_same_key_single_load_under_concurrency(self):
        from audit_memo import VerifiedMemo
        from concurrent.futures import ThreadPoolExecutor
        from threading import Barrier,Event,Lock
        memo=VerifiedMemo();barrier=Barrier(3);entered=Event();release=Event();calls=[];guard=Lock();value=object()
        def load():
            with guard:calls.append(1)
            entered.set()
            if not release.wait(2):raise RuntimeError('test release deadline')
            return value
        def work():barrier.wait(timeout=2);return memo.get_or_compute('same',load)
        with ThreadPoolExecutor(max_workers=2) as pool:
            a=pool.submit(work);b=pool.submit(work)
            try:barrier.wait(timeout=2);self.assertTrue(entered.wait(2))
            finally:release.set()
            self.assertIs(a.result(timeout=2),value);self.assertIs(b.result(timeout=2),value)
        self.assertEqual(calls,[1]);self.assertEqual(len(memo),1)

    def test_failure_unlocks_without_caching_failure(self):
        from audit_memo import VerifiedMemo
        from threading import Event,Thread
        memo=VerifiedMemo()
        def fail():raise ValueError('expected failure')
        with self.assertRaises(ValueError):memo.get_or_compute('key',fail)
        self.assertEqual(len(memo),0);done=Event();results=[]
        def next_load():results.append(memo.get_or_compute('key',lambda:'valid'));done.set()
        thread=Thread(target=next_load,daemon=True);thread.start()
        self.assertTrue(done.wait(2));self.assertEqual(results,['valid'])

    def test_unrelated_keys_do_not_share_compute_lock(self):
        from audit_memo import VerifiedMemo
        from concurrent.futures import ThreadPoolExecutor
        from threading import Event
        memo=VerifiedMemo();entered=Event();release=Event()
        def slow():
            entered.set()
            if not release.wait(2):raise RuntimeError('test release deadline')
            return 'a'
        with ThreadPoolExecutor(max_workers=2) as pool:
            a=pool.submit(memo.get_or_compute,'a',slow)
            try:
                self.assertTrue(entered.wait(2));b=pool.submit(memo.get_or_compute,'b',lambda:'b');self.assertEqual(b.result(timeout=1),'b')
            finally:release.set()
            self.assertEqual(a.result(timeout=2),'a')


class Reconciliation(unittest.TestCase):
    def test_map_registration_preserves_first_evidence_observation(self):
        import sys
        from audit_memo import VerifiedMemo
        rs=importlib.util.spec_from_file_location('reconcile',Path(__file__).with_name('reconcile.py'));r=importlib.util.module_from_spec(rs)
        with patch.dict(sys.modules,{'inventory':m}):rs.loader.exec_module(r)
        memo=VerifiedMemo();first={'source_sha256':'a'*64,'bytes':10}
        r.register_evidence(memo,'proof.json',first)
        self.assertIs(r.register_evidence(memo,'proof.json',{**first,'git_blob_oid':'b'*40}),first)
        for changed in [{**first,'source_sha256':'c'*64},{**first,'bytes':11}]:
            with self.assertRaises(ValueError):r.register_evidence(memo,'proof.json',changed)
        self.assertIs(memo['proof.json'],first)

    def test_parallel_proofs_merge_in_order_and_dynamic_modes_stay_serial(self):
        import sys
        from concurrent.futures import ThreadPoolExecutor
        from threading import Event,get_ident
        rs=importlib.util.spec_from_file_location('reconcile',Path(__file__).with_name('reconcile.py'));r=importlib.util.module_from_spec(rs)
        with patch.dict(sys.modules,{'inventory':m}):rs.loader.exec_module(r)
        signal=Event();finished=[];main=get_ident();worker_ids=[]
        def verify(row):
            worker_ids.append(get_ident())
            if row['i']==0:
                if not signal.wait(2):raise RuntimeError('test ordering deadline')
            else:signal.set()
            finished.append(row['i']);return row['i']
        with ThreadPoolExecutor(max_workers=2) as pool:
            result=list(r.ordered_proof_results(pool,[{'i':0,'disposition':'upstream_build_input'},{'i':1,'disposition':'upstream_build_input'}],verify))
        self.assertEqual(result,[0,1]);self.assertTrue(all(i!=main for i in worker_ids))
        class NoParallel:
            def map(self,*args):raise AssertionError('dynamic mode dispatched to worker')
        for kind in ['observed_source_recovery','generated_javascript_fixture','generated_literal','generated_template']:
            ids=list(r.ordered_proof_results(NoParallel(),[{'disposition':kind}],lambda row:get_ident()));self.assertEqual(ids,[main])
        with self.assertRaises(ValueError):list(r.ordered_proof_results(NoParallel(),[None],lambda row:row))

    def test_final_recheck_rejects_changed_shared_memo_input(self):
        import sys
        from audit_memo import VerifiedMemo
        rs=importlib.util.spec_from_file_location('reconcile',Path(__file__).with_name('reconcile.py'));r=importlib.util.module_from_spec(rs)
        with patch.dict(sys.modules,{'inventory':m}):rs.loader.exec_module(r)
        memo=VerifiedMemo();memo.get_or_compute('proof.json',lambda:{'source_sha256':'a'*64,'bytes':10});snapshot=memo.snapshot()
        errors=r.recheck_evidence(snapshot,'sha1',lambda path,fmt:{'source_sha256':'b'*64,'bytes':10})
        self.assertEqual(len(errors),1);self.assertEqual(errors[0]['stage'],'evidence_final_recheck')

    def test_frozen_inventory_union_preserves_evidence_and_refuses_conflicts(self):
        import sys,copy
        ds=importlib.util.spec_from_file_location('disposition',Path(__file__).with_name('disposition.py'));d=importlib.util.module_from_spec(ds);ds.loader.exec_module(d)
        rs=importlib.util.spec_from_file_location('reconcile',Path(__file__).with_name('reconcile.py'));r=importlib.util.module_from_spec(rs)
        with patch.dict(sys.modules,{'inventory':m,'disposition':d}):rs.loader.exec_module(r)
        row={'path':'artifacts/a.py','source_sha256':'a'*64,'bytes':1,'git_blob_oid':'b'*40,'tracked_worktree_matches':['scripts/a.py']}
        first={'schema':'airk.private.source-inventory/1','repository':'repo','head':'c'*40,'object_format':'sha1','candidates':[row],'errors':[{'path':'denied'}],'excluded':[{'path':'vendor'}],'reported_prior_scan_notes':['prior'],'roots':['artifacts/a']}
        second=copy.deepcopy(first);second['candidates'][0]['tracked_worktree_matches']=['scripts/b.py'];second['roots']=['artifacts/b']
        merged=r.merge_inventories([first,second])
        self.assertEqual(len(merged['candidates']),1);self.assertEqual(merged['candidates'][0]['tracked_worktree_matches'],['scripts/a.py','scripts/b.py'])
        self.assertEqual(merged['errors'],first['errors']*2);self.assertEqual(merged['excluded'],first['excluded']*2);self.assertEqual(merged['reported_prior_scan_notes'],['prior','prior']);self.assertEqual(merged['roots'],['artifacts/a','artifacts/b'])
        self.assertEqual(first['candidates'][0]['tracked_worktree_matches'],['scripts/a.py'])
        for key in ['repository','head','object_format','schema']:
            bad=copy.deepcopy(second);bad[key]='different'
            with self.assertRaises(ValueError):r.merge_inventories([first,bad])
        for key,value in [('source_sha256','f'*64),('bytes',2),('git_blob_oid','d'*40),('normalized_text_git_blob_oid','e'*40)]:
            bad=copy.deepcopy(second);bad['candidates'][0][key]=value
            with self.assertRaises(ValueError):r.merge_inventories([first,bad])

    def test_missing_evidence_stays_unresolved_and_map_pins_preserved(self):
        import sys
        ds=importlib.util.spec_from_file_location('disposition',Path(__file__).with_name('disposition.py'))
        d=importlib.util.module_from_spec(ds);ds.loader.exec_module(d)
        rs=importlib.util.spec_from_file_location('reconcile',Path(__file__).with_name('reconcile.py'))
        r=importlib.util.module_from_spec(rs)
        with patch.dict(sys.modules,{'inventory':m,'disposition':d}):rs.loader.exec_module(r)
        row={'path':'artifacts/owner.py','source_sha256':'a'*64,'bytes':1,'classification':'remaining_implementation_candidate','disposition':'authored_operational_or_research_candidate'}
        self.assertEqual(r.resolve(row,{}, {})['status'],'unresolved')
        result=r.resolve(row,{'a'*64:[{'maintained':{'path':'scripts/owner.py','sha256':'b'*64}}]}, {})
        self.assertEqual(result['status'],'maintained_source_pin_verified')
        self.assertEqual(result['maintained_owners'][0]['maintained']['sha256'],'b'*64)
        row['classification']='normalized_text_committed_blob_match'
        row['normalized_text_git_blob_oid']='f'*40
        self.assertEqual(r.resolve(row,{}, {}, committed={'f'*40})['status'],'historical_normalized_text_identity')
        self.assertEqual(r.resolve(row,{}, {})['status'],'unresolved')
        row['classification']='tracked_worktree_match';row['tracked_worktree_matches']=['scripts/old.py']
        self.assertEqual(r.resolve(row,{}, {}, tracked_current={'b'*64:[{'path':'scripts/old.py'}]})['status'],'unresolved')
        self.assertEqual(r.resolve(row,{}, {}, tracked_current={})['status'],'unresolved')
        self.assertEqual(r.resolve(row,{}, {}, tracked_current={'a'*64:[{'path':'scripts/old.py','sha256':'a'*64,'bytes':1}]})['status'],'current_tracked_worktree_identity')
        cache={'proof.json':{'source_sha256':'a'*64,'bytes':10}}
        self.assertEqual(r.recheck_evidence(cache,'sha1',lambda path,fmt:{'source_sha256':'a'*64,'bytes':10}),[])
        errors=r.recheck_evidence(cache,'sha1',lambda path,fmt:{'source_sha256':'b'*64,'bytes':10})
        self.assertEqual(errors[0]['stage'],'evidence_final_recheck')
        def missing(path,fmt):raise FileNotFoundError('proof removed')
        self.assertEqual(r.recheck_evidence(cache,'sha1',missing)[0]['type'],'FileNotFoundError')



if __name__ == '__main__':
    unittest.main()
