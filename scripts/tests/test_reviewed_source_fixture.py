# SPDX-License-Identifier: Apache-2.0
"""Wire binding and replay guards; these fixtures do not supply native authority."""
import copy
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import tempfile
import time
import unittest
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location('reviewed_source', Path(__file__).parents[1]/'reviewed-source-fixture.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


def wire(retire=False):
    registration = {'binding':'b'*64,'root':'owned-source','data_root':'owned-data','collection':'fixture'}
    request = {'caller':'1'*32,'key':{'namespace':'2'*32,'epoch':0,'nonce':'3'*32},'expected_catalog_revision':4,
               'operation':{'operation':'retire_index' if retire else 'begin_index_source_change','id':'idx_'+'4'*32}}
    draft = {'request':request,'preview':{'request_digest':'5'*64,'authority_fence_required':True},'preview_digest':'6'*64}
    metadata = {'schema':1,'operation_id':'7'*32,'caller':request['caller'],'key':request['key'],
                'request_digest':'5'*64,'catalog_revision':5,'authority_fence_required':True}
    proof = {'plan_digest':'8'*64,'service':'9'*32,'gate_generation':1,'confirmations':[]}
    if retire:
        worker = {'project':registration['binding'],'service':proof['service'],'worker':'a'*32,'fencing_generation':1}
        proof.update(worker_obligation={'index_id':request['operation']['id'],'backing':registration['binding'],'workers':[worker]},
                     worker_confirmations=[{'worker':worker,'method':'native_worker_reap'}])
    full = {'request':request,'metadata':metadata,'preview_digest':'6'*64,'prepared_digest':'a'*64,'proof':proof}
    return registration,draft,full


class ReviewedSourceFixtureTests(unittest.TestCase):
    def test_source_draft_waits_only_for_exact_typed_work_guard_without_mutation(self):
        selection = {'index_id':'idx_'+'4'*32,'key':{'nonce':'3'*32},
                     'expected_catalog_revision':4,'change':{'operation':'add_root','alias':'specs','path':'owned'}}
        busy = {'status':'source_change_busy','code':'source_change_jobs_pending',
                'busy':{'index_id':selection['index_id'],'pending_jobs':1,'running_jobs':0}}
        draft = {'request':'complete-native-request','preview':'complete-native-preview'}
        client,_,_ = self.client([busy,{'status':'catalog_source_mutation_draft','draft':draft}])
        client.idle = Mock()
        self.assertEqual(client.source_draft(selection),draft)
        self.assertEqual(client.rpc.call_count,2)
        self.assertEqual(client.rpc.call_args_list[0],client.rpc.call_args_list[1])
        self.assertEqual(client.rpc.call_args.args,({'catalog_source_mutation_draft':{'selection':selection}},))
        client.idle.assert_called_once(); client.pause.assert_called_once()
        for change in [lambda v:v.update(code='unknown'),lambda v:v['busy'].update(index_id='foreign'),
                       lambda v:v['busy'].update(pending_jobs=0),lambda v:v['busy'].update(running_jobs=True),
                       lambda v:v['busy'].update(running_jobs=-1),lambda v:v['busy'].update(pending_jobs=4294967296),
                       lambda v:v.update(status='catalog_outcome_unknown')]:
            refused = copy.deepcopy(busy); change(refused)
            client,_,_ = self.client([refused]); client.idle = Mock()
            with self.assertRaises(AssertionError): client.source_draft(selection)
            client.rpc.assert_called_once(); client.idle.assert_not_called(); client.pause.assert_not_called()

    def test_source_draft_busy_does_not_extend_original_deadline(self):
        with tempfile.TemporaryDirectory() as temporary:
            selection = {'index_id':'idx_'+'4'*32,'key':{'nonce':'3'*32}}
            fixture = SimpleNamespace(single_image=True,owner=Path(temporary),job_timeout=180,
                projects={'p':{'binding':'b'*64}},scopes={'p':{'namespace':'2'*32}},rpc=Mock(return_value={
                    'status':'source_change_busy','code':'source_change_jobs_pending',
                    'busy':{'index_id':selection['index_id'],'pending_jobs':1,'running_jobs':0}}))
            client = driver.SourceReview(fixture,'p'); client.deadline = time.monotonic()+0.05
            original = client.deadline
            with self.assertRaisesRegex(AssertionError,'deadline exhausted'): client.source_draft(selection)
            fixture.rpc.assert_called_once_with({'catalog_source_mutation_draft':{'selection':selection}},deadline=original)
            self.assertEqual(client.deadline,original)
            self.assertEqual(client.records,1)

    def client(self, replies):
        registration,draft,full = wire()
        client = driver.SourceReview.__new__(driver.SourceReview)
        client.registration = registration
        client.rpc = Mock(side_effect=replies)
        client.pause = Mock()
        client.retain = Mock()
        return client,draft,full

    def test_unknown_apply_observes_same_request_without_mutation_replay(self):
        registration,draft,full = wire()
        unknown = {'status':'catalog_outcome_unknown','caller':draft['request']['caller'],
                   'key':draft['request']['key'],'request_digest':draft['preview']['request_digest']}
        pending = {'status':'catalog_receipt','receipt':None,'fence':{'state':'pending',
                   'value':{'request':draft['request'],'preview_digest':draft['preview_digest']}}}
        complete = {'status':'catalog_receipt','receipt':full['metadata'],'fence':{'state':'applied','value':full}}
        client,_,_ = self.client([unknown,pending,complete])
        self.assertEqual(client.apply(draft),full)
        methods = [next(iter(call.args[0])) for call in client.rpc.call_args_list]
        self.assertEqual(methods,['catalog_apply','catalog_receipt','catalog_receipt'])
        for call in client.rpc.call_args_list[1:]:
            self.assertEqual(call.args[0]['catalog_receipt'],{'caller':draft['request']['caller'],'key':draft['request']['key']})
        self.assertEqual(full['request'],draft['request'])

    def test_crossed_unknown_or_terminal_proof_never_becomes_success(self):
        _,draft,full = wire()
        unknown = {'status':'catalog_outcome_unknown','caller':'f'*32,'key':draft['request']['key'],'request_digest':'5'*64}
        client,_,_ = self.client([unknown])
        with self.assertRaises(AssertionError): client.apply(draft)
        self.assertEqual(client.rpc.call_count,1)
        full['request'] = {**full['request'],'caller':'f'*32}
        client,_,_ = self.client([{'status':'catalog_applied','fenced_receipt':full}])
        with self.assertRaises(AssertionError): client.apply(draft)
        self.assertEqual(client.rpc.call_count,1)

    def test_transport_unknown_retains_error_then_only_reads_original_receipt(self):
        _,draft,full = wire()
        client,_,_ = self.client([TimeoutError('actual transport outcome unknown'),
            {'status':'catalog_receipt','receipt':full['metadata'],'fence':{'state':'applied','value':full}}])
        self.assertEqual(client.apply(draft),full)
        client.retain.assert_called_once_with({'unknown_apply':'actual transport outcome unknown'})
        self.assertEqual(sum('catalog_apply' in call.args[0] for call in client.rpc.call_args_list),1)

    def test_retirement_requires_complete_exact_native_worker_reap_census(self):
        registration,draft,full = wire(retire=True)
        driver.validate_fenced(full,draft,registration)
        for mutate in [lambda v:v['proof'].update(worker_confirmations=[]),
                       lambda v:v['proof']['worker_obligation'].update(backing='foreign'),
                       lambda v:v['proof']['worker_confirmations'][0].update(method='invented_thread_exit'),
                       lambda v:v['metadata'].update(catalog_revision=6)]:
            crossed = copy.deepcopy(full); mutate(crossed)
            with self.assertRaises(AssertionError): driver.validate_fenced(crossed,draft,registration)

    def test_expired_shared_deadline_refuses_before_any_native_rpc(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = SimpleNamespace(single_image=True,owner=Path(temporary),job_timeout=180,
                projects={'p':{'binding':'b'*64}},scopes={'p':{'namespace':'2'*32}},rpc=Mock())
            client = driver.SourceReview(fixture,'p'); client.deadline = time.monotonic()-1
            with self.assertRaisesRegex(AssertionError,'budget exhausted'): client.rpc('health')
            fixture.rpc.assert_not_called()

    def test_new_grant_uses_only_complete_current_native_source_roster(self):
        selected = {'schema':1,'context_key':'1'*64,'namespace':'2'*32,'expected_catalog_revision':4,
                    'grant_id':'grt_'+'3'*32,'expected_grant_revision':1,'index_id':'idx_'+'4'*32,'project_ids':['prj_'+'5'*32]}
        original = copy.deepcopy(selected)
        client = driver.SourceReview.__new__(driver.SourceReview)
        client.selected = selected; client.name = 'p'; client.fixture = SimpleNamespace(scopes={'p':selected})
        operations = []; revision = 4
        def rpc(method):
            nonlocal revision
            if method == 'request_epoch': return {'namespace':selected['namespace'],'epoch':0}
            if 'catalog_host_context' in method: return {'descriptor':{'namespace':selected['namespace'],'catalog_revision':revision}}
            if 'catalog_page' in method:
                return {'page':{'namespace':selected['namespace'],'revision':revision,'continuation':None,
                    'items':[{'kind':'source','value':{'index_id':selected['index_id'],'id':'src_primary','enabled':True}},
                             {'kind':'source','value':{'index_id':selected['index_id'],'id':'src_alias','enabled':True}},
                             {'kind':'source','value':{'index_id':selected['index_id'],'id':'src_removed','enabled':False}}]}}
            if 'catalog_preview' in method:
                request = method['catalog_preview']['request']; operations.append(request['operation'])
                return {'status':'catalog_preview','preview':{'operation':request['operation'],'catalog_revision':revision,'next_revision':revision+1},'preview_digest':'6'*64}
            selection = method['catalog_grant_draft']['selection']
            self.assertEqual(selection['project_ids'],[operations[-1]['membership']['project_id']])
            return {'status':'catalog_grant_draft','draft':{'request':{'operation':{'operation':'create_grant',
                'grant':{'id':'grt_'+'a'*32,'revision':1,'revoked':False,'memberships':[{'membership':operations[-1]['membership']}]}}}}}
        def apply(draft):
            nonlocal revision
            revision += 1
            return {'metadata':{'catalog_revision':revision}}
        client.rpc=Mock(side_effect=rpc); client.apply=Mock(side_effect=apply); client.retain=Mock()
        client.grant_sources()
        self.assertEqual(selected,original,'The original grant is immutable')
        self.assertNotEqual(client.fixture.scopes['p']['grant_id'],selected['grant_id'])
        scope = operations[-1]['membership']['scope']
        self.assertEqual([atom['source_root_id'] for atom in scope],['src_alias','src_primary'])
        self.assertTrue(all(atom['kind']=='subtree' and atom['path']=='' for atom in scope))


if __name__ == '__main__': unittest.main()
