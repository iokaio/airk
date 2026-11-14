# SPDX-License-Identifier: Apache-2.0
import unittest

from calibration_labels import counterfactual_utilities,validate_search_outcome


class CounterfactualIsolationTests(unittest.TestCase):
    def test_search_projection_refuses_query_rewrite_and_wider_budget(self):
        native={'records':[{'canonical_result':{'hits':[{'text':'Earlier text: Marie Curie visited New York City.'}]}}]}
        value={'choice':{'operation':'search','target':'airk.literal-search/1:opaque-native-owned-data'},'arguments':{'query':'Marie Curie','limit':4,'max_bytes':4096,'full_chunks':True,'response_mode':'full','include_status':True}}
        validate_search_outcome(value,native)
        value['arguments']['query']='Marie Curie birthplace'
        with self.assertRaisesRegex(ValueError,'already-returned source bytes'):validate_search_outcome(value,native)
        value['arguments']['query']='Marie Curie';value['arguments']['max_bytes']=32768
        with self.assertRaisesRegex(ValueError,'budget/catalog differs'):validate_search_outcome(value,native)

    def test_one_choice_cannot_leak_its_observed_support_into_another_choice(self):
        class Sources:
            def __init__(self):self.records={}
            def observe(self,value):
                for name in value:self.records[name]={'file':name,'byte_start':0,'byte_end':1}
        task={'alternatives':[{'human_annotation_id':'developer-unit-example-not-human-gold','facts':[{'locations':[{'file':'a','byte_start':0,'byte_end':1}]},{'locations':[{'file':'b','byte_start':0,'byte_end':1}]}]}]}
        choices=[{'choice':{'id':'one'},'canonical_result':['b']},{'choice':{'id':'two'},'canonical_result':['a']}]
        labels=counterfactual_utilities(task,Sources,['a'],choices)
        self.assertFalse(labels['before_any_original_alternative_complete'])
        self.assertEqual([value['useful'] for value in labels['choices']],[True,False])
