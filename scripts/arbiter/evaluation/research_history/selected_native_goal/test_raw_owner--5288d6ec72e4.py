# SPDX-License-Identifier: Apache-2.0
import copy
from pathlib import Path
import unittest
import tempfile
import run_raw_reviewed as owner
class RawOwnerTests(unittest.TestCase):
    def image(self):return {'path':str(Path('synthetic-native-kernel.exe').resolve()),'sha256':'01'*32}
    def natural(self):
        return {'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'unexpected_third_process':False,'owned_total_processes':1,'owned_job_final_member_pids':[],'owned_job_final_active_processes':0,'retained_member_exits':[{'exit_observed':True,'exit_code':0}],'child_assigned_before_resume':True,'parent_assigned_to_job':False,'whole_process_deadline_not_refreshed':True,'owned_leader_pid':123,'suspended_assignment':{'members':[123],'active':1,'total':1,'image':self.image()},'handle_close_proof':{'forced':False,'settled':True,'final_member_pids':[],'final_active':0,'retained_exits':[{'exit_observed':True,'exit_code':0}]}}
    def test_exact_natural_one_process_accepts(self):self.assertTrue(owner.natural(self.natural(),self.image()))
    def test_forced_close_cannot_pass(self):
        value=self.natural();value['owned_job_forced_termination']=True;self.assertFalse(owner.natural(value,self.image()))
    def test_leader_exit_does_not_prove_empty_job(self):
        value=self.natural();value['owned_job_final_member_pids']=[123];value['owned_job_final_active_processes']=1;self.assertFalse(owner.natural(value,self.image()))
    def test_unobserved_retained_member_cannot_pass(self):
        value=self.natural();value['retained_member_exits'][0]['exit_observed']=False;self.assertFalse(owner.natural(value,self.image()))
    def test_missing_assignment_or_deadline_proof_refuses(self):
        for key in ('child_assigned_before_resume','parent_assigned_to_job','whole_process_deadline_not_refreshed'):
            value=self.natural();del value[key];self.assertFalse(owner.natural(value,self.image()))
    def test_forced_handle_close_cannot_be_hidden(self):
        value=self.natural();value['handle_close_proof']['forced']=True;self.assertFalse(owner.natural(value,self.image()))
    def test_wrong_suspended_image_refuses(self):
        value=self.natural();value['suspended_assignment']['image']['sha256']='02'*32;self.assertFalse(owner.natural(value,self.image()))
    def test_missing_handle_close_proof_refuses(self):
        value=self.natural();del value['handle_close_proof'];self.assertFalse(owner.natural(value,self.image()))
    def test_large_cpu16_keeps_all_original_non_cpu_caps(self):
        self.assertEqual(owner.SCOPES['arbiter-context-qwen3-4b'],{'cpu':16,'reservation':40*1024**3,'host':48*1024**3,'seconds':3360})
    def header(self):
        packet={'binary':{'sha256':'01'*32},'raw_header':{'model':'actual-fixed-model','profile':'02'*32}}
        rows=[{'id':'synthetic:'+str(i),'split':'calibration'}for i in range(100)]
        value={'schema':'airk.arbiter.context-action.selected-cal-raw/1','complete':True,'task_qualified':False,'release_qualified':False,'human_semantic_gold_count':0,'test_binary_sha256':'01'*32,'model':'actual-fixed-model','profile':'02'*32,'rows':rows}
        return value,packet,{'rows':copy.deepcopy(rows)}
    def test_raw_header_is_not_process_or_qualification_proof(self):
        value,packet,preflight=self.header();owner.validate_raw_header(value,packet,preflight)
        value['task_qualified']=True
        with self.assertRaises(ValueError):owner.validate_raw_header(value,packet,preflight)
    def test_missing_row_or_foreign_image_refuses(self):
        for mutation in ('row','image'):
            value,packet,preflight=self.header()
            if mutation=='row':value['rows'].pop()
            else:value['test_binary_sha256']='03'*32
            with self.assertRaises(ValueError):owner.validate_raw_header(value,packet,preflight)
    def test_100_duplicate_or_foreign_ids_refuse(self):
        for kind in ('duplicate','foreign'):
            value,packet,preflight=self.header();value['rows'][0]['id']=value['rows'][1]['id']if kind=='duplicate'else 'foreign'
            with self.assertRaises(ValueError):owner.validate_raw_header(value,packet,preflight)
    def test_hash_duration_is_inside_original_phase(self):
        with tempfile.TemporaryDirectory()as root:
            p=Path(root)/'artifact';p.write_bytes(b'fixture');clock=[9.0]
            def hashing(path):clock[0]=11.0;return '01'*32
            digest,finished,expired=owner.finalize_artifact(p,10.0,lambda:clock[0],hashing)
            self.assertEqual(digest,'01'*32);self.assertEqual(finished,11.0);self.assertTrue(expired)
    def test_confirmation_cannot_become_CAL(self):
        value,packet,preflight=self.header();value['rows'][0]['split']='test'
        with self.assertRaises(ValueError):owner.validate_raw_header(value,packet,preflight)
if __name__=='__main__':unittest.main()
