# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the loop prototype's own rules."""

import unittest

import arbiter_loop_prototype as proto
import gather_context_loss_stage as loss


def passage(path, start, end, score, needs=(0,)):
    return {"path": path, "start": start, "end": end, "score": score, "needs": list(needs),
            "text": "\n".join(f"line {number}" for number in range(start, end + 1))}


class RulesTest(unittest.TestCase):
    def test_names_are_identifier_shaped_and_ranked_by_count(self):
        text = "let max_tokens = MaxTokens::new(); max_tokens.check(LIMIT_FILES); return value with this"
        self.assertEqual(proto.names_in(text, 3), ["max_tokens", "LIMIT_FILES", "MaxTokens"])

    def test_windows_cover_every_line_with_true_line_numbers(self):
        text = "\n".join(str(number) for number in range(101, 201)) + "\n"
        found = proto.windows("a.rs", 101, text)
        self.assertEqual((found[0]["start"], found[0]["end"]), (101, 140))
        self.assertEqual(found[-1]["end"], 200)
        self.assertEqual(found[1]["text"].split("\n")[0], str(found[1]["start"]))
        covered = {line for item in found for line in range(item["start"], item["end"] + 1)}
        self.assertEqual(covered, set(range(101, 201)))

    def test_overlapping_passages_merge_without_repeating_lines(self):
        merged = proto.merge([passage("a.rs", 10, 20, 0.4), passage("a.rs", 15, 30, 0.9, (1,)),
                              passage("a.rs", 40, 41, 0.5), passage("b.rs", 1, 2, 0.6)])
        self.assertEqual([(item["path"], item["start"], item["end"]) for item in merged],
                         [("a.rs", 10, 30), ("a.rs", 40, 41), ("b.rs", 1, 2)])
        self.assertEqual(merged[0]["text"], passage("a.rs", 10, 30, 0)["text"])
        self.assertEqual((merged[0]["score"], merged[0]["needs"]), (0.9, [0, 1]))

    def test_delivery_serves_every_need_before_any_second_passage_and_keeps_the_floor(self):
        first = [passage("a.rs", 1, 50, 0.9), passage("a.rs", 100, 150, 0.8), passage("c.rs", 1, 5, 0.1)]
        second = [passage("b.rs", 1, 50, 0.5)]
        size = len(first[0]["text"].encode())
        delivered = proto.deliver([first, second], keep=0.3, per_need_limit=3, budget=2 * size)
        self.assertEqual([(item["path"], item["start"]) for item in delivered], [("a.rs", 1), ("b.rs", 1)])

    def test_the_dynamic_cutoff_follows_each_need_and_never_leaves_one_empty(self):
        clear = [passage("a.rs", 1, 5, 0.9), passage("a.rs", 20, 25, 0.3)]
        even = [passage("b.rs", 1, 5, 0.8), passage("b.rs", 20, 25, 0.7), passage("b.rs", 40, 45, 0.5)]
        weak = [passage("c.rs", 1, 5, 0.02), passage("c.rs", 20, 25, 0.001)]
        delivered = proto.deliver([clear, even, weak], keep=0.3, per_need_limit=9, budget=10_000, relative=0.5)
        self.assertEqual([(item["path"], item["start"]) for item in delivered],
                         [("a.rs", 1), ("b.rs", 1), ("b.rs", 20), ("b.rs", 40), ("c.rs", 1)])

    def test_shares_keep_a_need_with_many_passages_from_starving_the_others(self):
        many = [passage("a.rs", 100 * rank, 100 * rank + 49, 0.9) for rank in range(1, 9)]
        few = [passage("b.rs", 1, 50, 0.9), passage("b.rs", 100, 149, 0.8)]
        size = len(many[0]["text"].encode())
        plain = proto.deliver([many, few], keep=0.3, per_need_limit=8, budget=4 * size)
        shared = proto.deliver([many, few], keep=0.3, per_need_limit=8, budget=5 * size, shares=True)
        self.assertEqual(sum(item["path"] == "b.rs" for item in plain), 2)
        self.assertEqual(sum(item["path"] == "b.rs" for item in shared), 2)
        self.assertEqual(sum(item["path"] == "a.rs" for item in shared), 3)

    def test_a_weak_need_keeps_more_passages_and_a_need_is_split_into_clauses(self):
        strong = [passage("a.rs", 1, 5, 0.9), passage("a.rs", 20, 25, 0.2)]
        unsure = [passage("b.rs", 1, 5, 0.3), passage("b.rs", 20, 25, 0.06), passage("b.rs", 40, 45, 0.01)]
        delivered = proto.deliver([strong, unsure], keep=0.3, per_need_limit=9, budget=10_000, relative=0.5,
                                  weak=[False, True])
        self.assertEqual([(item["path"], item["start"]) for item in delivered],
                         [("a.rs", 1), ("b.rs", 1), ("b.rs", 20), ("b.rs", 40)])
        self.assertEqual(proto.clauses("Enforcement sites on the REST transport and on the gRPC transport, "
                                       "and the error returned when exceeded"),
                         ["Enforcement sites on the REST transport", "on the gRPC transport",
                          "the error returned when exceeded"])

    def test_cited_paths_and_the_cheap_first_cut(self):
        texts = ["Run `pwsh pro/scripts/test-all.ps1 -Full` once.", "see pro/scripts/test-all.ps1 and "
                 "dotnet test pro/src/App.Tests/App.Tests.csproj; not a path: v1.2 or and/or."]
        self.assertEqual(proto.cited_paths(texts, 5),
                         ["pro/scripts/test-all.ps1", "pro/src/App.Tests/App.Tests.csproj"])
        pool = {("a.cs", 1, 9): {"text": "string GetDefaultModel(AIProviderType p) => p switch { _ => throw new "
                                         "ArgumentOutOfRangeException() };"},
                ("b.md", 1, 9): {"text": "The plan for onboarding screens and their colours."},
                ("c.cs", 1, 9): {"text": "void Save() { repository.Save(item); }"}}
        self.assertEqual(proto.likeliest("Places that could throw ArgumentOutOfRangeException for a provider "
                                         "switch with a missing case", pool, 1), [("a.cs", 1, 9)])
        self.assertEqual(proto.likeliest("nothing shared here", {("c.cs", 1, 9): pool[("c.cs", 1, 9)]}, 3), [])

    def test_a_delivered_passage_counts_as_opened_for_the_loss_stage_script(self):
        reply = proto.reply_of([passage("src/a.rs", 10, 60, 0.9)])
        scenario = {"notes": [{"id": "n1", "mark": "essential", "path": "src/a.rs", "lines": [20, 30]},
                              {"id": "n2", "mark": "essential", "path": "src/a.rs", "lines": [200, 210]},
                              {"id": "n3", "mark": "essential", "path": "src/z.rs", "lines": [1, 2]}]}
        stages = {row["id"]: row["stage"] for row in loss.classify(scenario, reply)["notes"]}
        self.assertEqual(stages, {"n1": "opened", "n2": "file", "n3": "never"})


if __name__ == "__main__":
    unittest.main()
