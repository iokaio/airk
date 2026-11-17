# SPDX-License-Identifier: Apache-2.0
"""Pure inventory controls; no fixture files, Git mutations, or candidate execution."""
from collections import Counter
from contextlib import nullcontext
import hashlib
import importlib.util
import io
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


class Reconciliation(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
