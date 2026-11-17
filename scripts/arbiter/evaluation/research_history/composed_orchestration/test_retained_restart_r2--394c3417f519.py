# SPDX-License-Identifier: Apache-2.0
"""Actual R5 stop/exit/Job proof plus explicit Start and cleanup semantic controls."""
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from copy import deepcopy
import importlib.util,json,time,unittest
BASE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('restart_phase',BASE/'run_retained_restart_r2.py');R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
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
