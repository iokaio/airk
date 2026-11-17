# SPDX-License-Identifier: Apache-2.0
"""Independently authored native-shape vectors; no service, model or output fixture."""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import unittest

BASE=Path(__file__).resolve().parent
REPO=next(p for p in BASE.parents if (p/'Cargo.toml').is_file())

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

A=load('private_public_focus_r2',BASE/'public_focus_evidence_r3.py')
E=load('public_focus_canonical_evidence',REPO/'scripts/combined_evidence.py')
Q=load('public_focus_canonical_oracle',REPO/'scripts/check-retrieval-quality.py')
R=load('public_focus_r2_runner',BASE/'run_public_focus_r3.py')
INDEX='index-fixture-a'; ROOT='source-fixture-a'; HASH='ab'*32
PUBLICATION={'collection':'fixture-a','incarnation':'01'*16,'generation':7,'revision':'02'*32,'token':'airk-r1:'+('01'*16)+':7:'+('02'*32)}
AUTHORITY={'owner':'agent','scope_identity':'03'*32,'catalog_revision':9}
ORACLE={'sources':{(INDEX,ROOT,'a.txt'):{'sha256':HASH,'project_ids':[],'virtual_path':'a.txt'}}}


def envelope(body):
    return {'content':[{'type':'text','text':json.dumps(body)}], 'structuredContent':deepcopy(body),'isError':False}


def fixture(kind='search',wrapped=False,focus=False):
    # Explicit public Core/Kernal shapes, not emitted from the code under test.
    scope={'schema':1,'qualifier':'selected_subset','fingerprint':'04'*32,'selection':deepcopy(AUTHORITY),
           'members':[{'index_id':INDEX,'memberships':[],'source_root_ids':[ROOT],
                       'source_revision':5,'policy_epoch':0,'policy_digest':None}]}
    leaf={'collection':'fixture-a','generation':7,'publication':deepcopy(PUBLICATION),
          'scope_observation':scope,'truncated':False,
          'semantic_readiness':{'version':1,'state':'ready','default_search_policy':'partial_hybrid',
              'selected_artifacts':8,'completed_artifacts':8,'selected_units':8,'completed_units':8,
              'first_ready_unix_ms':998,'fully_ready_unix_ms':999,'selection_digest':'13'*32,
              'pipeline_digest':'14'*32,'failure':None}}
    if kind=='search':
        leaf.update({'execution':{'version':1,'requested_mode':'hybrid','served_mode':'hybrid','incomplete':False},
                     'hits':[{'citation':{'path':'a.txt','content_hash':HASH,'start_line':1,'end_line':1,
                                         'location':{'Lines':{'start':1,'end':1}}},
                              'text':'eight original fixture documents','truncated':False}]})
        if focus:
            leaf['focus']={'kind':'receipt','receipt':{'schema':'airk.arbiter.focus.ranking/1',
                'scope':{'installation':'05'*32,'catalog':'06'*16,'index_id':INDEX,'authorization':'04'*32,
                         'configuration_revision':2,'publication':deepcopy(PUBLICATION)},
                'identity':{'profile':R.FOCUS,'runtime':'onnxruntime-cpu/1.30.0','compiler':'07'*32,
                            'calibration':'08'*32,'task_policy':'09'*32},
                'availability':'ready','reason':'observed native scoring','applied':True,'input_digest':'10'*32,
                'original_order':['11'*32],'ranked_order':['11'*32],
                'scores':[{'candidate':'11'*32,'no_logit':0.0,'yes_logit':0.0,'relevance':0.5}],
                'presentations':[{'candidate':'11'*32,'utf8_start':0,'utf8_end':8,'source_bytes':32}],
                'presented_count':1,'candidate_count':1,'scored_count':1,'omitted_count':0,
                'reserved_input_tokens':512,'observed_input_tokens':64,
                'work':{'calls':1,'rounds':1,'fanout':1,'input_tokens':64,'candidates':1,'bytes':512,
                        'queue_us':1,'compile_us':2,'inference_us':3,'policy_us':4}}}
    else:
        leaf['publication']={'token':PUBLICATION['token']}
        leaf.update({'files':8,'chunks':8,'errors':[]})
    result=envelope(leaf) if wrapped else leaf
    prefix='/members/0/result'+('/structuredContent' if wrapped else '')
    combined={'schema':'airk.combined-read/1',
        'selection':{'project_ids':[],'index_ids':[INDEX],'selection':deepcopy(AUTHORITY),
                     'member_availability':'report','identity':'12'*32},
        'publication_vector':{'authority':'12'*32,'observed_from_unix_ms':1000,'observed_to_unix_ms':1001,
                              'members':[{'index_id':INDEX,'publication':deepcopy(PUBLICATION)}]},
        'rank_policy':'airk.rank-fusion.rrf-equal/1','members':[{'index_id':INDEX,'is_error':False,
            'allocation':{'index_id':INDEX,'limit':8,'max_bytes':65536,'deadline_ms':120000,
                          'verification_max_sources':8,'verification_max_bytes':65536,'verification_deadline_ms':120000},
            'outcome':'complete','result':result,'elapsed_ms':1}],
        'ranking':([] if kind=='diagnostics' else [{'result_pointer':prefix+'/hits/0','index_id':INDEX,
            'local_rank':0,'rank_contribution':1/61,'origin':{'index_id':INDEX,'source_root_id':ROOT,'project_ids':[],
            'source_relative_path':'a.txt','publication_ref':{'member':0}}}]),'partial':False,'truncated':False}
    return combined


class PublicFocusCombinedTests(unittest.TestCase):
    def check(self,value,kind='search',**kwargs):
        return A.observe(envelope(value),INDEX,ORACLE,E,Q,kind=kind,**kwargs)

    def test_direct_and_enveloped_search_preserve_every_native_view(self):
        for wrapped in (False,True):
            raw=fixture(wrapped=wrapped); before=deepcopy(raw)
            result=self.check(raw)
            self.assertEqual(result['combined'],before)
            self.assertEqual(result['envelope'],envelope(before))
            self.assertEqual(result['member_body']['hits'][0]['citation']['content_hash'],HASH)
            self.assertEqual(result['qualified_origins'][0]['publication'],PUBLICATION)
            self.assertEqual(raw,before)

    def test_diagnostics_token_only_publication_and_shared_header(self):
        raw=fixture('diagnostics')
        identity='3c2b1d950550b1d7514895aa1645c730a1221132ba50b864215167c9d49f3bc9'
        raw['shared_values']={'schema':'airk.shared-native-header-templates/1','values':{identity:{
            'fields':{'freshness':{'observed_unix_ms':None,'state':'unavailable'}},
            'slots':[{'path':['freshness','observed_unix_ms'],'kind':'integer'}]}}}
        raw['members'][0]['result']['$airk_shared_native_header_fields']={'id':identity,'values':[1001]}
        result=self.check(raw,'diagnostics')
        self.assertEqual(result['member_body']['publication'],{'token':PUBLICATION['token']})
        self.assertEqual(result['member_body']['freshness']['observed_unix_ms'],1001)
        self.assertEqual(result['publication_members'],[{'index_id':INDEX,'publication':PUBLICATION}])
        raw['shared_values']['values'][identity]['fields']['freshness']['state']='forged'
        with self.assertRaises(ValueError):self.check(raw,'diagnostics')

    def test_member_publication_reference_is_expanded_by_canonical_owner(self):
        raw=fixture(); raw['members'][0]['result']['publication']={'member':0}
        result=self.check(raw)
        self.assertEqual(result['member_body']['publication'],PUBLICATION)
        self.assertEqual(result['combined']['members'][0]['result']['publication'],{'member':0})

    def test_focus_is_only_selected_search_member_with_bound_publication(self):
        raw=fixture(wrapped=True,focus=True); observation=self.check(raw)
        self.assertTrue(A.applied_focus(observation))
        A.require_applied_focus(observation,INDEX,R.FOCUS)
        self.assertNotIn('focus',observation['combined'])
        for field,value in [('index_id','foreign'),('authorization','forged'),('publication',{'token':'forged'})]:
            altered=deepcopy(observation); altered['member_body']['focus']['receipt']['scope'][field]=value
            with self.assertRaises(ValueError):A.require_applied_focus(altered,INDEX,R.FOCUS)
        altered=deepcopy(observation);altered['member_body']['focus']['receipt']['scores'][0]['yes_logit']=float('nan')
        with self.assertRaises(ValueError):A.require_applied_focus(altered,INDEX,R.FOCUS)

    def test_exact_selection_publication_origins_hash_and_pointer_refusals(self):
        for mutation in ('index','project','grant','authority','vector','partial','member','duplicate',
                         'root','root_duplicate','origin','origin_project','origin_publication','hash','pointer','rank','scope'):
            with self.subTest(mutation=mutation):
                raw=fixture(); leaf=raw['members'][0]['result']; rank=raw['ranking'][0]
                if mutation=='index':raw['selection']['index_ids']=['foreign']
                if mutation=='project':raw['selection']['project_ids']=['management-grant-project']
                if mutation=='grant':raw['selection']['grant_id']='management-grant'
                if mutation=='authority':raw['publication_vector']['authority']='forged'
                if mutation=='vector':raw['publication_vector']['members'][0]['publication']['revision']='forged'
                if mutation=='partial':raw['partial']=True
                if mutation=='member':raw['members'][0]['outcome']='failed'
                if mutation=='duplicate':raw['members'].append(deepcopy(raw['members'][0]))
                if mutation=='root':leaf['scope_observation']['members'][0]['source_root_ids']=['foreign']
                if mutation=='root_duplicate':leaf['scope_observation']['members'][0]['source_root_ids']=[ROOT,ROOT]
                if mutation=='origin':rank['origin']['source_relative_path']='foreign.txt'
                if mutation=='origin_project':rank['origin']['project_ids']=['management-grant-project']
                if mutation=='origin_publication':rank['origin']['publication_ref']={'member':1}
                if mutation=='hash':leaf['hits'][0]['citation']['content_hash']='forged'
                if mutation=='pointer':rank['result_pointer']='/members/0/result/hits/0/citation'
                if mutation=='rank':rank['local_rank']=1
                if mutation=='scope':leaf['scope_observation']['selection']['catalog_revision']=10
                with self.assertRaises((ValueError,KeyError,TypeError)):self.check(raw)

    def test_original_publication_cannot_advance_across_reads(self):
        raw=fixture(); frozen=self.check(raw)['publication_members']; raw['publication_vector']['members'][0]['publication']['generation']=8
        with self.assertRaises(ValueError):self.check(raw,expected_publications=frozen)

    def test_native_outer_member_and_body_refusals_remain_refusals(self):
        raw=fixture(); outer=envelope(raw);outer['isError']=True
        with self.assertRaises(ValueError):A.observe(outer,INDEX,ORACLE,E,Q,kind='search')
        raw=fixture(wrapped=True);raw['members'][0]['result']['isError']=True
        with self.assertRaises(ValueError):self.check(raw)
        raw=fixture();raw['members'][0]['result']['error']={'code':'not_ready','retryable':False}
        with self.assertRaises(ValueError):self.check(raw)

    def test_text_structured_divergence_refused_at_both_envelope_boundaries(self):
        outer=envelope(fixture());outer['structuredContent']['partial']=True
        with self.assertRaises(ValueError):A.observe(outer,INDEX,ORACLE,E,Q,kind='search')
        raw=fixture(wrapped=True);raw['members'][0]['result']['structuredContent']['collection']='foreign'
        with self.assertRaises(ValueError):self.check(raw)

    def test_diagnostics_cannot_borrow_foreign_collection_or_token(self):
        for field in ('collection','token'):
            raw=fixture('diagnostics')
            if field=='collection':raw['members'][0]['result']['collection']='foreign'
            else:raw['members'][0]['result']['publication']['token']='foreign'
            with self.assertRaises(ValueError):self.check(raw,'diagnostics')

    def test_minilm_execution_is_checked_for_every_search_sibling(self):
        observation=self.check(fixture());A.require_hybrid(observation)
        for mode,incomplete in [('lexical',False),('hybrid',True)]:
            altered=deepcopy(observation);altered['member_body']['execution'].update(served_mode=mode,incomplete=incomplete)
            with self.assertRaises(ValueError):A.require_hybrid(altered)

    def test_runner_uses_both_canonical_siblings_and_keeps_original_deadline(self):
        self.assertIn('combined_evidence.py',R.HELPERS)
        self.assertIn('check-retrieval-quality.py',R.HELPERS)
        with self.assertRaises(TimeoutError):R.remaining(-1)
        self.assertFalse(A.applied_focus(self.check(fixture())))

class CurrentRecipeFailureTests(unittest.TestCase):
    def test_null_build_is_explicit_source_refusal_and_health_is_unchanged(self):
        health={'user':R.SID,'build':None,'build_error':'build_receipt_missing_or_unverified'}
        original=deepcopy(health)
        with self.assertRaisesRegex(ValueError,'source unavailable'):
            A.require_health_source(health,R.SID,'f'*64)
        self.assertEqual(health,original)
        health={'user':R.SID,'build':{'source_sha256':'f'*64},'build_error':None}
        A.require_health_source(health,R.SID,'f'*64)
        with self.assertRaises(ValueError):A.require_health_source(health,R.SID,'e'*64)

    def test_one_stop_cutoff_is_shared_by_all_stages_and_never_refreshed(self):
        clock=[100.0];limits=R.StopCutoffs(1000,clock=lambda:clock[0])
        before_stop=limits.capture('first-instance');self.assertEqual(before_stop,220.0)
        clock[0]=215.0;self.assertEqual(limits.capture('first-instance'),before_stop)
        clock[0]=221.0;self.assertEqual(limits.capture('first-instance'),before_stop)
        self.assertLess(limits.capture('first-instance')-clock[0],0)

    def test_stop_cutoff_intersects_whole_phase_and_new_instance_has_no_phase_renewal(self):
        clock=[100.0];limits=R.StopCutoffs(180,clock=lambda:clock[0])
        self.assertEqual(limits.capture('first'),180)
        clock[0]=160.0;self.assertEqual(limits.capture('actual-new-instance'),180)
        self.assertEqual(limits.capture('first'),180)

    def test_original_handle_failure_retains_real_signal_and_never_invents_exit_zero(self):
        class Handle:
            pid=99;exit_observation=None;seen=[]
            def wait(self,deadline):
                self.seen.append(deadline)
                self.exit_observation={'pid':99,'process_handle_signaled':True,'exit_code':77}
                raise RuntimeError('actual process failed')
        h=Handle();row=R.retain_original_exit(h,120)
        self.assertEqual(h.seen,[120]);self.assertEqual(row['observation']['exit_code'],77)
        self.assertEqual(row['failure']['type'],'RuntimeError')
        class Unknown:
            pid=98;exit_observation=None
            def wait(self,deadline):raise TimeoutError('original cutoff expired')
        row=R.retain_original_exit(Unknown(),120)
        self.assertIsNone(row['observation']);self.assertEqual(row['failure']['type'],'TimeoutError')

if __name__=='__main__':unittest.main()