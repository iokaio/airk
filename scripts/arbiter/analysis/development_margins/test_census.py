# SPDX-License-Identifier: Apache-2.0
import unittest

from census import margin, source_menus


class DevelopmentMarginTests(unittest.TestCase):
    def test_cutoffs_are_inclusive(self):
        def row(scores):
            return margin({"name": "x", "reference_original_fp32": scores,
                           "input": {"input_ids": [1], "input_digest": "x", "option_mask": [1, 1]}})
        self.assertTrue(row([0.020, 0])["near_0020"])
        self.assertFalse(row([0.140, 0])["near_0020"])
        self.assertTrue(row([0.140, 0])["near_0140"])
        self.assertFalse(row([0.141, 0])["near_0140"])
        with self.assertRaises(ValueError):
            row([0.01, float("nan")])

    def test_zero_requires_original_budget_and_empty_activity(self):
        from unittest.mock import Mock
        from zero_capture import verify_empty_journal, zero_disposition
        directory = Mock()
        directory.iterdir.return_value = []
        binding = {"publication": {"token": "s"}, "scope_observation": {"owner": "real"}, "index_id": "idx"}
        answer = {"question": "q", "request_id": "x", "read_scope": {"source": "named", "index_ids": ["idx"]},
                  "scope_observation": binding["scope_observation"], "assessment_history": [],
                  "assessment": {"scope": {"publication": binding["publication"]}, "termination": "budget"},
                  "stop": "budget", "work": {"calls": 8, "fanout": 7}, "records": [],
                  "candidate_count": 3, "omitted_count": 1}
        request = {"question": "q", "budget": {"calls": 8, "fanout": 8}}
        self.assertEqual(zero_disposition(answer, request, binding, directory)["native_admissions"], 0)
        with self.assertRaises(ValueError):
            zero_disposition(dict(answer, work={"calls": 7, "fanout": 7}), request, binding, directory)
        with self.assertRaises(ValueError):
            zero_disposition(dict(answer, assessment_history=[1]), request, binding, directory)
        summary = dict.fromkeys(("records", "bytes", "last_sequence", "dropped", "writer_failed"), 0)
        verify_empty_journal(dict(summary, complete=True), [])
        with self.assertRaises(ValueError):
            verify_empty_journal(dict(summary, complete=True, dropped=1), [])

    def test_staging_selects_exact_memberships(self):
        from stage import layout
        docs = [f"doc-{i}" for i in range(50)]
        self.assertEqual(layout([{"id": "TRAIN_Q103", "menu_ids": docs}])["TRAIN_Q103"][0],
                         ("doc-0", "TRAIN_Q103/doc-0.txt"))
        for bad in ([{"id": "../escape", "menu_ids": docs}],
                    [{"id": "TRAIN_Q103", "menu_ids": docs[:-1] + [docs[0]]}]):
            with self.assertRaises(ValueError):
                layout(bad)

    def test_fence_includes_whole_menu_and_retains_order(self):
        rows = [{"QUESTION_ID": key, "ANSWERABLE": "Y", "DOC_IDS": menu,
                 "QUESTION_TITLE": "Title", "QUESTION_TEXT": "Body"}
                for key, menu in [("b", ["y", "x"]), ("a", ["x", "d"])]]
        fence = {"entire_DEV_source_id_closure": ["d"],
                 "records": [{"original_TRAIN_id": key, "answerable": True,
                              "whole_original_context_closure_disjoint": disjoint}
                             for key, disjoint in [("a", False), ("b", True)]]}
        result = source_menus(rows, fence)
        self.assertEqual([r["id"] for r in result], ["b"])
        self.assertEqual(result[0]["menu_ids"], ["y", "x"])
        self.assertEqual(result[0]["request"], "Title\n\nBody")
        fence["records"][0]["whole_original_context_closure_disjoint"] = True
        with self.assertRaises(ValueError):
            source_menus(rows, fence)


if __name__ == "__main__":
    unittest.main()
