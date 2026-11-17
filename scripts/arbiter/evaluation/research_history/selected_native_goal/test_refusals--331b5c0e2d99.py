# SPDX-License-Identifier: Apache-2.0
import copy,hashlib,json,unittest
from pathlib import Path
from selected_cal_refusals import canonical_refusal,refusal_row
class RefusalRetentionTests(unittest.TestCase):
    def actual(self):
        p=Path(__file__).resolve().parents[2]/'actual-composed-service-r2'/'CAL-capture-postfix-r129'/'canonical-calls'/'call-0001'/'receipt.json'
        raw=p.read_bytes();self.assertEqual(hashlib.sha256(raw).hexdigest(),'5eb9a6d0a0525c97d6cb122f9aaf05afee9b4d675faa8f538270f6d0a9369e5a');return json.loads(raw)
    def test_actual_canonical_budget_refusal_recognized_without_success_relabel(self):
        call=self.actual();self.assertTrue(canonical_refusal(call));self.assertFalse(call['validated']);self.assertEqual(call['returncode'],1)
    def test_undelivered_admission_never_becomes_request_or_label(self):
        call=self.actual();row=refusal_row({'id':'synthetic-id','question':'synthetic question'},call,[{'path':'retained-observation','sha256':'01'*32}]);self.assertIsNone(row['request']);self.assertIsNone(row['admission']);self.assertEqual(row['counterfactual_choices'],[]);self.assertFalse(row['semantic_negative']);self.assertEqual(len(row['undelivered_native_observations']),1)
    def test_identity_parse_and_timeout_faults_are_fatal(self):
        for key,value in [('identity_error','foreign instance'),('parse_error','invalid JSON'),('timed_out',True)]:
            call=self.actual();call[key]=value;self.assertFalse(canonical_refusal(call))
    def test_generic_refusal_does_not_retry_or_create_label(self):
        call=self.actual();call['full_canonical_value']['error']['code']='other_canonical_error';call['full_canonical_value']['error']['retryable']=True
        self.assertTrue(canonical_refusal(call));row=refusal_row({'id':'synthetic','question':'synthetic'},call,[]);self.assertIsNone(row['request']);self.assertFalse(row['semantic_negative'])
    def test_malformed_error_cannot_continue(self):
        for key,value in [('code',''),('retryable',None),('message',12)]:
            call=self.actual();call['full_canonical_value']['error'][key]=value;self.assertFalse(canonical_refusal(call))
    def test_call_arguments_or_session_mismatch_fatal(self):
        call=self.actual();call['session_exchange']['arguments_sha256']='02'*32;self.assertFalse(canonical_refusal(call))
    def test_missing_original_health_observations_fatal(self):
        call=self.actual();call['actual_health_observations'].pop();self.assertFalse(canonical_refusal(call))
    def test_successful_context_is_not_a_retained_refusal(self):
        call=self.actual();call['returncode']=0;call['validated']=True;self.assertFalse(canonical_refusal(call))
if __name__=='__main__':unittest.main()
