# SPDX-License-Identifier: Apache-2.0
"""Actual retained observation parsing + independent refusal mutations; zero NN."""
import copy,json,unittest
from pathlib import Path
from prepare_post_index import ROOT,validate_setup_outcome
BASE=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102'
class Tests(unittest.TestCase):
    def setUp(self):
        self.observed=json.loads((BASE/'postfix-observed-setup-state/observation-r2.json').read_bytes())
        self.packet=BASE/'setup-postfix-r122.proposed.json';self.diagnostics=BASE/'postfix-desired-background-diagnostics.log'
    def test_actual_observed_state_preserves_original_failed_wrapper(self):
        self.assertGreater(len(validate_setup_outcome(self.observed,self.packet,self.diagnostics)),26)
        original=json.loads(Path(self.observed['original_failed_execution']['path']).read_bytes())
        self.assertIs(original['complete'],False)
        self.assertEqual(self.observed['commands'][0],original['commands'][0])
    def test_original_failed_execution_cannot_pass_as_complete_setup(self):
        original=json.loads(Path(self.observed['original_failed_execution']['path']).read_bytes())
        with self.assertRaises(ValueError):validate_setup_outcome(original,self.packet,self.diagnostics)
    def test_opaque_complete_flag_is_not_observed_evidence(self):
        value={k:self.observed[k] for k in ('phase','packet_sha256','complete')}
        with self.assertRaises(ValueError):validate_setup_outcome(value,self.packet,self.diagnostics)
    def test_missing_or_duplicate_background_job_refuses(self):
        for change in ('missing','duplicate'):
            value=copy.deepcopy(self.observed)
            if change=='missing':value['background_jobs']=[x for x in value['background_jobs'] if x['kind']!='INDEX']
            else:value['background_jobs'].append(copy.deepcopy(value['background_jobs'][0]))
            with self.assertRaises(ValueError):validate_setup_outcome(value,self.packet,self.diagnostics)
    def test_changed_original_setup_or_job_identity_refuses(self):
        for target in ('commands','background_jobs'):
            value=copy.deepcopy(self.observed)
            if target=='commands':value[target][0]['label']='invented'
            else:value[target][0]['job_id']='invented'
            with self.assertRaises(ValueError):validate_setup_outcome(value,self.packet,self.diagnostics)
    def test_foreign_diagnostics_refuses(self):
        with self.assertRaises(ValueError):validate_setup_outcome(self.observed,self.packet,self.packet)
if __name__=='__main__':unittest.main()
