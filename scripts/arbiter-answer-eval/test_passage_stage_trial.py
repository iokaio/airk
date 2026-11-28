# SPDX-License-Identifier: Apache-2.0
"""In-memory controls for segmenting losses in the topical passage experiment."""

import unittest

import live_gather
import passage_stage_trial


class PassageStages(unittest.TestCase):
    def test_each_need_nominates_one_file_and_original_rank_breaks_ties(self):
        ranked = ["leader", "second", "third", "target", "tied"]
        held = {p: {} for p in ranked}
        held["target"] = held["tied"] = {"RepairJournal": [2]}
        index = {"files": {p: {"path_words": []} for p in ranked}}
        order = passage_stage_trial.nominate_files(ranked, held, index, ["repair journal", "journal", "unknown"])
        self.assertEqual(order, ["leader", "target", "second", "third", "tied"])
        self.assertEqual(passage_stage_trial.nominate_files(ranked, held, index, []), ranked)

    def test_a_hit_in_the_fifth_separate_range_survives_merging_but_loses_the_cut(self):
        repo = live_gather.Repository.__new__(live_gather.Repository)
        lines = [""] * 750
        starts = {}
        for i in range(5):
            first = i * 150 + 1
            lines[first - 1] = f"def section_{i}():"
            lines[first] = "    return UniqueFlag"
            starts[f"section_{i}"] = [first]
        repo.texts = {"one.py": lines, **{f"other{i}.py": ["pass"] for i in range(8)}}
        repo.index = {"files": {p: {"role": "source", "path_words": [], "lines": len(v),
                                     "defines": starts if p == "one.py" else {}} for p, v in repo.texts.items()},
                      "word_names": {}, "word_files": {}}
        note = {"id": "last", "mark": "essential", "path": "one.py", "lines": [601, 610]}
        notes, first, _ = passage_stage_trial.evaluate(repo, {"anchors": ["UniqueFlag"]}, {"notes": [note]})
        self.assertEqual(first, ["one.py"])
        observed = notes["last"]
        self.assertTrue(observed["raw"])
        self.assertTrue(observed["accepted"])
        self.assertEqual(observed["expanded"], "where")
        self.assertEqual(observed["merged_six"], "where")
        self.assertEqual(observed["cut_six"], "file")
        self.assertEqual(observed["all_spread"], "file")


if __name__ == "__main__":
    unittest.main()
