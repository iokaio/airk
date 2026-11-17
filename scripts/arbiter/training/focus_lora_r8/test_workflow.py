# SPDX-License-Identifier: Apache-2.0
import copy,unittest
from workflow import continuity_key,validate_owned

def plan():
    result={k:{'path':'D:/'+k,'sha256':k+'SHA'}for k in('native_census','native_owned_receipt','cohort','replay','base_python','reference_module','reference_worker','ownership_module','human_identity_helper','worker')}
    result.update({'schema':'airk.private.F1-F2-owned-plan/1','human_sid':'human1002','repository_root':'D:/repo','versions':{'torch':'2.10.0'},'optimizer':{'rank':16,'lr':5e-5,'batch':32},'immutable_input_seals':{'model.safetensors':'weightsA','runtime.dll':'runtimeA'},'environment':{'PYTHONPATH':'sealed/runtime','TORCH_HOME':'out-score/torch','CUDA_CACHE_PATH':'out-score/cuda','CUBLAS_WORKSPACE_CONFIG':':4096:8'},'gpu':{'uuid':'gpuA','name':'A6000','driver_version':'driverA','compute_capability':'8.6','free_mib':9000},'limits':{'wall_seconds':600,'host_committed_bytes':16*1024**3,'cuda_allocator_bytes':8*1024**3,'minimum_cuda_free_bytes':8*1024**3},'mode':'score','selected_instruction':'publisher','general_replay_instruction':'publisher','log_dir':'out-score','worker_output':'out-score/worker','max_optimizer_steps':14})
    result['continuity_key']=continuity_key(result);return result

def evidence():
    p=plan();sha='exactPlanSHA';start={'plan':{'path':'D:/plan.json','sha256':sha},'identity':{'sid':'human1002'},'argv':['D:/base_python','-B','-S','D:/worker','--plan','D:/plan.json','--plan-sha256',sha]}
    image={'path':'D:/base_python','sha256':'base_pythonSHA'}
    owned={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'owned_total_processes':1,'child_assigned_before_resume':True,'parent_assigned_to_job':False,'whole_process_deadline_not_refreshed':True,'retained_member_exits':[{'exit_observed':True,'exit_code':0}],'owned_leader_pid':42,'suspended_assignment':{'single_suspended_worker':42,'parent_in_child_job':False,'image':copy.deepcopy(image)},'before_GO':{'same_actual_leader_gate':42,'members':[42],'image':copy.deepcopy(image)},'peak_host_committed_bytes':1000}
    execution={'schema':'airk.private.F1-F2-owned-execution/1','plan_sha256':sha,'identity':{'sid':'human1002'},'argv':start['argv'],'owned':owned,'failure':None,'disposition':{'passed':True,'naturally_settled':True,'ownership_unknown_or_unsettled':False},'elapsed_seconds':20}
    worker={'mode':'score','plan_sha256':sha,'complete':True,'native_census_sha256':'native_censusSHA','peak_cuda_allocated_bytes':1000,'peak_cuda_reserved_bytes':2000,'forward_attempts':170,'forward_calls':170,'optimizer_attempts':0,'gradient_steps':0,'backward_attempts':0,'backwards_completed':0}
    return execution,start,p,worker,sha
class ContinuationControls(unittest.TestCase):
    def test_real_owned_score_predicates_positive(self):
        e,s,p,w,sha=evidence();self.assertEqual(validate_owned(e,s,p,w,sha,'score'),continuity_key(p))
    def test_score_probe_stage_outputs_wall_and_chosen_recipe_can_differ(self):
        score=plan();probe=copy.deepcopy(score);probe.update({'mode':'probe','max_optimizer_steps':1,'selected_instruction':'software_evidence_v1','log_dir':'other','worker_output':'other/worker','selection':{'sha256':'actual-choice'}});probe['limits']['wall_seconds']=120;probe['environment']['TORCH_HOME']='other/torch';probe['environment']['CUDA_CACHE_PATH']='other/cuda';probe['gpu']['free_mib']=8000
        self.assertEqual(continuity_key(score),continuity_key(probe))
    def test_same_census_different_checkpoint_runtime_source_or_owner_refuses_continuation(self):
        original=plan();key=continuity_key(original)
        for field in('reference_module','reference_worker','ownership_module','base_python','human_identity_helper','worker','cohort','replay','native_owned_receipt'):
            bad=copy.deepcopy(original);bad[field]['sha256']='different-'+field;self.assertNotEqual(continuity_key(bad),key,field)
        for path in('model.safetensors','runtime.dll'):
            bad=copy.deepcopy(original);bad['immutable_input_seals'][path]='different';self.assertNotEqual(continuity_key(bad),key,path)
    def test_caps_common_environment_driver_and_optimizer_are_bound(self):
        original=plan();key=continuity_key(original)
        for category,field,value in[('environment','PYTHONPATH','different/runtime'),('gpu','uuid','otherGPU'),('gpu','driver_version','otherdriver'),('limits','cuda_allocator_bytes',4*1024**3),('optimizer','rank',32)]:
            bad=copy.deepcopy(original);bad[category][field]=value;self.assertNotEqual(continuity_key(bad),key)
    def test_boolean_pass_does_not_override_actual_exit_or_handle(self):
        for mutate in('nonzero','unobservedHANDLE','wrongImage','differentActor','unknownSubmission','overcap'):
            e,s,p,w,sha=evidence()
            if mutate=='nonzero':e['owned']['exit_code']=3
            elif mutate=='unobservedHANDLE':e['owned']['retained_member_exits'][0]['exit_observed']=False
            elif mutate=='wrongImage':e['owned']['before_GO']['image']['sha256']='otherimage'
            elif mutate=='differentActor':e['identity']['sid']='otherhuman'
            elif mutate=='unknownSubmission':w['forward_attempts']+=1
            else:e['owned']['peak_host_committed_bytes']=p['limits']['host_committed_bytes']+1
            with self.assertRaises(ValueError,msg=mutate):validate_owned(e,s,p,w,sha,'score')
    def test_changed_plan_key_cannot_relabel_original_score(self):
        e,s,p,w,sha=evidence();p['reference_module']['sha256']='othercheckpoint'
        with self.assertRaisesRegex(ValueError,'immutable'):validate_owned(e,s,p,w,sha,'score')
    def test_site_enabled_child_argv_cannot_be_accepted_as_current(self):
        e,s,p,w,sha=evidence()
        s['argv'].remove('-S');e['argv']=list(s['argv'])
        with self.assertRaisesRegex(ValueError,'actual command differs'):validate_owned(e,s,p,w,sha,'score')
    def test_probe_requires_exact1update32draws_not_zero_work(self):
        e,s,p,w,sha=evidence();p['mode']='probe';p['limits']['wall_seconds']=120;p['max_optimizer_steps']=1;p['continuity_key']=continuity_key(p);w.update({'mode':'probe','gradient_steps':1,'optimizer_attempts':1,'forward_attempts':32,'forward_calls':32,'backward_attempts':32,'backwards_completed':32,'probe_adapter_discarded':True})
        validate_owned(e,s,p,w,sha,'probe');w['gradient_steps']=0;w['optimizer_attempts']=0
        with self.assertRaisesRegex(ValueError,'one-update'):validate_owned(e,s,p,w,sha,'probe')
if __name__=='__main__':unittest.main(verbosity=2)
