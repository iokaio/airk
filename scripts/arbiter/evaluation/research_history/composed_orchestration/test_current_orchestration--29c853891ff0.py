# SPDX-License-Identifier: Apache-2.0
"""Synthetic decision controls only; never model, source-gold or custody proof."""
import copy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import Mock,patch
from native_policy import NativePolicy
from goal_trial import GoalTrial
from run_composed import await_ready,run,controller_request

BASE=Path(__file__).parent
(BASE/'test-work').mkdir(exist_ok=True)
class CurrentControls(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(dir=BASE/'test-work');self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.policy_file=self.root/'synthetic-policy-input.json'
        self.policy_file.write_text(json.dumps({'synthetic_unittest_only':True,
            'policy':{'deadline_ms':120000,'shared_input_tokens':8192}}),encoding='utf-8')
        self.pin={'path':str(self.policy_file),'sha256':hashlib.sha256(self.policy_file.read_bytes()).hexdigest()}
        self.template={'schema':'airk.arbiter.context.request/2','question':'synthetic',
            'budget':{'calls':8,'rounds':3,'fanout':8,'input_tokens':8192,'candidates':32,
                      'max_bytes':262144,'deadline_ms':120000}}
    def test_policy_reader_preserves_opaque_native_declarations_and_refuses_drift(self):
        policy=NativePolicy(self.pin)
        self.assertEqual(policy.deadline_seconds,120)
        self.assertEqual(policy.shared_input_tokens,8192)
        self.policy_file.write_text('{}',encoding='utf-8')
        with self.assertRaisesRegex(ValueError,'changed'):policy.verify()
        with self.assertRaisesRegex(ValueError,'digest'):NativePolicy(self.pin)
    def test_goal_template_cannot_substitute_historical_budget(self):
        old=copy.deepcopy(self.template);old['budget']['deadline_ms']=30000
        with self.assertRaisesRegex(ValueError,'differ'):
            GoalTrial(None,None,None,old,'ordinary_followup',resource_policy_input=self.pin)
    def test_goal_calls_share_one_original_native_time_allowance(self):
        client=Mock();client.call.side_effect=lambda *_:{'elapsed_seconds':70.,'stdout_bytes':0,
            'full_canonical_value':{'execution':{'served_mode':'hybrid','incomplete':False}},
            'validated':True,'identity_error':None,'parse_error':None,'timed_out':False,
            'returncode':0,'envelope':{'isError':False,'content':[]}}
        sources=Mock();sources.visible_candidates.return_value=[]
        generate=Mock(return_value={'elapsed_seconds':0.,'shown_records':[],
            'input_tokens':1,'output_tokens':1,'parsed':{'need_more':True,'query':'second query'}})
        trial=GoalTrial(client,sources,generate,self.template,'ordinary_followup',resource_policy_input=self.pin)
        with patch('goal_trial.observe',return_value={'Context_history':[],'work_unknown':False,'known_input_tokens':0}), \
             patch('goal_trial.grade',return_value={'completion':False}), \
             patch('goal_trial.evidence_recall',return_value=(Fraction(0),False)):
            result=trial.run({'id':'synthetic-only','question':'first query'})
        self.assertEqual([call.args[2] for call in client.call.call_args_list],[120.,50.])
        self.assertEqual(result['failure'],'native_total_budget_exceeded')
        self.assertEqual(generate.call_count,1)
    def warm_fixture(self):
        stage={'enabled':True,'selected_profile':'a'*64,'indexes':[]}
        config={'revision':7,'focus':stage,'context':{'enabled':False}}
        configured={'configuration':config}
        idle={'schema':'airk.arbiter.status/2','focus':{'active_profile':None,'running':0,'queued':0,'availability':'disabled'}}
        rows=[]
        for n,value in enumerate((idle,configured)):
            raw=json.dumps(value).encode();(self.root/f'{n:03d}-stdout.bin').write_bytes(raw)
            rows.append({'request':{'kind':'native_command'},'result':{'stdout_sha256':hashlib.sha256(raw).hexdigest()}})
        event={'schema':'airk.private.arbiter-warm-completion/1','observer_id':'native-observer',
            'profile_manifest':'a'*64,'accepted_configure_revision':7,'outcome':'ready'}
        rows.append({'request':{'kind':'controller','request':{'kind':'watch'}},'result':{'value':{'event':event}}})
        step={'stage':'focus','argv':['actual-cli','arbiter','status'],'configuration_observation_step':1,
            'prior_idle_observation_step':0,'completion_watch_observation_step':2,'warm_observer_id':'native-observer',
            'profile_manifest':'a'*64,'deadline_seconds':10}
        ready={'schema':'airk.arbiter.status/2','configuration':config,
            'focus':{'selected_profile':'a'*64,'active_profile':'a'*64,'availability':'ready'}}
        return rows,step,ready
    def test_controller_warm_watch_uses_original_configure_cutoff(self):
        rows,step,ready=self.warm_fixture()
        watch={'kind':'controller','request':{'kind':'watch','timeout_ms':120000},
               'warm_configuration_observation_step':1,'warm_deadline_seconds':10}
        with patch('run_composed.time.monotonic',return_value=100):
            actual=controller_request(watch,2,rows,{1:95},NativePolicy(self.pin),lambda:50)
        self.assertEqual(actual['timeout_ms'],5000)
        self.assertEqual(watch['request']['timeout_ms'],120000)
        with patch('run_composed.time.monotonic',return_value=106),self.assertRaises(TimeoutError):
            controller_request(watch,2,rows,{1:95},NativePolicy(self.pin),lambda:50)
    def test_warm_event_uses_whole_phase_allowance_without_model_read_cap(self):
        rows,step,ready=self.warm_fixture()
        watch={'kind':'controller','request':{'kind':'watch','timeout_ms':1800000},
               'warm_configuration_observation_step':1,'warm_deadline_seconds':1800}
        with patch('run_composed.time.monotonic',return_value=100):
            actual=controller_request(watch,2,rows,{1:95},NativePolicy(self.pin),lambda:200)
        self.assertEqual(actual['timeout_ms'],200000)
        self.assertGreater(actual['timeout_ms'],NativePolicy(self.pin).deadline_ms)
        self.assertTrue(actual['warm_observation'])
        self.assertEqual(watch['request']['timeout_ms'],1800000)
    def test_warm_wakeup_does_not_refresh_original_configure_deadline(self):
        rows,step,ready=self.warm_fixture()
        response=SimpleNamespace(stdout=json.dumps(ready).encode(),stderr=b'',returncode=0)
        with patch('run_composed.time.monotonic',return_value=100),patch('run_composed.subprocess.run',return_value=response) as invoke:
            observed=await_ready(step,rows,{1:95},self.root,3,['actual-cli'],lambda:50)
        self.assertTrue(observed['ready_observed'])
        self.assertEqual(invoke.call_count,1)
        self.assertEqual(invoke.call_args.kwargs['timeout'],5)
    def test_foreign_or_stale_warm_event_never_supplies_readiness(self):
        for field,value in [('observer_id','foreign'),('accepted_configure_revision',6),('outcome','failed')]:
            rows,step,ready=self.warm_fixture();rows[2]['result']['value']['event'][field]=value
            with patch('run_composed.subprocess.run') as invoke,self.assertRaisesRegex(ValueError,'wakeup'):
                await_ready(step,rows,{1:time.monotonic()},self.root,3,['actual-cli'],lambda:10)
            invoke.assert_not_called()
    def test_controller_launch_failure_retains_failed_observation_receipt(self):
        packet=self.root/'packet.json';packet.write_text(json.dumps({'resource_policy_input':self.pin,'whole_phase_seconds':1}),encoding='utf-8')
        plan={'schema':'airk.private.composed-execution-plan/1','whole_phase_seconds':1,
            'steps':[{'kind':'controller','request':{'kind':'finish'}}],
            'resource_policy_input':self.pin,'controller_packet':str(packet),
            'pins':{str(self.policy_file):self.pin['sha256'],str(packet):hashlib.sha256(packet.read_bytes()).hexdigest()},'node':'unavailable-node','controller':'unused'}
        plan_file=self.root/'plan.json';plan_file.write_text(json.dumps(plan),encoding='utf-8')
        output=self.root/'fresh-run'
        with patch('run_composed.subprocess.Popen',side_effect=OSError('synthetic launch failure')),self.assertRaises(OSError):
            run(plan_file,output)
        receipt=json.loads((output/'driver-receipt.json').read_text(encoding='utf-8'))
        self.assertEqual(receipt['status'],'failed_or_incomplete')
        self.assertIsNone(receipt['controller_pid'])
        self.assertIsNone(receipt['controller_exitcode'])
        self.assertFalse(receipt['qualification_granted'])

if __name__=='__main__':unittest.main()
