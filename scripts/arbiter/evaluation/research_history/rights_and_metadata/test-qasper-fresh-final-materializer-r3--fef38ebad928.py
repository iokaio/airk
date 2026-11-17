# SPDX-License-Identifier: Apache-2.0
"""Synthetic isolation/alignment controls; never reads candidate corpus content."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

path=Path(__file__).with_name('qasper-fresh-final-materializer-r3.py')
spec=importlib.util.spec_from_file_location('subject',path)
subject=importlib.util.module_from_spec(spec);spec.loader.exec_module(subject)

class Controls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subject.load_owner(subject.OWNER.read_bytes())
    def setUp(self):
        self.paper={'title':'SYNTHETIC work', 'full_text':[{'section_name':'Definitions','paragraphs':['Alpha.', 'Beta.', 'A "quotation".']}],
            'qas':[{'question_id':'synthetic-q', 'question':'SYNTHETIC question', 'answers':[
                {'annotation_id':'a1','answer':{'unanswerable':False,'evidence':['Alpha.'],'free_form_answer':'Alpha'}},
                {'annotation_id':'a2','answer':{'unanswerable':False,'evidence':['Beta.'],'free_form_answer':'Beta'}}]}]}
        self.row={'paper_id':'synthetic-paper','publisher_split':'test','title':self.paper['title'],
            'source_text_sha256':hashlib.sha256(json.dumps(self.paper['full_text'],sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest(),
            'question_ids':['synthetic-q'],'human_annotation_ids':['a1','a2']}

    def test_preserves_all_alternatives_and_never_unions_evidence(self):
        view=subject.align_paper(self.paper,self.row)
        q=view['questions'][0]
        self.assertEqual([a['original_human_record'] for a in q['human_alternatives']],self.paper['qas'][0]['answers'])
        self.assertEqual([a['evidence'][0]['allowed_locations'] for a in q['human_alternatives']], [['synthetic-paper:0:0'],['synthetic-paper:0:1']])
        self.assertEqual(q['eligibility'],'eligible')

    def test_quote_and_missing_evidence_are_not_admitted(self):
        self.paper['qas'][0]['answers'][0]['answer']['evidence']=['A "quotation".']
        self.paper['qas'][0]['answers'][1]['answer']['evidence']=['Missing.']
        view=subject.align_paper(self.paper,self.row)
        self.assertEqual(view['questions'][0]['eligibility'],'no_complete_original_alternative')
        self.assertEqual([a['evidence'][0]['allowed_locations'] for a in view['questions'][0]['human_alternatives']],[[],[]])
        self.assertEqual(len(view['excluded_passages']),1)

    def test_all_unanswerable_not_sufficiency_or_eligible(self):
        for answer in self.paper['qas'][0]['answers']: answer['answer']['unanswerable']=True
        self.assertEqual(subject.align_paper(self.paper,self.row)['questions'][0]['eligibility'],'all_alternatives_unanswerable')

    def test_incomplete_alternative_does_not_destroy_complete_separate_alternative(self):
        self.paper['qas'][0]['answers'][0]['answer']['evidence']=['Missing.']
        view=subject.align_paper(self.paper,self.row)
        self.assertEqual(view['questions'][0]['eligibility'],'eligible')
        self.assertEqual(view['questions'][0]['human_alternatives'][0]['evidence'][0]['allowed_locations'],[])

    def test_source_hash_split_and_original_id_rosters_refuse(self):
        for field,value in [('source_text_sha256','0'*64),('publisher_split','train'),('question_ids',['other']),('human_annotation_ids',['other'])]:
            row=copy.deepcopy(self.row);row[field]=value
            with self.subTest(field=field),self.assertRaises(ValueError): subject.align_paper(self.paper,row)

    def test_fixed_hash_selection_no_refill_on_shortfall(self):
        view=subject.align_paper(self.paper,self.row)
        self.assertEqual(subject.choose([view],2,'frozen:')[0],[])
        self.assertEqual(subject.choose([view],1,'frozen:')[0],['synthetic-q'])
        with self.assertRaisesRegex(ValueError,'duplicate'):subject.choose([view,view],1,'frozen:')

    def test_output_inside_raw_or_alias_refused_before_creation(self):
        parent=Path(__file__).parent
        with tempfile.TemporaryDirectory(dir=parent) as temporary:
            raw=Path(temporary)/'raw';raw.mkdir()
            anchor=Path(temporary)/'designated-anchor';anchor.mkdir()
            observer=lambda path:{'current_owner_only':True}
            bad=raw/'not-created'
            with self.assertRaises(ValueError):subject.output_guard(bad,[raw],anchor,observer)
            self.assertFalse(bad.exists())
            separate=anchor/'materialized'
            self.assertEqual(subject.output_guard(separate,[raw],anchor,observer)[0],separate.resolve())
            self.assertFalse(separate.exists())

    def test_every_frozen_metadata_helper_rights_root_refused(self):
        root=Path(__file__).parent
        with tempfile.TemporaryDirectory(dir=root) as temporary:
            base=Path(temporary);anchor=base/'designated-anchor';anchor.mkdir()
            names=['raw','old-fixtures','old-corpus','versioned-primary','additional-versioned','methodological-publisher','helper-rights']
            inputs=[base/name for name in names]
            for item in inputs:item.mkdir()
            for item in inputs:
                with self.subTest(root=item.name),self.assertRaises(ValueError):subject.output_guard(item/'materialized',inputs,anchor,lambda p:{})
                self.assertFalse((item/'materialized').exists())

    def test_owner_proof_required_before_creation(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as temporary:
            anchor=Path(temporary);output=anchor/'materialized'
            def refused(path):raise ValueError('owner_proof_failed')
            with self.assertRaisesRegex(ValueError,'owner_proof_failed'):subject.output_guard(output,[],anchor,refused)
            self.assertFalse(output.exists())

    def test_owner_load_executes_retained_bytes_without_reopening_source(self):
        subject.load_owner(b"sentinel = 'retained helper bytes'\n")
        self.assertEqual(subject.owner.sentinel,'retained helper bytes')
        self.assertFalse(hasattr(subject.owner,'prepare'))
        subject.load_owner(subject.OWNER.read_bytes())

if __name__=='__main__':unittest.main()
