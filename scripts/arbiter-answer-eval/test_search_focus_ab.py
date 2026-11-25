# SPDX-License-Identifier: Apache-2.0
"""Synthetic checks for the matched public Search Focus diagnostic."""

import copy
import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
import search_focus_ab as ab


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


if __name__ == "__main__":
    unittest.main()
