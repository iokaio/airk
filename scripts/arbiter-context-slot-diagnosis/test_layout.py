# SPDX-License-Identifier: Apache-2.0
"""Controls for the slot diagnosis layout helpers. No tokenizer, model or private file.

Every expected value below is written out by hand, not computed by the code under test.
"""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import layout

PROMPT = ('<|im_start|>system\nPick.<|im_end|>\n<|im_start|>user\nState:\n"{\\"k\\":\\"café\\\\n\\"}"\n'
          'Question:\n"Which \\"one\\"?"\nOptions:\n'
          'A = "a1": "read r1; gap unsupported"\nB = "a2": "sufficient; gap sufficient"\n'
          'Choose one option symbol:<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n')
PARTS = {"prefix": "<|im_start|>system\nPick.<|im_end|>\n", "state": '{"k":"café\\n"}',
         "question": 'Which "one"?',
         "options": [["A", "a1", "read r1; gap unsupported"], ["B", "a2", "sufficient; gap sufficient"]]}
STATE = ('{"format":"f","indexed_evidence":[[0,1,1,0,5,5,false,"e"]],"paths":["p0","p1","p2"],'
         '"read_choices_available":5,"read_choices_omitted":2,'
         '"verified_reads":[["target","context","p9:1-33"],["literal","search",1,1,0,3,"abc"],'
         '["literal","search",2,4,2,6,"wxyz"]]}')
# The state with only its third read kept: one path fewer, and four of the five available reads omitted.
THIRD_READ = ('{"format":"f","indexed_evidence":[[0,1,1,0,5,5,false,"e"]],"paths":["p0","p2"],'
              '"read_choices_available":5,"read_choices_omitted":4,'
              '"verified_reads":[["literal","search",1,4,2,6,"wxyz"]]}')
UNITS = [{"record": "x", "result_pointer": "/hits/0"}, {"record": "x", "result_pointer": "/hits/1"}]


def card(action, gap, reason, role=None, relevance=None):
    return {"id": "k", "action": action, "gap": gap, "reason": reason,
            "assessments": [] if role is None else [{"evidence": unit, "role": role, "relevance": relevance}
                                                    for unit in UNITS]}


class Prompt(unittest.TestCase):
    def test_parse_splits_the_template_and_decodes_state_and_question(self):
        self.assertEqual(layout.parse_prompt(PROMPT), PARTS)

    def test_render_writes_the_compiled_characters_back(self):
        self.assertEqual(layout.render_prompt(PARTS), PROMPT)

    def test_parse_refuses_other_shapes(self):
        cut = PROMPT[:-len("</think>\n\n")]
        swapped = PROMPT.replace('A = "a1"', 'B = "a1"').replace('B = "a2"', 'A = "a2"')
        trailing = PROMPT.replace('unsupported"\n', 'unsupported" extra\n')
        spaced = PROMPT.replace('"a1": ', '"a1":  ')
        for bad in (cut, swapped, trailing, spaced, PROMPT.replace("Question:\n", "Query:\n")):
            with self.assertRaises(ValueError):
                layout.parse_prompt(bad)

    def test_card_lines_restate_the_native_option_text(self):
        roster = {"basis": {"source_units_presented": UNITS, "read_choices_scored": ["c1", "c2"]}, "cards": [
            card({"kind": "read", "choice": "c1"}, "unsupported", "additional_evidence_may_help", "unknown", "uncertain"),
            card({"kind": "read", "choice": "c2"}, "unsupported", "additional_evidence_may_help", "unknown", "uncertain"),
            card({"kind": "sufficient"}, "sufficient", "retained_evidence_covers_question", "document", "useful"),
            card({"kind": "clarify"}, "ambiguous", "ambiguous_question")]}
        self.assertEqual(layout.card_lines(roster), [
            ["a1", "read r1; gap unsupported; reason additional_evidence_may_help; hypotheses h1"],
            ["a2", "read r2; gap unsupported; reason additional_evidence_may_help; hypotheses h1"],
            ["a3", "sufficient; gap sufficient; reason retained_evidence_covers_question; hypotheses h2"],
            ["a4", "clarify; gap ambiguous; reason ambiguous_question; hypotheses none"]])

    def test_kinds_of_option_texts_cards_and_reads(self):
        self.assertEqual([layout.body_kind(text) for text in (
            "read r4; gap unsupported", "sufficient; gap sufficient", "clarify; gap ambiguous",
            "no_justified_read; gap unsupported", "read rx; gap unsupported", "reading")],
            ["read", "sufficient", "clarify", "no_justified_read", "other", "other"])
        generic = card({"kind": "read", "choice": "c1"}, "unsupported", "additional_evidence_may_help",
                       "unknown", "uncertain")
        named = card({"kind": "read", "choice": "c1"}, "definition", "missing_definition", "unknown", "uncertain")
        self.assertEqual(layout.card_kind(generic), "read/unsupported/additional_evidence_may_help/generic")
        self.assertEqual(layout.card_kind(named), "read/definition/missing_definition")
        self.assertEqual(layout.card_kind(card({"kind": "clarify"}, "ambiguous", "ambiguous_question")),
                         "clarify/ambiguous/ambiguous_question")
        self.assertEqual(layout.read_form(["literal", "search", 1, 1, 0, 3, "abc"]), "literal:search")
        self.assertEqual(layout.read_form(["target", "context", "p:1-33"]), "target:context")
        with self.assertRaises(ValueError):
            layout.read_form(["other", "search", "x"])


class Permutations(unittest.TestCase):
    def test_rotation_and_reversal_sources(self):
        self.assertEqual(layout.rotation(7, 1), [6, 0, 1, 2, 3, 4, 5])
        self.assertEqual(layout.rotation(7, 3), [4, 5, 6, 0, 1, 2, 3])
        self.assertEqual(layout.rotation(5, 0), [0, 1, 2, 3, 4])
        self.assertEqual(layout.reversal(7), [6, 5, 4, 3, 2, 1, 0])
        self.assertEqual(layout.full_sources([1, 2, 0], 5), [1, 2, 0, 3, 4])
        for count, shift in ((7, 7), (7, -1), (0, 0)):
            with self.assertRaises(ValueError):
                layout.rotation(count, shift)

    def test_moved_options_keep_the_symbol_and_move_the_text(self):
        options = [["A", "a1", "x"], ["B", "a2", "y"], ["C", "a3", "z"]]
        self.assertEqual(layout.moved_options(options, [2, 0, 1], False),
                         [["A", "a1", "z"], ["B", "a2", "x"], ["C", "a3", "y"]])
        self.assertEqual(layout.moved_options(options, [2, 0, 1], True),
                         [["A", "a3", "z"], ["B", "a1", "x"], ["C", "a2", "y"]])
        with self.assertRaises(ValueError):
            layout.moved_options(options, [0, 0, 1], False)

    def test_moved_reads_reorder_the_reads_and_rebuild_the_path_table(self):
        self.assertEqual(layout.moved_reads(STATE, [0, 1, 2]), STATE)
        self.assertEqual(layout.moved_reads(STATE, [2, 1, 0]), (
            '{"format":"f","indexed_evidence":[[0,1,1,0,5,5,false,"e"]],"paths":["p0","p2","p1"],'
            '"read_choices_available":5,"read_choices_omitted":2,'
            '"verified_reads":[["literal","search",1,4,2,6,"wxyz"],["literal","search",2,1,0,3,"abc"],'
            '["target","context","p9:1-33"]]}'))
        for bad in ([0, 0, 1], [0, 3], [], [1.0]):
            with self.assertRaises(ValueError):
                layout.moved_reads(STATE, bad)
        with self.assertRaises(ValueError):
            layout.moved_reads(STATE.replace('"paths":["p0","p1","p2"]', '"paths":["p0","p1","p2","p3"]'), [0, 1, 2])

    def test_moved_reads_keep_a_selection_and_count_the_rest_as_omitted(self):
        self.assertEqual(layout.moved_reads(STATE, [2]), THIRD_READ)
        self.assertEqual(layout.moved_reads(STATE, [0]), (
            '{"format":"f","indexed_evidence":[[0,1,1,0,5,5,false,"e"]],"paths":["p0"],'
            '"read_choices_available":5,"read_choices_omitted":4,"verified_reads":[["target","context","p9:1-33"]]}'))

    def test_read_statement_uses_the_fields_of_the_read(self):
        self.assertEqual(layout.read_statement(["literal", "search", 1, 2, 0, 10, "John Smith"]),
                         "search for: John Smith")
        self.assertEqual(layout.read_statement(["target", "context", "docs/a.md:12-44"]),
                         "context of docs/a.md lines 12-44")
        self.assertEqual(layout.read_statement(["target", "context", "C:/x.md:1-33"]), "context of C:/x.md lines 1-33")
        self.assertEqual(layout.read_statement(["target", "context", "odd"]), "context of odd")

    def test_stated_options_add_the_read_to_read_cards_only(self):
        options = [["A", "a1", "read r2; gap unsupported"], ["B", "a2", "read r1; gap unsupported"],
                   ["C", "a3", "sufficient; gap sufficient"]]
        self.assertEqual(layout.stated_options(options, STATE), [
            ["A", "a1", "read r2 (search for: abc); gap unsupported"],
            ["B", "a2", "read r1 (context of p9 lines 1-33); gap unsupported"],
            ["C", "a3", "sufficient; gap sufficient"]])
        with self.assertRaises(ValueError):
            layout.stated_options([["A", "a1", "read r4; gap unsupported"]], STATE)

    def test_pair_parts_keep_one_read_card_and_the_sufficient_card(self):
        parts = PARTS | {"state": STATE, "options": [["A", "a1", "read r1; x"], ["B", "a2", "read r2; x"],
                                                      ["C", "a3", "sufficient; y"], ["D", "a4", "clarify; z"]]}
        plain = layout.pair_parts(parts, 2, False)
        self.assertEqual((plain["state"], plain["options"], plain["question"], plain["prefix"]),
                         (THIRD_READ, [["A", "a1", "read r1; x"], ["B", "a2", "sufficient; y"]],
                          'Which "one"?', "<|im_start|>system\nPick.<|im_end|>\n"))
        self.assertEqual(layout.pair_parts(parts, 2, True)["options"],
                         [["A", "a1", "sufficient; y"], ["B", "a2", "read r1; x"]])
        for options in ([["A", "a1", "read r2; x"], ["B", "a2", "sufficient; y"]],
                        [["A", "a1", "read r1; x"], ["B", "a2", "sufficient; y"], ["C", "a3", "sufficient; y"]],
                        [["A", "a1", "read r1; x"], ["B", "a2", "clarify; z"]]):
            with self.assertRaises(ValueError):
                layout.pair_parts(parts | {"options": options}, 2, False)

    def test_within_input_limits_follow_the_compiler(self):
        self.assertIs(layout.within_input_limits(PARTS, 8), True)
        self.assertIs(layout.within_input_limits(PARTS, 2), True)
        self.assertIs(layout.within_input_limits(PARTS, 1), False)
        self.assertIs(layout.within_input_limits(PARTS | {"question": " \n"}, 8), False)
        self.assertIs(layout.within_input_limits(PARTS | {"state": "s" * 65537}, 8), False)

        def with_first(name, text):
            return PARTS | {"options": [["A", name, text], ["B", "a2", "sufficient; gap sufficient"]]}

        self.assertIs(layout.within_input_limits(with_first("a1", "t" * 8192), 8), True)
        self.assertIs(layout.within_input_limits(with_first("a1", "t" * 8193), 8), False)
        # 4,097 two-byte letters are 8,194 bytes.
        self.assertIs(layout.within_input_limits(with_first("a1", "é" * 4097), 8), False)
        self.assertIs(layout.within_input_limits(with_first("a2", "t"), 8), False)
        self.assertIs(layout.within_input_limits(with_first("", "t"), 8), False)
        self.assertIs(layout.within_input_limits(with_first("a\t1", "t"), 8), False)
        self.assertIs(layout.within_input_limits(with_first("n" * 129, "t"), 8), False)

    def test_variant_plan_lists_every_family_after_the_identity(self):
        plan = layout.variant_plan(7, 5, ["cards", "cards_ids", "reads"])
        self.assertEqual([name for name, _, _ in plan], [
            "identity", "cards_rot1", "cards_rot2", "cards_rot3", "cards_rot4", "cards_rot5", "cards_rot6",
            "cards_reverse", "cards_ids_rot1", "cards_ids_rot3", "cards_ids_reverse",
            "reads_rot1", "reads_rot2", "reads_rot3", "reads_rot4"])
        chosen = {name: (family, sources) for name, family, sources in plan}
        self.assertEqual(chosen["identity"], ("identity", [0, 1, 2, 3, 4, 5, 6]))
        self.assertEqual(chosen["cards_ids_rot3"], ("cards_ids", [4, 5, 6, 0, 1, 2, 3]))
        self.assertEqual(chosen["reads_rot2"], ("reads", [3, 4, 0, 1, 2]))
        self.assertEqual(layout.variant_plan(2, 1, ["cards_ids"]), [
            ("identity", "identity", [0, 1]), ("cards_ids_rot1", "cards_ids", [1, 0]),
            ("cards_ids_reverse", "cards_ids", [1, 0])])
        self.assertEqual(layout.variant_plan(7, 5, ["letters"]), [
            ("identity", "identity", [0, 1, 2, 3, 4, 5, 6]), ("letters_rot1", "letters", [6, 0, 1, 2, 3, 4, 5]),
            ("letters_rot3", "letters", [4, 5, 6, 0, 1, 2, 3])])
        self.assertEqual(layout.variant_plan(7, 2, ["stated", "pairs"]), [
            ("identity", "identity", [0, 1, 2, 3, 4, 5, 6]), ("stated", "stated", [0, 1, 2, 3, 4, 5, 6]),
            ("stated_reads_rot1", "stated_reads", [1, 0]), ("pair_r1", "pair", [0]),
            ("pair_r1_swapped", "pair_swapped", [0]), ("pair_r2", "pair", [1]),
            ("pair_r2_swapped", "pair_swapped", [1])])
        with self.assertRaises(ValueError):
            layout.variant_plan(7, 5, ["symbols"])

    def test_moved_symbols_keep_every_line_and_move_the_symbol(self):
        options = [["A", "a1", "x"], ["B", "a2", "y"], ["C", "a3", "z"]]
        self.assertEqual(layout.moved_symbols(options, [2, 0, 1]),
                         [["B", "a1", "x"], ["C", "a2", "y"], ["A", "a3", "z"]])
        self.assertEqual(layout.variant_prompt(PARTS, "letters", [1, 0]), PROMPT.replace(
            'A = "a1"', 'B = "a1"').replace('B = "a2"', 'A = "a2"'))
        with self.assertRaises(ValueError):
            layout.moved_symbols(options, [0, 1])

    def test_variant_prompt_moves_only_what_its_family_names(self):
        self.assertEqual(layout.variant_prompt(PARTS, "identity", [0, 1]), PROMPT)
        swapped = PROMPT.replace('A = "a1": "read r1; gap unsupported"\nB = "a2": "sufficient; gap sufficient"',
                                 'A = "a1": "sufficient; gap sufficient"\nB = "a2": "read r1; gap unsupported"')
        self.assertEqual(layout.variant_prompt(PARTS, "cards", [1, 0]), swapped)
        self.assertEqual(layout.variant_prompt(PARTS, "cards_ids", [1, 0]),
                         swapped.replace('A = "a1"', 'A = "a2"').replace('B = "a2": "read', 'B = "a1": "read'))
        moved = layout.variant_prompt(PARTS | {"state": STATE}, "reads", [2, 1, 0])
        self.assertIn('[[\\"literal\\",\\"search\\",1,4,2,6,\\"wxyz\\"],', moved)
        self.assertTrue(moved.endswith(PROMPT[PROMPT.index("\nQuestion:\n"):]))
        with self.assertRaises(ValueError):
            layout.variant_prompt(PARTS, "identity", [1, 0])
        with self.assertRaises(ValueError):
            layout.variant_prompt(PARTS, "symbols", [1, 0])

    def test_variant_prompt_of_the_probe_families(self):
        parts = PARTS | {"state": STATE}
        tail = PROMPT[PROMPT.index("\nQuestion:\n"):]
        stated = layout.variant_prompt(parts, "stated", [0, 1])
        self.assertTrue(stated.endswith(tail.replace("read r1;", "read r1 (context of p9 lines 1-33);")))
        self.assertEqual(stated[:stated.index("\nQuestion:\n")],
                         layout.variant_prompt(parts, "identity", [0, 1])[:-len(tail)])
        # After the rotation the first read is the one that was third.
        turned = layout.variant_prompt(parts, "stated_reads", [2, 0, 1])
        self.assertTrue(turned.endswith(tail.replace("read r1;", "read r1 (search for: wxyz);")))
        self.assertEqual(turned[:turned.index("\nQuestion:\n")],
                         layout.variant_prompt(parts, "reads", [2, 0, 1])[:-len(tail)])
        pair = layout.variant_prompt(parts, "pair", [2])
        self.assertEqual(pair, layout.render_prompt(parts | {"state": THIRD_READ}))
        self.assertEqual(layout.variant_prompt(parts, "pair_swapped", [2]), pair.replace(
            'A = "a1": "read r1; gap unsupported"\nB = "a2": "sufficient; gap sufficient"',
            'A = "a1": "sufficient; gap sufficient"\nB = "a2": "read r1; gap unsupported"'))
        with self.assertRaises(ValueError):
            layout.variant_prompt(parts, "stated", [1, 0])
        with self.assertRaises(ValueError):
            layout.variant_prompt(parts, "pair", [0, 1])


class Outcomes(unittest.TestCase):
    def test_order_and_best_break_ties_towards_the_lower_slot(self):
        self.assertEqual(layout.order([1.0, 3.0, 3.0, 2.0]), [1, 2, 3, 0])
        self.assertEqual(layout.best([5.0, 9.0, 7.0], [0, 2]), 2)
        self.assertEqual(layout.best([5.0, 9.0, 5.0], [2, 0]), 0)
        self.assertIsNone(layout.best([5.0, 9.0], []))

    def test_follow_tells_content_from_slot(self):
        moved = [6, 0, 1, 2, 3, 4, 5]
        self.assertEqual(layout.follow(3, 4, moved), "content")
        self.assertEqual(layout.follow(3, 3, moved), "slot")
        self.assertEqual(layout.follow(3, 5, moved), "neither")
        mirrored = [6, 5, 4, 3, 2, 1, 0]
        self.assertEqual(layout.follow(3, 3, mirrored), "unmoved_kept")
        self.assertEqual(layout.follow(3, 1, mirrored), "unmoved_lost")
        self.assertEqual(layout.follow(5, 1, mirrored), "content")

    def test_centred_and_spread(self):
        self.assertEqual(layout.centred([1.0, 2.0, 3.0]), [-1.0, 0.0, 1.0])
        self.assertEqual(layout.spread([1.0, 2.0, 3.0]), {"count": 3, "mean": 2.0, "sd": 1.0, "min": 1.0, "max": 3.0})
        self.assertEqual(layout.spread([4.0]), {"count": 1, "mean": 4.0, "sd": 0.0, "min": 4.0, "max": 4.0})

    def test_profile_share_is_one_for_a_fixed_profile_and_zero_without_one(self):
        self.assertEqual(layout.profile_share([[3.0, 1.0], [5.0, 3.0]]), {"profile": [1.0, -1.0], "share": 1.0})
        self.assertEqual(layout.profile_share([[3.0, 1.0], [1.0, 3.0]]), {"profile": [0.0, 0.0], "share": 0.0})
        with self.assertRaises(ValueError):
            layout.profile_share([[1.0, 2.0], [1.0]])

    def test_split_by_row_separates_row_means_from_the_spread_inside_rows(self):
        # Row means 2 and 6 around 4: between 2 * (4 + 4) = 16. Inside the rows: 4 * 1 = 4.
        out = layout.split_by_row([[1.0, 3.0], [5.0, 7.0]])
        self.assertEqual((out["rows"], out["columns"], out["mean"], out["between_row_share"], out["within_row_share"]),
                         (2, 2, 4.0, 0.8, 0.2))
        self.assertAlmostEqual(out["row_mean_sd"], 2.8284271, places=6)
        self.assertAlmostEqual(out["within_row_sd"], 1.4142136, places=6)
        same = layout.split_by_row([[2.0, 2.0], [2.0, 2.0]])
        self.assertEqual((same["between_row_share"], same["within_row_share"]), (None, None))
        for bad in ([[1.0, 2.0]], [[1.0], [2.0]], [[1.0, 2.0], [1.0]]):
            with self.assertRaises(ValueError):
                layout.split_by_row(bad)

    def test_pearson_is_one_for_a_rising_line_and_none_without_variation(self):
        self.assertEqual(layout.pearson([1.0, 2.0, 3.0], [2.0, 4.0, 6.0]), 1.0)
        self.assertEqual(layout.pearson([1.0, 2.0, 3.0], [3.0, 2.0, 1.0]), -1.0)
        self.assertIsNone(layout.pearson([1.0, 2.0, 3.0], [5.0, 5.0, 5.0]))
        with self.assertRaises(ValueError):
            layout.pearson([1.0, 2.0], [1.0])

    def test_without_profile_removes_the_mean_of_the_other_rows(self):
        self.assertEqual(layout.without_profile([[3.0, 1.0], [5.0, 3.0], [1.0, 3.0]]),
                         [[1.0, -1.0], [1.0, -1.0], [-2.0, 2.0]])
        for bad in ([[3.0, 1.0]], [[3.0, 1.0], [1.0]]):
            with self.assertRaises(ValueError):
                layout.without_profile(bad)

    def test_effects_give_the_whole_variation_to_the_slot_when_scores_ignore_the_body(self):
        out = layout.effects([("a", [0, 1], [3.0, 1.0]), ("a", [1, 0], [3.0, 1.0])])
        self.assertEqual((out["rows"], out["forwards"], out["slot_mean"], out["body_mean"]),
                         (1, 2, [1.0, -1.0], [0.0, 0.0]))
        self.assertEqual(out["share"], {"slot_common": 1.0, "slot_by_row": 0.0, "body_common": 0.0,
                                        "body_by_row": 0.0, "residual": 0.0})
        self.assertEqual(out["row_body"], {"a": [0.0, 0.0]})

    def test_effects_give_it_to_the_body_by_row_when_rows_prefer_different_bodies(self):
        out = layout.effects([("a", [0, 1], [3.0, 1.0]), ("a", [1, 0], [1.0, 3.0]),
                              ("b", [0, 1], [1.0, 3.0]), ("b", [1, 0], [3.0, 1.0])])
        self.assertEqual((out["rows"], out["forwards"], out["slot_mean"], out["body_mean"]),
                         (2, 4, [0.0, 0.0], [0.0, 0.0]))
        self.assertEqual(out["share"], {"slot_common": 0.0, "slot_by_row": 0.0, "body_common": 0.0,
                                        "body_by_row": 1.0, "residual": 0.0})
        self.assertEqual(out["row_body"], {"a": [1.0, -1.0], "b": [-1.0, 1.0]})

    def test_effects_split_a_mixed_row_evenly(self):
        out = layout.effects([("a", [0, 1], [4.0, 0.0]), ("a", [1, 0], [1.0, 1.0])])
        self.assertEqual((out["slot_mean"], out["body_mean"]), ([1.0, -1.0], [1.0, -1.0]))
        self.assertEqual(out["share"], {"slot_common": 0.5, "slot_by_row": 0.0, "body_common": 0.5,
                                        "body_by_row": 0.0, "residual": 0.0})

    def test_effects_refuse_an_unbalanced_or_empty_design(self):
        with self.assertRaises(ValueError):
            layout.effects([("a", [0, 1], [3.0, 1.0])])
        with self.assertRaises(ValueError):
            layout.effects([])
        with self.assertRaises(ValueError):
            layout.effects([("a", [0, 0], [3.0, 1.0]), ("a", [1, 0], [3.0, 1.0])])


if __name__ == "__main__":
    unittest.main()
