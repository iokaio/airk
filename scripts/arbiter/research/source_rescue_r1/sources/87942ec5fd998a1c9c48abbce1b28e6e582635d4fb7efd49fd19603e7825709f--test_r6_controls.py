# SPDX-License-Identifier: Apache-2.0
"""Pure readiness/ownership/custody controls; no native process or model execution."""
import copy,importlib.util,json,pathlib,tempfile,types,unittest
from unittest.mock import patch
BASE=pathlib.Path(__file__).resolve().parent
ROOT=BASE.parents[4]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
route=load('r5route',BASE/'launch_route.py');observer=load('r5observer',BASE/'launch_owned.py')
clients=load('r5clients',route.BASE/'shared/check-service-clients.py')
P=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2'
ACTUAL=ROOT/'artifacts/v190/final-plan/candidate-worktree-r3/artifacts/r154-fresh-current-section-r1/work-r4/r154-fresh-attribution-r4'

def status():
    identity={'service':'original-instance','project':'native-project','worker':'native-worker','fencing_generation':7}
    return {'index_state':'available','collection':'r154','publication':{'collection':'r154','token':'native-publication'},'files':753,
        'semantic_readiness':{'state':'ready','selected_artifacts':753,'completed_artifacts':753,'failure':None},
        'discovery_readiness':{'state':'ready','failure':None},
        'runtime':{'service':{'instance':'original-instance','responsive':True},'service_error':None},
        'work':{'basis':'instance_bound_service_metadata','failed_observations':[],'serving_worker':identity,
            'progress_current_worker':True,'active_job':None,'pending_job':None,'queued_jobs':[],'queue_truncated':False,
            'worker':{'identity':identity,'matches_serving_worker':True,'restarting':False,'automatic_start_blocked':False}}}

def health():
    return {'instance':'original-instance','process_id':42,'executable_sha256':'original-image','user':route.SID,'version':'1.8.207','protocol':1}

def management():
    identity={'service':'original-instance','project':'native-project','worker':'native-worker','fencing_generation':7}
    return {'health_before':{'status':'health','observation':health()},'health_after':{'status':'health','observation':health()},
        'progress':{'status':'progress','project':'native-project','pending_job':None,'queued_jobs':[],'queue_truncated':False,'project_paused':False,
            'observation':{'worker':identity,'current_fencing_generation':7,'job':{'project':'native-project','id':'native-job','state':'succeeded','fencing_generation':7},'progress':{'job':'native-job'}}},
        'watch':{'status':'watch','project':'native-project','observation':{'enabled':True,'configuration_observed':True,'requested_revision':3,'completed_revision':3,'failure':None,'automatic_retries_exhausted':False,'automatic_retry_canceled':False}},
        'worker':{'status':'worker','project':'native-project','identity':identity,'restarting':False,'automatic_start_blocked':False}}

def wait(read,record=lambda value:None,wake=lambda deadline:None):
    return route.await_semantic_population(read,clients,{'collection':'r154','binding':'native-project'},'original-instance',1800,record,wake,lambda d:management(),health())

class ReadinessControls(unittest.TestCase):
    def test_full_native_population_and_idle_is_required(self):
        value=status();self.assertTrue(route.full_semantic_population(value,clients));self.assertTrue(route.canonical_work_idle(value,'original-instance'))
    def test_actual_r4_160_of753_pending_population_never_admits(self):
        proof=json.loads((P/'r154-publication-diagnosis-r1/writer-proof.json').read_bytes())
        # Independently observed R4 facts, not derived from gate predicates.
        value=status();value['semantic_readiness'].update(state='partial',completed_artifacts=160)
        self.assertFalse(route.full_semantic_population(value,clients))
        self.assertIn('160',json.dumps(proof));self.assertIn('593',json.dumps(proof))
    def test_ready_state_with_incomplete_population_never_admits(self):
        for selected,completed in ((752,752),(753,752),(0,0)):
            value=status();value['semantic_readiness'].update(selected_artifacts=selected,completed_artifacts=completed)
            self.assertFalse(route.full_semantic_population(value,clients))
    def test_unknown_or_invalid_counts_refuse(self):
        for selected,completed in ((True,1),(754,754),(753,754),(753,True),(None,0)):
            value=status();value['semantic_readiness'].update(selected_artifacts=selected,completed_artifacts=completed)
            with self.assertRaises((ValueError,AssertionError)):route.full_semantic_population(value,clients)
    def test_semantic_and_discovery_failure_refuse(self):
        for kind in ('semantic_readiness','discovery_readiness'):
            value=status();value[kind]['state']='failed'
            with self.assertRaises(ValueError):route.full_semantic_population(value,clients)
    def test_discovery_must_be_ready_and_exact_source_population(self):
        value=status();value['discovery_readiness']['state']='partial';self.assertFalse(route.full_semantic_population(value,clients))
        value=status();value['files']=752
        with self.assertRaisesRegex(ValueError,'753 source'):route.full_semantic_population(value,clients)
    def test_each_live_or_unknown_queue_class_blocks(self):
        for key,value in [('active_job',{'state':'running'}),('pending_job',{'state':'pending'}),('queued_jobs',[{'id':'job'}]),('queue_truncated',True)]:
            sample=status();sample['work'][key]=value;self.assertFalse(route.canonical_work_idle(sample,'original-instance'))
    def test_missing_queue_fields_never_infer_empty(self):
        for key in ('active_job','pending_job','queued_jobs','queue_truncated'):
            sample=status();del sample['work'][key]
            with self.assertRaises(ValueError):route.canonical_work_idle(sample,'original-instance')
    def test_stale_wrong_instance_and_failed_observations_refuse(self):
        for change in ('progress','matches','identity','instance','failed'):
            sample=status()
            if change=='progress':sample['work']['progress_current_worker']=False
            elif change=='matches':sample['work']['worker']['matches_serving_worker']=False
            elif change=='identity':sample['work']['worker']['identity']={}
            elif change=='instance':sample['runtime']['service']['instance']='replacement'
            else:sample['work']['failed_observations']=['unknown']
            with self.assertRaises(ValueError):route.canonical_work_idle(sample,'original-instance')
    def test_event_registered_before_reads_and_original_deadline_forwarded(self):
        samples=[];events=[];values=[status(),status()];values[0]['semantic_readiness'].update(state='partial',completed_artifacts=160)
        def read(deadline):events.append(('read',deadline));return values.pop(0)
        with patch.object(route.time,'monotonic',return_value=100):result=wait(read,samples.append,lambda d:events.append(('event',d)))
        self.assertEqual(events,[('read',1800),('event',1800),('read',1800)]);self.assertEqual(len(samples),4);self.assertEqual(result,status())
    def test_final_in_memory_idle_transition_does_not_require_new_file_event(self):
        values=[status(),status()];owner=[management(),management()];owner[0]['progress']['pending_job']={'project':'native-project'}
        with patch.object(route.time,'monotonic',return_value=100):
            route.await_semantic_population(lambda deadline:values.pop(0),clients,{'collection':'r154','binding':'native-project'},'original-instance',1800,lambda v:None,lambda d:self.fail('No guaranteed final filesystem transition'),lambda d:owner.pop(0),health())
        self.assertEqual(values,[])
    def test_late_ready_after_read_refuses_at_and_after_original_cutoff(self):
        for late in (1800,1801):
            seen=[]
            with patch.object(route.time,'monotonic',side_effect=[100,late]):
                with self.assertRaises(TimeoutError):wait(lambda d:status(),seen.append)
            self.assertEqual(len(seen),1)
    def test_late_event_never_renews_deadline_or_permits_second_read(self):
        sample=status();sample['semantic_readiness'].update(state='partial',completed_artifacts=160);reads=[]
        with patch.object(route.time,'monotonic',side_effect=[100,101,102,1800]):
            with self.assertRaises(TimeoutError):wait(lambda d:reads.append(d) or sample)
        self.assertEqual(reads,[1800])
    def test_setup_orders_gate_before_scope_forwarding(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            root=pathlib.Path(temporary);native=route.NativeRoute({'index_data':str(root/'data'),'source_root':'frozen-root'},root,1800)
            native.control='native-control';native.ready={'health':health()};order=[]
            native.owner_work_observation=lambda fixture,until:management()
            class Fixture:
                def register(self,*args):self.projects['r154']={'collection':'r154','binding':'native-project'};self.scopes['r154']={'unchanged':'scope'}
                def job(self,name,command):order.append(command);return {'state':'succeeded'}
                def query(self,*args,**kwargs):
                    self_kwargs=kwargs;order.append('canonical-diagnostics');assert kwargs=={'deadline':1800};return status()
            class Changes:
                def __init__(self,control):order.append('observer-registered')
                def close(self):order.append('observer-closed')
            native.publish_reviewed_scope=lambda scopes:order.append('scope-forwarded')
            with patch.object(route.time,'monotonic',return_value=100):native.setup(types.SimpleNamespace(Fixture=Fixture,semantic_readiness=clients.semantic_readiness,publication_pending=clients.publication_pending,source_work_settled=clients.source_work_settled),Changes)
            self.assertEqual(order,['index','setup-model','index','observer-registered','canonical-diagnostics','observer-closed','scope-forwarded'])
            self.assertTrue(native.report['full753_semantic_ready_and_no_active_publication_work']['before_scope_emission_and_warm'])

class OwnershipControls(unittest.TestCase):
    def test_actual_r4_settled_nonzero_is_failed_without_false_ownership_label(self):
        actual=json.loads((ACTUAL/'owned-execution.json').read_bytes())['owned']
        for module in (route,observer):
            self.assertFalse(module.natural(actual));self.assertTrue(module.ownership_settled(actual))
            self.assertEqual(module.ownership_outcome(actual),'naturally_settled_nonzero')
    def test_forced_timeout_unknown_or_member_left_never_naturally_settled(self):
        actual=json.loads((ACTUAL/'owned-execution.json').read_bytes())['owned']
        for key,value in [('timed_out',True),('owned_job_forced_termination',True),('owned_descendants_settled',False),('owned_job_final_member_pids',[123]),('exit_code',None),('exit_code',True)]:
            for module in (route,observer):self.assertFalse(module.ownership_settled({**actual,key:value}))
    def test_actual_native_failure_and_cleanup_receipt_are_retained(self):
        report={};child=observer.retain_child_report(ACTUAL,report)
        self.assertFalse(child['complete']);self.assertEqual(report['diagnostic']['full_result'],child)
        self.assertEqual(child['canonical_call']['full_canonical_value']['error']['message'],'joint source authority/publication changed before delivery')
        self.assertEqual(child['native_service_shutdown']['full_response']['status'],'stopping')
    def test_retained_child_proof_only_authorizes_own_original_handle_observation(self):
        child=json.loads((ACTUAL/'worker/diagnostic.json').read_bytes());owned=json.loads((ACTUAL/'owned-execution.json').read_bytes());report={'original_service_before':owned['original_service_before']};calls=[]
        handle=types.SimpleNamespace(observe_original_exit=lambda d:calls.append(d) or {'actual_observer_wait':True})
        observer.observe_child_shutdown(child,report,handle,1800);self.assertEqual(calls,[1800]);self.assertEqual(report['original_service_after'],{'actual_observer_wait':True})
    def test_wrong_birth_or_unobserved_child_never_copies_exit_into_outer_proof(self):
        actual=json.loads((ACTUAL/'worker/diagnostic.json').read_bytes());before=json.loads((ACTUAL/'owned-execution.json').read_bytes())['original_service_before']
        for key,value in [('process_created_filetime',0),('exit_observed',False),('exit_code',1),('handle_closed',False)]:
            child=copy.deepcopy(actual);child['fresh_service_process_settlement'][key]=value;report={'original_service_before':before}
            observer.observe_child_shutdown(child,report,types.SimpleNamespace(observe_original_exit=lambda d:self.fail('Unproven shutdown cannot imply outer exit')),1800)
            self.assertNotIn('original_service_after',report)
    def test_controller_retains_failed_observer_report_and_first_failure_without_duplicate_stop(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            root=pathlib.Path(temporary);work=root/'work';work.mkdir();card=json.loads((route.BASE/'launch-card.json').read_bytes());card['work']=str(work)
            for name in ('ready.json','r154context.bindings.json','context-request.json'):(work/name).write_text('{}')
            out=work/'r154-fresh-attribution-r6';out.mkdir();(out/'owned-execution.json').write_text('{"complete":false,"owned_outcome":"naturally_settled_nonzero"}')
            native=route.NativeRoute(card,root,1800);error=RuntimeError('actual observer exit1')
            with patch.object(route.time,'monotonic',return_value=100),patch.object(native,'call',side_effect=error):
                with self.assertRaises(RuntimeError) as caught:native.diagnostic()
            self.assertIs(caught.exception,error);self.assertTrue(native.delegated_stop)
            self.assertEqual(native.report['r154_owned_execution']['full_result']['owned_outcome'],'naturally_settled_nonzero')
    def test_missing_secondary_report_preserves_original_command_failure(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            root=pathlib.Path(temporary);work=root/'work';work.mkdir();card=json.loads((route.BASE/'launch-card.json').read_bytes());card['work']=str(work)
            for name in ('ready.json','r154context.bindings.json','context-request.json'):(work/name).write_text('{}')
            native=route.NativeRoute(card,root,1800);error=RuntimeError('actual failure')
            with patch.object(route.time,'monotonic',return_value=100),patch.object(native,'call',side_effect=error):
                with self.assertRaises(RuntimeError) as caught:native.diagnostic()
            self.assertIs(caught.exception,error);self.assertIn('observer_report_failure',native.report)


class TypedReadAndActualSampleControls(unittest.TestCase):
    def busy(self):return json.loads(pathlib.Path(json.loads((BASE/'actual-r5-evidence.json').read_bytes())['busy_envelope']['path']).read_bytes())
    def attached(self,root):
        native=route.NativeRoute({'airkctl':'actual-airkctl'},root,1800);native.control='original-control'
        (root/'commands').mkdir();fixture=native.attached_fixture(clients)
        fixture.arguments=lambda project:['--service-runtime-dir','original-control','--catalog-session','frozen-exact-selection']
        return native,fixture
    def test_all89_actual_r5_samples_are_privately_scoped_not_worker_visibility(self):
        rows=json.loads((BASE/'actual-r5-evidence.json').read_bytes())['all89_samples'];self.assertEqual(len(rows),89)
        self.assertEqual({row['completed'] for row in rows},{32,64,96,128})
        for row in rows:
            sample=json.loads(pathlib.Path(row['path']).read_bytes())
            self.assertEqual(sample['runtime'],{'schema':1,'error':'native_runtime_context_unavailable'})
            self.assertEqual(sample['work'],{'error':'native_work_context_unavailable'})
            self.assertFalse(route.full_semantic_population(sample,clients))
            with self.assertRaisesRegex(ValueError,'work sample incomplete'):route.canonical_work_idle(sample,'original-instance')
    def test_actual_busy_accepts_only_existing_native_retry_shape(self):
        value=self.busy();self.assertEqual(clients.read_retry_delay(value),0.025)
        for field,changed in [('code','unknown'),('action','refresh_scope'),('retryable',False),('retry_after_ms',True)]:
            body=clients.tool_value(value);body[field]=changed
            self.assertIsNone(clients.read_retry_delay({'content':[{'type':'text','text':json.dumps(body)}],'isError':True}))
    def test_actual_busy_traverses_attached_run_and_query_with_exact_scope_unchanged(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            native,fixture=self.attached(pathlib.Path(temporary));busy=self.busy();accepted=json.loads((ROOT/'artifacts/v190/final-plan/candidate-worktree-r3/artifacts/r154-fresh-current-section-r1/composed-route-r5/route/commands/03359-setup/stdout.bin').read_bytes())
            replies=[types.SimpleNamespace(returncode=1,stdout=json.dumps(busy).encode(),stderr=b'refused'),types.SimpleNamespace(returncode=0,stdout=json.dumps(accepted).encode(),stderr=b'')]
            with patch.object(route.time,'monotonic',return_value=100),patch.object(route.subprocess,'run',side_effect=replies) as run,patch.object(clients.time,'sleep') as pacing:
                result=fixture.query('r154','tools','call','airk_diagnostics','--args','{"response_mode":"full"}',deadline=1800)
            self.assertEqual(result['semantic_readiness']['completed_artifacts'],128);self.assertEqual(run.call_count,2)
            self.assertEqual(result['scope_observation'],clients.tool_value(busy)['scope_observation'])
            self.assertEqual(run.call_args_list[0].args,run.call_args_list[1].args);self.assertEqual(run.call_args_list[0].kwargs['timeout'],90)
            self.assertEqual(run.call_args_list[1].kwargs['timeout'],90);pacing.assert_called_once_with(0.025)
            records=list((native.root/'commands').glob('*/after.json'));self.assertEqual([json.loads(p.read_bytes())['returncode'] for p in records],[1,0])
            self.assertEqual(clients.tool_value(busy)['scope_observation']['fingerprint'],'3f8163b9ab2a26958a055ccf4e494bb12cc48b3e3e46f22c4742e614f66c9552')
    def test_exhaustion_retains_unchanged41_attempt_cap_without_acceptance(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            native,fixture=self.attached(pathlib.Path(temporary));result=types.SimpleNamespace(returncode=1,stdout=json.dumps(self.busy()).encode(),stderr=b'refused')
            with patch.object(route.time,'monotonic',return_value=100),patch.object(route.subprocess,'run',return_value=result) as run,patch.object(clients.time,'sleep') as pacing:
                with self.assertRaisesRegex(AssertionError,'publication_busy'):fixture.query('r154','tools','call','airk_diagnostics','--args','{"response_mode":"full"}',deadline=1800)
            self.assertEqual(run.call_count,41);self.assertEqual(pacing.call_count,40)
    def test_outer_deadline_shortens90second_query_and_forbids_late_result(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            native,fixture=self.attached(pathlib.Path(temporary));accepted=types.SimpleNamespace(returncode=0,stdout=b'{"content":[{"type":"text","text":"{}"}],"isError":false}',stderr=b'')
            with patch.object(route.time,'monotonic',return_value=1799),patch.object(route.subprocess,'run',return_value=accepted) as run:
                fixture.query('r154','tools','call','airk_diagnostics','--args','{"response_mode":"full"}',deadline=1800)
            self.assertEqual(run.call_args.kwargs['timeout'],1)
    def test_untyped_exit1_transport_timeout_and_unusual_exit_never_replay(self):
        for failure in ('untyped','timeout','exit7'):
            with tempfile.TemporaryDirectory(dir=BASE) as temporary:
                native,fixture=self.attached(pathlib.Path(temporary));result=types.SimpleNamespace(returncode=7 if failure=='exit7' else 1,stdout=b'{"content":[{"type":"text","text":"{}"}],"isError":true}',stderr=b'failure')
                error=route.subprocess.TimeoutExpired(['exact-command'],1) if failure=='timeout' else None
                with patch.object(route.time,'monotonic',return_value=100),patch.object(route.subprocess,'run',side_effect=error,return_value=result) as run,patch.object(clients.time,'sleep') as pacing:
                    with self.assertRaises((AssertionError,route.subprocess.TimeoutExpired)):fixture.query('r154','tools','call','airk_diagnostics','--args','{"response_mode":"full"}',deadline=1800)
                self.assertEqual(run.call_count,1);pacing.assert_not_called()
    def test_mutation_nonzero_checktrue_stays_failed_and_unchecked_mutation_refused_before_spawn(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            native,fixture=self.attached(pathlib.Path(temporary));result=types.SimpleNamespace(returncode=1,stdout=b'{}',stderr=b'refused')
            with patch.object(route.time,'monotonic',return_value=100),patch.object(route.subprocess,'run',return_value=result) as run:
                with self.assertRaisesRegex(ValueError,'exited 1'):fixture.run('index',check=True)
                self.assertEqual(run.call_count,1)
                with self.assertRaisesRegex(ValueError,'Unchecked result'):fixture.run('index',check=False)
                self.assertEqual(run.call_count,1)
    def test_management_idle_uses_native_existing_helper_and_matching_worker(self):
        self.assertTrue(route.owner_work_idle(management(),clients,{'binding':'native-project'},health()))
        value=management();value['progress']['pending_job']={'project':'native-project'}
        self.assertFalse(route.owner_work_idle(value,clients,{'binding':'native-project'},health()))
    def test_management_missing_stale_or_foreign_proof_refuses_even_before_full_population(self):
        for kind in ('health','project','service','fence','job','worker'):
            value=management()
            if kind=='health':value['health_after']['observation']['instance']='replacement'
            elif kind=='project':value['worker']['project']='foreign'
            elif kind=='service':value['worker']['identity']={**value['worker']['identity'],'service':'replacement'}
            elif kind=='fence':value['progress']['observation']['current_fencing_generation']=8
            elif kind=='job':value['progress']['observation']['progress']['job']='other-job'
            else:value['worker']['identity']=None
            with self.assertRaises((ValueError,AssertionError)):route.owner_work_idle(value,clients,{'binding':'native-project'},health())
    def test_idle_owner_metadata_does_not_make_partial_population_ready(self):
        sample=status();sample['semantic_readiness'].update(state='partial',completed_artifacts=128);seen=[]
        with patch.object(route.time,'monotonic',side_effect=[100,101,102,1800]):
            with self.assertRaises(TimeoutError):wait(lambda d:sample,seen.append)
        self.assertEqual(len(seen),2);self.assertIn('owner_management_observation',seen[-1])
    def test_late_management_sample_is_retained_but_never_admitted(self):
        samples=[]
        with patch.object(route.time,'monotonic',side_effect=[100,101,1800]):
            with self.assertRaises(TimeoutError):wait(lambda d:status(),samples.append)
        self.assertEqual(len(samples),2)

    def test_owner_sampler_preserves_actual_native_responses_and_original_deadlines(self):
        with tempfile.TemporaryDirectory(dir=BASE) as temporary:
            native=route.NativeRoute({},pathlib.Path(temporary),1800);native.control='original-control';events=[];packet=management()
            native.alive=lambda:events.append('original-handle-alive')
            def cli(*args,**kwargs):
                events.append(('health',kwargs['timeout']));return types.SimpleNamespace(stdout=json.dumps(packet['health_before']))
            native.cli=cli
            def rpc(method,deadline):
                name=next(iter(method));self.assertEqual(method[name]['project'],'native-project');events.append((name,deadline));return packet[name]
            fixture=types.SimpleNamespace(projects={'r154':{'binding':'native-project'}},rpc=rpc)
            with patch.object(route.time,'monotonic',return_value=100):observed=native.owner_work_observation(fixture,1800)
            self.assertEqual(events,['original-handle-alive',('health',1700),('progress',1800),('watch',1800),('worker',1800),('health',1700),'original-handle-alive'])
            for name in ('progress','watch','worker'):self.assertIs(observed[name],packet[name])
    def test_busy_delay_cannot_renew_or_cross_original_query_cutoff(self):
        calls=[]
        with patch.object(clients.time,'monotonic',return_value=1799.99),patch.object(clients.time,'sleep') as pacing:
            with self.assertRaisesRegex(AssertionError,'publication_busy'):clients.read_envelope(lambda d:calls.append(d) or self.busy(),1800)
        self.assertEqual(calls,[1800]);pacing.assert_not_called()

if __name__=='__main__':unittest.main(verbosity=2)
