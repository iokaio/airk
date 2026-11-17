# SPDX-License-Identifier: Apache-2.0
"""Actual R5 stop/exit/Job proof plus explicit Start and cleanup semantic controls."""
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from copy import deepcopy
import hashlib,importlib.util,json,time,unittest
BASE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('restart_phase',BASE/'run_retained_restart_r4.py');R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
A=BASE.parent.parent
OLD=A/'public-focus-native-r5';OWNER=A/'public-focus-owned-runtime-r5'

class Fixture:
    def __init__(self,code=0,response=None):
        self.install='exact-retained-r5-install';self.calls=[];self.managed_stop=None;self.stop_cutoff=None
        self.code=code;self.response={'observation':{'instance':'actual-new'}} if response is None else response
    def service_run(self,*args,**kwargs):
        self.calls.append(('command',args,kwargs));return SimpleNamespace(returncode=self.code,stdout='literal raw native stdout',stderr='literal raw native stderr')
    def rpc(self,method,**kwargs):self.calls.append(('rpc',method,kwargs));return self.response
    def close(self):self.calls.append(('close',));self.managed_stop={'actual':'Stop'}

class RestartControls(unittest.TestCase):
    def test_actual_original_failed_r5_stop_exit_and_job_are_separate_positive_facts(self):
        health=json.loads((OLD/'001-actual-start.json').read_text(encoding='utf-8'))
        native={'previous_instance':health['instance'],'previous_pid':health['process_id'],'runtime_sha256':health['runtime_sha256']}
        stop=json.loads((OLD/'842-first-native-stop.json').read_text(encoding='utf-8'))
        exited=json.loads((OLD/'843-first-process-exit.json').read_text(encoding='utf-8'))
        job=json.loads((OWNER/'receipt.json').read_text(encoding='utf-8'))['owner']
        R.prior_proof_values(native,stop,exited,job)
        for name in ('stop','exit','force','retag'):
            a,b,c=deepcopy(stop),deepcopy(exited),deepcopy(job)
            if name=='stop':a['record']['phase']='draining'
            if name=='exit':b['process_handle_signaled']=False
            if name=='force':c['owned_job_forced_termination']=True
            if name=='retag':c['exit_code']=0
            with self.subTest(name=name),self.assertRaises(ValueError):R.prior_proof_values(native,a,b,c)

    def test_explicit_native_start_never_automatic_ensure_and_raw_record_first(self):
        f=Fixture();records=[];until=time.monotonic()+120
        result=R.explicit_start(f,until,lambda k,v:records.append((k,v)))
        self.assertEqual(result,{'instance':'actual-new'})
        self.assertEqual(f.calls[0][1],('runtime','start','--install-dir','exact-retained-r5-install'))
        self.assertFalse(f.calls[0][2]['check']);self.assertLessEqual(f.calls[0][2]['timeout'],120)
        self.assertEqual(records[0][0],'raw-explicit-start');self.assertEqual(records[0][1]['stdout'],'literal raw native stdout')
        self.assertEqual(f.calls[1],('rpc','health',{'deadline':until}))

    def test_explicit_start_refusal_keeps_first_result_and_never_queries_or_synthesizes_health(self):
        f=Fixture(code=1);records=[]
        with self.assertRaises(ValueError):R.explicit_start(f,time.monotonic()+120,lambda k,v:records.append((k,v)))
        self.assertEqual(len(f.calls),1);self.assertEqual(records[0][1]['exit_code'],1)

    def test_expired_original_cutoff_cannot_dispatch_or_renew_start(self):
        f=Fixture()
        with self.assertRaises(TimeoutError):R.explicit_start(f,time.monotonic()-1,lambda *args:None)
        self.assertEqual(f.calls,[])

    def test_missing_native_health_keeps_refusal_and_does_not_invent_observation(self):
        f=Fixture(response={'status':'service_unavailable'});records=[]
        with self.assertRaises(ValueError):R.explicit_start(f,time.monotonic()+120,lambda k,v:records.append((k,v)))
        self.assertEqual(records[-1],('actual-health-response',{'status':'service_unavailable'}))

    def test_cleanup_missing_health_never_queries_unavailable_service(self):
        f=Fixture();report={}
        self.assertIsNone(R.cleanup(f,None,False,False,None,100,lambda *a:None,report))
        self.assertEqual(f.calls,[]);self.assertNotIn('actual_native_stop',report)

    def test_cleanup_known_settled_instance_never_repeats_stop(self):
        f=Fixture();report={};health={'user':R.SID,'instance':'actual'}
        self.assertEqual(R.cleanup(f,health,True,False,123,200,lambda *a:None,report),123)
        self.assertEqual(f.calls,[]);self.assertNotIn('actual_native_stop',report)

    def test_cleanup_uncertain_original_stop_never_repeats_or_renews(self):
        f=Fixture();report={};health={'user':R.SID,'instance':'actual'}
        self.assertEqual(R.cleanup(f,health,False,True,123,200,lambda *a:None,report),123)
        self.assertEqual(f.calls,[])

    def test_cleanup_actual_samehuman_uses_original_intersection_and_preserves_primary_failure(self):
        f=Fixture();report={'failure':{'message':'first actual failure'}};health={'user':R.SID,'instance':'actual'}
        with patch.object(R.time,'monotonic',return_value=100):
            self.assertEqual(R.cleanup(f,health,False,False,None,150,lambda *a:None,report),150)
        self.assertEqual(f.stop_cutoff,150);self.assertEqual(f.calls,[('close',)])
        self.assertEqual(report['failure'],{'message':'first actual failure'})

    def test_cleanup_foreign_health_refuses_without_masking_earliest_failure(self):
        f=Fixture();report={'failure':{'message':'first actual failure'}}
        R.cleanup(f,{'user':'foreign'},False,False,None,150,lambda *a:None,report)
        self.assertEqual(f.calls,[]);self.assertEqual(report['failure']['message'],'first actual failure')
        self.assertIn('cleanup_failure',report)

    def test_startup_health_observer_keeps_one_original_absolute_cutoff(self):
        f=Fixture();until=180.0
        with patch.object(R.time,'monotonic',side_effect=[100.0]):
            R.explicit_start(f,until,lambda *args:None)
        self.assertEqual(f.calls[0][2]['timeout'],80.0)
        self.assertEqual(f.calls[1][2]['deadline'],180.0)

    def test_legitimate_numeric_pid_reuse_requires_old_exit_and_new_instance_handle(self):
        original=json.loads((OLD/'001-actual-start.json').read_text(encoding='utf-8'))
        native={'airk_sha256':original['executable_sha256'],'runtime_sha256':original['runtime_sha256'],
            'previous_instance':original['instance'],'previous_pid':original['process_id']}
        stop=json.loads((OLD/'842-first-native-stop.json').read_text(encoding='utf-8'))
        exited=json.loads((OLD/'843-first-process-exit.json').read_text(encoding='utf-8'))
        job=json.loads((OWNER/'receipt.json').read_text(encoding='utf-8'))['owner']
        R.prior_proof_values(native,stop,exited,job)
        current=deepcopy(original);current['instance']='11'*16
        handle=SimpleNamespace(pid=original['process_id'],handle=1)
        R.require_restarted_identity(current,native,handle)
        self.assertEqual(current['process_id'],original['process_id'])
        for name in ('same_instance','wrong_owner','wrong_image','wrong_runtime','wrong_handle','closed_handle'):
            changed=deepcopy(current);observed=SimpleNamespace(pid=handle.pid,handle=1)
            if name=='same_instance':changed['instance']=native['previous_instance']
            if name=='wrong_owner':changed['user']='foreign'
            if name=='wrong_image':changed['executable_sha256']='00'*32
            if name=='wrong_runtime':changed['runtime_sha256']='00'*32
            if name=='wrong_handle':observed.pid=123
            if name=='closed_handle':observed.handle=None
            with self.subTest(name=name),self.assertRaises(ValueError):R.require_restarted_identity(changed,native,observed)

    def immutable_fixture(self):
        files=[BASE/'CONTRACT.md',BASE/'CONTRACT-r2.md',BASE/'test_retained_restart_r2.py']
        return {'pins':[{'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}

    def test_complete_immutable_roster_detects_first_middle_and_last_pin_drift(self):
        card=self.immutable_fixture()
        for changed in range(3):
            called=[]
            def observe(path):
                called.append(path)
                return '00'*32 if path==card['pins'][changed]['path'] else next(r['sha256'] for r in card['pins'] if r['path']==path)
            with self.subTest(changed=changed),patch.object(R,'sha',side_effect=observe),self.assertRaises(ValueError):
                R.verify_immutable_pins(card)
            self.assertEqual(called,[r['path'] for r in card['pins'][:changed+1]])

    def test_postverification_excludes_mutable_retained_state_preimage(self):
        card=self.immutable_fixture();card['census']='nonexistent-mutable-preimage-must-not-be-reopened'
        R.verify_immutable_pins(card)

    def test_slow_final_hash_is_charged_before_single_final_clock_and_refuses(self):
        card=self.immutable_fixture();clock=[0.0];called=[];report={'original_R5_whole_journey_passed':False}
        def slow_hash(path):
            called.append(path);clock[0]+=40
            return next(r['sha256'] for r in card['pins'] if r['path']==path)
        with patch.object(R,'sha',side_effect=slow_hash),patch.object(R.time,'monotonic',side_effect=lambda:clock[0]) as final_clock:
            self.assertFalse(R.final_observation(report,0.0,100.0,True,lambda:R.verify_immutable_pins(card)))
        self.assertEqual(final_clock.call_count,1);self.assertEqual(len(called),3)
        self.assertEqual(report['elapsed_seconds'],120.0);self.assertFalse(report['completion_observed_before_cutoff'])
        self.assertEqual(report['failure']['type'],'TimeoutError');self.assertFalse(report['original_R5_whole_journey_passed'])

    def test_postverification_failure_and_expiry_preserve_earliest_native_failure(self):
        first={'type':'NativeFailure','message':'original earliest actual failure'}
        report={'failure':deepcopy(first),'original_R5_whole_journey_passed':False}
        def changed():raise ValueError('later immutable drift')
        with patch.object(R.time,'monotonic',return_value=120.0) as final_clock:
            self.assertFalse(R.final_observation(report,0.0,100.0,False,changed))
        self.assertEqual(report['failure'],first);self.assertEqual(final_clock.call_count,1)
        self.assertEqual(report['secondary_failures'][0]['stage'],'postverification')
        self.assertFalse(report['passed']);self.assertFalse(report['original_R5_whole_journey_passed'])

    def test_final_completion_only_after_postproof_before_original_cutoff(self):
        events=[];report={}
        with patch.object(R.time,'monotonic',side_effect=lambda:events.append('clock') or 99.0) as final_clock:
            self.assertTrue(R.final_observation(report,0.0,100.0,True,lambda:events.append('proof')))
        self.assertEqual(events,['proof','clock']);self.assertEqual(final_clock.call_count,1)
        self.assertTrue(report['passed']);self.assertEqual(report['elapsed_seconds'],99.0)

    def native_scope_epochs(self):
        old=json.loads((OLD/'010-semantic-ready-a.json').read_text(encoding='utf-8'))['member_body']['scope_observation']
        raw=json.loads((A/'public-focus-retained-restart-native-r1/007-raw-diagnostics-a.json').read_text(encoding='utf-8'))
        new=json.loads(raw['stdout'])['structuredContent']['scope_observation']
        oldhealth=json.loads((OLD/'001-actual-start.json').read_text(encoding='utf-8'))
        newhealth=json.loads((A/'public-focus-retained-restart-native-r1/004-actual-start.json').read_text(encoding='utf-8'))
        return old,new,oldhealth['instance'],newhealth['instance']

    def test_actual_native_scope_epochs_rebind_private_fingerprint_without_changing_any_payload(self):
        old,new,oldinstance,newinstance=self.native_scope_epochs();before=(deepcopy(old),deepcopy(new))
        R.require_restart_scope_continuity(old,new,oldinstance,newinstance)
        self.assertEqual((old,new),before);self.assertNotEqual(old['fingerprint'],new['fingerprint'])
        self.assertEqual(old['selection'],new['selection']);self.assertEqual(old['members'],new['members'])

    def test_restart_scope_replay_unknown_and_all_meaningful_changes_refuse(self):
        old,current,oldinstance,newinstance=self.native_scope_epochs()
        for name in ('replay','same_instance','owner','identity','catalog_revision','index','roots','revision','policy','membership','unknown'):
            new=deepcopy(current);instance=newinstance
            if name=='replay':new['fingerprint']=old['fingerprint']
            if name=='same_instance':instance=oldinstance
            if name=='owner':new['selection']['owner']='admin'
            if name=='identity':new['selection']['scope_identity']='ff'*32
            if name=='catalog_revision':new['selection']['catalog_revision']+=1
            if name=='index':new['members'][0]['index_id']='foreign'
            if name=='roots':new['members'][0]['source_root_ids']=['foreign']
            if name=='revision':new['members'][0]['source_revision']+=1
            if name=='policy':new['members'][0]['policy_epoch']=1;new['members'][0]['policy_digest']='ff'*32
            if name=='membership':new['members'][0]['memberships']=[{'project_id':'foreign','membership_revision':1}]
            if name=='unknown':new['future_authority']='unknown'
            with self.subTest(name=name),self.assertRaises(ValueError):R.require_restart_scope_continuity(old,new,oldinstance,instance)

    def test_first_actual_child_failure_is_distinct_from_true_job_settlement(self):
        actual=json.loads((A/'public-focus-retained-restart-owned-r1/receipt.json').read_text(encoding='utf-8'))
        self.assertTrue(actual['owner']['owned_descendants_settled'])
        self.assertFalse(actual['owner']['owned_job_forced_termination'])
        with self.assertRaisesRegex(RuntimeError,'immutable native source scope changed'):
            R.require_child_success(actual['owner'],actual['child_receipt_observation'])

    def test_latest_failed_restart_is_actual_settlement_ingress_without_retagging_original_r5(self):
        latest=A/'public-focus-retained-restart-native-r1'
        health=json.loads((latest/'004-actual-start.json').read_text(encoding='utf-8'))
        card={'native':{'previous_instance':health['instance'],'previous_pid':health['process_id'],'runtime_sha256':health['runtime_sha256']},
            'prior':{'original_r5_child_receipt':str(OLD/'receipt.json'),'child_receipt':str(latest/'receipt.json'),
                'owner_receipt':str(A/'public-focus-retained-restart-owned-r1/receipt.json')}}
        R.prior_proof(card)
