# SPDX-License-Identifier: Apache-2.0
"""Range nomination stays bounded, uses source text and attributes only delivered evidence."""
from types import SimpleNamespace
import unittest

import range_choice as choice


class RangeChoiceTests(unittest.TestCase):
    def setUp(self):
        texts = {
            "a.py": ["def authorize():", "    return grant", "def handle():", "    return authorize()"],
            "b.py": ["def retry_transport():", "    return retry_on_gateway_timeout", "retry_transport()"],
            "c.py": ["documentation", "background"],
        }
        self.repo = SimpleNamespace(texts=texts, index={"files": {p: {"defines": {}} for p in texts}})
        self.pool = [{"file": p, "locations": locs} for p, locs in (
            ("a.py", ["a.py:1-2", "a.py:3-4"]), ("b.py", ["b.py:1-2", "b.py:3-3"]), ("c.py", ["c.py:1-2"]))]
        self.args = {"task": "Determine the relevant files.", "needs": ["who can authorize", "retry transport disposition"]}

    def test_balanced_needs_scope_and_distinct_files_first(self):
        result = choice.nominate(self.repo, self.pool, self.args, 40)
        self.assertEqual([1, 2]*5, [r.need for r in result])
        for n in (1, 2):
            per_need = [r for r in result if r.need == n]
            self.assertEqual(len({r.source.path for r in per_need[:3]}), 3)
            self.assertEqual(len({r.source.at for r in per_need}), 5)
        self.assertEqual(result[0].source.path, "a.py")
        self.assertEqual(result[1].source.path, "b.py")

    def test_shared_range_keeps_distinct_needs_and_charges_once(self):
        path = choice.SourceRange("f", 1, 1, 4, ("UTF-8 αβ",))
        items = [choice.Nomination(n, path, path.at, "q", "d") for n in (1, 2)]
        size = len("UTF-8 αβ\n".encode())
        actual = choice.assemble(["A", "B"], items, [.4, .7], byte_limit=size)
        self.assertEqual(actual["passage_bytes"], size)
        self.assertEqual(actual["passages"][0]["needs"], [1, 2])
        self.assertEqual([r["status"] for r in actual["reply_needs"]], ["candidate_evidence"]*2)
        tiny = choice.assemble(["A", "B"], items, [.4, .7], byte_limit=size-1)
        self.assertFalse(tiny["passages"])
        self.assertEqual([r["status"] for r in tiny["reply_needs"]], ["no_candidate_evidence"]*2)
        self.assertEqual(len(tiny["omissions"]), 2)

    def test_window_can_choose_later_evidence_after_oversized_line(self):
        src = choice.SourceRange("d", 1, 1, 3, ("x"*400, "h"*501, "retry transport", "call(b)"))
        self.assertEqual(choice.excerpt(src, {"retry","transport"}, {"retry":1., "transport":2.}), (2, 3))
        self.assertIsNone(choice.excerpt(choice.SourceRange("d", 1, 1, 1, ("x"*501,)), set(), {}))

    def test_invalid_range_cannot_name_another_file(self):
        with self.assertRaisesRegex(ValueError, "Invalid nominated source range"):
            choice.source_ranges(self.repo, [{"file":"a.py", "locations":["c.py:1-2"]}])

    def test_limited_prefix_does_not_change_longer_roster(self):
        texts = {f"p{i}": [f"alpha source line {i}"] for i in range(40)}
        repo = SimpleNamespace(texts=texts,index={"files":{p:{"defines":{}} for p in texts}})
        pool = [{"file":p,"locations":[f"{p}:1-1"]} for p in texts]
        forty = choice.nominate(repo, pool, self.args, 40)
        eighty = choice.nominate(repo, pool, self.args, 80)
        self.assertEqual(forty, eighty[:40])
        self.assertEqual((len(forty),len(eighty)), (40,80))
        self.assertEqual(len({(r.need,r.source.at) for r in eighty}),80)


if __name__ == "__main__":
    unittest.main()
