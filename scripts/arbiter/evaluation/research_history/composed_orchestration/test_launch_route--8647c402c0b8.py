# SPDX-License-Identifier: Apache-2.0
"""Pure route wiring/cutoff/cleanup controls; never starts native or model work."""
import importlib.util,json,subprocess,time,types,unittest
from pathlib import Path
from unittest.mock import patch
BASE=Path(__file__).parent
spec=importlib.util.spec_from_file_location('route',BASE/'launch_route.py');route=importlib.util.module_from_spec(spec);spec.loader.exec_module(route)
ROOT=BASE/'cheap-control-fixtures-r3'
ROOT.mkdir()

class Handle:
    handle=None
    def observe_original_exit(self,deadline):self.observed=deadline;return {'exit_observed':True,'exit_code':0,'handle_closed':True}
class Popen:
    _handle=types.SimpleNamespace(Close=lambda:None)
    def poll(self):return None
    def wait(self,timeout):self.wait_budget=timeout;return 0

class RouteControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):pass
    def fixture(self,name):
        root=ROOT/name;root.mkdir();work=root/'work';work.mkdir()
        value={'work':str(work),'stage_root':str(root),'service_image':{'path':'actual-service.exe','sha256':'pinned-service'},
               'source_root':str(root/'source'),'whole_seconds':1800,'index_data':str(root/'index-data')}
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

class CorrectiveReaderControls(unittest.TestCase):
    def test_11_delete_shared_reader_succeeds_while_rename_publisher_handle_is_live(self):
        import ctypes,private_record_reader as reader
        from ctypes import wintypes as w
        root=ROOT/'live-publisher';root.mkdir();temporary=root/'temporary';ready=root/'ready.json'
        raw=b'{"complete_atomic_published_record":true}'
        k=reader.K;k.WriteFile.argtypes=[w.HANDLE,ctypes.c_void_p,w.DWORD,ctypes.POINTER(w.DWORD),ctypes.c_void_p];k.WriteFile.restype=w.BOOL
        k.FlushFileBuffers.argtypes=[w.HANDLE];k.FlushFileBuffers.restype=w.BOOL
        k.MoveFileExW.argtypes=[w.LPCWSTR,w.LPCWSTR,w.DWORD];k.MoveFileExW.restype=w.BOOL
        handle=k.CreateFileW(str(temporary),0xc0010000,7,None,1,0x80,None)
        self.assertNotEqual(handle,w.HANDLE(-1).value)
        try:
            written=w.DWORD();self.assertTrue(k.WriteFile(handle,raw,len(raw),ctypes.byref(written),None));self.assertTrue(k.FlushFileBuffers(handle))
            self.assertTrue(k.MoveFileExW(str(temporary),str(ready),0))
            ordinary=k.CreateFileW(str(ready),0x80000000,3,None,3,0x00200080,None)
            self.assertEqual(ordinary,w.HANDLE(-1).value);self.assertEqual(ctypes.get_last_error(),32)
            before=reader.information(handle,8192);value,proof=reader.read_observed(ready,time.monotonic()+5,8192)
            self.assertEqual(value,raw);self.assertEqual(proof['share_flags'],7);self.assertEqual(proof['observed_sharing_contentions'],0)
            self.assertEqual(reader.information(handle,8192),before)
            self.assertEqual(proof['security']['owner_sid'],route.SID);self.assertEqual(proof['metadata']['links'],1)
        finally:self.assertTrue(k.CloseHandle(handle))
    def test_12_actual_access_denied_is_hard_refusal_and_synthetic_source_unchanged(self):
        import ctypes,private_record_reader as reader
        from ctypes import wintypes as w
        class SA(ctypes.Structure):_fields_=[('size',w.DWORD),('descriptor',ctypes.c_void_p),('inherit',w.BOOL)]
        reader.A.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes=[w.LPCWSTR,w.DWORD,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p]
        descriptor=ctypes.c_void_p();sddl=f'O:{route.SID}G:{route.SID}D:P(D;;0x1;;;{route.SID})(A;;FA;;;{route.SID})'
        self.assertTrue(reader.A.ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl,1,ctypes.byref(descriptor),None))
        attributes=SA(ctypes.sizeof(SA),descriptor,False);path=ROOT/'fresh-read-denied.bin'
        handle=reader.K.CreateFileW(str(path),0x40000000,7,ctypes.byref(attributes),1,0x80,None)
        reader.K.LocalFree(descriptor);self.assertNotEqual(handle,w.HANDLE(-1).value)
        try:
            raw=b'fresh-synthetic-only';written=w.DWORD();self.assertTrue(reader.K.WriteFile(handle,raw,len(raw),ctypes.byref(written),None));self.assertTrue(reader.K.FlushFileBuffers(handle))
            before=reader.information(handle,8192);calls=[]
            def observed(*args):calls.append(args);return reader.read_once(*args)
            with self.assertRaises(OSError) as failure:reader.read_observed(path,time.monotonic()+5,8192,reader=observed)
            self.assertEqual(failure.exception.winerror,5);self.assertEqual(len(calls),1)
            self.assertEqual(reader.information(handle,8192),before)
        finally:self.assertTrue(reader.K.CloseHandle(handle))
    def test_13_only_exact_windows_sharing_codes_reobserve_under_original_cutoff(self):
        import ctypes,private_record_reader as reader
        calls=[];clock=iter([0,0.1,0.2,0.3]);failures=iter([32,33,None])
        def read(*args):
            calls.append(args);code=next(failures)
            if code:raise ctypes.WinError(code)
            return b'complete',{'share_flags':7}
        value,proof=reader.read_observed('same-path',1,reader=read,clock=lambda:next(clock))
        self.assertEqual(value,b'complete');self.assertEqual(proof['observed_sharing_contentions'],2);self.assertEqual(len(calls),3)
        times=iter([0,1,2]);calls.clear()
        def locked(*args):calls.append(args);raise ctypes.WinError(32)
        with self.assertRaises(TimeoutError):reader.read_observed('same-path',2,reader=locked,clock=lambda:next(times))
        self.assertEqual(len(calls),2)
        def denied(*args):raise PermissionError(13,'generic ACL denial without winerror')
        with self.assertRaises(PermissionError):reader.read_observed('same-path',2,reader=denied,clock=lambda:0)
    def test_14_first_failure_is_retained_before_cleanup_and_not_replaced(self):
        import sys
        root=ROOT/'first-failure';root.mkdir();output=root/'outer';output.mkdir()
        value={'owned_helpers':str(root),'output':str(output)};seen=[]
        def cleanup(subject):
            record=json.loads((subject.root/'route-progress.json').read_text());seen.append(record['first_failure'])
            raise RuntimeError('secondary cleanup refusal')
        modules={'custody':types.SimpleNamespace(current_job=lambda:{'flags':8192,'job_memory':0,'active_limit':0}),
            'reference_worker':types.SimpleNamespace(windows_sid=lambda:route.SID)}
        with patch.dict(sys.modules,modules),patch.object(route,'verify',lambda value:None),patch.object(route,'load',lambda *args:types.SimpleNamespace(Changes=object)),             patch.object(route.NativeRoute,'start',side_effect=PermissionError(13,'original readiness failure')),patch.object(route.NativeRoute,'cleanup',cleanup):
            started=time.monotonic();result=route.worker(value,started,started+1800)
        self.assertEqual(result,1);self.assertEqual(seen[0]['type'],'PermissionError')
        record=json.loads((output/'route/receipt.json').read_text());self.assertEqual(record['first_failure'],seen[0]);self.assertIn('cleanup_failure',record)
    def test_15_safe_actual_ready_recovery_precedes_one_canonical_stop(self):
        native=RouteControls.fixture(self,'recover-ready');native.ready=None
        recovered=[]
        def observe(cutoff):
            recovered.append(cutoff);native.ready={'health':{'instance':'original-instance','process_id':42,
                'executable_sha256':'pinned-service','user':route.SID,'version':'1.8.207','protocol':1}}
        native.observe_ready=observe;calls=[]
        def cli(*args,**kwargs):
            calls.append(args);return subprocess.CompletedProcess(args,0,json.dumps({'status':'health','observation':native.ready['health']}
                if args[-1]=='health' else {'status':'stopping'}),'')
        native.cli=cli;native.cleanup()
        self.assertEqual(len(recovered),1);self.assertEqual(recovered[0],native.handle.observed)
        self.assertEqual([a[-1] for a in calls],['health','shutdown']);self.assertTrue(native.report['cleanup_recovered_actual_ready'])

class SourceCohortControls(unittest.TestCase):
    def current(self):return json.loads((BASE/'launch-card.json').read_text())
    def scope(self,value,roots,name):
        path=ROOT/name;path.write_text(json.dumps({'source_roots':roots}))
        import hashlib
        return {**value,'source_scope_input':{'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}}
    def test_16_actual_failed_R2_mismatch_refuses_before_any_Popen(self):
        import sys
        actual=json.loads((BASE.parent/'r154-composed-launch-preparation-r2/launch-card.json').read_text())
        with patch.object(route,'card',return_value=(actual,b'oldcard')),patch.object(sys,'argv',['launch','--card-sha256','reviewed','--execute-route']),             patch.object(route.subprocess,'Popen') as spawned:
            with self.assertRaisesRegex(ValueError,'Source cohort mismatch'):route.main()
            spawned.assert_not_called()
    def test_17_current_exact_card_scope_diagnostic_and753_copyroots_agree(self):
        with patch.object(route.subprocess,'Popen') as spawned:
            proof=route.verify_source_cohort(self.current())
            self.assertEqual(proof['copied_source_files'],753);self.assertTrue(proof['canonical_directory_identity_agrees'])
            spawned.assert_not_called()
    def test_18_same_bytes_other_copy_directory_is_not_same_source_identity(self):
        value=self.current();old=BASE.parent/'r154-fresh-attribution-preparation-r1/source-fixture/sources/hotpot'
        with self.assertRaisesRegex(ValueError,'Source cohort mismatch'):
            route.verify_source_cohort({**value,'source_root':str(old)})
    def test_19_extra_or_empty_declared_scope_cannot_broaden_single_index(self):
        value=self.current()
        for ordinal,roots in enumerate([[],[value['source_root'],value['source_root']]]):
            changed=self.scope(value,roots,'extra-scope-'+str(ordinal)+'.json')
            with self.assertRaisesRegex(ValueError,'Source cohort mismatch'):route.verify_source_cohort(changed)
    def test_20_copy_path_outside_frozen_physical_fixture_refuses(self):
        import hashlib
        value=self.current();packet=json.loads(Path(value['r154_preparation']['path']).read_text())
        packet['copied_source_files'][0]['copy_path']=packet['copied_source_files'][0]['original_path']
        path=ROOT/'wrong-copy-cohort.json';path.write_text(json.dumps(packet))
        changed={**value,'r154_preparation':{'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}}
        with self.assertRaisesRegex(ValueError,'Source cohort mismatch'):route.verify_source_cohort(changed)

if __name__=='__main__':unittest.main(verbosity=2)