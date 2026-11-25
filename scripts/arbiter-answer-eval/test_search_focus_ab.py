# SPDX-License-Identifier: Apache-2.0
"""Synthetic checks for the matched public Search Focus diagnostic."""

import copy
import json
import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
import search_focus_ab as ab
import search_shortlist_audit as audit


PROFILE = "f" * 64
TOKEN = "publication"
INDEX = "index"
REVISION = 7


def observation():
    a, b, c = ("a" * 64, "b" * 64, "c" * 64)
    return {"kind": "receipt", "receipt": {
        "schema": "airk.arbiter.focus.ranking/1",
        "scope": {"publication": {"token": TOKEN}, "index_id": INDEX, "configuration_revision": REVISION},
        "identity": {"profile": PROFILE, "runtime": ab.CUDA + "d" * 64},
        "applied": True, "availability": "ready", "input_digest": "e" * 64, "work": {"inference_us": 4},
        "original_order": [a, b, c], "ranked_order": [b, a, c],
        "candidate_count": 3, "scored_count": 2, "presented_count": 2,
        "scores": [{"candidate": a, "relevance": 0.1}, {"candidate": b, "relevance": 0.9}],
    }}


def hit(path, line):
    return {"citation": {"path": path, "start_line": line, "end_line": line + 5, "content_hash": "hash"},
            "text": "Synthetic returned hit.", "truncated": False}


class SearchFocusAB(unittest.TestCase):
    def test_cuda_scope_and_native_tail(self):
        result = ab.focus_receipt(observation(), PROFILE, TOKEN, INDEX, REVISION, ["B", "A", "C"])
        self.assertEqual(result["state"], "cuda_applied")
        self.assertEqual(result["unscored_tail_count"], 1)
        self.assertEqual(result["original_hit_sha256s"], ["A", "B", "C"])
        self.assertEqual(result["scored_hits"], [{"hit_sha256": "A", "relevance": 0.1},
                                                 {"hit_sha256": "B", "relevance": 0.9}])

    def test_foreign_runtime_moved_tail_and_disabled_focus_are_refused(self):
        for mutate in (
            lambda receipt: receipt["identity"].update(runtime="onnxruntime/1.30.0"),
            lambda receipt: receipt["ranked_order"].reverse(),
            lambda receipt: receipt["scope"].update(configuration_revision=19),
            lambda receipt: receipt.update(scored_count=1),
        ):
            with self.subTest(mutate=mutate):
                value = observation()
                mutate(value["receipt"])
                with self.assertRaises(common.HarnessError):
                    ab.focus_receipt(value, PROFILE, TOKEN, INDEX, REVISION, ["B", "A", "C"])
        with self.assertRaisesRegex(common.HarnessError, "off Search"):
            ab.focus_receipt(observation(), None, TOKEN, INDEX, REVISION, ["B", "A", "C"])

    def test_rules_measure_returned_citations_without_copying_content(self):
        request = {"repo": "bookbuilder", "slot": "C1", "key": "case", "request_sha256": "a" * 64,
                   "limit": 10, "max_bytes": 8192, "response_mode": "terse (product default)",
                   "essential": [{"id": "note", "mark": "essential", "path": "src/case.rs", "lines": [12, 14]}]}
        body = {"publication": {"token": TOKEN}, "execution": {"served_mode": "hybrid", "incomplete": False},
                "truncated": False, "hits": [hit("src/other.rs", 1), hit("src/case.rs", 10)]}
        result = common.CallOutcome(True, 0, b"private bytes", b"", 14, {"structuredContent": body}, None)
        row = ab.measure(request, result, None, TOKEN, INDEX, REVISION)
        self.assertEqual(row["notes"][0]["rule2"], 2)
        self.assertEqual(row["focus"]["state"], "absent")
        self.assertNotIn("Synthetic", str(row))

    def test_handle_physical_alias_is_bound_to_native_index_identity(self):
        binding = {"index_id": INDEX, "binding": "binding", "source_commit": "commit",
                   "pipeline_digest": "digest", "publication_token": TOKEN}
        handle = common.Handle(Path("handle"), "handle-sha", ("cli",), "control", {},
                               {"bookbuilder": INDEX}, bindings={"bookbuilder": "binding"},
                               aliases={"bookbuilder": "bookbuilder-v2"})
        raw = {"lifetime": "life", "instance": "instance", "image": {
            "git_head": "head", "service": {"sha256": "service"}, "cli": {"sha256": "cli"}},
            "indexes": {"bookbuilder-v2": binding}}
        identity = ab.live_identity(handle, raw, ["bookbuilder"])
        self.assertEqual(identity["indexes"]["bookbuilder"], {"physical": "bookbuilder-v2", **binding})
        raw["indexes"]["bookbuilder-v2"]["binding"] = "changed"
        with self.assertRaisesRegex(common.HarnessError, "alias or binding"):
            ab.live_identity(handle, raw, ["bookbuilder"])

    def arms(self):
        left = [{"repo": repo, "slot": slot, "key": f"{repo}/{slot}", "request_sha256": "b" * 64,
                 "limit": 10, "max_bytes": 8192, "response_mode": "terse (product default)", "elapsed_ms": 10,
                 "ok": True, "incomplete": False, "truncated": False, "hits_returned": 2,
                 "hit_sha256s": ["a", "b"], "notes": [{"id": "n", "rule2": 2}], "focus": {"state": "absent"}}
                for repo, slot in sorted(ab.EXPECTED)]
        right = copy.deepcopy(left)
        for row in right:
            row["elapsed_ms"] = 14
            row["hit_sha256s"] = ["b", "a"]
            row["notes"][0]["rule2"] = 1
            row["focus"] = {"state": "cuda_applied", "tail_preserved": True, "unscored_tail_count": 1,
                            "original_hit_sha256s": ["a", "b"]}
        shared = {"schema": ab.SCHEMA, "cohort_sha256": "c" * 64, "identity": {"publication": TOKEN}}
        return {**shared, "kind": "off", "rows": left}, {**shared, "kind": "focus", "rows": right}

    def test_paired_gain_and_latency(self):
        off, focus = self.arms()
        value = ab.compare(off, focus)
        self.assertTrue(value["complete"])
        self.assertEqual(value["counts"]["both_ranked_higher_with_focus"], 20)
        self.assertEqual(value["counts"]["applied_with_nonempty_tail"], 20)
        self.assertEqual(value["median_ms"]["paired_focus_minus_off"], 4)

    def test_missing_application_and_failed_read_are_counted(self):
        off, focus = self.arms()
        focus["rows"][0]["ok"] = False
        focus["rows"][1]["focus"] = {"state": "absent"}
        value = ab.compare(off, focus)
        self.assertFalse(value["complete"])
        self.assertEqual(value["counts"]["matched"], 19)
        self.assertEqual(value["counts"]["cuda_applied"], 18)

    def test_publication_and_request_drift_fail_closed(self):
        off, focus = self.arms()
        with self.assertRaisesRegex(common.HarnessError, "publication"):
            ab.compare(off, focus | {"identity": {"publication": "changed"}})
        focus["rows"][0]["limit"] = 40
        with self.assertRaisesRegex(common.HarnessError, "request"):
            ab.compare(off, focus)


class CumulativeSearch(unittest.TestCase):
    def report(self):
        off, focus = SearchFocusAB().arms()
        focus["stage_before"] = {"focus": PROFILE}
        report = ab.compare(off, focus)
        report["arm_sha256"] = {"off": "1" * 64, "focus": "2" * 64}
        report["identity"] = {"lifetime": "direct-Search", "indexes": {
            repo: {"physical": repo, "source_commit": "exact-source", "binding": "binding", "index_id": "index",
                   "pipeline_digest": "pipeline", "publication_token": "settled"}
            for repo in common.REPOS}}
        return report

    def first(self):
        original = self.report()
        current = copy.deepcopy(original)
        current["identity"]["lifetime"] = "milestone-1"
        return {"original": original, "prior": copy.deepcopy(original), "current": current}, {
            "original": "a" * 64, "prior": "a" * 64, "current": "b" * 64}

    def test_reports_paired_gains_losses_rank_and_focus_interaction(self):
        reports, pins = self.first()
        for row in (reports["original"]["rows"][0], reports["prior"]["rows"][0]):
            row["notes"] += [{"id": "extra", "off_rank": None, "focus_rank": 4}]
        current = reports["current"]["rows"][0]
        current["notes"][0]["off_rank"] = 1
        current["notes"] += [{"id": "extra", "off_rank": 2, "focus_rank": None}]
        current["focus_ms"] = 18
        report = ab.cumulative(1, reports, pins, False)
        result = report["comparisons"]["original"]["metrics"]["all"]
        self.assertEqual(result["essentials"], 21)
        self.assertEqual(result["off"]["gained"], 1)
        self.assertEqual(result["off"]["rank_up"], 1)
        self.assertEqual(result["focus"]["lost"], 1)
        self.assertEqual(result["focus_interaction"]["covered"], -2)
        self.assertEqual(result["off"]["current"]["p95_ms"], 10)
        self.assertEqual(len(report["comparisons"]["prior"]["changed_notes"]), 2)

    def test_cumulative_compares_distinct_prior_and_original(self):
        reports, pins = self.first()
        pins["prior"] = "c" * 64
        reports["prior"]["rows"][0]["notes"][0]["off_rank"] = None
        report = ab.cumulative(2, reports, pins, False)
        metrics = {name: row["metrics"]["all"] for name, row in report["comparisons"].items()}
        self.assertEqual(metrics["prior"]["off"]["gained"], 1)
        self.assertEqual(metrics["original"]["off"]["gained"], 0)
        with self.assertRaisesRegex(common.HarnessError, "step 1"):
            ab.cumulative(1, reports, pins, False)

    def test_same_publication_required_until_explicit_step_6(self):
        reports, pins = self.first()
        repo = common.REPOS[0]
        trial = reports["current"]["identity"]["indexes"][repo]
        trial["publication_token"] = "different"
        with self.assertRaisesRegex(common.HarnessError, "publication differs"):
            ab.cumulative(1, reports, pins, False)
        with self.assertRaisesRegex(common.HarnessError, "only at step 6"):
            ab.cumulative(1, reports, pins, True)
        pins["prior"] = "c" * 64
        result = ab.cumulative(6, reports, pins, True)
        self.assertEqual(result["index_changes"]["current"], {repo: ["publication_token"]})
        trial["source_commit"] = "different-source"
        with self.assertRaisesRegex(common.HarnessError, "source revisions"):
            ab.cumulative(6, reports, pins, True)
        trial.update(source_commit="exact-source", publication_token="settled", pipeline_digest="different-pipeline-only")
        with self.assertRaisesRegex(common.HarnessError, "different publication"):
            ab.cumulative(6, reports, pins, True)

    def test_shortlist_and_count_steps_keep_all_comparison_publications_pinned(self):
        for step in (7, 8):
            with self.subTest(step=step):
                reports, pins = self.first()
                pins["prior"] = "c" * 64
                result = ab.cumulative(step, reports, pins, False)
                self.assertEqual(result["index_changes"], {"original": {}, "prior": {}, "current": {}})

                repo = common.REPOS[0]
                for name in ("prior", "current"):
                    reports[name]["identity"]["indexes"][repo]["publication_token"] = "different"
                with self.assertRaisesRegex(common.HarnessError, "prior: .*publication differs"):
                    ab.cumulative(step, reports, pins, False)
                with self.assertRaisesRegex(common.HarnessError, "only at step 6"):
                    ab.cumulative(step, reports, pins, True)

    def test_refuses_request_settings_cohort_and_focus_profile_drift(self):
        for field, value in (("max_bytes", 7), ("limit", 9), ("response_mode", "full"), ("request_sha256", "q")):
            with self.subTest(field=field):
                reports, pins = self.first()
                reports["current"]["rows"][0][field] = value
                with self.assertRaisesRegex(common.HarnessError, "request settings"):
                    ab.cumulative(1, reports, pins, False)
        for field, value in (("focus_profile", "d" * 64), ("cohort_sha256", "e" * 64)):
            with self.subTest(field=field):
                reports, pins = self.first()
                reports["current"][field] = value
                with self.assertRaisesRegex(common.HarnessError, "cohort or native Focus profile"):
                    ab.cumulative(1, reports, pins, False)

    def test_original_pair_reuses_authenticated_historical_arm(self):
        reports, pins = self.first()
        old = reports["original"]
        for name in ("original", "prior"):
            reports[name].pop("focus_profile")
        original_focus = {"schema": ab.SCHEMA, "kind": "focus", "identity": old["identity"],
                          "cohort_sha256": old["cohort_sha256"],
                          "stage_before": {"focus": PROFILE, "context": None, "revision": 6},
                          "stage_after": {"focus": PROFILE, "context": None, "revision": 6}}
        self.assertEqual(ab.cumulative(1, reports, pins, False, original_focus)["focus_profile"], PROFILE)
        with self.assertRaisesRegex(common.HarnessError, "authenticated native Focus"):
            ab.cumulative(1, reports, pins, False)
        original_focus["stage_after"]["focus"] = "0" * 64
        with self.assertRaisesRegex(common.HarnessError, "changed during collection"):
            ab.cumulative(1, reports, pins, False, original_focus)

    def test_requires_complete_pair_and_valid_ranks(self):
        reports, pins = self.first()
        reports["current"]["complete"] = False
        with self.assertRaisesRegex(common.HarnessError, "authenticated complete"):
            ab.cumulative(1, reports, pins, False)
        reports["current"]["complete"] = True
        reports["current"]["rows"][0]["notes"][0]["off_rank"] = 11
        with self.assertRaisesRegex(common.HarnessError, "invalid matched rank"):
            ab.cumulative(1, reports, pins, False)


class AdaptiveSearch(unittest.TestCase):
    def test_only_omitted_limit_with_adaptive_profile_can_exceed_ten(self):
        request = {"repo": "bookbuilder", "slot": "C1", "key": "case", "request_sha256": "a" * 64,
                   "limit": 10, "max_bytes": 8192, "response_mode": "terse (product default)",
                   "arguments": {"query": "search"}, "essential": []}
        hits = [hit(f"src/{index}.rs", 1) | {"text": "\u00e9"} for index in range(12)]
        body = {"publication": {"token": TOKEN}, "execution": {"served_mode": "hybrid", "incomplete": False},
                "truncated": False, "hits": hits}
        outcome = common.CallOutcome(True, 0, b"private bytes", b"", 14, {"structuredContent": body}, None)
        result = ab.measure(request, outcome, None, TOKEN, INDEX, REVISION, adaptive=True)
        self.assertEqual(result["hits_returned"], 12)
        self.assertEqual(result["adaptive_response"], {"text_bytes": 24, "unique_paths": 12})
        self.assertTrue(result["adaptive_limit_omitted"])
        with self.assertRaisesRegex(common.HarnessError, "original limit"):
            ab.measure(request, outcome, None, TOKEN, INDEX, REVISION)
        explicit = request | {"arguments": request["arguments"] | {"limit": 10}}
        with self.assertRaisesRegex(common.HarnessError, "count or byte budget"):
            ab.measure(explicit, outcome, None, TOKEN, INDEX, REVISION, adaptive=True)
        body["hits"][0]["text"] = "\u00e9" * 4096
        with self.assertRaisesRegex(common.HarnessError, "count or byte budget"):
            ab.measure(request, outcome, None, TOKEN, INDEX, REVISION, adaptive=True)
        body["hits"] = hits[1:] * 2
        with self.assertRaisesRegex(common.HarnessError, "count or byte budget"):
            ab.measure(request, outcome, None, TOKEN, INDEX, REVISION, adaptive=True)

    def test_pair_checks_native_permutation_and_records_response_sizes(self):
        off, focus = SearchFocusAB().arms()
        off["identity"]["search_profile"] = "adaptive_preserve"
        for arm in (off, focus):
            for row in arm["rows"]:
                row.update(adaptive_limit_omitted=True, adaptive_response={"text_bytes": 22, "unique_paths": 2})
        value = ab.compare(off, focus)
        self.assertTrue(value["complete"])
        self.assertEqual(value["adaptive_responses"]["off"]["hits"]["below_ten"], 20)
        self.assertEqual(value["adaptive_responses"]["focus"]["text_bytes"]["median"], 22)
        self.assertEqual(value["counts"]["essential_focus_top10"], 20)
        focus["rows"][0]["focus"]["original_hit_sha256s"].reverse()
        with self.assertRaisesRegex(common.HarnessError, "adaptive Focus changed"):
            ab.compare(off, focus)

    def test_cumulative_splits_full_coverage_from_top_ten_and_checks_actual_depth(self):
        reports, pins = CumulativeSearch().first()
        pins["prior"] = "c" * 64
        current = reports["current"]
        current["identity"]["search_profile"] = "adaptive_results"
        for row in current["rows"]:
            row.update(off_hits=12, focus_hits=12, adaptive_limit_omitted=True,
                       off_adaptive_response={"text_bytes": 24, "unique_paths": 9},
                       focus_adaptive_response={"text_bytes": 24, "unique_paths": 9})
        current["rows"][0]["notes"][0]["off_rank"] = 12
        result = ab.cumulative(8, reports, pins, False)
        for reference in ("original", "prior"):
            metrics = result["comparisons"][reference]["metrics"]["all"]["off"]
            self.assertEqual(metrics["delta"]["covered"], 0)
            self.assertEqual(metrics["delta"]["top10"], -1)
            self.assertEqual(metrics["delta"]["top3"], -1)
        self.assertEqual(result["adaptive_responses"]["off"]["hits"]["above_ten"], 20)
        with self.assertRaisesRegex(common.HarnessError, "Step 8 current"):
            ab.cumulative(7, reports, pins, False)
        current["rows"][0]["notes"][0]["off_rank"] = 13
        with self.assertRaisesRegex(common.HarnessError, "invalid matched rank"):
            ab.cumulative(8, reports, pins, False)



class PrivateCutoff(unittest.TestCase):
    def event(self, applied):
        held = [f"{i:064x}" for i in range(8)]
        original = [f"{i:064x}" for i in range(8, 12)]
        order = (2, 1, 3, 0) if applied else (0, 1, 2, 3)
        ranked = [original[i] for i in order]
        native = observation()
        candidates = [c * 64 for c in "abcd"]
        native["receipt"].update(original_order=candidates, ranked_order=[candidates[i] for i in order],
                                 candidate_count=4, scored_count=4, presented_count=4)
        native["receipt"]["scores"] = [{"candidate": candidate, "relevance": score}
                                        for candidate, score in zip(candidates, (.1, .6, .9, .3), strict=True)]
        return {"query_sha256": common.sha256_bytes(b"test"), "publication": {"token": TOKEN},
                "held_hit_sha256s": held, "original_hit_sha256s": original, "ranked_hit_sha256s": ranked,
                "accepted": applied, "observation": native if applied else None}

    def row(self, event, profile):
        hits = event["held_hit_sha256s"] + (
            event["ranked_hit_sha256s"] if event["accepted"] else event["original_hit_sha256s"])[:2]
        receipt, proof = ab.cutoff_receipt(event, profile, TOKEN, INDEX, REVISION, hits)
        return {"focus": receipt, "cutoff": proof, "hit_sha256s": hits}

    def test_cutoff_event_binds_one_append_scope_query_and_tail(self):
        event = self.event(True)
        before = [b"previous event"]
        encoded = json.dumps(event).encode("utf-8")
        self.assertEqual(ab.cutoff_event(before, [*before, encoded], "test", TOKEN), event)
        with self.assertRaisesRegex(common.HarnessError, "query or publication"):
            ab.cutoff_event(before, [*before, encoded], "wrong", TOKEN)
        with self.assertRaisesRegex(common.HarnessError, "exactly one new"):
            ab.cutoff_event(before, [*before, encoded, encoded], "test", TOKEN)
        with self.assertRaisesRegex(common.HarnessError, "exactly one new"):
            ab.cutoff_event(before, [b"changed", encoded], "test", TOKEN)

    def test_focus_receipt_rejects_drift_and_preserves_fallback(self):
        raw = self.event(True)
        off, focus = self.row(self.event(False), None), self.row(raw, PROFILE)
        self.assertEqual(focus["focus"]["state"], "cuda_cutoff_applied")
        self.assertNotIn("tail_preserved", focus["focus"])
        self.assertEqual(ab.compare_cutoff(off, focus), 1)

        with self.assertRaisesRegex(common.HarnessError, "public Search differs"):
            ab.cutoff_receipt(raw, PROFILE, TOKEN, INDEX, REVISION, off["hit_sha256s"])
        with self.assertRaisesRegex(common.HarnessError, "off Search returned"):
            ab.cutoff_receipt(raw, None, TOKEN, INDEX, REVISION, focus["hit_sha256s"])

        partial = copy.deepcopy(raw)
        receipt = partial["observation"]["receipt"]
        receipt.update(scored_count=2, presented_count=2, scores=receipt["scores"][:2],
                       ranked_order=[receipt["original_order"][i] for i in (1, 0, 2, 3)])
        partial["ranked_hit_sha256s"] = [partial["original_hit_sha256s"][i] for i in (1, 0, 2, 3)]
        with self.assertRaisesRegex(common.HarnessError, "acceptance.*differs"):
            self.row(partial, PROFILE)
        fallback = self.row(partial | {"accepted": False}, PROFILE)
        self.assertEqual(fallback["focus"]["state"], "not_applied")
        self.assertEqual(ab.compare_cutoff(off, fallback), 0)

        altered = self.event(False)
        altered["original_hit_sha256s"][-1] = altered["ranked_hit_sha256s"][-1] = "f" * 64
        with self.assertRaisesRegex(common.HarnessError, "private cutoff custody"):
            ab.compare_cutoff(self.row(altered, None), focus)

    def test_paired_comparison_counts_only_complete_cutoff(self):
        off, focus = SearchFocusAB().arms()
        off["identity"]["search_profile"] = ab.CUTOFF_PROFILE
        for arm, accepted, profile in ((off, False, None), (focus, True, PROFILE)):
            for row in arm["rows"]:
                row.update(self.row(self.event(accepted), profile), hits_returned=10)
        comparison = ab.compare(off, focus)
        self.assertTrue(comparison["complete"])
        self.assertEqual(comparison["counts"]["cutoff_applied"], 20)
        self.assertEqual(comparison["counts"]["cutoff_replacements"], 20)
        self.assertEqual(comparison["counts"]["applied_with_nonempty_tail"], 0)
        focus["rows"][0].update(self.row(self.event(False), PROFILE))
        comparison = ab.compare(off, focus)
        self.assertFalse(comparison["complete"])
        self.assertEqual(comparison["counts"]["cutoff_applied"], 19)



class ShortlistAudit(unittest.TestCase):
    def test_deterministic_projection_binds_focus_and_rule2(self):
        candidates = [[{**hit(path, 1)["citation"], "location": {"Lines": {"start": 1, "end": 6}}}, semantic]
                      for path, semantic in (("first.md", False), ("second.md", False), ("needed.md", True))]
        note = {"id": "n", "path": "needed.md", "lines": [1, 6]}
        request = {"limit": 2, "essential": [note]}
        trace = {"candidates": candidates, "selected": [0, 2]}
        off = {"hit_citations": [candidates[i][0] for i in (0, 2)], "hit_sha256s": ["a", "c"],
               "notes": [{"id": "n", "rule2": 2}]}
        focus = {"hit_citations": [candidates[i][0] for i in (2, 0)], "hit_sha256s": ["c", "a"],
                 "focus": {"state": "cuda_applied", "original_hit_sha256s": ["a", "c"]},
                 "notes": [{"id": "n", "rule2": 1}]}
        projected = audit.validated_projection(trace, request, off, focus)
        self.assertEqual(audit.note_ranks(request, *projected, off, focus),
                         [{"id": "n", "shortlist_rank": 3, "semantic_shortlist_rank": 3, "off_rank": 2,
                           "focus_rank": 1, "from_below_cutoff": True}])
        with self.assertRaisesRegex(common.HarnessError, "differ from the native selection"):
            audit.validated_projection(trace | {"selected": [0, 1]}, request, off, focus)
        with self.assertRaisesRegex(common.HarnessError, "differ from the native permutation"):
            audit.validated_projection(trace, request, off, focus | {"hit_citations": off["hit_citations"]})

    def test_trace_requires_two_identical_observations_from_pinned_publication(self):
        requests = [{"repo": "bookbuilder", "slot": "C1", "arguments": {"query": "what is Search?"}}]
        indexes = {"bookbuilder": {"publication_token": "airk-r1:abc:7:xyz"}}
        row = {"query_sha256": common.sha256_bytes(b"what is Search?"),
               "publication": {"incarnation": "abc", "sequence": 7, "digest": "xyz"},
               "candidates": [], "selected": []}

        def log(*rows):
            return "".join(audit.MARKER + json.dumps(r) + "\n" for r in rows).encode()

        self.assertEqual(audit.trace_pairs(log(row, row), requests, indexes), {("bookbuilder", "C1"): row})
        with self.assertRaisesRegex(common.HarnessError, "identical pre-Focus"):
            audit.trace_pairs(log(row, row | {"selected": [0]}), requests, indexes)
        with self.assertRaisesRegex(common.HarnessError, "unexpected shortlist query or publication"):
            audit.trace_pairs(log(row | {"publication": row["publication"] | {"sequence": 8}}), requests, indexes)



if __name__ == "__main__":
    unittest.main()
