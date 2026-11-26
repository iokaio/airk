# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the Arbiter pair probe and its case builder."""

import pathlib
import tempfile
import unittest

import arbiter_pair_probe as probe
import arbiter_probe_cases as cases


def group(group_id, use, scored):
    return {"id": group_id, "use": use, "instruction": "i", "query": "q",
            "candidates": [{"id": name, "text": "t", "label": label, "score": score}
                           for name, label, score in scored]}


class RankSummaryTest(unittest.TestCase):
    def test_top1_and_reciprocal_rank_per_use(self):
        groups = [group("a", "passage", [("r", 1, 0.9), ("w", 0, 0.2)]),
                  group("b", "passage", [("r", 1, 0.1), ("w1", 0, 0.8), ("w2", 0, 0.5)]),
                  group("c", "action", [("r", 1, 0.4), ("w", 0, 0.6)])]
        summary = probe.rank_summary(groups)
        self.assertEqual(summary["passage"]["groups"], 2)
        self.assertEqual(summary["passage"]["top1_share"], 0.5)
        self.assertEqual(summary["passage"]["mean_reciprocal_rank"], round((1 + 1 / 3) / 2, 4))
        self.assertEqual(summary["action"]["top1_share"], 0.0)
        self.assertEqual(summary["action"]["mean_reciprocal_rank"], 0.5)

    def test_the_best_right_candidate_sets_the_rank(self):
        summary = probe.rank_summary([group("a", "passage", [("r1", 1, 0.2), ("r2", 1, 0.7), ("w", 0, 0.5)])])
        self.assertEqual(summary["passage"]["top1_share"], 1.0)

    def test_a_group_that_cannot_rank_is_skipped(self):
        summary = probe.rank_summary([group("a", "coverage", [("r1", 1, 0.2), ("r2", 1, 0.7)]),
                                      group("b", "coverage", [("w1", 0, 0.2), ("w2", 0, 0.7)])])
        self.assertEqual(summary["coverage"]["groups"], 0)
        self.assertEqual(summary["coverage"]["skipped"], 2)
        self.assertIsNone(summary["coverage"]["top1_share"])

    def test_ties_break_by_candidate_id_not_input_order(self):
        summary = probe.rank_summary([group("a", "passage", [("z", 1, 0.5), ("a", 0, 0.5)])])
        self.assertEqual(summary["passage"]["top1_share"], 0.0)


class ValidateTest(unittest.TestCase):
    def test_rejects_missing_fields_duplicates_and_single_candidates(self):
        good = {"id": "a", "use": "passage", "instruction": "i", "query": "q",
                "candidates": [{"id": "r", "text": "t", "label": 1}, {"id": "w", "text": "t", "label": 0}]}
        self.assertEqual(len(probe.validate({"groups": [good]})), 1)
        for broken in ({"groups": []}, {"groups": [good, dict(good)]},
                       {"groups": [dict(good, candidates=good["candidates"][:1])]},
                       {"groups": [dict(good, query="")]},
                       {"groups": [dict(good, candidates=[{"id": "r", "text": "t", "label": 2},
                                                          {"id": "w", "text": "t", "label": 0}])]}):
            with self.assertRaises(ValueError):
                probe.validate(broken)

    def test_the_pair_text_follows_the_publishers_layout(self):
        self.assertEqual(probe.pair_text("I", "Q", "D"), "<Instruct>: I\n<Query>: Q\n<Document>: D")


class CaseBuilderTest(unittest.TestCase):
    def test_groups_use_note_lines_as_right_and_unrelated_delivered_text_as_wrong(self):
        with tempfile.TemporaryDirectory() as folder:
            source = pathlib.Path(folder)
            (source / "src").mkdir()
            (source / "src" / "a.rs").write_text("\n".join(f"line {n}" for n in range(1, 41)), "utf-8")
            scenario = {"instruction": "do the task", "notes": [
                {"id": "N1", "mark": "essential", "path": "src/a.rs", "lines": [5, 14], "fact": "fact one"},
                {"id": "N2", "mark": "essential", "path": "src/a.rs", "lines": [30, 33], "fact": "fact two"},
                {"id": "N3", "mark": "useful", "path": "src/a.rs", "lines": [1, 2], "fact": "ignored"}]}
            long_text = "unrelated delivered text " * 6
            reply = {"records": [{"operation": "search", "arguments": {}, "canonical_result": {"hits": [
                {"citation": {"path": "docs/guide.md", "start_line": 1, "end_line": 9}, "text": long_text},
                {"citation": {"path": "src/a.rs", "start_line": 6, "end_line": 9}, "text": long_text},
                {"citation": {"path": "docs/short.md", "start_line": 1, "end_line": 2}, "text": "too short"}]}}]}
            groups = cases.build_case("demo-C1", scenario, reply, source)
        by_id = {item["id"]: item for item in groups}
        passage = by_id["demo-C1/passage"]
        self.assertEqual([c["id"] for c in passage["candidates"]], ["N1", "N2", "w0"])
        self.assertEqual(passage["candidates"][0]["text"].splitlines()[0], "line 5")
        self.assertEqual(passage["query"], "do the task")
        action = by_id["demo-C1/action"]
        self.assertEqual(action["candidates"][0]["text"].splitlines(), ["src/a.rs lines 5-14", "line 5", "line 6", "line 7"])
        coverage = by_id["demo-C1/coverage/N1"]
        self.assertEqual(coverage["query"], "fact one")
        self.assertEqual([(c["id"], c["label"]) for c in coverage["candidates"]],
                         [("whole", 1), ("first-part", 0), ("other-note", 0)])
        self.assertEqual(coverage["candidates"][1]["text"].splitlines(), ["line 5", "line 6", "line 7", "line 8"])
        # A four-line note is too short to cut; it still has the other note as a wrong candidate.
        self.assertEqual([c["id"] for c in by_id["demo-C1/coverage/N2"]["candidates"]], ["whole", "other-note"])

    def test_a_case_without_wrong_candidates_builds_nothing(self):
        with tempfile.TemporaryDirectory() as folder:
            source = pathlib.Path(folder)
            (source / "a.rs").write_text("one\ntwo\nthree\n", "utf-8")
            scenario = {"instruction": "t", "notes": [
                {"id": "N1", "mark": "essential", "path": "a.rs", "lines": [1, 2], "fact": "f"}]}
            self.assertEqual(cases.build_case("demo-C1", scenario, {"records": []}, source), [])


class ReportTest(unittest.TestCase):
    def test_recall_counts_right_candidates_in_the_first_places(self):
        import arbiter_pair_report as report
        groups = [group("alpha-C1/passage", "passage",
                        [("r1", 1, 0.9), ("r2", 1, 0.1), ("w1", 0, 0.8), ("w2", 0, 0.7), ("w3", 0, 0.6),
                         ("w4", 0, 0.5), ("w5", 0, 0.4)]),
                  group("alpha-C1/coverage/N1", "coverage", [("whole", 1, 0.2), ("first-part", 0, 0.9)])]
        row = report.recall(groups)["passage/alpha"]
        self.assertEqual((row["right"], row["in_top5"], row["in_top_r"], row["candidates"]), (2, 1, 1, 7))
        self.assertEqual(report.coverage_losses(groups),
                         {"groups": 1, "whole_first": 0, "first-part": 1, "other-note": 0})

    def test_agreement_compares_top_places_and_refuses_mismatched_groups(self):
        import arbiter_pair_report as report
        first = [group("a-C1/passage", "passage", [("x", 1, 0.9), ("y", 0, 0.5), ("z", 0, 0.1)])]
        second = [group("a-C1/passage", "passage", [("x", 1, 0.4), ("y", 0, 0.6), ("z", 0, 0.1)])]
        row = report.agreement(first, second)["passage"]
        self.assertEqual((row["same_top_share"], row["mean_top5_overlap"], row["max_score_delta"]), (0.0, 1.0, 0.5))
        with self.assertRaises(ValueError):
            report.agreement(first, [group("a-C1/passage", "passage", [("x", 1, 0.4), ("q", 0, 0.6)])])
        with self.assertRaises(ValueError):
            report.agreement(first, [])


if __name__ == "__main__":
    unittest.main()
