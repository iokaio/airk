# SPDX-License-Identifier: Apache-2.0
"""Bookkeeping/scope controls on retained Rust-emitted cases and explicit mutations.
Not a Python reimplementation of the native semantic validator.
"""
import copy
import json
from pathlib import Path
import unittest
from selected_stage_observations import observe,check_status
ROOT=Path(__file__).resolve().parents[5]
def fixture(name):return json.loads((ROOT/"fixtures/split/arbiter"/(name+".json")).read_bytes())
class NativeViewTests(unittest.TestCase):
    def test_exact_Context2_history_and_current_coverage_retained(self):
        raw=fixture("context_action_cases")["cases"][0]["input"];before=copy.deepcopy(raw)
        profile=raw["assessment_history"][0]["result"]["advice"]["profile"]
        result=observe(raw,"answer",profile,generic_read_only=False)
        self.assertIs(result["Context_history"],raw["assessment_history"])
        self.assertIs(result["Context_current_coverage"],raw["assessment"]["coverage"])
        self.assertEqual(result["known_input_tokens"],100);self.assertEqual(raw,before)
    def test_Sufficient_never_admitted_by_generic_Read_qualification(self):
        raw=fixture("context_action_cases")["cases"][0]["input"]
        profile=raw["assessment_history"][0]["result"]["advice"]["profile"]
        with self.assertRaisesRegex(ValueError,"unqualified semantic"):observe(raw,"answer",profile)
    def test_Context_profile_substitution_refuses(self):
        raw=fixture("context_action_cases")["cases"][0]["input"]
        with self.assertRaisesRegex(ValueError,"substitution"):observe(raw,"answer","ff"*32)
    def test_Context_off_rejects_assessments(self):
        raw=fixture("context_action_cases")["cases"][0]["input"]
        with self.assertRaisesRegex(ValueError,"Context-off"):observe(raw,"answer")
    def test_Focus_only_independent_known_work_and_no_semantic_authority(self):
        raw=fixture("focus_cases")["observation_receipt"];before=copy.deepcopy(raw)
        profile=raw["receipt"]["identity"]["profile"]
        out=observe({"focus":raw},"search",focus_profile=profile)
        self.assertEqual(out["known_input_tokens"],90);self.assertEqual(out["Context_history"],[])
        self.assertIs(out["Focus_observations"][0],raw);self.assertEqual(raw,before)
    def test_Focus_off_rejects_receipt(self):
        with self.assertRaisesRegex(ValueError,"Focus-off"):observe({"focus":fixture("focus_cases")["observation_receipt"]},"search")
    def test_Focus_unknown_retained_not_zero(self):
        raw=fixture("focus_cases")["observation_receipt"];raw["receipt"]["observed_input_tokens"]=None
        out=observe({"focus":raw},"search",focus_profile=raw["receipt"]["identity"]["profile"])
        self.assertTrue(out["work_unknown"]);self.assertEqual(out["known_input_tokens"],90)
    def test_compact_not_applied_is_not_unknown_forward(self):
        out=observe({"focus":fixture("focus_cases")["compact_not_applied"]},"search",focus_profile="07"*32)
        self.assertFalse(out["work_unknown"]);self.assertEqual(out["known_input_tokens"],0)
    def test_Status2_exact_disabled_roster(self):
        raw=fixture("stage_cases")["cases"][0]["status"]
        self.assertIs(check_status(raw),raw)
    def test_Status2_stage_consent_drift_refuses(self):
        raw=fixture("stage_cases")["cases"][0]["status"];raw["configuration"]["focus"]["enabled"]=True
        with self.assertRaisesRegex(ValueError,"consent"):check_status(raw)
    def test_neural_observation_only_from_actual_Search_records(self):
        raw=fixture("context_action_cases")["cases"][0]["input"]
        raw["records"][0]["operation"]="read_evidence"
        raw["records"][0]["canonical_result"]["focus"]=fixture("focus_cases")["observation_receipt"]
        out=observe(raw,"answer",context_profile=raw["assessment_history"][0]["result"]["advice"]["profile"],generic_read_only=False)
        self.assertEqual(out["Focus_observations"],[])
if __name__=="__main__":unittest.main()
