# SPDX-License-Identifier: Apache-2.0
"""Input-only tests for chronological R5 admission; no model is loaded."""
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parent))
import r5_chronological_inputs as owner


class ChronologicalTests(unittest.TestCase):
    def setUp(self):
        self.case={"ordinal":0,"question_id":"alpha","captures":[{"path":"synthetic"}]}
        self.census=[{"profile":manifest,"model":name,"NN_invocations":0,"fits_remaining_allowance":True,
                      "compiled":{"input_ids":[7,8],"candidate_positions":[1]}}
                     for name,manifest in owner.MODELS.values()]
        self.record={"Context_NN_dispatch":False,"Focus_configuration":"ON","actual_Context_profile":{"manifest":owner.ORIGINAL},
                     "remaining_ns_at_observation":3_200_000,"compiler_census":self.census,
                     "request":{"scope":"scope","question":"q","records":[1],"roster":{"cards":["one"]},
                                "remaining_ms":9,"remaining_input_tokens":3}}
        self.event={"id":"alpha","captured":[self.record],
                    "answer":{"refused":False,"body":{"scope":"scope","assessment":{"scope":"scope"},
                                                       "question":"q","records":[1,2]}}}

    def project(self, model="compact"):
        with patch.object(owner,"read",return_value=self.record):
            return owner.model_row(self.case,self.event,model)

    def test_both_original_profiles_and_measured_remaining_allowance(self):
        for model in owner.MODELS:
            row=self.project(model)
            self.assertEqual((row["status"],row["remaining_ms"],row["remaining_input_tokens"]),("eligible",3,3))
            self.assertEqual(row["compiled"]["input_ids"],[7,8])

    def test_refused_ordinal_is_retained_for_one_model(self):
        self.census[1]["fits_remaining_allowance"]=False
        self.assertEqual(self.project("large")["status"],"compiler_refused_remaining_allowance")
        self.assertEqual(self.project()["status"],"eligible")
        self.record["remaining_ns_at_observation"]=10
        self.assertNotIn("compiled",self.project())

    def test_foreign_delivery_and_scored_or_missing_census_rejected(self):
        for mutate in (
                lambda: self.record.update(Context_NN_dispatch=True),
                lambda: self.event["answer"]["body"].update(question="changed"),
                lambda: self.record["request"].update(records=[3]),
                lambda: self.census[0].update(NN_invocations=1),
                lambda: self.census.pop(0),
                lambda: self.census[0]["compiled"].update(candidate_positions=[])):
            self.setUp()
            mutate()
            with self.assertRaises(ValueError):
                self.project()

    def test_no_original_request_survives_without_fake_tensors(self):
        self.case["captures"]=[]
        self.event["captured"]=[]
        self.assertEqual(self.project(),{"ordinal":0,"id":"alpha","captures":[],"status":"no_native_assessment_request"})

    def test_unsettled_terminal_and_missing_prefix_refuse(self):
        for lifetimes,terminal in (([],None),(["a","b"],None),(["b"],["o","w","m"]),(["a","b","c"],None)):
            with self.assertRaises(ValueError):
                owner.collect({},lifetimes,terminal)

    def test_two_model_prefix_preserves_all_56_no_request_rows_and_phases(self):
        ids = [[f"original-{8*n+i}" for i in range(8)] for n in range(7)]
        reports = []
        transcripts = {}
        for n, phase_ids in enumerate(ids):
            cases = [{"ordinal":8*n+i,"question_id":name,"captures":[]} for i,name in enumerate(phase_ids)]
            reports.append({"phase":n,"phase_owner":n,"plan_sha256":owner.PLAN,"input_audit_sha256":owner.AUDIT,
                            "certificate_count":0,"source_transcript":n,"rows":cases})
            events = [{"kind":"cal_question","evidence":{"id":name,"captured":[],"answer":{"refused":True}}}
                      for name in phase_ids]
            transcripts[n] = b"\n".join(json.dumps(e).encode() for e in events)
        with patch.object(owner,"checked",side_effect=transcripts.__getitem__):
            batches = owner.projections({"phase_ids":ids},reports,list(range(7)))
            for data in batches.values():
                self.assertEqual(data["original_requests"],56)
                self.assertEqual(data["statuses"],{"no_native_assessment_request":56})
                self.assertEqual(data["phases"],list(range(7)))
                self.assertIsNone(data["first_eligible_ordinal"])
            reports[4]["rows"][2]["ordinal"] -= 1
            with self.assertRaises(ValueError):
                owner.projections({"phase_ids":ids},reports,list(range(7)))

    def test_partial_reordered_or_duplicated_phases_refuse(self):
        for seq in (list(range(6)),[0,1,2,4,3,5,6],[0,1,2,3,4,5,5]):
            reports=[{"phase":n,"phase_owner":{}} for n in seq]
            with self.assertRaises(ValueError):
                owner.projections({},reports,seq)


if __name__=="__main__":
    unittest.main()
