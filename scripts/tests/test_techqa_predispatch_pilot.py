# SPDX-License-Identifier: Apache-2.0
"""Independent controls for publisher-question reconstruction and pre-forward census."""
import copy
import io
import json
from pathlib import Path
import runpy
import unittest

HERE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
prepared = runpy.run_path(str(HERE / "prepare_techqa_predispatch_pilot.py"))
audit = runpy.run_path(str(HERE / "run_techqa_predispatch_pilot.py"))
reader = runpy.run_path(str(HERE / "audit_techqa_validation_metadata.py"))["MetadataReader"]


class PilotTests(unittest.TestCase):
    def test_only_selected_publisher_title_and_text_with_original_order(self):
        rows = [{"QUESTION_TITLE": "other", "QUESTION_ID": "ignored", "QUESTION_TEXT": "private",
                 "ANSWER": "gold", "DOC_IDS": []},
                {"QUESTION_TEXT": "β\nbody", "ANSWER": "other label", "DOC_IDS": ["b", "a"],
                 "QUESTION_TITLE": "Exact title", "QUESTION_ID": "Q"}]
        data = json.dumps(rows).encode()
        self.assertEqual("Exact title\n\nβ\nbody", prepared["selected_question"](
            reader(io.BytesIO(data), lambda: None), data, "Q", ["b", "a"]))
        with self.assertRaisesRegex(ValueError, "order"):
            prepared["selected_question"](reader(io.BytesIO(data), lambda: None), data, "Q", ["a", "b"])

    def setUp(self):
        self.request = {"question": "Original question", "budget": {"input_tokens": 8192, "deadline_ms": 120000}}
        self.binding = {"context": "compact", "focus": "focus", "pipeline": "mini", "publication": {"token": "rev"},
                        "scope_observation": {"selection": {"owner": "agent"}}, "index_id": "idx"}
        scope = {"publication": self.binding["publication"]}
        self.answer = {"question": "Original question", "request_id": "1" * 32, "records": [{"id": "item", "value": 3}], "assessment": {"scope": scope},
                       "scope_observation": self.binding["scope_observation"], "read_scope": {"source": "named", "index_ids": ["idx"]},
                       "assessment_history": [{"ordinal": 1, "basis": {"r": 1}, "result": {"advice": {
                           "input_digest": "input", "observed_input_tokens": 0, "work": {"input_tokens": 0},
                           "outcome": {"status": "unavailable"}}}}]}
        self.record = {"schema": "airk.private-selected-Context-native-admission-observation/1", "Context_NN_dispatch": False,
                       "Focus_configuration": "ON", "actual_Context_profile": {"manifest": "compact"},
                       "actual_Focus_profile": "focus", "MiniLM_pipeline": "mini", "known_Focus_input_tokens": 100,
                       "planned_input_digest": "input", "verified_probe_requests": [], "actual_Focus_forward_observed": True,
                       "request": {"question": "Original question", "read_attempt": {"request_id": "2" * 32}, "scope": scope,
                                   "records": [{"id": "item", "value": 3}],
                                   "ordinal": 1, "parent_budget": self.request["budget"], "remaining_input_tokens": 8092,
                                   "roster": {"basis": {"r": 1}, "cards": ["c"]}, "choices": [1, 2]},
                       "compiler_census": [{"profile": "compact", "input_tokens": 2, "NN_invocations": 0,
                                            "fits_remaining_allowance": True,
                                            "compiled": {"input_ids": [1, 7], "input_digest": "compiled"}},
                                           {"profile": audit["LARGE"], "NN_invocations": 0, "fits_remaining_allowance": False,
                                            "compiler_refusal": "source exceeds remaining budget"}]}

    def census(self):
        return audit["confirmed_census"](self.record, self.answer, self.request, self.binding)

    def test_two_compilers_retained_even_with_one_refusal(self):
        result = self.census()
        self.assertEqual(0, result["context_forwards"])
        self.assertEqual((2, 1, 8092), (result["offered_choices"], result["roster_cards"], result["original_remaining_tokens"]))
        self.assertIn("refusal", result["compiler_census"][1])

    def test_dispatched_or_wrong_request_is_refused(self):
        for change in ({"Context_NN_dispatch": True}, {"actual_Focus_profile": "other"}):
            with self.subTest(change=change):
                with self.assertRaisesRegex(ValueError, "non-dispatch"):
                    audit["confirmed_census"](self.record | change, self.answer, self.request, self.binding)
        self.answer["request_id"] = "not-this-request"
        with self.assertRaisesRegex(ValueError, "request"):
            self.census()

    def test_crossed_evidence_rejected_with_valid_distinct_telemetry_and_attempt_ids(self):
        self.answer["records"][0]["value"] = 4
        with self.assertRaisesRegex(ValueError, "delivered source records"):
            self.census()

    def test_budget_or_observed_scoring_cannot_be_credited(self):
        previous = copy.deepcopy(self.answer)
        self.answer["assessment_history"][0]["result"]["advice"]["work"]["input_tokens"] = 8
        with self.assertRaisesRegex(ValueError, "dispatch"):
            self.census()
        self.answer = previous
        self.record["known_Focus_input_tokens"] = 998
        with self.assertRaisesRegex(ValueError, "original token budget"):
            self.census()


if __name__ == "__main__":
    unittest.main()
