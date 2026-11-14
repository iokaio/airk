# SPDX-License-Identifier: Apache-2.0
import unittest
from fractions import Fraction

from encoder_development import coverage
from qwen_export import verify_asset
from grade_native_retrieval import evidence_recall


class SelectionTests(unittest.TestCase):
    def test_native_retrieval_cannot_credit_an_entire_paper_from_one_hit(self):
        task={'alternatives':[{'facts':[{'locations':[{'file':'paper','byte_start':0,'byte_end':10}]},{'locations':[{'file':'paper','byte_start':20,'byte_end':30}]}]}]}
        self.assertEqual(evidence_recall(task,[{'file':'paper','byte_start':0,'byte_end':10}]),(Fraction(1,2),0))

    def test_native_span_recall_cannot_join_different_original_alternatives(self):
        fact=lambda name:{'locations':[{'file':name,'byte_start':0,'byte_end':1}]}
        task={'alternatives':[{'facts':[fact('a'),fact('b')]},{'facts':[fact('c'),fact('d')]}]}
        spans=[{'file':name,'byte_start':0,'byte_end':1} for name in ('a','d')]
        self.assertEqual(evidence_recall(task,spans),(Fraction(1,2),0))

    def test_evidence_alternatives_cannot_be_union_gold(self):
        task={'human_alternatives':[
            {'unanswerable':False,'alignment':'all_original_evidence_maps_to_allowed_text','evidence':[{'allowed_locations':['a']},{'allowed_locations':['b']}]},
            {'unanswerable':False,'alignment':'all_original_evidence_maps_to_allowed_text','evidence':[{'allowed_locations':['c']},{'allowed_locations':['d']}]}]}
        self.assertEqual(coverage(task,{'a','d'}),(.5,0))
        self.assertEqual(coverage(task,{'a','b'}),(1.,1))

    def test_pinned_asset_refuses_same_length_different_weights(self):
        asset={'bytes':3,'sha256':'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad','git_blob':None}
        self.assertEqual(verify_asset(b'abc',asset),asset['sha256'])
        with self.assertRaisesRegex(ValueError,'SHA256 differs'):
            verify_asset(b'bad',asset)
        with self.assertRaisesRegex(ValueError,'length differs'):
            verify_asset(b'abcd',asset)


if __name__=='__main__':unittest.main()
