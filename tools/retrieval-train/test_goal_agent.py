# SPDX-License-Identifier: Apache-2.0
import unittest

from eval_agent_smoke import bounded_records,parse_first_object,prompt_for
from native_goal_client import tool_value
import json


class AgentProtocolTests(unittest.TestCase):
    def test_actual_cli_reader_preserves_full_canonical_fields(self):
        expected={'results':[{'text':'Café','original_range':{'start':3,'end':12},'future_field':{'nested':[1,2]}}],
                  'readiness':{'status':'partial'},'coverage':{'omitted':7},'continuation':{'token':'next'},'error':{'retryable':True}}
        self.assertEqual(tool_value({'content':[{'type':'text','text':json.dumps(expected)}],'isError':False}),expected)
        with self.assertRaises(ValueError):tool_value({'content':[{'type':'text','text':'{}'},{'type':'text','text':'{}'}]})

    def test_prompt_budget_preserves_unicode_byte_prefix_and_discloses_truncation(self):
        class CharacterTokenizer:
            def encode(self,text,add_special_tokens=False):return list(text)
        tokenizer=CharacterTokenizer()
        original={'id':'r1','text':'日本語🚀'*4000,'file':'source.txt','byte_start':73,'byte_end':52073,'native_observations':[]}
        selected,omitted=bounded_records(tokenizer,'Which?', [original])
        self.assertLessEqual(len(prompt_for('Which?',selected)),4096)
        self.assertTrue(selected[0]['prompt_prefix_truncated'])
        self.assertEqual(selected[0]['byte_end'],73+len(selected[0]['text'].encode()))
        self.assertTrue(original['text'].startswith(selected[0]['text']))
        self.assertEqual(len(original['text']),16000)
        self.assertEqual(omitted,[])
        original['native_observations'].append({'future':'read'})
        self.assertEqual(selected[0]['native_observations'],[])

    def test_first_json_stopping_requires_exact_fields_and_types(self):
        valid='{"answer":"yes","citations":["r1"],"need_more":false,"query":""}'
        self.assertEqual(parse_first_object(valid+' extra'),{'answer':'yes','citations':['r1'],'need_more':False,'query':''})
        for malformed in ('```json\n'+valid,valid.replace('false','"false"'),valid.replace('"r1"','1'),valid.replace(',"query":""','')):
            with self.assertRaises(ValueError):parse_first_object(malformed)
