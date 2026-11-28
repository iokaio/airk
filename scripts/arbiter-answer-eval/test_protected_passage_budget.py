# SPDX-License-Identifier: Apache-2.0
import unittest
from adaptive_passage_budget import Passage
from protected_passage_budget import add_nominations
from passage_reply_budget import render


def segment(path, line, need, text="schema\n"):
    at = f"{path}:{line}-{line}"
    return Passage(at, path, line, line, text, need, at if need else None)


class ProtectedBudgetTests(unittest.TestCase):
    def exercise(self, seed, candidates, cap):
        frame = lambda picks: render("query", ["first", "second"], "rev", picks, len(candidates), cap)
        return add_nominations(seed, candidates, cap, frame)

    def test_identical_syntax_at_different_location_and_unrepresented_need(self):
        seed = [segment("a", 1, 1)]
        same = segment("a", 1, 2)
        other = segment("b", 1, 2)
        data, chosen, omitted = self.exercise(seed, [*seed, same, other, segment("c", 1, 2)], 20000)
        self.assertEqual(chosen, [*seed, other])
        self.assertEqual([r["reason"] for r in omitted], ["no_new_location", "need_already_represented"])

    def test_new_attribution_and_citation_metadata_must_fit(self):
        a, b = segment("a", 1, 1), segment("b", 1, 2, "🙂\n")
        ref, _ = render("query", ["first", "second"], "rev", [a], 2, 20000)
        wire, chosen, omitted = self.exercise([a], [a, b], len(ref) + b.size)
        self.assertEqual(wire, ref)
        self.assertEqual(chosen, [a])
        self.assertEqual(omitted[0]["reason"], "wire_limit")
