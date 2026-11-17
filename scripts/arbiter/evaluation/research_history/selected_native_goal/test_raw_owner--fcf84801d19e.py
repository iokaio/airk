# SPDX-License-Identifier: Apache-2.0
import copy
import unittest
import run_raw_reviewed as owner
class RawOwnerTests(unittest.TestCase):
    def natural(self):return {'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'unexpected_third_process':False,'owned_total_processes':1,'owned_job_final_member_pids':[],'owned_job_final_active_processes':0,'retained_member_exits':[{'exit_observed':True,'exit_code':0}]}
    def test_exact_natural_one_process_accepts(self):self.assertTrue(owner.natural(self.natural()))
    def test_forced_close_cannot_pass(self):
        value=self.natural();value['owned_job_forced_termination']=True;self.assertFalse(owner.natural(value))
    def test_leader_exit_does_not_prove_empty_job(self):
        value=self.natural();value['owned_job_final_member_pids']=[123];value['owned_job_final_active_processes']=1;self.assertFalse(owner.natural(value))
    def test_unobserved_retained_member_cannot_pass(self):
        value=self.natural();value['retained_member_exits'][0]['exit_observed']=False;self.assertFalse(owner.natural(value))
    def header(self):
        packet={'binary':{'sha256':'01'*32},'raw_header':{'model':'actual-fixed-model','profile':'02'*32}}
        value={'schema':'airk.arbiter.context-action.selected-cal-raw/1','complete':True,'task_qualified':False,'release_qualified':False,'human_semantic_gold_count':0,'test_binary_sha256':'01'*32,'model':'actual-fixed-model','profile':'02'*32,'rows':[{}for _ in range(100)]}
        return value,packet
    def test_raw_header_is_not_process_or_qualification_proof(self):
        value,packet=self.header();owner.validate_raw_header(value,packet)
        value['task_qualified']=True
        with self.assertRaises(ValueError):owner.validate_raw_header(value,packet)
    def test_missing_row_or_foreign_image_refuses(self):
        for mutation in ('row','image'):
            value,packet=self.header()
            if mutation=='row':value['rows'].pop()
            else:value['test_binary_sha256']='03'*32
            with self.assertRaises(ValueError):owner.validate_raw_header(value,packet)
if __name__=='__main__':unittest.main()
