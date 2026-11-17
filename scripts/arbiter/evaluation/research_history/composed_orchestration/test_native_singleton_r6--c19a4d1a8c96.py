# SPDX-License-Identifier: Apache-2.0
"""Saved genuine R4 diagnostics and independent native singleton Search fixtures."""
from copy import deepcopy
import hashlib,json,unittest
from pathlib import Path
from test_public_focus_r6 import A,E,Q,R,envelope,fixture,INDEX,ORACLE
BASE=Path(__file__).resolve().parent
ACTUAL=BASE.parent.parent/'public-focus-native-r4'
RAW=ACTUAL/'008-raw-diagnostics-a.json'
ORACLES=ACTUAL/'007-pre-read-source-oracles.json'
DIAGNOSTIC_REQUEST={'response_mode':'full'}
SEARCH_REQUEST={'full_chunks':True,'include_status':True,'limit':8,'max_bytes':65536,
    'mode':'hybrid','query':'Where is the SyntheticRelay lantern maintenance schedule documented?',
    'response_mode':'full'}


def saved():
    original=json.loads(RAW.read_text(encoding='utf-8'))
    assert original['exit_code']==0
    env=json.loads(original['stdout'])
    rows=json.loads(ORACLES.read_text(encoding='utf-8'))
    oracle=deepcopy(rows['a'])
    oracle['sources']={(row['index_id'],row['source_root_id'],row['source_relative_path']):
        {k:v for k,v in row.items() if k not in ('index_id','source_root_id','source_relative_path')}
        for row in oracle['sources']}
    return env,oracle,rows


def native_search():
    # SearchResponse/Citation/VerificationReceipt independently literal fields
    # from Core lib.rs + verification.rs and Kernel ANN's native partial case.
    # Publication/scope/source identities come from the genuine retained A snapshot.
    env,oracle,_=saved();diag=env['structuredContent'];pub=deepcopy(diag['publication'])
    key,row=next(iter(oracle['sources'].items()));index,root,path=key
    original_root=oracle['native_catalog']['page']['items'][0]['value']['canonical_path']
    source_id='ab'*32
    receipt={'schema':'airk.saved-source-verification/1','method':'sha256-full-saved-source/1','mode':'report',
        'status':'partial','scope_digest':'11'*32,'selection_digest':'22'*32,
        'observed_from_unix_ms':1000,'observed_to_unix_ms':1001,'publication_fence':'current',
        'inventory_complete':True,'all_references_accounted':True,'source_rows_omitted':0,
        'requirements_satisfied':False,
        'counts':{'references':1,'references_lower_bound':1,'requested_sources':1,'requested_sources_lower_bound':1,
            'checked_sources':0,'matched_sources':0,'mismatched_sources':0,'unchecked_sources':1,'bytes_read':0},
        'limits':{'max_sources':8,'max_bytes':16777216,'deadline_ms':2000,'effective_deadline_ms':2000,
            'stream_bytes':65536,'eof_probe_reserve_bytes':1,'concurrency':1,'reference_inventory_max':4096,
            'receipt_bytes_max':32768,'receipt_bytes_budget':32768,'hit':['sources']},
        'origins':[{'id':'origin-0','index_id':index,'legacy_project':'33'*32,'collection':pub['collection'],
            'incarnation':pub['incarnation'],'publication':pub['token'],'project_ids':[]}],
        'sources':[{'id':source_id,'origin_id':'origin-0','source_root_id':root,'original_root':original_root,
            'source_relative_path':path,'path':path,'expected_hash':row['sha256'],'observed_hash':None,
            'profile_digest':'44'*32,'project_ids':[],'reference_ids':['ref-0'],'state':'budget_exhausted',
            'reason':'sources','observed_from_unix_ms':1000,'observed_to_unix_ms':1001,'bytes_read':0}],
        'references':[{'id':'ref-0','source_id':source_id,'result_pointer':'/hits/0/citation','kind':'citation'}],
        'qualifications':[]}
    leaf={'collection':pub['collection'],'generation':pub['generation'],'publication':pub,
        'hits':[{'citation':{'path':path,'content_hash':row['sha256'],'start_line':1,'end_line':1,
            'location':{'Lines':{'start':1,'end':1}}},'text':'literal independent native Search fixture','truncated':False}],
        'truncated':False,'execution':{'version':1,'requested_mode':'hybrid','served_mode':'hybrid','incomplete':False},
        'read_scope':deepcopy(diag['read_scope']),'scope_observation':deepcopy(diag['scope_observation']),
        'semantic_readiness':deepcopy(diag['semantic_readiness']),'source_verification':receipt}
    return envelope(leaf),oracle


class NativeSingletonTests(unittest.TestCase):
    def observe(self,env,oracle,kind='search',**kw):
        return A.observe(env,oracle['selection']['index_id'],oracle,E,Q,kind=kind,
            request_arguments=deepcopy(DIAGNOSTIC_REQUEST if kind=='diagnostics' else SEARCH_REQUEST),**kw)

    def test_genuine_failed_r4_raw008_is_current_positive_without_rewriting(self):
        env,oracle,_=saved();before=deepcopy(env)
        result=self.observe(env,oracle,'diagnostics')
        self.assertEqual(result['envelope'],before)
        self.assertEqual(result['member_body'],before['structuredContent'])
        self.assertIsNone(result['combined']);self.assertIsNone(result['materialized_member_result'])
        self.assertEqual(result['native_shape'],'singleton');self.assertEqual(env,before)

    def test_independent_search_retains_partial_verification_and_all_fields(self):
        env,oracle=native_search();before=deepcopy(env)
        result=self.observe(env,oracle);A.require_hybrid(result)
        self.assertEqual(result['member_body'],before['structuredContent'])
        self.assertFalse(result['member_body']['source_verification']['requirements_satisfied'])
        self.assertEqual(result['member_body']['source_verification']['status'],'partial')
        self.assertEqual(env,before)

    def test_fixed_snapshot_publication_and_scope_do_not_advance(self):
        diagnostic,oracle,_=saved();ready=self.observe(diagnostic,oracle,'diagnostics')
        search,_=native_search()
        result=self.observe(search,oracle,expected_publications=ready['publication_identity'],
            expected_scope=ready['member_body']['scope_observation'])
        self.assertEqual(result['publication_identity'],ready['publication_identity'])
        for target in ('publication','scope'):
            wrong=deepcopy(search['structuredContent'])
            if target=='publication':wrong['publication']['revision']='ff'*32
            else:wrong['scope_observation']['selection']['scope_identity']='ff'*32
            with self.subTest(target=target),self.assertRaises(ValueError):
                self.observe(envelope(wrong),oracle,expected_publications=ready['publication_identity'],
                    expected_scope=ready['member_body']['scope_observation'])

    def test_foreign_index_root_source_and_citation_pointer_refusals(self):
        env,oracle=native_search()
        mutations=(('index',lambda v:v['read_scope'].update(index_ids=['foreign'])),
            ('scope',lambda v:v['scope_observation']['members'][0].update(index_id='foreign')),
            ('root',lambda v:v['scope_observation']['members'][0].update(source_root_ids=['foreign'])),
            ('grant',lambda v:v['scope_observation'].update(grant={'id':'foreign','revision':1})),
            ('origin',lambda v:v['source_verification']['origins'][0].update(index_id='foreign')),
            ('origin_project',lambda v:v['source_verification']['origins'][0].update(project_ids=['foreign'])),
            ('source_hash',lambda v:v['source_verification']['sources'][0].update(expected_hash='ff'*32)),
            ('source_root_path',lambda v:v['source_verification']['sources'][0].update(original_root='D:/foreign')),
            ('source_path',lambda v:v['source_verification']['sources'][0].update(source_relative_path='foreign.txt')),
            ('citation',lambda v:v['hits'][0]['citation'].update(content_hash='ff'*32)),
            ('pointer',lambda v:v['source_verification']['references'][0].update(result_pointer='/hits/9/citation')),
            ('source_ref',lambda v:v['source_verification']['references'][0].update(source_id='ff'*32)),
            ('token',lambda v:v['publication'].update(token='airk-r1:foreign')),
            ('unknown_schema',lambda v:v.update(schema='future-native/9')),
            ('wrong_shape',lambda v:v.update(members=[])))
        for name,mutate in mutations:
            value=deepcopy(env['structuredContent']);mutate(value)
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,TypeError)):
                self.observe(envelope(value),oracle)

    def test_snapshot_a_cannot_use_b_oracle_even_with_same_document_hash(self):
        env,oracle,rows=saved();other=deepcopy(oracle);other['selection']=rows['b']['selection']
        other['sources']={(r['index_id'],r['source_root_id'],r['source_relative_path']):r for r in rows['b']['sources']}
        with self.assertRaises(ValueError):self.observe(env,other,'diagnostics')

    def test_explicit_subset_or_missing_request_cannot_accept_singleton(self):
        env,oracle=native_search();index=oracle['selection']['index_id']
        for control in ('index_ids','project_ids','member_availability'):
            request={**SEARCH_REQUEST,control:[]}
            with self.subTest(control=control),self.assertRaises(ValueError):
                A.observe(env,index,oracle,E,Q,kind='search',request_arguments=request)
        with self.assertRaises(ValueError):A.observe(env,index,oracle,E,Q,kind='search')

    def test_empty_truncated_and_unknown_verification_stay_unqualified(self):
        env,oracle=native_search();value=deepcopy(env['structuredContent']);value['truncated']=True
        value['execution']['incomplete']=True
        observed=self.observe(envelope(value),oracle)
        self.assertEqual(observed['member_body'],value)
        with self.assertRaises(ValueError):A.require_hybrid(observed)
        value['hits']=[];value['source_verification']['sources']=[];value['source_verification']['references']=[]
        observed=self.observe(envelope(value),oracle)
        self.assertEqual(observed['member_body'],value)
        with self.assertRaises(ValueError):A.require_hybrid(observed)

    def test_native_refusal_and_text_mismatch_not_hidden_by_singleton(self):
        env,oracle=native_search()
        failed=deepcopy(env);failed['isError']=True
        with self.assertRaises(ValueError):self.observe(failed,oracle)
        wrong=deepcopy(env);wrong['structuredContent']['truncated']=True
        with self.assertRaises(ValueError):self.observe(wrong,oracle)
        diagnostic,oracle,_=saved();diagnostic['structuredContent']['error']={'code':'not_ready'}
        diagnostic['content'][0]['text']=json.dumps(diagnostic['structuredContent'])
        with self.assertRaises(ValueError):self.observe(diagnostic,oracle,'diagnostics')

    def test_receipt_roster_and_envelope_bounds_refuse(self):
        env,oracle=native_search();v=deepcopy(env['structuredContent'])
        v['source_verification']['references']*=4097
        with self.assertRaises(ValueError):self.observe(envelope(v),oracle)
        env,oracle=native_search();env['oversized']='x'*1048577
        with self.assertRaises(ValueError):self.observe(env,oracle)

    def test_one_index_shape_matches_original_explicit_subset_presence(self):
        combined=envelope(fixture())
        with self.assertRaises(ValueError):
            A.observe(combined,INDEX,ORACLE,E,Q,kind='search',request_arguments=SEARCH_REQUEST)
        explicit={**SEARCH_REQUEST,'index_ids':[INDEX]}
        result=A.observe(combined,INDEX,ORACLE,E,Q,kind='search',request_arguments=explicit)
        self.assertEqual(result['native_shape'],'combined')
        self.assertEqual(result['combined'],combined['structuredContent'])
