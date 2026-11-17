# SPDX-License-Identifier: Apache-2.0
"""Experiment association fences on Rust-owned inert fixture values; no NN/gold."""
import copy
import json
from pathlib import Path
import unittest
from capture_selected_service_cal import associate
ROOT=Path(__file__).resolve().parents[5]
class AssociationTests(unittest.TestCase):
    def setUp(self):
        raw=json.loads((ROOT/'fixtures/split/arbiter/native_context_action_cases.json').read_bytes())
        request=raw['cases'][0]['input']
        self.release={'Focus_profile':'01'*32,'MiniLM_pipeline':'02'*32,'Context_bootstrap_profile':'03'*32}
        self.record={'schema':'airk.private-selected-Context-native-admission-observation/1','Context_NN_dispatch':False,
                     'Focus_configuration':'ON','actual_Focus_profile':self.release['Focus_profile'],'MiniLM_pipeline':self.release['MiniLM_pipeline'],
                     'actual_Context_profile':{'manifest':self.release['Context_bootstrap_profile']},'request':request,'planned_input_digest':'04'*32}
        self.reply={'assessment':{'scope':copy.deepcopy(request['scope'])},'assessment_history':[{'ordinal':request['ordinal'],
                    'basis':copy.deepcopy(request['roster']['basis']),'result':{'advice':{'input_digest':'04'*32,'observed_input_tokens':0,
                    'work':{'input_tokens':0},'outcome':{'status':'unavailable'}}}}]}
        self.question={'question':request['question']}
    def check(self):return associate(self.record,self.reply,self.question,self.release)
    def test_exact_native_admission_identity(self):self.assertIs(self.check(),self.record['request'])
    def test_source_scope_mismatch_refuses(self):
        self.reply['assessment']['scope']['configuration_revision']+=1
        with self.assertRaises(ValueError):self.check()
    def test_unobserved_tokens_cannot_become_zero(self):
        self.reply['assessment_history'][0]['result']['advice']['observed_input_tokens']=None
        with self.assertRaises(ValueError):self.check()
    def test_accidental_Context_forward_refuses(self):
        self.reply['assessment_history'][0]['result']['advice']['work']['input_tokens']=1
        with self.assertRaises(ValueError):self.check()
    def test_selected_action_is_not_capture_only(self):
        self.reply['assessment_history'][0]['result']['advice']['outcome']['status']='accepted'
        with self.assertRaises(ValueError):self.check()
    def test_wrong_model_input_digest_refuses(self):
        self.reply['assessment_history'][0]['result']['advice']['input_digest']='ff'*32
        with self.assertRaises(ValueError):self.check()
if __name__=='__main__':unittest.main()
