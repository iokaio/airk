# SPDX-License-Identifier: Apache-2.0
"""Classification of truth notes against a brief and a full reply; no service."""

from __future__ import annotations

import unittest

import gather_context_call_replay as replay


def note(identity: str, path: str, start: int, end: int, **extra) -> dict:
    return {"id": identity, "mark": "essential", "path": path, "lines": [start, end], **extra}


class ReplayTest(unittest.TestCase):
    def test_each_located_essential_note_is_placed_at_the_nearest_stage_that_holds_it(self) -> None:
        task = {"notes": [note("N1", "a.rs", 10, 20), note("N2", "b.rs", 5, 9), note("N3", "c.rs", 1, 8),
                          note("N4", "d.rs", 1, 8), {"id": "N5", "mark": "essential", "absence": True},
                          {**note("N6", "a.rs", 10, 20), "mark": "helpful"}]}
        brief = {"passages": [{"at": "a.rs:8-22", "needs": [1], "text": "body"}],
                 "needs": [{"need": 1, "where": [{"at": "a.rs:8-22", "why": "w"}, {"at": "b.rs:1-30", "why": "w"}]}],
                 "omitted": ["zzz"]}
        full = {"records": [{"canonical_result": {"projection": "airk.arbiter.loop-evidence/1", "spans": [
            {"path": "c.rs", "start_line": 1, "end_line": 12, "text": "gathered text"}]}}]}
        self.assertEqual(replay.classify(task, brief, full),
                         {"N1": "passage", "N2": "where", "N3": "gathered", "N4": "missed"})

    def test_a_location_is_parsed_from_its_last_colon(self) -> None:
        span = replay.span_of("C:/x/y.rs:3-7")
        self.assertEqual((span.path, span.start, span.end), ("C:/x/y.rs", 3, 7))
        self.assertIsNone(replay.span_of("no lines here"))


if __name__ == "__main__":
    unittest.main()
