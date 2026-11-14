# SPDX-License-Identifier: Apache-2.0
import copy
import unittest

from goal_summary import summarize
from summarize_goal_campaign import close_conditions,verify_execution_closure,PROTOCOL_FILES
from summarize_native_calibration import MODELS


class GoalCampaignClosureTests(unittest.TestCase):
    def setUp(self):
        self.ids=[str(i) for i in range(100)]
        self.ready={'profiles':[{'manifest':str(i+1)*64,'model':name} for i,name in enumerate(sorted(MODELS))]}
        self.pins={'rubric_sha256':'a'*64,'question_scopes_sha256':'b'*64,'native_fixture_ready_sha256':'c'*64,'request_template_sha256':'d'*64}
        self.conditions=[]
        for arm,profile in [('ordinary_followup',None),('deterministic_packet',None),*[('arbiter',row['manifest']) for row in self.ready['profiles']]]:
            self.conditions.append({'complete':True,'evaluation':True,'release_qualified':False,'split':'final','arm':arm,'expected_profile':profile,**self.pins,
                'trials':[{'task_id':key,'arm':arm,'expected_profile':profile,'success':int(key)<(75 if arm=='arbiter' else 70),'external_model_turns':4 if arm=='arbiter' else 5,'citation_identity_errors':0,'failure':None} for key in self.ids]})

    def result(self):return close_conditions(self.conditions,self.ready,self.ids,self.pins)

    def test_each_optional_has_distinct_actual_ready_model(self):
        self.assertEqual(set(self.result()),MODELS|{'ordinary_followup','deterministic_packet'})
        self.conditions[-1]=copy.deepcopy(self.conditions[-2])
        with self.assertRaisesRegex(ValueError,'duplicate model/condition'):self.result()

    def test_partial_failed_or_wrong_input_campaign_refused(self):
        original=copy.deepcopy(self.conditions)
        for key,value in [('complete',False),('campaign_error','infra failure'),('rubric_sha256','changed')]:
            self.conditions[0][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):self.result()
            self.conditions=copy.deepcopy(original)

    def test_optional_floor_does_not_invent_a_baseline_acceptance_floor(self):
        result=summarize(self.result(),self.ids,sorted(MODELS))
        self.assertTrue(result['all_optional_completion_floors_pass'])
        self.assertFalse(result['conditions']['ordinary_followup']['completion_floor_pass'])
        self.assertTrue(result['primary_turn_floor_pass'])


class ExecutionIdentityClosureTests(unittest.TestCase):
    def setUp(self):
        self.protocol={name:str(index+4)*64 for index,name in enumerate(PROTOCOL_FILES)}
        seals={'D:/repo/'+name:pin for name,pin in self.protocol.items()}
        seals['D:/pair/airk-runtime.json']='3'*64
        self.conditions=[]
        for _ in range(6):
            identity={'input_seals':copy.deepcopy(seals),'image':{'sha256':'2'*64},
                      'index_id':'actual-index','ready_sha256':'a'*64,'bindings_sha256':'b'*64,
                      'service_instance':'original-instance','catalog_namespace':'original-catalog'}
            call={'validated':True,'returncode':0,'runtime_image_pins':{'airk':'1'*64,'airkctl':'2'*64},
                  'index_id':'actual-index','session_exchange':{'identity':identity}}
            closed={'kind':'closed','helper_exit_observed':True,'helper_exit_code':0,
                    'child_exit_observed':True,'child_exit_code':0,'child_signal_code':None,
                    'poisoned':None,'launch_count':1,'identity':copy.deepcopy(identity)}
            self.conditions.append({'agent_asset_seals':{'public-base-weights':'c'*64,'research-asset-pins.json':'d'*64},
                                    'native_fixture_ready_sha256':'a'*64,'native_index_bindings_sha256':'b'*64,
                                    'trials':[{'all_native_calls':[call],'client_close':closed}]})
        self.ready={'health':{'instance':'original-instance','catalog_clock':{'namespace':'original-catalog'}}}

    def verify(self):return verify_execution_closure(self.conditions,self.protocol,self.ready)

    def test_same_actual_agent_pair_protocol_and_normal_exit_close(self):
        self.assertEqual(self.verify()['runtime_image_pins'],{'airk':'1'*64,'airkctl':'2'*64})

    def test_different_answering_agent_weights_or_catalog_cannot_close(self):
        for key in ('public-base-weights','research-asset-pins.json'):
            original=copy.deepcopy(self.conditions)
            self.conditions[-1]['agent_asset_seals'][key]='f'*64
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,'answering agent'):self.verify()
            self.conditions=original

    def test_changed_native_pair_or_metadata_within_a_question_cannot_close(self):
        original=copy.deepcopy(self.conditions)
        second=copy.deepcopy(self.conditions[0]['trials'][0]['all_native_calls'][0])
        self.conditions[0]['trials'][0]['all_native_calls'].append(second)
        second['runtime_image_pins']['airk']='f'*64
        with self.assertRaisesRegex(ValueError,'native images'):self.verify()
        self.conditions=original
        self.conditions[-1]['trials'][0]['all_native_calls'][0]['session_exchange']['identity']['input_seals']['D:/pair/airk-runtime.json']='f'*64
        with self.assertRaisesRegex(ValueError,'runtime metadata'):self.verify()

    def test_each_actual_i2_helper_or_sdk_change_is_refused(self):
        for name in PROTOCOL_FILES:
            original=copy.deepcopy(self.conditions)
            self.conditions[-1]['trials'][0]['all_native_calls'][0]['session_exchange']['identity']['input_seals']['D:/repo/'+name]='f'*64
            with self.subTest(name=name),self.assertRaisesRegex(ValueError,'helper/SDK'):self.verify()
            self.conditions=original

    def test_nonzero_missing_signal_or_poisoned_normal_exit_is_refused(self):
        for key,value in [('helper_exit_observed',False),('helper_exit_code',2),('child_exit_observed',False),
                          ('child_exit_code',1),('child_signal_code','SIGTERM'),('poisoned','session_lost'),
                          ('launch_count',2),('child_exit_code',False)]:
            original=copy.deepcopy(self.conditions)
            self.conditions[0]['trials'][0]['client_close'][key]=value
            with self.subTest(key=key,value=value),self.assertRaisesRegex(ValueError,'normal native session'):self.verify()
            self.conditions=original

    def test_closed_child_and_original_instance_must_match_actual_calls(self):
        self.conditions[0]['trials'][0]['client_close']['identity']['index_id']='foreign'
        with self.assertRaisesRegex(ValueError,'closed I2 child'):self.verify()
        self.setUp()
        self.conditions[0]['trials'][0]['all_native_calls'][0]['session_exchange']['identity']['service_instance']='replacement'
        with self.assertRaisesRegex(ValueError,'original Ready'):self.verify()


if __name__=='__main__':unittest.main()
