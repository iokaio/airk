# SPDX-License-Identifier: Apache-2.0
import unittest
from launch_owned import finalize_receipt
class FinalClockControls(unittest.TestCase):
    def owned(self,exit_code=0):return{'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'retained_member_exits':[{'exit_observed':True,'exit_code':exit_code}],'exit_code':exit_code,'timed_out':False,'log_oversized':False,'owned_total_processes':1,'peak_host_committed_bytes':12}
    def plan(self):return{'worker_output':'D:/owned/worker','log_dir':'D:/owned','execution_sources':{'last.py':{'path':'D:/last.py','sha256':'sourceSHA'}},'limits':{'wall_seconds':120,'host_committed_bytes':100}}
    def finalize(self,hash_file,clock,owned=None,failure=None):return finalize_receipt(self.owned()if owned is None else owned,{'complete':True},self.plan(),'D:/plan.json','planSHA',0,['actual-argv'],{'sid':'human1002'},failure,120,hash_file=hash_file,clock=clock)
    def test_injected_slow_final_source_hash_is_charged_before_last_clock(self):
        ticks=[119.];events=[]
        def hash_file(path):
            path=str(path);events.append(path)
            if path.endswith('last.py'):ticks[0]+=2;return'sourceSHA'
            return'planSHA'if path.endswith('plan.json')else'observed'
        def clock():events.append('FINAL_CLOCK');return ticks[0]
        receipt=self.finalize(hash_file,clock);self.assertEqual(receipt['elapsed_seconds'],121.);self.assertFalse(receipt['disposition']['passed']);self.assertEqual(events[-1],'FINAL_CLOCK');self.assertTrue(receipt['disposition']['naturally_settled'])
    def test_injected_slow_result_hash_is_not_hidden_by_pre_hash_elapsed(self):
        ticks=[0.]
        def hash_file(path):
            path=str(path)
            if path.endswith('result.json'):ticks[0]=120.
            return'sourceSHA'if path.endswith('last.py')else'planSHA'if path.endswith('plan.json')else'observed'
        receipt=self.finalize(hash_file,lambda:ticks[0]);self.assertEqual(receipt['elapsed_seconds'],120.);self.assertFalse(receipt['disposition']['passed'])
    def test_hash_error_preserves_original_nonzero_failure_and_still_samples_clock(self):
        failure={'type':'OriginalChildFailure','message':'retained actual failure'};ticks=[]
        def hash_file(path):
            if str(path).endswith('stdout.log'):raise OSError('injected final log hash failure')
            return'sourceSHA'if str(path).endswith('last.py')else'planSHA'if str(path).endswith('plan.json')else'observed'
        receipt=self.finalize(hash_file,lambda:ticks.append('sample')or 3.,owned=self.owned(7),failure=failure)
        self.assertIs(receipt['failure'],failure);self.assertEqual(receipt['owned']['exit_code'],7);self.assertTrue(receipt['disposition']['known_natural_nonzero']);self.assertFalse(receipt['disposition']['passed']);self.assertEqual(len(receipt['final_hash_failures']),1);self.assertEqual(ticks,['sample'])
    def test_parent_failure_after_natural_success_cannot_claim_pass(self):
        failure={'type':'InterruptedAfterWorkerRead','message':'actual parent failure'}
        def hash_file(path):return'sourceSHA'if str(path).endswith('last.py')else'planSHA'if str(path).endswith('plan.json')else'observed'
        receipt=self.finalize(hash_file,lambda:3.,failure=failure);self.assertFalse(receipt['disposition']['passed']);self.assertTrue(receipt['disposition']['naturally_settled']);self.assertFalse(receipt['disposition']['known_natural_nonzero']);self.assertIs(receipt['failure'],failure)
    def test_source_mismatch_fails_without_inventing_unsettled_process(self):
        def hash_file(path):return'planSHA'if str(path).endswith('plan.json')else'wrongsource'if str(path).endswith('last.py')else'observed'
        receipt=self.finalize(hash_file,lambda:2.);self.assertFalse(receipt['disposition']['passed']);self.assertTrue(receipt['disposition']['naturally_settled']);self.assertFalse(receipt['disposition']['ownership_unknown_or_unsettled'])
    def test_fast_complete_hashes_can_pass_with_explicit_receipt_write_exclusion(self):
        def hash_file(path):return'sourceSHA'if str(path).endswith('last.py')else'planSHA'if str(path).endswith('plan.json')else'observed'
        receipt=self.finalize(hash_file,lambda:3.);self.assertTrue(receipt['disposition']['passed']);self.assertEqual(len(receipt['final_hashes']),4);self.assertIn('execution.json serialization/write',receipt['timing_scope'])
if __name__=='__main__':unittest.main(verbosity=2)
