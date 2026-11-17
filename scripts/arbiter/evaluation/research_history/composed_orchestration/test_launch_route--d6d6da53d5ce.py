# SPDX-License-Identifier: Apache-2.0
"""Pure route wiring/cutoff/cleanup controls; never starts native or model work."""
import importlib.util,json,subprocess,time,types,unittest
from pathlib import Path
from unittest.mock import patch
BASE=Path(__file__).parent
spec=importlib.util.spec_from_file_location('route',BASE/'launch_route.py');route=importlib.util.module_from_spec(spec);spec.loader.exec_module(route)
ROOT=BASE/'cheap-control-fixtures-r1'

class Handle:
    handle=None
    def observe_original_exit(self,deadline):self.observed=deadline;return {'exit_observed':True,'exit_code':0,'handle_closed':True}
class Popen:
    _handle=types.SimpleNamespace(Close=lambda:None)
    def poll(self):return None
    def wait(self,timeout):self.wait_budget=timeout;return 0

class RouteControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):ROOT.mkdir()
    def fixture(self,name):
        root=ROOT/name;root.mkdir();work=root/'work';work.mkdir()
        value={'work':str(work),'stage_root':str(root),'service_image':{'path':'actual-service.exe','sha256':'pinned-service'},
               'source_root':str(root/'source'),'whole_seconds':1800}
        native=route.NativeRoute(value,root,time.monotonic()+20);native.control=work/'control'
        native.ready={'health':{'instance':'original-instance','process_id':42,'executable_sha256':'pinned-service',
            'user':route.SID,'version':'1.8.207','protocol':1}}
        native.service=Popen();native.handle=Handle();native.ready['warm_observer_id']='native-observer'
        (work/'closed.json').write_text(json.dumps({'schema':'airk.private-arbiter-fixture-closed/1','evaluation':True,
            'release_qualified':False,'control_root':str(native.control),'instance':'original-instance',
            'shutdown_completed':True,'service_lease_reacquired':True}))
        for journal in ('composed-observer','warm-completions'):
            (work/(journal+'.closed.json')).write_text('{"complete":true,"dropped":0}')
            (work/(journal+'.jsonl')).write_text('')
        return native
    def test_01_original_whole_cutoff_refuses_boundary_without_renewal(self):
        self.assertEqual(route.remaining(1800,now=1799),1)
        for observed in (1800,1801):
            with self.assertRaises(TimeoutError):route.remaining(1800,now=observed)
    def test_02_forced_unknown_or_nonempty_job_never_passes(self):
        good={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,
            'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],
            'parent_assigned_to_job':False,'child_assigned_before_resume':True}
        self.assertTrue(route.natural(good))
        for key,value in [('timed_out',True),('owned_job_forced_termination',True),('owned_job_final_active_processes',1),
            ('owned_job_final_member_pids',[42]),('exit_code',1),('child_assigned_before_resume',False)]:
            self.assertFalse(route.natural({**good,key:value}))
    def test_03_stop_handoff_never_sends_duplicate_native_shutdown(self):
        native=self.fixture('handoff');native.delegated_stop=True
        native.cli=lambda *a,**k:self.fail('Shutdown was already delegated to unchanged R154 owner')
        native.cleanup();self.assertEqual(native.handle.observed,native.cutoff)
        self.assertTrue(native.report['service_process_wait_exit0']);self.assertTrue(native.report['popen_original_handle_closed'])
    def test_04_preobserver_cleanup_binds_original_instance_and_one_stop(self):
        native=self.fixture('owned-stop');calls=[]
        def cli(*a,**k):
            calls.append(a)
            value={'status':'health','observation':native.ready['health']} if a[-1]=='health' else {'status':'stopping'}
            return subprocess.CompletedProcess(a,0,json.dumps(value),'')
        native.cli=cli;native.cleanup()
        self.assertEqual([args[-1] for args in calls],['health','shutdown'])
        self.assertEqual(native.handle.observed,native.cutoff)
    def test_05_stale_health_refuses_before_any_shutdown(self):
        native=self.fixture('stale-health');calls=[]
        def cli(*a,**k):
            calls.append(a);return subprocess.CompletedProcess(a,0,json.dumps({'status':'health',
                'observation':{**native.ready['health'],'instance':'substituted-instance'}}),'')
        native.cli=cli
        with self.assertRaisesRegex(ValueError,'Health binding'):native.cleanup()
        self.assertEqual([args[-1] for args in calls],['health'])
    def test_06_refused_shutdown_never_reissued_or_called_exit_proof(self):
        native=self.fixture('refused-stop');calls=[]
        def cli(*a,**k):
            calls.append(a);return subprocess.CompletedProcess(a,0,json.dumps({'status':'health','observation':native.ready['health']}
                if a[-1]=='health' else {'status':'failure','code':'genuine-refusal'}),'')
        native.cli=cli
        with self.assertRaisesRegex(ValueError,'Shutdown refused'):native.cleanup()
        self.assertEqual([args[-1] for args in calls],['health','shutdown']);self.assertFalse(hasattr(native.handle,'observed'))
    def test_07_closed_receipt_control_or_lease_tamper_refuses(self):
        for ordinal,changes in enumerate([{'control_root':'different-control'},{'service_lease_reacquired':False},{'instance':'other'}]):
            native=self.fixture('tamper-'+str(ordinal));native.delegated_stop=True;path=Path(native.value['work'])/'closed.json'
            body=json.loads(path.read_text());path.write_text(json.dumps({**body,**changes}))
            with self.assertRaisesRegex(ValueError,'lease/closed'):native.cleanup()
    def test_08_setup_reuses_existing_single_submissions_and_literal_job_order(self):
        native=self.fixture('setup');submitted=[]
        class Fixture:
            def register(self,name,source,data):
                self.projects[name]={'binding':'native-registration'};self.scopes[name]={'index_id':'native-index'}
            def rpc(self,method,deadline):
                self.assert_deadline=deadline;return {'job':{'state':'succeeded','id':method['job']['id']}}
            def job(self,name,command):
                submitted.append(command);return self.wait({'job':{'id':command+str(len(submitted))}})
        native.setup(types.SimpleNamespace(Fixture=Fixture))
        self.assertEqual(submitted,['index','setup-model','index'])
        self.assertTrue(native.report['native_index_setup_complete'])
    def test_09_r154_argv_uses_existing_parser_and_one_unchanged360_owner(self):
        native=self.fixture('diagnostic');card=json.loads((BASE/'launch-card.json').read_text());native.value.update(card)
        native.value['work']=str(native.root/'work');native.cutoff=time.monotonic()+500
        work=Path(native.value['work'])
        for name in ('ready.json','r154context.bindings.json','context-request.json'): (work/name).write_text('{}')
        captured=[]
        def call(argv,label,**kwargs):captured.append(argv);raise RuntimeError('stop-before-process-control')
        native.call=call
        with self.assertRaisesRegex(RuntimeError,'stop-before-process-control'):native.diagnostic()
        self.assertEqual(len(captured),1);self.assertTrue(native.delegated_stop)
        prep=Path(card['r154_launcher']).parent
        diagnostic=route.load(prep/'r154_diagnostic.py','existing_r154_parser')
        args=diagnostic.parser().parse_args(captured[0][4:])
        self.assertEqual(args.phase_seconds,360);self.assertEqual(args.context_manifest,card['context_manifest'])
        self.assertEqual(args.service_executable_sha256,card['service_image']['sha256'])
        self.assertEqual(args.sdk_manifest_sha256,'3411049ba9793cbb6bddf1ed2cdd7362015cde3add94a81266f21cfc955fe8e1')
    def test_10_controller_closure_includes_native_policy_and_existing_sdk(self):
        card=json.loads((BASE/'launch-card.json').read_text())
        self.assertEqual(card['controller_pins'][card['resource_policy_input']['path']],card['resource_policy_input']['sha256'])
        for path in [card['session_controller'],card['session_helper'],card['session_library'],*card['client_dependencies']]:
            self.assertIn(path,card['controller_pins'])
        self.assertEqual(card['service_environment']['AIRK_ARBITER_EVALUATION_SECONDS'],'1800')
        self.assertEqual(card['observer_job_bytes'],8<<30)

if __name__=='__main__':unittest.main(verbosity=2)