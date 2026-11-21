# SPDX-License-Identifier: Apache-2.0
"""Controls for the panel drift script's pure helpers. No provider, model or device work.

Every expected value below is written out by hand, not computed by the script under test.
"""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import panel

RAW = "airk.private.r8-compact-scientific-raw-panel/1"
OWN = "airk.private.webgpu-panel/1"
PAIRED = "airk.private.compact-protected-cal-r7-reference-comparison/1"
REUSE = "airk.private.r8-compact-scientific-cpu-reuse/1"


def compiled(tokens=4, options=2, digest="ab" * 32):
    return {"input_ids": list(range(tokens)), "attention_mask": [1] * tokens, "position_ids": list(range(tokens)),
            "decision_position": tokens - 1, "candidate_positions": [32 + index for index in range(options)],
            "option_mask": [True] * options, "input_digest": digest}


def panel_doc():
    return {"rows": [{"ordinal": 0, "id": "a", "status": "eligible", "compiled": compiled(4, 2, "aa" * 32)},
                     {"ordinal": 1, "id": "b", "status": "eligible", "compiled": compiled(6, 3, "bb" * 32)}]}


def paired_row(ordinal, name, candidate, matching, publisher):
    return {"ordinal": ordinal, "id": name, "numerical": {
        "candidate_scores": candidate, "matching_scores": matching, "original_scores": publisher}}


class Loading(unittest.TestCase):
    def test_named_splits_at_the_first_equals_sign(self):
        self.assertEqual(panel.named("calibrated=D:/x/a=b.json"), ("calibrated", Path("D:/x/a=b.json")))
        for bad in ("calibrated", "=path", "calibrated="):
            with self.assertRaises(ValueError):
                panel.named(bad)

    def test_panel_rows_keep_order_tokens_options_and_digest(self):
        rows = panel.panel_rows(panel_doc(), "control")
        self.assertEqual([(row["ordinal"], row["id"], row["tokens"], row["options"], row["input_digest"]) for row in rows],
                         [(0, "a", 4, 2, "aa" * 32), (1, "b", 6, 3, "bb" * 32)])
        self.assertEqual(rows[1]["compiled"]["candidate_positions"], [32, 33, 34])

    def test_panel_rows_accept_candidate_token_ids_beyond_the_sequence_length(self):
        # Candidate positions are vocabulary rows, not sequence positions, so 32 in a 4-token row is valid.
        self.assertEqual(panel.panel_rows(panel_doc(), "control")[0]["compiled"]["candidate_positions"], [32, 33])

    def test_panel_rows_refuse_unusable_rows(self):
        def changed(edit):
            doc = panel_doc()
            edit(doc)
            return doc

        cases = {
            "masked option": lambda doc: doc["rows"][0]["compiled"].update(option_mask=[True, False]),
            "ineligible": lambda doc: doc["rows"][1].update(status="refused"),
            "decision outside": lambda doc: doc["rows"][0]["compiled"].update(decision_position=4),
            "repeated ordinal": lambda doc: doc["rows"][1].update(ordinal=0),
            "short mask": lambda doc: doc["rows"][0]["compiled"].update(attention_mask=[1, 1, 1]),
            "one option": lambda doc: doc["rows"][0]["compiled"].update(candidate_positions=[32], option_mask=[True]),
            "no rows": lambda doc: doc.update(rows=[]),
        }
        for label, edit in cases.items():
            with self.subTest(label), self.assertRaises(ValueError):
                panel.panel_rows(changed(edit), "control")

    def test_vector_rows_read_a_raw_panel_and_an_earlier_receipt(self):
        raw = {"schema": RAW, "rows": [{"ordinal": 0, "id": "a", "input_digest": "aa" * 32, "scores": [1.5, -0.25]}]}
        own = {"schema": OWN, "rows": [{"ordinal": 7, "id": "h", "input_digest": None, "scores": [0.5, 2]}]}
        self.assertEqual(panel.vector_rows(raw, "raw"),
                         {0: {"id": "a", "input_digest": "aa" * 32, "scores": [1.5, -0.25]}})
        self.assertEqual(panel.vector_rows(own, "own"), {7: {"id": "h", "input_digest": None, "scores": [0.5, 2]}})

    def test_vector_rows_refuse_other_schemas_repeats_and_unusable_scores(self):
        row = {"ordinal": 0, "id": "a", "scores": [1.5, -0.25]}
        bad = [{"schema": "other", "rows": [row]},
               {"schema": RAW, "rows": [row, dict(row)]},
               {"schema": RAW, "rows": [row | {"scores": [1.5, float("nan")]}]},
               {"schema": RAW, "rows": [row | {"scores": [1.5, float("inf")]}]},
               {"schema": RAW, "rows": [row | {"scores": [1.5]}]},
               {"schema": OWN, "rows": [row | {"scores": [1.5, "nan"]}]}]
        for doc in bad:
            with self.assertRaises(ValueError):
                panel.vector_rows(doc, "control")

    def test_paired_rows_read_flat_and_grouped_comparisons(self):
        first = paired_row(0, "a", [3.0, 1.0], [2.75, 1.0], [3.0, 1.25])
        second = paired_row(1, "b", [0.0, 1.0, 2.0], [0.0, 1.0, 2.5], [0.5, 1.0, 2.0])
        flat = panel.paired_rows({"schema": PAIRED, "rows": [first]}, "flat")
        grouped = panel.paired_rows({"schema": REUSE, "groups": [{"rows": [first]}, {"rows": [second]}]}, "grouped")
        self.assertEqual(flat, {0: {"id": "a", "candidate_scores": [3.0, 1.0], "matching_scores": [2.75, 1.0],
                                    "original_scores": [3.0, 1.25]}})
        self.assertEqual(sorted(grouped), [0, 1])
        self.assertEqual(grouped[1]["original_scores"], [0.5, 1.0, 2.0])
        with self.assertRaises(ValueError):
            panel.paired_rows({"schema": RAW, "rows": [first]}, "wrong schema")
        with self.assertRaises(ValueError):
            panel.paired_rows({"schema": PAIRED, "rows": [first, first]}, "repeat")

    def test_bound_checks_each_reference_row_against_its_panel_row(self):
        rows = panel.panel_rows(panel_doc(), "control")
        good = {0: {"id": "a", "input_digest": "aa" * 32, "scores": [1.0, 2.0]},
                1: {"id": "b", "input_digest": None, "scores": [1.0, 2.0, 3.0]}}
        self.assertEqual(panel.bound(rows, good, "control"), {0: [1.0, 2.0], 1: [1.0, 2.0, 3.0]})
        self.assertEqual(panel.bound(rows, {1: good[1]}, "subset"), {1: [1.0, 2.0, 3.0]})
        bad = [{0: good[0] | {"id": "b"}}, {0: good[0] | {"input_digest": "bb" * 32}},
               {0: good[0] | {"scores": [1.0, 2.0, 3.0]}}, {2: good[0]}]
        for vectors in bad:
            with self.assertRaises(ValueError):
                panel.bound(rows, vectors, "control")

    def test_paired_sets_require_the_candidate_to_equal_the_primary_reference(self):
        rows = panel.panel_rows(panel_doc(), "control")
        paired = panel.paired_rows({"schema": PAIRED, "rows": [paired_row(0, "a", [3.0, 1.0], [2.75, 1.0], [3.0, 1.25])]},
                                   "control")
        sets = panel.paired_sets(rows, {0: [3.0, 1.0], 1: [0.0, 1.0, 2.0]}, paired, "control")
        self.assertEqual(sets, {"matching_cpu": {0: [2.75, 1.0]}, "publisher_cpu": {0: [3.0, 1.25]}})
        with self.assertRaises(ValueError):
            panel.paired_sets(rows, {0: [3.0, 1.5]}, paired, "another candidate")
        with self.assertRaises(ValueError):
            panel.paired_sets(rows, {1: [0.0, 1.0, 2.0]}, paired, "ordinal absent from the primary")


class Statistics(unittest.TestCase):
    def test_top_two_gives_the_winner_and_its_margin(self):
        self.assertEqual(panel.top_two([1.0, 3.0, 2.5]), (1, 0.5))
        self.assertEqual(panel.top_two([2.0, 2.0, 1.0]), (0, 0.0))
        self.assertEqual(panel.top_two([-4.0, -1.0]), (1, 3.0))

    def test_compared_reports_difference_winner_and_margin(self):
        changed = panel.compared([1.0, 3.0, 2.5], [1.25, 2.0, 2.75])
        self.assertEqual(changed, {"reference": [1.25, 2.0, 2.75], "max_abs_difference": 1.0, "reference_winner": 2,
                                   "same_winner": False, "reference_margin": 0.75,
                                   "margin_below_twice_difference": True})
        kept = panel.compared([1.0, 3.0], [1.125, 3.0])
        self.assertEqual((kept["max_abs_difference"], kept["same_winner"], kept["reference_margin"],
                          kept["margin_below_twice_difference"]), (0.125, True, 1.875, False))
        with self.assertRaises(ValueError):
            panel.compared([1.0, 3.0], [1.0, 3.0, 2.0])

    def test_a_margin_equal_to_twice_the_difference_is_not_below_it(self):
        edge = panel.compared([0.25, 0.5], [0.0, 0.5])
        self.assertEqual((edge["max_abs_difference"], edge["reference_margin"], edge["margin_below_twice_difference"]),
                         (0.25, 0.5, False))
        self.assertTrue(panel.compared([0.375, 0.5], [0.0, 0.5])["margin_below_twice_difference"])

    def test_nearest_rank_takes_the_smallest_value_covering_the_share(self):
        self.assertEqual(panel.nearest_rank(list(range(1, 21)), 95), 19)
        self.assertEqual(panel.nearest_rank(list(range(100, 0, -1)), 95), 95)
        self.assertEqual(panel.nearest_rank(list(range(1, 22)), 95), 20)
        self.assertEqual(panel.nearest_rank([4, 1, 3, 2], 50), 2)
        self.assertEqual(panel.nearest_rank([5.5], 95), 5.5)
        self.assertEqual(panel.nearest_rank([1, 2, 3], 100), 3)
        for values, percent in (([], 95), ([1], 0), ([1], 101), ([1], 95.0)):
            with self.assertRaises(ValueError):
                panel.nearest_rank(values, percent)

    def test_spread_reports_count_maximum_median_and_p95(self):
        self.assertEqual(panel.spread([4.0, 1.0, 3.0, 2.0]),
                         {"rows": 4, "max": 4.0, "median": 2.5, "p95_nearest_rank": 4.0})

    def test_band_edges_are_inclusive_upper_bounds(self):
        self.assertEqual([panel.band(tokens, [10, 20]) for tokens in (1, 10, 11, 20, 21, 9000)],
                         ["1-10", "1-10", "11-20", "11-20", "21+", "21+"])

    def test_plain_keeps_a_non_finite_score_as_text(self):
        self.assertEqual(panel.plain([1.5, float("nan"), float("inf"), float("-inf")]), [1.5, "nan", "inf", "-inf"])

    def test_summary_counts_changes_thin_margins_bands_and_margins(self):
        def scored(ordinal, name, tokens, scores, margin, reference, difference, same, reference_margin, below):
            return {"ordinal": ordinal, "id": name, "tokens": tokens, "scores": scores, "margin": margin,
                    "comparisons": {"ref": {"reference": reference, "max_abs_difference": difference,
                                            "reference_winner": 1, "same_winner": same,
                                            "reference_margin": reference_margin,
                                            "margin_below_twice_difference": below}}}

        rows = [scored(0, "a", 8, [1.0, 3.0], 2.0, [1.0, 2.5], 0.5, True, 1.5, False),
                scored(1, "b", 30, [2.0, 1.0], 1.0, [1.75, 2.0], 1.0, False, 0.25, True),
                scored(2, "c", 9, [0.0, 0.125], 0.125, [0.0, 0.25], 0.125, True, 0.25, False),
                {"ordinal": 3, "id": "d", "tokens": 9, "scores": [9.0, 0.0], "margin": 9.0, "comparisons": {}}]
        changed = {"ordinal": 1, "id": "b", "tokens": 30, "scores": [2.0, 1.0], "reference": [1.75, 2.0],
                   "max_abs_difference": 1.0, "margin": 1.0, "reference_margin": 0.25}
        self.assertEqual(panel.summary(rows, "ref", [10], [0.25, 1.5]), {
            "difference": {"rows": 3, "max": 1.0, "median": 0.5, "p95_nearest_rank": 1.0},
            "winner_changes": 1, "margin_below_twice_difference": 1,
            "winner_change_rows": [changed], "margin_below_twice_difference_rows": [changed],
            "bands": {"1-10": {"difference": {"rows": 2, "max": 0.5, "median": 0.3125, "p95_nearest_rank": 0.5},
                               "winner_changes": 0, "margin_below_twice_difference": 0},
                      "11+": {"difference": {"rows": 1, "max": 1.0, "median": 1.0, "p95_nearest_rank": 1.0},
                              "winner_changes": 1, "margin_below_twice_difference": 1}},
            "margin_at_least": {"0.25": {"reference": 3, "scored": 2}, "1.5": {"reference": 1, "scored": 1}}})
        with self.assertRaises(ValueError):
            panel.summary(rows, "absent", [10], [0.25])


if __name__ == "__main__":
    unittest.main()
