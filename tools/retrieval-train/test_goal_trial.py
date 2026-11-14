# SPDX-License-Identifier: Apache-2.0
import json
import unittest

from goal_trial import GoalTrial


class Sources:
    def __init__(self):self.records=[]
    def observe(self,value):
        i=len(self.records)+1;self.records.append({'id':f'r{i}','text':str(i),'file':str(i),'byte_start':0,'byte_end':1})
    def visible_candidates(self):return list(reversed(self.records))
    def read_followup(self,identifier):
        if identifier!='r1':raise ValueError('unverified test target')
        return 'context',{'path':'verified-source.txt','line':1,'before':0,'after':32,'max_bytes':4096}


class GoalBudgetTests(unittest.TestCase):
    def test_legitimate_read_followup_has_two_real_model_turns_and_two_charged_reads(self):
        class Client:
            def __init__(self):self.requests=[]
            def call(self,tool,arguments,remaining):
                self.requests.append((tool,arguments));value={'observed':'full native result','extra':{'preserved':True},'execution':{'served_mode':'hybrid','incomplete':False}}
                text=json.dumps(value);envelope={'content':[{'type':'text','text':text}]}
                return {'validated':True,'full_canonical_value':value,'elapsed_seconds':.01,'timed_out':False,'returncode':0,'envelope':envelope,'stdout_bytes':len(json.dumps(envelope).encode())}
        generated=[]
        def generate(question,records):
            generated.append(question);last=len(generated)==2
            parsed={'answer':'correct' if last else '', 'citations':['r1','r2'] if last else [],'need_more':not last,'query':'' if last else 'READ r1'}
            return {'parsed':parsed,'shown_records':records,'elapsed_seconds':.001,'input_tokens':100,'output_tokens':30}
        task={'id':'developer-only','question':'Find both original facts','alternatives':[{'human_annotation_id':'test-not-human-gold','answer':'correct','facts':[{'locations':[{'file':str(i),'byte_start':0,'byte_end':1}]} for i in (1,2)]}]}
        client=Client();trial=GoalTrial(client,Sources(),generate,{'schema':'airk.arbiter.context.request/1','budget':{}},'ordinary_followup')
        result=trial.run(task)
        self.assertTrue(result['success']);self.assertEqual(result['external_model_turns'],2);self.assertEqual(result['canonical_calls'],2)
        self.assertEqual([request[0] for request in client.requests],['search','context'])
        self.assertEqual(client.requests[0][1]['response_mode'],'full')
        self.assertIs(client.requests[0][1]['include_status'],True)
        self.assertEqual(client.requests[1][1]['path'],'verified-source.txt')
        self.assertEqual(result['all_native_calls'][0]['full_canonical_value']['extra'],{'preserved':True})

    def test_failed_packet_preserves_unknown_inner_work_instead_of_inventing_zero(self):
        class Client:
            def call(self,*args):return {'full_canonical_value':None,'elapsed_seconds':.01,'timed_out':True,'returncode':None,'envelope':None,'stdout_bytes':0}
        task={'id':'developer-only','question':'Question','alternatives':[]}
        result=GoalTrial(Client(),Sources(),lambda *_:self.fail('model must not run after failed native call'),{'schema':'airk.arbiter.context.request/1','budget':{}},'deterministic_packet').run(task)
        self.assertFalse(result['success']);self.assertIsNone(result['canonical_calls']);self.assertTrue(result['unobserved_native_inner_work'])
        self.assertEqual(result['native_rpc_attempts'],1)

    def test_full_result_with_failed_identity_or_parse_validation_never_reaches_model(self):
        for field in ('identity_error','parse_error'):
            class Client:
                def call(self,*args):
                    return {'validated':field!='identity_error',field:'untrusted response fence','full_canonical_value':{'actual_raw':'retained'},'elapsed_seconds':.01,'timed_out':False,'returncode':0,'envelope':{'isError':False},'stdout_bytes':17}
            task={'id':'developer-only','question':'Question','alternatives':[]}
            result=GoalTrial(Client(),Sources(),lambda *_:self.fail('model must not receive unvalidated source'),{'schema':'airk.arbiter.context.request/1','budget':{}},'ordinary_followup').run(task)
            self.assertEqual(result['failure'],'native_call_failed')
            self.assertEqual(result['all_native_calls'][0]['full_canonical_value'],{'actual_raw':'retained'})
            self.assertEqual(result['external_model_turns'],0)

    def test_initial_lexical_fallback_or_incomplete_hybrid_cannot_change_population(self):
        for mode,incomplete in [('lexical',False),('hybrid',True)]:
            class Client:
                def call(self,*args):
                    return {'validated':True,'full_canonical_value':{'execution':{'served_mode':mode,'incomplete':incomplete}},'elapsed_seconds':.01,'timed_out':False,'returncode':0,'envelope':{'isError':False},'stdout_bytes':17}
            task={'id':'developer-only','question':'Question','alternatives':[]}
            result=GoalTrial(Client(),Sources(),lambda *_:self.fail('model must not receive changed route population'),{'schema':'airk.arbiter.context.request/1','budget':{}},'ordinary_followup').run(task)
            self.assertEqual(result['failure'],'native_read_population_changed')
            self.assertEqual(result['external_model_turns'],0)
