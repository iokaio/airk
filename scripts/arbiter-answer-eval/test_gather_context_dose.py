# SPDX-License-Identifier: Apache-2.0
"""The model-dose step on hand-built replies; no service and no model."""

from __future__ import annotations

import json
import unittest

import gather_context_dose as dose


def record(identity: str, *rows) -> dict:
    return {"id": identity, "canonical_result": {"spans": [
        {"path": path, "start_line": start, "end_line": end, "text": text, **extra}
        for path, start, end, text, extra in rows]}}


def full_reply() -> dict:
    return {"schema": dose.FULL_SCHEMA,
            "parts": [{"id": 1, "sub_needs": [{"id": 1}, {"id": 2}]}],
            "records": [record("r1", ("a.rs", 1, 5, "alpha one", {}), ("a.rs", 20, 30, "alpha two", {})),
                        record("r2", ("b.rs", 4, 9, "beta", {}), ("b.rs", 50, 60, "trimmed", {"overlaps": [{"x": 1}]})),
                        record("r3", ("a.rs", 20, 30, "alpha two", {}))],
            "record_uses": [{"part": 1, "need": 1, "record": "r1"}, {"part": 1, "need": 2, "record": "r2"},
                            {"part": 1, "need": 2, "record": "r3"}, {"part": 1, "need": 9, "record": "r1"}]}


def brief_reply() -> dict:
    return {"schema": dose.BRIEF_SCHEMA, "task": "t",
            "needs": [{"need": 1, "text": "first", "where": [{"at": "a.rs:1-5", "why": "x"}]},
                      {"need": 2, "text": "second", "where": [{"at": "b.rs:4-9", "why": "y"}]}],
            "passages": [{"at": "a.rs:1-5", "needs": [1], "text": "alpha one"}], "omitted": ["b.rs:4-9"],
            "verification": "kept"}


class DoseTest(unittest.TestCase):
    def test_candidates_follow_record_uses_and_skip_trimmed_rows(self) -> None:
        pool = dose.candidates(full_reply())
        self.assertEqual([item["at"] for item in pool[1]], ["a.rs:1-5", "a.rs:20-30"])
        self.assertEqual([item["at"] for item in pool[2]], ["b.rs:4-9", "a.rs:20-30"])
        self.assertNotIn(9, {number for number in pool if number > 2})

    def test_the_dose_is_dealt_one_candidate_a_need_at_a_time(self) -> None:
        pool = dose.candidates(full_reply())
        self.assertEqual(dose.deal(pool, 3), [(1, 0), (2, 0), (1, 1)])
        self.assertEqual(len(dose.deal(pool, 99)), 4)

    def test_the_best_scored_passage_of_each_need_goes_first_and_a_shared_one_is_sent_once(self) -> None:
        asked = []

        def score(instruction, pairs):
            asked.extend(pairs)
            return [0.9 if text == "alpha two" else 0.1 for _, text in pairs]

        out, spent = dose.choose(brief_reply(), full_reply(), score, 4)
        self.assertEqual(asked, [("first", "alpha one"), ("second", "beta"), ("first", "alpha two"), ("second", "alpha two")])
        self.assertEqual([(item["at"], item["needs"]) for item in out["passages"]],
                         [("a.rs:20-30", [1, 2]), ("a.rs:1-5", [1]), ("b.rs:4-9", [2])])
        self.assertNotIn("omitted", out)
        self.assertEqual(out["verification"], "kept")
        self.assertEqual((spent["judgments"], spent["candidates"], spent["kept_from_service"]), (4, 4, 1))

    def test_a_passage_that_does_not_fit_is_listed_as_omitted_and_the_next_is_tried(self) -> None:
        full = full_reply()
        full["records"][0]["canonical_result"]["spans"][0]["text"] = "x" * (dose.PASSAGE_BYTES + 1)
        out, _ = dose.choose(brief_reply(), full, lambda instruction, pairs: [0.5] * len(pairs), 4)
        self.assertEqual([item["at"] for item in out["passages"]], ["b.rs:4-9", "a.rs:20-30"])
        self.assertEqual(out["omitted"], ["a.rs:1-5"])
        self.assertLessEqual(len(json.dumps(out).encode("utf-8")), dose.MAX_BRIEF_BYTES)

    def test_no_dose_or_another_shape_leaves_the_brief_alone(self) -> None:
        def never(instruction, pairs):
            raise AssertionError("nothing is judged")

        brief = brief_reply()
        self.assertIs(dose.choose(brief, full_reply(), never, 0)[0], brief)
        self.assertIs(dose.choose({"schema": "other"}, full_reply(), never, 5)[0]["schema"], "other")


if __name__ == "__main__":
    unittest.main()
