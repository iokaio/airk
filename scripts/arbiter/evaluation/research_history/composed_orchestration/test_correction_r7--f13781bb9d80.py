# SPDX-License-Identifier: Apache-2.0
"""Pure derivative controls; no service/model/child process or asset reads."""
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from copy import deepcopy
import importlib.util,unittest
B=Path(__file__).resolve().parent

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
R=load('focus_r7',B/'run_public_focus_r7.py')
M=load('focus_outer_r7',B/'run_owned_public_focus_r7.py')
P=load('focus_outer_prior',B.parent/'public-focus-recipe-r6/test_outer_r6.py')
T=load('restart_scope_prior',B.parent/'public-focus-retained-restart-preparation-r1/test_retained_restart_r4.py')
T.R=R

class WholeJourneyCorrections(unittest.TestCase):
    def test_retained_actual_epoch_controls_against_derivative(self):
        for name in ('test_actual_native_scope_epochs_rebind_private_fingerprint_without_changing_any_payload',
                     'test_restart_scope_replay_unknown_and_all_meaningful_changes_refuse'):
            case=T.RestartControls(name);getattr(case,name)()

    def test_explicit_start_full_raw_before_health_and_original_cutoff(self):
        events=[];until=400.0
        class Fixture:
            install='literal-existing-install'
            def service_run(self,*args,**kwargs):
                events.append(('command',args,kwargs));return SimpleNamespace(returncode=0,stdout='original native bytes',stderr='original stderr')
            def rpc(self,*args,**kwargs):events.append(('health',args,kwargs));return {'observation':{'instance':'new-native'}}
        with patch.object(R.time,'monotonic',return_value=100.0):
            value=R.explicit_restart_health(Fixture(),until,lambda label,body:events.append((label,body)))
        self.assertEqual(events[0],('command',('runtime','start','--install-dir','literal-existing-install'),{'check':False,'timeout':120}))
        self.assertEqual(events[1][1]['stdout'],'original native bytes')
        self.assertEqual(events[2],('health',('health',),{'deadline':until}))
        self.assertEqual(value,{'instance':'new-native'})

    def test_explicit_refusal_or_expiry_never_ensures_or_synthesizes(self):
        calls=[]
        class Fixture:
            install='literal-existing-install'
            def service_run(self,*args,**kwargs):calls.append((args,kwargs));return SimpleNamespace(returncode=7,stdout='native refusal',stderr='retained')
            def rpc(self,*args,**kwargs):raise AssertionError('Health after refusal')
        with patch.object(R.time,'monotonic',return_value=100.0):
            with self.assertRaises(RuntimeError):R.explicit_restart_health(Fixture(),200,lambda *args:None)
            self.assertEqual(len(calls),1)
            with self.assertRaises(TimeoutError):R.explicit_restart_health(Fixture(),99,lambda *args:None)
            self.assertEqual(len(calls),1)

    def test_outer_numeric_pid_reuse_needs_both_actual_exit_proofs_and_new_instance(self):
        report,observed,pins=P.native_success()
        observed['actual-restart-health']['process_id']=observed['actual-start']['process_id']
        observed['final-process-exit']['pid']=observed['actual-start']['process_id']
        M.validate_native_observations(report,observed,pins)
        for name in ('old_exit_unknown','new_exit_unknown','same_instance'):
            changed=deepcopy(observed)
            if name=='old_exit_unknown':changed['first-process-exit']['process_handle_signaled']=False
            if name=='new_exit_unknown':changed['final-process-exit']['process_handle_signaled']=False
            if name=='same_instance':changed['actual-restart-health']['instance']=changed['actual-start']['instance']
            with self.subTest(name=name),self.assertRaises(ValueError):M.validate_native_observations(report,changed,pins)

if __name__=='__main__':unittest.main()
