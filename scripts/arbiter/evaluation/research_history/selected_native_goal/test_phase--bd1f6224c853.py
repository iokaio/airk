# SPDX-License-Identifier: Apache-2.0
import copy,unittest
from pathlib import Path
from run_phase_reviewed import natural
class CompletionProofTests(unittest.TestCase):
    def image(self):return {'path':str(Path('synthetic-native.exe').resolve()),'sha256':'01'*32}
    def value(self):
        exits=[{'exit_observed':True,'exit_code':0}]
        return {'owned_leader_pid':123,'suspended_assignment':{'members':[123],'active':1,'total':1,'image':self.image()},'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_member_pids':[],'owned_job_final_active_processes':0,'owned_total_processes':1,'retained_member_exits':exits,'child_assigned_before_resume':True,'parent_assigned_to_job':False,'whole_process_deadline_not_refreshed':True,'handle_close_proof':{'forced':False,'settled':True,'final_member_pids':[],'final_active':0,'retained_exits':copy.deepcopy(exits)}}
    def test_natural_one_native_process(self):self.assertTrue(natural(self.value(),'fit',self.image()))
    def test_natural_two_python_launcher_members(self):
        v=self.value();v['owned_total_processes']=2;v['retained_member_exits']*=2;v['handle_close_proof']['retained_exits']*=2;self.assertTrue(natural(v,'certificate',self.image()))
    def test_unobserved_member_refuses(self):
        v=self.value();v['retained_member_exits'][0]['exit_observed']=False;self.assertFalse(natural(v,'certificate',self.image()))
    def test_forced_or_incomplete_handle_close_refuses(self):
        for key,value in [('forced',True),('settled',False),('final_active',1)]:
            v=self.value();v['handle_close_proof'][key]=value;self.assertFalse(natural(v,'certificate',self.image()))
    def test_third_process_refuses(self):
        v=self.value();v['owned_total_processes']=3;self.assertFalse(natural(v,'certificate',self.image()))
    def test_native_fit_extra_child_refuses(self):
        v=self.value();v['owned_total_processes']=2;v['retained_member_exits']*=2;v['handle_close_proof']['retained_exits']*=2;self.assertFalse(natural(v,'fit',self.image()))
    def test_false_initial_assignment_or_image_refuses(self):
        for field in ('members','image'):
            v=self.value()
            if field=='members':v['suspended_assignment']['members']=[999]
            else:v['suspended_assignment']['image']['sha256']='02'*32
            self.assertFalse(natural(v,'fit',self.image()))
    def test_timeout_cannot_be_complete(self):
        v=self.value();v['timed_out']=True;self.assertFalse(natural(v,'certificate',self.image()))
if __name__=='__main__':unittest.main()
