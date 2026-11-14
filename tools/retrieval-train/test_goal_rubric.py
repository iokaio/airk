# SPDX-License-Identifier: Apache-2.0
import unittest

from goal_rubric import grade,next_read_utility


class HumanRubricTests(unittest.TestCase):
    def test_one_original_fact_can_be_complete_across_contiguous_cited_reads(self):
        task={'alternatives':[{'human_annotation_id':'one','answer':'yes','facts':[{'locations':[{'file':'a','byte_start':0,'byte_end':20}]}]}]}
        reads=[{'file':'a','byte_start':0,'byte_end':10},{'file':'a','byte_start':10,'byte_end':20}]
        self.assertTrue(grade(task,'yes',reads,reads)['completion'])
        self.assertFalse(grade(task,'yes',reads,reads[:1])['completion'])
        gap=[reads[0],{**reads[1],'byte_start':11}]
        self.assertFalse(grade(task,'yes',gap,gap)['completion'])

    def test_answer_and_support_must_match_same_original_alternative(self):
        def span(name):return {'file':name,'byte_start':0,'byte_end':20}
        task={'alternatives':[{'human_annotation_id':'one','answer':'red','facts':[{'locations':[span('a')]},{'locations':[span('b')]}]},
                              {'human_annotation_id':'two','answer':'blue','facts':[{'locations':[span('c')]},{'locations':[span('d')]}]}]}
        self.assertFalse(grade(task,'red',[span('a'),span('d')],[span('a'),span('d')])['completion'])
        self.assertFalse(grade(task,'blue',[span('a'),span('b')],[span('a'),span('b')])['completion'])
        self.assertFalse(grade(task,'red',[span('a'),span('b')],[span('a')])['completion'])
        self.assertTrue(grade(task,'red',[span('a'),span('b')],[span('a'),span('b')])['completion'])
        self.assertTrue(next_read_utility(task,[span('a')],[span('a'),span('d')])['useful'])
        self.assertFalse(next_read_utility(task,[span('a')],[span('a')])['useful'])
