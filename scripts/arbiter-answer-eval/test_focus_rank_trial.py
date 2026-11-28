# SPDX-License-Identifier: Apache-2.0
"""Synthetic controls for the free ranking experiment, using in-memory source text."""

import math
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import focus_rank_trial
import live_gather


class RankingExperiments(unittest.TestCase):
    def test_score_only_discovers_a_file_without_nominating_its_lines(self):
        repo = live_gather.Repository.__new__(live_gather.Repository)
        repo.texts = {"src.py": ["def StartHere():", "    return OtherTarget()"],
                      "target.py": ["def OtherTarget():", "    pass"]}
        repo.texts.update({f"filler{i}.py": ["pass"] for i in range(10)})
        records = {path: {"role": "source", "path_words": [], "defines": {}, "lines": len(lines)}
                   for path, lines in repo.texts.items()}
        records["src.py"]["defines"] = {"StartHere": [1]}
        records["target.py"]["defines"] = {"OtherTarget": [1]}
        repo.index = {"files": records, "word_files": {}, "word_names": {}}
        trace = {}
        with patch.object(live_gather, "HOP_TERMS", 0):
            brief = live_gather.gather(repo, {"anchors": ["StartHere"]}, trace, extra_hop_weight=0.15)
        self.assertEqual(trace["held"]["target.py"], [])
        rank = dict(trace["ranked"])["target.py"]
        self.assertAlmostEqual(rank, math.log(1 + 12 / 2) * 0.6 * 1.6 * 0.15, places=3)
        self.assertFalse(any(row["at"].startswith("target.py:") for row in brief["needs"][0]["where"]))

    def test_spread_gives_a_second_file_space_without_splitting_a_range(self):
        repo = SimpleNamespace(texts={"a": ["abc", "def"], "b": ["xyz"]})
        brief = {"schema": "test", "task": "t", "result": "complete", "searched": "",
                 "needs": [{"where": [{"at": "a:1-1"}, {"at": "a:2-2"}, {"at": "b:1-1"}]}]}
        with patch.object(live_gather, "FOCUS_PASSAGE_BYTES", 8):
            focused = live_gather.focus(repo, brief, spread=True)
        self.assertEqual([p["at"] for p in focused["passages"]], ["a:1-1", "b:1-1"])
        self.assertEqual([p["text"] for p in focused["passages"]], ["abc\n", "xyz\n"])
        self.assertEqual(focused["omitted"], ["a:2-2"])

    def test_project_link_uses_nearest_unique_manifest_and_never_invents_lines(self):
        files = {"pkg/src/main.py": {"role": "source"}, "pkg/pyproject.toml": {"role": "configuration"},
                 "package.json": {"role": "configuration"}}
        repo = SimpleNamespace(index={"files": files})
        for ambiguous in (False, True):
            with self.subTest(ambiguous=ambiguous):
                if ambiguous:
                    files["pkg/go.mod"] = {"role": "configuration"}
                score, held = {"pkg/src/main.py": 10}, {"pkg/src/main.py": {"anchor": [4]}}
                focus_rank_trial.apply_file_rule("project", repo, [], score, held)
                self.assertNotIn("package.json", score)
                if ambiguous:
                    self.assertNotIn("pkg/pyproject.toml", score)
                else:
                    self.assertEqual(score["pkg/pyproject.toml"], 1.5)
                    self.assertEqual(held["pkg/pyproject.toml"], {})

    def test_direct_document_rule_ignores_map_expansions(self):
        repo = SimpleNamespace(index={"files": {p: {"role": "document"} for p in ("one.md", "two.md")}})
        terms = [{"term": "asked", "source": "quoted"}, {"term": "derived", "source": "map"}]
        score = {"one.md": 7, "two.md": 7}
        focus_rank_trial.apply_file_rule("direct-doc", repo, terms, score,
                                         {"one.md": {"asked": [1]}, "two.md": {"derived": [1]}})
        self.assertAlmostEqual(score["one.md"], 20)
        self.assertEqual(score["two.md"], 7)


if __name__ == "__main__":
    unittest.main()
