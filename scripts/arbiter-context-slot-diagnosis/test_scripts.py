# SPDX-License-Identifier: Apache-2.0
"""Controls for the pure helpers of the slot diagnosis scripts. No tokenizer, model, provider or private file.

Every expected value below is written out by hand, not computed by the code under test.
"""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_variants
import layout
import report
import score_variants
import slot_table

READ = "read/unsupported/additional_evidence_may_help/generic"
ENOUGH = "sufficient/sufficient/retained_evidence_covers_question"
UNIT = {"record": "x", "result_pointer": "/hits/0"}
STATE = ('{"indexed_evidence":[[0,1,1,0,5,9,false,"hello"]],"paths":["p0","p1"],"read_choices_available":4,'
         '"read_choices_omitted":2,"verified_reads":[["target","context","p0:1-33"],'
         '["literal","search",1,2,0,3,"abc"]]}')
PROMPT = ('<|im_start|>system\nPick.<|im_end|>\n<|im_start|>user\nState:\n'
          '"{\\"indexed_evidence\\":[[0,1,1,0,5,9,false,\\"hello\\"]],\\"paths\\":[\\"p0\\",\\"p1\\"],'
          '\\"read_choices_available\\":4,\\"read_choices_omitted\\":2,\\"verified_reads\\":'
          '[[\\"target\\",\\"context\\",\\"p0:1-33\\"],[\\"literal\\",\\"search\\",1,2,0,3,\\"abc\\"]]}"\n'
          'Question:\n"Q?"\nOptions:\n'
          'A = "a1": "read r1; gap unsupported; reason additional_evidence_may_help; hypotheses h1"\n'
          'B = "a2": "read r2; gap unsupported; reason additional_evidence_may_help; hypotheses h1"\n'
          'C = "a3": "sufficient; gap sufficient; reason retained_evidence_covers_question; hypotheses h2"\n'
          'Choose one option symbol:<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n')


def card(identity, action, gap, reason, role, relevance):
    return {"id": identity, "action": action, "gap": gap, "reason": reason,
            "assessments": [{"evidence": UNIT, "role": role, "relevance": relevance}]}


def panel_row(**changes):
    roster = {"basis": {"source_units_presented": [UNIT], "read_choices_scored": ["c1", "c2"]}, "cards": [
        card("k1", {"kind": "read", "choice": "c1"}, "unsupported", "additional_evidence_may_help", "unknown", "uncertain"),
        card("k2", {"kind": "read", "choice": "c2"}, "unsupported", "additional_evidence_may_help", "unknown", "uncertain"),
        card("k3", {"kind": "sufficient"}, "sufficient", "retained_evidence_covers_question", "document", "useful")]}
    compiled = {"prompt": PROMPT, "input_ids": [5, 6, 7, 8], "attention_mask": [1, 1, 1, 1],
                "position_ids": [0, 1, 2, 3], "decision_position": 3, "candidate_positions": [32, 33, 34],
                "option_mask": [True, True, True]}
    return {"ordinal": 0, "remaining_input_tokens": 10, "roster": roster, "compiled": compiled | changes}


def described(ordinal, scores, ranked, forms, certified, progress, shown):
    return {"ordinal": ordinal, "tokens": 4, "evidence_units": 1, "evidence_excerpt_bytes": [shown],
            "evidence_original_bytes": [9], "read_choices_available": 4, "read_choices_omitted": 2,
            "card_kinds": [READ, READ, ENOUGH], "option_kinds": ["read", "read", "sufficient"], "read_forms": forms,
            "scores": scores, "order": ranked, "margin": 2.0, "certified_slots": certified,
            "reference_progress": progress}


class SlotTable(unittest.TestCase):
    def test_static_checks_pass_a_row_built_as_the_template_says(self):
        self.assertEqual(slot_table.static_checks(panel_row(), [32, 33, 34, 35], 2048), {
            "options_equal_roster_cards": True, "option_texts_distinct": True, "one_option_per_card": True,
            "candidates_are_the_symbols_in_order": True, "no_option_masked": True,
            "decision_is_the_last_token": True, "attention_is_all_ones": True, "positions_are_a_plain_range": True,
            "within_compiler_token_limit": True, "within_remaining_allowance": True})

    def test_static_checks_name_each_departure(self):
        def failed(row, limit=2048):
            return sorted(name for name, ok in slot_table.static_checks(row, [32, 33, 34, 35], limit).items() if not ok)

        self.assertEqual(failed(panel_row(candidate_positions=[33, 32, 34])), ["candidates_are_the_symbols_in_order"])
        self.assertEqual(failed(panel_row(decision_position=2)), ["decision_is_the_last_token"])
        self.assertEqual(failed(panel_row(option_mask=[True, False, True])), ["no_option_masked"])
        self.assertEqual(failed(panel_row(attention_mask=[1, 1, 1, 0])), ["attention_is_all_ones"])
        self.assertEqual(failed(panel_row(), limit=3), ["within_compiler_token_limit"])
        self.assertEqual(failed(panel_row(prompt=PROMPT.replace("read r2", "read r1"))),
                         ["option_texts_distinct", "options_equal_roster_cards"])
        short = panel_row()
        short["remaining_input_tokens"] = 3
        self.assertEqual(failed(short), ["within_remaining_allowance"])

    def test_described_keeps_slots_kinds_and_numbers_only(self):
        original = {"reference_proxies": {
            "valid_reference_proxy_whole_cards": [
                {"card": "k2", "choice": "c2", "proxy": "generic_read_original_reference_complement"}],
            "counterfactual_reference_progress": [{"choice": "c1", "classification": "UNKNOWN"},
                                                  {"choice": "c2", "classification": "REFERENCE_GAIN"}],
            "counterfactual_unresolved": []}}
        self.assertEqual(layout.parse_prompt(PROMPT)["state"], STATE)
        self.assertEqual(slot_table.described(panel_row(), [1.0, 5.0, 3.0], original), {
            "ordinal": 0, "tokens": 4, "state_characters": 207, "evidence_units": 1, "evidence_excerpt_bytes": [5],
            "evidence_original_bytes": [9], "read_choices_available": 4, "read_choices_omitted": 2,
            "card_kinds": [READ, READ, ENOUGH], "option_kinds": ["read", "read", "sufficient"],
            "read_forms": ["target:context", "literal:search"], "scores": [1.0, 5.0, 3.0], "order": [1, 2, 0],
            "margin": 2.0, "certified_slots": [1], "reference_progress": ["UNKNOWN", "REFERENCE_GAIN", None]})
        original["reference_proxies"]["valid_reference_proxy_whole_cards"][0]["choice"] = "c1"
        with self.assertRaises(ValueError):
            slot_table.described(panel_row(), [1.0, 5.0, 3.0], original)

    def test_described_marks_an_unresolved_probe(self):
        original = {"reference_proxies": {
            "valid_reference_proxy_whole_cards": [],
            "counterfactual_reference_progress": [{"choice": "c1", "classification": "UNKNOWN"}],
            "counterfactual_unresolved": [{"choice": {"id": "c2"}}]}}
        out = slot_table.described(panel_row(), [1.0, 5.0, 3.0], original)
        self.assertEqual((out["certified_slots"], out["reference_progress"]), ([], ["UNKNOWN", "UNRESOLVED", None]))

    def test_tables_count_by_slot(self):
        rows = [described(0, [1.0, 5.0, 3.0], [1, 2, 0], ["target:context", "literal:search"], [1],
                          ["UNKNOWN", "REFERENCE_GAIN", None], 5),
                described(1, [3.0, 1.0, 5.0], [2, 0, 1], ["target:context", "target:context"], [],
                          ["UNKNOWN", "UNKNOWN", None], 9)]
        out = slot_table.tables(rows)
        self.assertEqual(out["rows"], 2)
        self.assertEqual(out["kind_by_slot"], [{READ: 2}, {READ: 2}, {ENOUGH: 2}])
        self.assertIs(out["kind_constant_in_every_slot"], True)
        self.assertEqual(out["read_form_by_read_slot"],
                         [{"target:context": 2}, {"literal:search": 1, "target:context": 1}])
        self.assertEqual([item["mean"] for item in out["score_by_slot"]], [2.0, 3.0, 4.0])
        self.assertEqual([item["mean"] for item in out["centred_score_by_slot"]], [-1.0, 0.0, 1.0])
        self.assertEqual(out["rank_counts_by_slot"], [
            {"first": 0, "second": 1, "last": 1, "by_rank": [0, 1, 1]},
            {"first": 1, "second": 0, "last": 1, "by_rank": [1, 0, 1]},
            {"first": 1, "second": 1, "last": 0, "by_rank": [1, 1, 0]}])
        kinds = out["score_by_option_kind"]
        self.assertEqual((kinds["read"]["raw"]["count"], kinds["read"]["raw"]["mean"], kinds["read"]["centred"]["mean"]),
                         (4, 2.5, -0.5))
        self.assertEqual((kinds["sufficient"]["raw"]["mean"], kinds["sufficient"]["centred"]["mean"]), (4.0, 1.0))
        self.assertEqual([(item["slot"], item["form"], item["count"], item["mean"])
                          for item in out["centred_score_by_read_slot_and_form"]],
                         [(0, "target:context", 2, -1.0), (1, "literal:search", 1, 2.0), (1, "target:context", 1, -2.0)])
        self.assertEqual(out["read_slot_orders"], {"1>0": 1, "0>1": 1})
        self.assertEqual(out["fixed_profile"], {"profile": [-1.0, 0.0, 1.0], "share": 0.25})
        self.assertEqual((out["winner_slot_counts"], out["winner_margin"]["mean"]), ({1: 1, 2: 1}, 2.0))
        self.assertEqual((out["certified_rows"], out["certified_cards"], out["certified_by_slot"],
                          out["certified_by_read_form"], out["certified_rank_counts"],
                          out["certified_rank_among_reads"]), ([0], 1, {1: 1}, {"literal:search": 1}, {1: 1}, {1: 1}))
        # With the other row's centred scores removed, the rows are [-2, 4, -2] and [2, -4, 2].
        self.assertEqual(out["profile_removed"], {"winner_slot_counts": {0: 1, 1: 1},
                                                  "best_read_slot_counts": {0: 1, 1: 1},
                                                  "certified_rank_among_reads": {1: 1}})
        self.assertEqual(out["reference_progress_by_slot"],
                         [{"UNKNOWN": 2}, {"REFERENCE_GAIN": 1, "UNKNOWN": 1}, {}])
        self.assertEqual((out["evidence_units"], out["evidence_units_cut_short"], out["read_choices_available"],
                          out["read_choices_omitted"]), ({1: 2}, 1, {4: 2}, {2: 2}))


class Variants(unittest.TestCase):
    def test_chosen_ordinals_join_the_listed_and_the_spaced_ones(self):
        available = list(range(10))
        self.assertEqual(build_variants.chosen_ordinals(available, [3], 4), [0, 3, 4, 8])
        self.assertEqual(build_variants.chosen_ordinals(available, [7, 2], None), [2, 7])
        self.assertEqual(build_variants.chosen_ordinals(available, [], None), available)
        with self.assertRaises(ValueError):
            build_variants.chosen_ordinals(available, [12], None)

    def test_the_assistant_turn_is_the_end_of_the_template(self):
        self.assertEqual(build_variants.ASSISTANT, "<|im_start|>assistant\n<think>\n\n</think>\n\n")


class FakeArray:
    def __init__(self, values, dtype, ndim=1):
        self.values, self.dtype, self.ndim = values, dtype, ndim

    def __getitem__(self, key):
        return FakeArray(self.values, self.dtype, self.ndim + 1)


class FakeNumpy:
    int64, bool_ = "int64", "bool"

    @staticmethod
    def asarray(values, dtype):
        return FakeArray(values, dtype)


GRAPH = [("input_ids", "tensor(int64)", 2), ("attention_mask", "tensor(int64)", 2), ("position_ids", "tensor(int64)", 2),
         ("decision_positions", "tensor(int64)", 1), ("candidate_positions", "tensor(int64)", 2),
         ("option_mask", "tensor(bool)", 2)]


class Scoring(unittest.TestCase):
    def test_compiled_inputs_decide_at_the_last_token_with_no_padding(self):
        self.assertEqual(score_variants.compiled_inputs([7, 8, 9], [32, 33]), {
            "input_ids": [7, 8, 9], "attention_mask": [1, 1, 1], "position_ids": [0, 1, 2],
            "decision_positions": [2], "candidate_positions": [32, 33], "option_mask": [True, True]})
        for ids, candidates in (([], [32, 33]), ([7], [32])):
            with self.assertRaises(ValueError):
                score_variants.compiled_inputs(ids, candidates)

    def test_feeds_add_the_batch_axis_only_where_the_graph_has_one(self):
        out = score_variants.feeds(GRAPH, score_variants.compiled_inputs([7, 8, 9], [32, 33]), FakeNumpy)
        self.assertEqual({name: array.ndim for name, array in out.items()},
                         {"input_ids": 2, "attention_mask": 2, "position_ids": 2, "decision_positions": 1,
                          "candidate_positions": 2, "option_mask": 2})
        self.assertEqual((out["decision_positions"].values, out["option_mask"].dtype), ([2], "bool"))

    def test_feeds_refuse_another_graph(self):
        values = score_variants.compiled_inputs([7, 8, 9], [32, 33])
        for graph in (GRAPH + [("past_key", "tensor(float)", 1)], GRAPH[:5],
                      GRAPH[:3] + [("decision_positions", "tensor(int64)", 3)] + GRAPH[4:]):
            with self.assertRaises(ValueError):
                score_variants.feeds(graph, values, FakeNumpy)

    def test_difference_is_the_largest_absolute_gap(self):
        self.assertEqual(score_variants.difference([1.0, 2.5], [1.25, 2.0]), 0.5)
        with self.assertRaises(ValueError):
            score_variants.difference([1.0], [1.0, 2.0])


KINDS = ["read", "read", "sufficient"]


def forward(name, family, sources, scores):
    return {"name": name, "family": family, "sources": sources, "scores": scores}


class Report(unittest.TestCase):
    def test_merged_rows_join_receipts_by_ordinal_and_keep_each_variant_once(self):
        first = {"rows": [{"ordinal": 3, "variants": [forward("identity", "identity", [0, 1], [3.0, 1.0]),
                                                      forward("cards_rot1", "cards", [1, 0], [2.0, 4.0])]}]}
        second = {"rows": [{"ordinal": 3, "variants": [forward("identity", "identity", [0, 1], [3.0005, 1.0]),
                                                       forward("letters_rot1", "letters", [1, 0], [5.0, 6.0])]},
                           {"ordinal": 1, "variants": [forward("identity", "identity", [0, 1], [7.0, 8.0])]}]}
        self.assertEqual(report.merged_rows([first, second]), [
            {"ordinal": 1, "variants": [forward("identity", "identity", [0, 1], [7.0, 8.0])]},
            {"ordinal": 3, "variants": [forward("identity", "identity", [0, 1], [3.0, 1.0]),
                                        forward("cards_rot1", "cards", [1, 0], [2.0, 4.0]),
                                        forward("letters_rot1", "letters", [1, 0], [5.0, 6.0])]}])
        second["rows"][0]["variants"][0]["scores"] = [3.5, 1.0]
        with self.assertRaises(ValueError):
            report.merged_rows([first, second])

    def test_outcome_of_moved_symbols_tells_the_symbol_from_the_line(self):
        # Symbol B is attached to line 0 and symbol A to line 1. The original winner was line 1, a read.
        moved = forward("letters_rot1", "letters", [1, 0, 2], [6.0, 1.0, 3.0])
        self.assertEqual(report.outcome([1.0, 5.0, 3.0], moved, KINDS)["winner"], "content")
        stayed = forward("letters_rot1", "letters", [1, 0, 2], [1.0, 6.0, 3.0])
        self.assertEqual(report.outcome([1.0, 5.0, 3.0], stayed, KINDS)["winner"], "slot")

    def test_outcome_labels_content_slot_and_neither(self):
        identity, moved = [1.0, 5.0, 3.0], [2, 0, 1]
        self.assertEqual(report.outcome(identity, forward("cards_rot1", "cards", moved, [0.0, 1.0, 6.0]), KINDS), {
            "winner": "content", "winner_slot": 2, "winner_body": 1, "winner_kind": "read",
            "winner_kind_before": "read", "best_read": "content", "best_read_slot": 2, "best_read_body": 1,
            "original_read_slot_still_shows_a_read": True})
        self.assertEqual(report.outcome(identity, forward("cards_rot1", "cards", moved, [0.0, 6.0, 1.0]), KINDS), {
            "winner": "slot", "winner_slot": 1, "winner_body": 0, "winner_kind": "read",
            "winner_kind_before": "read", "best_read": "slot", "best_read_slot": 1, "best_read_body": 0,
            "original_read_slot_still_shows_a_read": True})
        self.assertEqual(report.outcome(identity, forward("cards_rot1", "cards", moved, [6.0, 1.0, 0.0]), KINDS), {
            "winner": "neither", "winner_slot": 0, "winner_body": 2, "winner_kind": "sufficient",
            "winner_kind_before": "read", "best_read": "slot", "best_read_slot": 1, "best_read_body": 0,
            "original_read_slot_still_shows_a_read": True})

    def test_outcome_extends_a_read_permutation_to_the_other_slots(self):
        out = report.outcome([1.0, 5.0, 3.0], forward("reads_rot1", "reads", [1, 0], [5.0, 1.0, 3.0]), KINDS)
        self.assertEqual((out["winner"], out["winner_slot"], out["winner_body"], out["best_read"]),
                         ("content", 0, 1, "content"))
        kept = report.outcome([1.0, 2.0, 9.0], forward("reads_rot1", "reads", [1, 0], [2.0, 1.0, 9.0]), KINDS)
        self.assertEqual((kept["winner"], kept["winner_kind"], kept["best_read"]),
                         ("unmoved_kept", "sufficient", "content"))

    def test_counted_tallies_every_label(self):
        identity, moved = [1.0, 5.0, 3.0], [2, 0, 1]
        outcomes = [report.outcome(identity, forward("cards_rot1", "cards", moved, scores), KINDS)
                    for scores in ([0.0, 1.0, 6.0], [0.0, 6.0, 1.0], [6.0, 1.0, 0.0])]
        self.assertEqual(report.counted(outcomes), {
            "forwards": 3, "winner": {"content": 1, "neither": 1, "slot": 1}, "best_read": {"content": 1, "slot": 2},
            "winner_slot": {0: 1, 1: 1, 2: 1}, "winner_body": {0: 1, 1: 1, 2: 1},
            "winner_kind": {"read": 2, "sufficient": 1}, "winner_kind_changed": 1,
            "best_read_slot": {1: 2, 2: 1}, "best_read_body": {0: 2, 1: 1},
            "original_read_slot_still_shows_a_read": 3})

    def test_design_uses_the_identity_and_the_rotations_of_complete_rows(self):
        kinds = ["read", "sufficient"]
        rows = [{"ordinal": 7, "variants": [forward("identity", "identity", [0, 1], [3.0, 1.0]),
                                            forward("cards_rot1", "cards", [1, 0], [3.0, 1.0]),
                                            forward("cards_reverse", "cards", [1, 0], [9.0, 0.0])]},
                {"ordinal": 8, "variants": [forward("identity", "identity", [0, 1], [3.0, 1.0])]}]
        split, preferred = report.design(rows, "cards", kinds)
        self.assertEqual((split["rows"], split["forwards"], split["slot_mean"], split["body_mean"]),
                         (1, 2, [1.0, -1.0], [0.0, 0.0]))
        self.assertEqual(split["share"]["slot_common"], 1.0)
        self.assertEqual(split["cell_mean_by_slot_then_body"], [[1.0, 1.0], [-1.0, -1.0]])
        self.assertEqual((split["slot_averaged_first_body"], split["slot_averaged_first_read"]), ({0: 1}, {0: 1}))
        self.assertEqual(preferred, {7: [0, 1]})
        self.assertIsNone(report.design(rows, "reads", ["read", "read"]))

    def test_design_of_reads_covers_the_read_slots_only(self):
        rows = [{"ordinal": 2, "variants": [forward("identity", "identity", [0, 1, 2], [3.0, 1.0, 50.0]),
                                            forward("reads_rot1", "reads", [1, 0], [1.0, 3.0, 70.0])]}]
        split, preferred = report.design(rows, "reads", KINDS)
        self.assertEqual((split["slot_mean"], split["body_mean"], split["share"]["body_by_row"]),
                         ([0.0, 0.0], [1.0, -1.0], 0.0))
        self.assertEqual((split["share"]["body_common"], preferred), (1.0, {2: [0, 1]}))

    def test_certified_ranks_count_among_the_read_cards(self):
        kinds = ["read", "read", "read", "sufficient"]
        table_rows = {4: {"certified_slots": [2]}, 5: {"certified_slots": []}}
        scored = {4: [5.0, 1.0, 3.0, 9.0], 5: [1.0, 2.0, 3.0, 4.0]}
        preferred = {"cards": {4: [3, 2, 0, 1]}, "reads": {4: [1, 2, 0]}}
        self.assertEqual(report.certified_ranks(table_rows, scored, preferred, kinds), [
            {"ordinal": 4, "certified_slots": [2], "original": [2], "cards_slot_averaged": [1],
             "reads_slot_averaged": [2]}])


if __name__ == "__main__":
    unittest.main()
