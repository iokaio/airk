# SPDX-License-Identifier: Apache-2.0
"""Cheap policy/refusal controls only; no process/service/model launch."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import unittest
B=Path(__file__).parent
S=importlib.util.spec_from_file_location('private_focus_outer',B/'run_owned_public_focus_r3.py')
M=importlib.util.module_from_spec(S);S.loader.exec_module(M)


def owner_success():
    return {'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,
            'owned_job_forced_termination':False,'owned_job_final_active_processes':0,
            'owned_job_final_member_pids':[],'child_assigned_before_resume':True,'parent_assigned_to_job':False}


def native_success():
    h={'user':M.SID,'executable_sha256':'a'*64,'runtime_sha256':'b'*64,'version':'1.8.207',
       'instance':'1'*32,'process_id':1234,'build':{'source_sha256':'c'*64}}
    second=deepcopy(h);second.update(instance='2'*32,process_id=5678)
    values={'actual-start':h,'actual-restart-health':second}
    for label,health in [('first',h),('final',second)]:
        values[label+'-native-stop']={'status':'lifecycle','record':{'phase':'complete','request':{
            'action':'stop','expected_instance':health['instance'],'runtime':health['runtime_sha256'],'manager':False}}}
        values[label+'-process-exit']={'process_handle_signaled':True,'exit_code':0,'pid':health['process_id']}
    receipt={'passed':True,'native_nn_calls_claimed':True,'release_qualified':False,
        'large_public_finalize_exercised':False,'source_files_unchanged':True,'import_source_files_unchanged':True,
        'actual_native_stop':deepcopy(values['final-native-stop']),
        'actual_controller_children':[{'pid':n,'exit_code':0} for n in range(7000,7300)]}
    return receipt,values,{'airk_sha256':'a'*64,'source_sha256':'c'*64}


class OuterTests(unittest.TestCase):
    def test_natural_multi_descendant_empty_job_proof(self):
        self.assertTrue(M.natural_owner(owner_success()))
        for field,value in [('exit_code',1),('timed_out',True),('log_oversized',True),
            ('owned_descendants_settled',False),('owned_job_forced_termination',True),
            ('owned_job_final_active_processes',1),('owned_job_final_member_pids',[999]),
            ('child_assigned_before_resume',False),('parent_assigned_to_job',True),('cleanup_refusal','unknown')]:
            altered=owner_success();altered[field]=value
            with self.subTest(field=field):self.assertFalse(M.natural_owner(altered))
        self.assertFalse(M.natural_owner(None))

    def test_two_actual_stop_exit_and_restart_proofs_accept_many_cli_children(self):
        report,observed,pins=native_success()
        self.assertTrue(M.validate_native_observations(report,observed,pins)['native_stop_and_original_process_exits'])

    def test_native_failure_cannot_be_upgraded_by_an_empty_job(self):
        for mutation in ('failed','no_work','release_claim','large_claim','source_mutated','import_mutated',
                'controller_unknown','controller_failed','cleanup_unknown','wrong_user','wrong_image','same_instance',
                'same_pid','wrong_source','first_stop_pending','final_stop_foreign','first_exit_unknown','final_exit_failed'):
            with self.subTest(mutation=mutation):
                report,observed,pins=native_success()
                if mutation=='failed':report['passed']=False
                if mutation=='no_work':report['native_nn_calls_claimed']=False
                if mutation=='release_claim':report['release_qualified']=True
                if mutation=='large_claim':report['large_public_finalize_exercised']=True
                if mutation=='source_mutated':report['source_files_unchanged']=False
                if mutation=='import_mutated':report['import_source_files_unchanged']=False
                if mutation=='controller_unknown':report['actual_controller_children'][0]['exit_code']=None
                if mutation=='controller_failed':report['actual_controller_children'][0]['exit_code']=1
                if mutation=='cleanup_unknown':report['cleanup_failure']={'message':'unknown'}
                if mutation=='wrong_user':observed['actual-restart-health']['user']='other-SID'
                if mutation=='wrong_image':observed['actual-restart-health']['executable_sha256']='d'*64
                if mutation=='same_instance':observed['actual-restart-health']['instance']='1'*32
                if mutation=='same_pid':observed['actual-restart-health']['process_id']=1234
                if mutation=='wrong_source':observed['actual-start']['build']['source_sha256']='d'*64
                if mutation=='first_stop_pending':observed['first-native-stop']['record']['phase']='pending'
                if mutation=='final_stop_foreign':observed['final-native-stop']['record']['request']['expected_instance']='3'*32
                if mutation=='first_exit_unknown':observed['first-process-exit']['process_handle_signaled']=False
                if mutation=='final_exit_failed':observed['final-process-exit']['exit_code']=77
                self.assertTrue(M.natural_owner(owner_success()))
                with self.assertRaises(ValueError):M.validate_native_observations(report,observed,pins)

    def test_no_refresh_or_native_deadline_relaxation(self):
        for key,value in [('whole_phase_seconds',1801),('native_read_ceiling_seconds',121)]:
            card={'schema':'airk.private-public-focus-outer-card/1','whole_phase_seconds':1800,
                  'native_read_ceiling_seconds':120,'owned_entrypoint':'run_owned'}
            card[key]=value
            with self.assertRaises(ValueError):M.validate_card(card)

    def test_unchanged_maintained_owner_imports_with_exact_lock_closure(self):
        owner=M.load(B/'frozen-owner-source/scripts/provision-static-onnx-runtime.py')
        self.assertTrue(callable(owner.run_owned))
        self.assertTrue(callable(owner.run_windows_owned))
        self.assertEqual(owner.MAX_LOG,64*1024*1024)

if __name__=='__main__':unittest.main()