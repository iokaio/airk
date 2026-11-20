# SPDX-License-Identifier: Apache-2.0
"""Model-free regression controls for the native scientific raw assembly."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parent))
from assemble import match_row, settled
from close_scores import validate_readout
from chronological_inputs import chronology


class ScientificJoinTests(unittest.TestCase):
    def test_original_cpu_preserves_all_refusals_and_eligible_card_order(self):
        from run_original_cpu import ORIGINAL,compiled_rows
        rows = [{"id":str(i),"ordinal":i,"status":"no_native_assessment_request"} for i in range(100)]
        cert = [{"id":r["id"],"source_group":"group","request":None} for r in rows]
        roster = {"cards":["A","B"]}
        census = {"profile":ORIGINAL,"compiled":{"input_ids":[3,9]},"fits_remaining_allowance":True}
        for i in (0,99):
            rows[i].update(roster=roster,remaining_ms=2,status="eligible",compiled=census["compiled"])
            cert[i].update(request={"roster":roster},remaining_ms_at_observation=2,compiler_census=[dict(census)])
        rows[99]["status"] = "compiler_refused_remaining_allowance"
        cert[99]["compiler_census"][0]["fits_remaining_allowance"] = False
        checked = compiled_rows(rows,cert)
        self.assertEqual(checked[0]["status"],"compiled_original_input")
        self.assertEqual(checked[1]["status"],"no_native_assessment_request")
        self.assertEqual(checked[99]["status"],"compiler_refused_remaining_allowance")
        self.assertTrue(all("scores" not in r for r in checked))
        with self.assertRaises(ValueError):
            compiled_rows(rows[:-1],cert[:-1])
        rows[0]["compiled"] = {"input_ids":[9,3]}
        with self.assertRaises(ValueError):
            compiled_rows(rows,cert)

    def setUp(self):
        self.compiled={"id":"A","source_group":"source","compiled":{"input_ids":[4,5],"candidate_positions":[1]},
                       "status":"compiled_original_input","roster":{"cards":["A"]},"remaining_ms":2}
        self.base={"id":"A","source_group":"source","split":"calibration","scores":[.3],"status":"raw_scores_only",
                   "roster":self.compiled["roster"],"compiled":self.compiled["compiled"],"compile_us":500,"inference_us":1500}

    def test_native_return_preserves_deadline_and_all_card_scores(self):
        validate_readout(self.base,self.compiled,self.compiled,1)
        for change in ({"inference_us":1501},{"scores":[]},{"scores":[float("nan")]},{"id":"B"},
                       {"compiled":{"input_ids":[5,4],"candidate_positions":[1]}}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_readout(self.base|change,self.compiled,self.compiled,1)

    def test_independent_reference_rejects_wrong_prompt_and_card_order(self):
        reference={"id":"A","compiled":self.compiled["compiled"],"status":"eligible","roster":self.compiled["roster"],
                   "original_fp32":[.3],"matching_fp16":[.25]}
        self.assertEqual(match_row(self.compiled,reference)["matching_fp16"],[.25])
        for change in ({"compiled":{"input_ids":[5,4]}},{"roster":{"cards":["B"]}},{"original_fp32":[]},{"matching_fp16":[float("inf")]}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                match_row(self.compiled,reference|change)

    def test_chronological_input_retains_every_original_ordinal(self):
        def reader(phase, *, model, retain_budget):
            self.assertEqual(model, "large")
            self.assertTrue(retain_budget)
            return [{"ordinal": i, "id": str(i), "status": "eligible" if i % 2 else "no_native_assessment_request",
                     "compiled": {"input_ids": [i]}, "roster": {"cards": [str(i)]}}
                    for i in range(8 * phase, min(8 * phase + 8, 100))]
        result = chronology(list(range(13)), "large", 99, reader)
        self.assertEqual(result["original_requests"], 100)
        self.assertEqual(result["statuses"], {"eligible": 50, "no_native_assessment_request": 50})
        self.assertEqual(result["first_eligible_ordinal"], 1)
        self.assertEqual(result["selected"]["ordinal"], 99)
        for phases in ([1], [0, 0], [0, 2]):
            with self.subTest(phases=phases), self.assertRaisesRegex(ValueError, "phase"):
                chronology(phases, "large", reader=reader)
        with self.assertRaisesRegex(ValueError, "outside"):
            chronology([0], "large", 8, reader)

    def test_owned_natural_settlement(self):
        job={"exit_code":0,"timed_out":False,"log_oversized":False,"child_assigned_before_resume":True,
             "owned_descendants_settled":True,"owned_job_forced_termination":False,
             "owned_job_final_active_processes":0,"owned_job_final_member_pids":[]}
        self.assertTrue(settled(job))
        for change in ({"exit_code":False},{"owned_job_final_member_pids":[17]},{"owned_job_forced_termination":True}):
            self.assertFalse(settled(job|change))


if __name__=="__main__":
    unittest.main()
