# SPDX-License-Identifier: Apache-2.0
import copy,math,struct,unittest
import worker_logic as w
from launch_owned import disposition
from focus_worker import verify_plan

def row(id,role,label=1,component='c'):
    input={'input_ids':[0]*512,'attention_mask':[1]*512,'position_ids':list(range(512)),'candidate_positions':[2152,9693],'option_mask':[True,True],'decision_position':511,'input_digest':'ab'*32}
    return{'pair':{'id':id,'role':role,'label':label,'rights_scope':'PERMITTED_PRIVATE_RESEARCH_SCOPE'},'publisher':{'kind':'compiled','input':input},'software_evidence_v1':{'kind':'compiled','input':copy.deepcopy(input)},'component':component}
def schedule_rows():return{r['pair']['id']:r for r in [row('code'+str(i),'TRAIN',i%2)for i in range(400)]+[row('replay'+str(i),'REPLAY')for i in range(45)]}
def score_fixture(gain=True):
    rows={};scores=[]
    for i in range(85):
        id='dev'+str(i);label=i%2;r=row(id,'DEV',label,'component'+str(i%3));rows[id]=r
        observed={'id':id,'label':label}
        for recipe in('publisher','software_evidence_v1'):
            margin=(1 if label else-1)if recipe=='software_evidence_v1'or not gain else 0
            observed[recipe]={'status':'scored','input_digest':'ab'*32,'no_logit':0.,'yes_logit':float(margin)}
        scores.append(observed)
    return rows,{'complete':True,'mode':'score','rows':scores}

class PureWorkerControls(unittest.TestCase):
    def test_pure_import_does_not_import_torch(self):
        import sys
        self.assertNotIn('torch',sys.modules)
    def test_original_targets_not_two_class_indices(self):
        self.assertEqual(w.target(0),2152);self.assertEqual(w.target(1),9693)
        for v in(-1,2,True):
            with self.assertRaises(ValueError):w.target(v)
    def test_full_vocabulary_denominator_sees_unrelated_large_logit(self):
        logits=[0.]*10000;logits[9693]=2;logits[17]=20
        observed=w.full_vocab_ce(logits,1)
        binary=math.log(math.exp(0)+math.exp(2))-2
        self.assertGreater(observed,17.99);self.assertLess(binary,.13)
    def test_full_vocab_nonfinite_and_truncated_head_refuse(self):
        with self.assertRaises(ValueError):w.full_vocab_ce([0.,2.],1)
        logits=[0.]*10000;logits[5]=float('inf')
        with self.assertRaises(ValueError):w.full_vocab_ce(logits,1)
    def test_exact_native_input_inspection_no_pad_crop(self):
        w.check_input(row('a','TRAIN')['publisher']['input'])
        for field,value in[('position_ids',[0]*512),('input_ids',[0]*511),('candidate_positions',[9693,2152]),('decision_position',510),('attention_mask',[1]*511+[0])]:
            data=copy.deepcopy(row('a','TRAIN')['publisher']['input']);data[field]=value
            with self.assertRaises(ValueError):w.check_input(data)
    def test_one_epoch400plus45_and_exact14groups(self):
        data=schedule_rows();schedule=w.training_schedule(data,'publisher');self.assertEqual(len(schedule),445);self.assertEqual(len({r['pair']['id']for r in schedule}),445)
        groups=w.batches(schedule);self.assertEqual([len(x)for x in groups],[32]*13+[29]);self.assertEqual(sum(r['pair']['role']=='REPLAY'for r in schedule),45)
        self.assertEqual([r['pair']['role']for r in schedule[:10]],['TRAIN']*9+['REPLAY'])
    def test_order_independent_and_never_score_driven(self):
        one=schedule_rows();two=copy.deepcopy(dict(reversed(list(one.items()))))
        for i,r in enumerate(two.values()):r['score']=float(i);r['pair']['label']=1-r['pair']['label']
        self.assertEqual([r['pair']['id']for r in w.training_schedule(one,'publisher')],[r['pair']['id']for r in w.training_schedule(two,'publisher')])
    def test_no_native_overflow_refill_or_drop(self):
        rows=schedule_rows();rows['code0']['publisher']={'kind':'refused','reason':'overflow'}
        with self.assertRaises(ValueError):w.training_schedule(rows,'publisher')
        rows=schedule_rows();del rows['replay0']
        with self.assertRaises(ValueError):w.training_schedule(rows,'publisher')
    def test_general_replay_uses_original_publisher_recipe(self):
        rows=schedule_rows();rows['replay0']['software_evidence_v1']={'kind':'refused','reason':'software-domain overflow'}
        self.assertEqual(len(w.training_schedule(rows,'software_evidence_v1')),445)
        self.assertEqual(w.effective_recipe(rows['replay0'],'software_evidence_v1'),'publisher')
        self.assertEqual(w.effective_recipe(rows['code0'],'software_evidence_v1'),'software_evidence_v1')
    def test_unknown_rights_never_replay_loss(self):
        rows=schedule_rows();rows['replay0']['pair']['rights_scope']='UNKNOWN'
        with self.assertRaises(ValueError):w.training_schedule(rows,'publisher')
    def test_selection_strict_gain_and_no_component_regression(self):
        rows,report=score_fixture();choice=w.choose_instruction(report,rows);self.assertEqual(choice['selected'],'software_evidence_v1');self.assertFalse(choice['promotion'])
    def test_tie_retains_original(self):
        rows,report=score_fixture(False);self.assertEqual(w.choose_instruction(report,rows)['selected'],'publisher')
    def test_native_refusal_cannot_be_hidden_to_chooseF1(self):
        rows,report=score_fixture();rows['dev0']['software_evidence_v1']={'kind':'refused','reason':'overflow'};report['rows'][0]['software_evidence_v1']={'status':'native_refused','reason':'overflow'}
        self.assertEqual(w.choose_instruction(report,rows)['selected'],'publisher')
    def test_missing_unknown_submission_is_not_compiler_refusal(self):
        rows,report=score_fixture();report['rows'][0]['software_evidence_v1']={'status':'native_refused','reason':'unknown submission'}
        with self.assertRaises(ValueError):w.choose_instruction(report,rows)
    def test_missing_row_or_changed_identity_refuses(self):
        rows,report=score_fixture();report['rows'].pop()
        with self.assertRaises(ValueError):w.choose_instruction(report,rows)
        rows,report=score_fixture();report['rows'][0]['publisher']['input_digest']='ff'*32
        with self.assertRaises(ValueError):w.choose_instruction(report,rows)
    def owned(self,exit_code=0):return{'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'retained_member_exits':[{'exit_observed':True}],'exit_code':exit_code,'timed_out':False,'log_oversized':False,'owned_total_processes':1}
    def test_natural_nonzero_distinct_from_unsettled_not_pass(self):
        result=disposition(self.owned(1),{'complete':False});self.assertTrue(result['known_natural_nonzero']);self.assertTrue(result['naturally_settled']);self.assertFalse(result['passed']);self.assertFalse(result['ownership_unknown_or_unsettled'])
    def test_forced_unknown_and_child_roster_never_pass(self):
        owned=self.owned();owned['owned_job_forced_termination']=True;self.assertFalse(disposition(owned,{'complete':True})['passed']);self.assertTrue(disposition(None,None)['ownership_unknown_or_unsettled'])
        owned=self.owned();owned['owned_total_processes']=2;self.assertFalse(disposition(owned,{'complete':True})['passed'])
    def test_natural_success_requires_complete_worker(self):
        self.assertFalse(disposition(self.owned(),{'complete':False})['passed']);self.assertTrue(disposition(self.owned(),{'complete':True})['passed'])
    def test_original_caps_reject_before_import_or_gradients(self):
        plan={'schema':'airk.private.F1-F2-owned-plan/1','mode':'probe','human_sid':'S-1-5-21-3678370870-3858642417-826968105-1002','limits':{'wall_seconds':121,'host_committed_bytes':16*1024**3,'cuda_allocator_bytes':8*1024**3},'max_optimizer_steps':1}
        with self.assertRaises(ValueError):verify_plan(plan)
    def test_native_subset_cannot_masquerade_as_complete(self):
        with self.assertRaises(ValueError):w.native_rows({'schema':'airk.private.focus-instruction-emission/1','census':{'schema':'airk.private.focus-instruction-census/1','qualification':False,'model_requests_dispatched':False}},[],[])
if __name__=='__main__':unittest.main(verbosity=2)
