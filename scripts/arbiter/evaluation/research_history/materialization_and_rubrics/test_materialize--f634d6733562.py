# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location("materialize",Path(__file__).with_name("materialize.py"))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class SelectionControls(unittest.TestCase):
    def setUp(self):self.members={q:("component-"+q,frozenset(["article-"+q]))for q in ("a","b","c","d")}
    def convert(self,row,allowed):
        if row.get("malformed"):raise ValueError("synthetic malformed tail")
        docs={"doc"+str(i):{"synthetic":i}for i in range(30)}
        return {"id":"hotpot:"+row['id']},docs,[],row.get('eligible',True)
    def rows(self,wanted):return ({'id':qid}for qid in reversed(wanted))
    def test_original_A0_goal_domain_and_order_not_physical_order(self):
        _,tasks,audit,counts=m.select('CONFIRMATION',self.members,self.rows,self.convert,2)
        self.assertEqual([t['id']for t in tasks],['hotpot:a','hotpot:c'])
        self.assertEqual(counts['projected'],4);self.assertEqual(counts['unselected_validated_tail'],2)
        self.assertEqual([t['experiment_partition']for t in tasks],['CONFIRMATION','CONFIRMATION'])
    def test_malformed_projected_tail_refuses_before_quota_return(self):
        def rows(wanted):return ({'id':q,'malformed':q=='b'}for q in wanted)
        with self.assertRaisesRegex(ValueError,'malformed tail'):m.select('CONFIRMATION',self.members,rows,self.convert,1)
    def test_only_mechanical_eligibility_can_skip_frozen_order(self):
        def rows(wanted):return ({'id':q,'eligible':q!='a'}for q in wanted)
        _,tasks,_,counts=m.select('CONFIRMATION',self.members,rows,self.convert,2)
        self.assertEqual([t['id']for t in tasks],['hotpot:c','hotpot:d']);self.assertEqual(counts['mechanically_ineligible'],1)
    def test_missing_or_duplicate_projection_refuses(self):
        for rows in (lambda wanted:[{'id':'a'}],lambda wanted:[{'id':'a'},{'id':'a'}]):
            with self.assertRaises(ValueError):m.select('CONFIRMATION',self.members,rows,self.convert,1)
    def test_cross_question_source_version_conflict_refuses(self):
        def convert(row,allowed):
            task,docs,excluded,eligible=self.convert(row,allowed);docs['doc0']={'different':row['id']};return task,docs,excluded,eligible
        with self.assertRaisesRegex(ValueError,'conflicting original source'):m.select('CONFIRMATION',self.members,self.rows,convert,2)
if __name__=='__main__':unittest.main()
