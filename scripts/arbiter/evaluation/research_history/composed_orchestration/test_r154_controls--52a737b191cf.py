# SPDX-License-Identifier: Apache-2.0
"""Independent synthetic controls; no AIRK/service/NN process is started."""
import hashlib,json,sys,tempfile,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(BASE/'helpers'))
import r154_diagnostic as diagnostic
from native_goal_client import NativeGoalClient
from native_delivery import NativeDelivery
from launch_owned import natural
BUDGET=BASE.parents[4]/'fixtures/split/arbiter/budget_cases.json'
# Resolve through the recorded external Rust owner input, not a handwritten DTO.
BUDGET=Path(json.loads((BASE/'preparation.json').read_bytes())['native_budget_cases']['path'])

class Controls(unittest.TestCase):
    def test_preparation_drift_cannot_self_certify(self):
        raw=b'{"question":"original"}';expected=hashlib.sha256(raw).hexdigest()
        self.assertEqual(diagnostic.frozen_packet(raw,expected)['question'],'original')
        changed=b'{"question":"changed"}'
        with self.assertRaises(ValueError):diagnostic.frozen_packet(changed,expected,raw)
        with self.assertRaises(ValueError):diagnostic.frozen_packet(changed,hashlib.sha256(changed).hexdigest(),raw)
    def test_question_must_match_both_original_args_and_declared_hash(self):
        question='literal original question';packet={'question':question,'original_question_sha256':hashlib.sha256(question.encode()).hexdigest()}
        diagnostic.validate_question(packet,{'question':question})
        for altered in [{'question':'other','original_question_sha256':packet['original_question_sha256']},
            {'question':question,'original_question_sha256':'0'*64}]:
            with self.assertRaises(ValueError):diagnostic.validate_question(altered,{'question':question})
    def test_final_source_sdk_retention_can_cross_original_cutoff(self):
        report={};now=[350]
        def retain_all_sources_and_sdk():now[0]=361
        retain_all_sources_and_sdk()
        self.assertTrue(diagnostic.final_observation(report,0,360,lambda:now[0]))
        self.assertEqual(report['elapsed_seconds'],361)
    def test_exact_whole_phase_expiry_refuses(self):
        report={};self.assertTrue(diagnostic.final_observation(report,10,370,lambda:370))
        self.assertEqual(report['elapsed_seconds'],360)
    def test_native_delivery_is_actual_rust_fixture_and_wrong_policy_refuses(self):
        pin={'path':str(BUDGET),'sha256':hashlib.sha256(BUDGET.read_bytes()).hexdigest()}
        policy=SimpleNamespace(deadline_ms=120000,shared_input_tokens=8192)
        delivery=NativeDelivery(pin,policy)
        self.assertEqual(delivery.deadline_ms,125000)
        with self.assertRaises(ValueError):NativeDelivery(pin,SimpleNamespace(deadline_ms=30000,shared_input_tokens=8192))
        with self.assertRaises(ValueError):NativeDelivery(dict(pin,sha256='0'*64),policy)
    def test_session_setup_does_not_reduce_full_delivery_or_change_native_budget(self):
        pin={'path':str(BUDGET),'sha256':hashlib.sha256(BUDGET.read_bytes()).hexdigest()}
        policy=SimpleNamespace(deadline_ms=120000,deadline_seconds=120,shared_input_tokens=8192)
        now=[0];seen=[]
        with tempfile.TemporaryDirectory(dir=BASE,prefix='synthetic-control-') as directory:
            client=NativeGoalClient.__new__(NativeGoalClient)
            client.policy=policy;client.delivery=NativeDelivery(pin,policy);client.delivery_prepared=False
            client.service_identity={'synthetic_only':True};client.session=None;client.sequence=0
            client.output=Path(directory);client.verify=lambda:None
            client.images={'airkctl':Path(sys.executable)};client.control=BASE;client.index_id='synthetic';client.pins={}
            client.binding=SimpleNamespace(path=BUDGET,validate=lambda value:None)
            def health(*_):now[0]+=17;return {'synthetic_only':True}
            client.observe_health=health
            class Session:
                def invoke(self,identifier,tool,args,timeout):
                    seen.append(timeout)
                    return {'result':{'stdout':json.dumps({'content':[{'type':'text','text':'{}'}]}),'stderr':'','exitCode':0},
                        'argv':['SYNTHETIC-ONLY'],'arguments_sha256':hashlib.sha256(Path(args).read_bytes()).hexdigest()}
            def factory(client,remaining):
                self.assertEqual(remaining,343);now[0]+=23;return Session()
            client.session_factory=factory
            arguments={'question':'SYNTHETIC; NO MODEL','budget':json.loads(BUDGET.read_bytes())['default_budget']}
            before=json.loads(json.dumps(arguments))
            with patch('native_goal_client.time.monotonic',side_effect=lambda:now[0]):
                client.prepare_delivery(360)
                result=client.call('answer',arguments,client.delivery.deadline_seconds,prepared_delivery=True)
                self.assertTrue(result['validated']);self.assertEqual(seen,[125])
                self.assertEqual(arguments,before);self.assertEqual(arguments['budget']['deadline_ms'],120000)
                with self.assertRaises(ValueError):client.call('answer',arguments,125,prepared_delivery=True)
    def test_complete_requires_validated_call_inputs_and_actual_native_settlement(self):
        report={'fresh_call_validated':True,'canonical_call_attempts':1,
            'prepared_sources_and_inputs_unchanged':True,'original_phase_expired':False,
            'native_service_shutdown':{'full_response':{'status':'stopping'}},
            'fresh_service_process_settlement':{'exit_observed':True,'exit_code':0,'handle_closed':True},
            'native_closure':{'sha256':'synthetic-only'},
            'native_journals':{'composed-observer':{},'warm-completions':{}}}
        self.assertTrue(diagnostic.capture_complete(report,None))
        self.assertFalse(diagnostic.capture_complete(report,ValueError('actual retained failure')))
        for field,value in [('fresh_call_validated',False),('prepared_sources_and_inputs_unchanged',False),
            ('original_phase_expired',True),('canonical_call_attempts',2),('native_closure',{}),
            ('native_journals',{}),('fresh_service_process_settlement',{'exit_observed':False}),
            ('fresh_service_process_settlement',{'exit_observed':True,'exit_code':77,'handle_closed':True}),
            ('native_service_shutdown',{'full_response':{'status':'unknown'}})]:
            with self.subTest(field=field):self.assertFalse(diagnostic.capture_complete(dict(report,**{field:value}),None))
    def test_real_owner_failure_flags_cannot_claim_natural_cleanup(self):
        proof={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,
            'owned_job_forced_termination':False,'owned_job_final_active_processes':0,
            'owned_job_final_member_pids':[],'parent_assigned_to_job':False,'child_assigned_before_resume':True}
        self.assertTrue(natural(proof))
        for field,value in [('exit_code',23),('timed_out',True),('owned_job_forced_termination',True),
            ('owned_job_final_member_pids',[77]),('owned_descendants_settled',False)]:
            with self.subTest(field=field):self.assertFalse(natural(dict(proof,**{field:value})))

if __name__=='__main__':unittest.main()
