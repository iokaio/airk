# SPDX-License-Identifier: Apache-2.0
"""Independent synthetic controls for the TechQA stage-arm runner and scorer."""
import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest

HERE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
runner = runpy.run_path(str(HERE / "run_techqa_stage_arms.py"))
scorer = runpy.run_path(str(HERE / "score_techqa_stage_arms.py"))

# Synthetic sources. The second gold line occupies bytes 11 to 34.
FILES = {"gold.txt": b"alpha line\nthe answer is forty two\nomega line\n", "other.txt": b"first\nsecond\nthird\n"}
PUBLICATION = {"token": "airk-r1:synthetic", "collection": "synthetic", "generation": 1}
SCOPE = {"index_id": "idx_synthetic", "publication": PUBLICATION}
OBSERVATION = {"fingerprint": "synthetic"}
WORK = {"inference_us": 4000, "compile_us": 10, "queue_us": 1, "policy_us": 1, "input_tokens": 1024}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


MEMBERS = [{"path": name, "original_sha256": sha(raw)} for name, raw in FILES.items()]
# "answer is forty" is bytes 15 to 30 of the gold file.
TARGET = {"original_path": "gold.txt", "byte_start": 15, "byte_end_exclusive": 30,
          "span_sha256": sha(FILES["gold.txt"][15:30]), "source_sha256": sha(FILES["gold.txt"])}


def hit(name, text, first, last):
    return {"text": text, "truncated": False,
            "citation": {"path": name, "content_hash": sha(FILES[name]), "start_line": first, "end_line": last,
                         "location": {"Lines": {"start": first, "end": last}}}}


def record(operation, hits, focus=None):
    result = {"publication": PUBLICATION, "scope_observation": OBSERVATION, "hits": hits}
    if focus is not None:
        result["focus"] = focus
    return {"operation": operation, "scope": SCOPE, "arguments": {"query": "question", "limit": 8},
            "canonical_result": result}


def packet(records, stop="budget"):
    return {"question": "question", "stop": stop, "assessment": {"scope": SCOPE, "termination": stop},
            "assessment_history": [], "records": records, "paragraphs": [], "scope_observation": OBSERVATION,
            "omitted_count": 0, "work": dict(WORK, inference_us=None)}


def receipt(original, ranked, applied=True, scored=None):
    scored = len(original) if scored is None else scored
    return {"kind": "receipt", "receipt": {
        "applied": applied, "availability": "ready" if applied else "failed", "reason": "synthetic",
        "original_order": original, "ranked_order": ranked, "presented_count": len(original),
        "scored_count": scored, "observed_input_tokens": 1024 * scored if applied else None,
        "reserved_input_tokens": 1024 * len(original), "work": dict(WORK, inference_us=4000 if applied else None),
        "presentations": [{"candidate": name, "utf8_start": 0, "utf8_end": 5, "source_bytes": 5} for name in original]}}


def arm(records=None, state="ok", stop="budget", elapsed=1_000_000):
    call = {"state": state, "elapsed_ns": elapsed if state == "ok" else None}
    answer = None if records is None else packet(records, stop)
    row = scorer["arm_row"](call, answer, "question", PUBLICATION["token"], MEMBERS, TARGET, FILES.__getitem__)
    return dict(row, request="sealed-request")


def paired(case, a, b, group="G01"):
    checks = scorer["pairing"](a, b)
    return {"case": case, "group": group, "exposed": False, "A": a, "B": b, "pairing": checks,
            "category": scorer["category"](a, b, checks),
            "census": {"expected_refusal": False, "observed_initial_focus": None}}


WHOLE = hit("gold.txt", "the answer is forty two", 2, 2)
OTHER = hit("other.txt", "second", 2, 2)


class DeliveredSpan(unittest.TestCase):
    def test_union_across_fragments_covers_the_span(self):
        row = arm([record("search", [hit("gold.txt", "the answer is", 2, 2)]),
                   record("context", [hit("gold.txt", "is forty two", 2, 2)])])
        self.assertEqual(("covered", 15, []), (row["coverage"], row["delivered_span_bytes"], row["missing_ranges"]))

    def test_clipped_hit_is_partial_with_the_missing_range(self):
        row = arm([record("search", [hit("gold.txt", "the answer", 2, 2)])])
        self.assertEqual(("partial", 6, [[21, 30]]),
                         (row["coverage"], row["delivered_span_bytes"], row["missing_ranges"]))
        self.assertTrue(row["gold_document_cited"])

    def test_other_document_only_is_not_delivered(self):
        row = arm([record("search", [OTHER])])
        self.assertEqual(("not_delivered", 0, [[15, 30]], False),
                         (row["coverage"], row["delivered_span_bytes"], row["missing_ranges"],
                          row["gold_document_cited"]))

    def test_repeated_text_in_the_cited_lines_is_unresolved(self):
        row = arm([record("search", [hit("gold.txt", "line", 1, 3)])])
        self.assertEqual(("unresolved", 1), (row["coverage"], row["unresolved_target_hits"]))
        self.assertEqual("unresolved", paired("Q1", arm([record("search", [WHOLE])]), row)["category"])

    def test_gap_between_fragments_is_reported(self):
        self.assertEqual([[4, 6], [9, 12]], scorer["missing"](2, 12, [(0, 4), (6, 9)]))
        self.assertEqual([[2, 12]], scorer["missing"](2, 12, [(12, 20)]))

    def test_unsupported_operation_or_foreign_revision_is_unresolved_not_a_miss(self):
        foreign = hit("gold.txt", "the answer is forty two", 2, 2)
        foreign["citation"]["content_hash"] = "0" * 64
        for records in ([record("references", [WHOLE])], [record("search", [foreign])]):
            with self.subTest(operation=records[0]["operation"]):
                self.assertEqual("unresolved", arm(records)["coverage"])


class Duplicates(unittest.TestCase):
    def test_repeated_bytes_contained_hits_and_unresolved_hits_are_counted_apart(self):
        row = arm([record("search", [WHOLE, OTHER]),
                   record("context", [hit("gold.txt", "answer is forty", 2, 2), hit("gold.txt", "line", 1, 3)])])
        counts = row["duplicates"]
        self.assertEqual((4, 44, 29, 15, 1, 1, 4),
                         tuple(counts[key] for key in ("hits", "delivered_bytes", "union_bytes", "duplicate_bytes",
                                                       "hits_inside_earlier", "unresolved_hits",
                                                       "unresolved_bytes")))
        self.assertEqual({"gold.txt": 23, "other.txt": 6}, counts["union_bytes_by_file"])


class FocusObservation(unittest.TestCase):
    def test_not_applied_and_failed_cases_stay_in_the_denominator(self):
        notice = {"kind": "not_applied", "availability": "constrained", "reason": "budget", "candidate_count": 1,
                  "work_unknown": False}
        a = arm([record("search", [WHOLE])])
        quiet = arm([record("search", [WHOLE], notice)])
        failed = arm([record("search", [WHOLE], receipt(["c1"], ["c1"], applied=False, scored=0))])
        self.assertEqual(["not_applied", "failed"], [quiet["focus"][0]["state"], failed["focus"][0]["state"]])
        self.assertEqual("no Focus observation", a["focus"][0]["reason"])
        rows = [paired("Q1", a, quiet), paired("Q2", a, failed)]
        self.assertEqual({"denominator": 2, "both": 2, "a_only": 0, "b_only": 0, "neither": 0, "unresolved": 0,
                          "error": 0, "net_b_minus_a": 0}, scorer["tally"](rows))
        focus = scorer["aggregate"](rows)["arms"]["B"]["focus"]
        self.assertEqual({"failed": 1, "not_applied": 1}, focus["initial_search"])
        self.assertEqual((0, 0), (focus["pairs_scored"], focus["cases_with_scored_pairs"]))

    def test_rank_before_focus_comes_from_the_receipt_and_orders_must_pair(self):
        first, second = hit("other.txt", "first", 1, 1), hit("other.txt", "third", 3, 3)
        a = arm([record("search", [first, second, WHOLE])])
        b = arm([record("search", [WHOLE, first, second], receipt(["c1", "c2", "c3"], ["c3", "c1", "c2"]))])
        self.assertEqual((3, 3, 3), (a["initial"]["gold_document_rank"], a["initial"]["gold_span_rank"],
                                     a["initial"]["gold_document_rank_before_focus"]))
        self.assertEqual((1, 3, True), (b["initial"]["gold_document_rank"],
                                        b["initial"]["gold_document_rank_before_focus"],
                                        b["initial"]["order_changed"]))
        self.assertEqual("both", paired("Q1", a, b)["category"])
        figures = scorer["aggregate"]([paired("Q1", a, b)])
        self.assertEqual({"rank_1_4": 1, "rank_5_8": 0, "absent": 0},
                         figures["arms"]["B"]["initial_search"]["gold_document_rank"])
        self.assertEqual({"movable_rank_1_4_before_focus": 1, "improved": 1, "unchanged": 0, "worsened": 0,
                          "first_before_focus": 0, "first_after_focus": 1}, figures["gold_document_rank_movement"])
        reordered = paired("Q1", arm([record("search", [second, first, WHOLE])]), b)
        self.assertEqual((True, False, "unresolved"), (reordered["pairing"]["initial_hits"],
                                                       reordered["pairing"]["order_before_focus"],
                                                       reordered["category"]))
        fewer = paired("Q1", arm([record("search", [first, WHOLE])]), b)
        self.assertEqual((False, "unresolved"), (fewer["pairing"]["initial_hits"], fewer["category"]))

    def test_focus_inference_is_summed_per_case_and_per_pair(self):
        a = arm([record("search", [WHOLE, OTHER])])
        b = arm([record("search", [WHOLE, OTHER], receipt(["c1", "c2"], ["c1", "c2"])),
                 record("search", [OTHER], receipt(["c3"], ["c3"]))], elapsed=9_000_000)
        self.assertEqual("both", paired("Q1", a, b)["category"])
        focus = scorer["aggregate"]([paired("Q1", a, b)])["arms"]["B"]["focus"]
        self.assertEqual((3, 1, 8.0, 2.667), (focus["pairs_scored"], focus["cases_with_scored_pairs"],
                                              focus["inference_ms_per_case"]["median"],
                                              focus["inference_ms_per_scored_pair"]))


class PairedSummary(unittest.TestCase):
    def test_arm_error_is_an_error_not_a_miss(self):
        covered = arm([record("search", [WHOLE])])
        for state in ("timeout", "cli_error", "tool_error"):
            with self.subTest(state=state):
                failed = arm(state=state)
                self.assertEqual((state, state), (failed["call_state"], failed["coverage"]))
                self.assertEqual("error", paired("Q1", covered, failed)["category"])
                self.assertEqual("error", paired("Q1", failed, covered)["category"])
        malformed = scorer["arm_row"]({"state": "ok", "elapsed_ns": 1}, {"stop": "budget"}, "question",
                                      PUBLICATION["token"], MEMBERS, TARGET, FILES.__getitem__)
        self.assertEqual("tool_error", malformed["coverage"])

    def test_refusal_is_scored_on_what_it_delivered_and_keeps_its_stop(self):
        refused = arm([], stop="refused")
        self.assertEqual(("ok", "refused", "not_delivered", None),
                         (refused["call_state"], refused["stop"], refused["coverage"], refused["initial"]))
        row = paired("Q1", arm([record("search", [WHOLE])]), refused)
        self.assertEqual("a_only", row["category"])
        self.assertEqual({"refused": 1}, scorer["aggregate"]([row])["arms"]["B"]["stops"])

    def test_group_summary_weights_groups_equally(self):
        # Both arms start from the same initial search; only the follow-up read differs.
        covered = arm([record("search", [OTHER]), record("context", [WHOLE])])
        missed, failed = arm([record("search", [OTHER])]), arm(state="timeout")
        rows = [paired("Q1", covered, covered), paired("Q2", missed, covered), paired("Q3", covered, missed),
                paired("Q4", missed, missed), paired("Q5", covered, failed), paired("Q6", missed, covered)]
        groups = [{"group": "G01", "cases": ["Q1", "Q2"]}, {"group": "G02", "cases": ["Q3"]},
                  {"group": "G03", "cases": ["Q4", "Q5", "Q6"]}]
        self.assertEqual({"denominator": 6, "both": 1, "a_only": 1, "b_only": 2, "neither": 1, "unresolved": 0,
                          "error": 1, "net_b_minus_a": 1}, scorer["tally"](rows))
        summary = scorer["group_summary"](rows, groups)
        self.assertEqual({"both": 0.5, "a_only": 1.0, "b_only": 0.833333, "neither": 0.333333, "unresolved": 0.0,
                          "error": 0.333333}, summary["equal_weight"])
        self.assertEqual((3, 2, 0, 1), (summary["groups"], summary["net_positive_groups"],
                                        summary["net_zero_groups"], summary["net_negative_groups"]))
        self.assertEqual([1, -1, 1], [row["net_b_minus_a"] for row in summary["rows"]])
        with self.assertRaisesRegex(ValueError, "group roster"):
            scorer["group_summary"](rows, [{"group": "G09", "cases": ["Q1", "Q9"]}])
        figures = scorer["aggregate"](rows)
        self.assertEqual({"covered->covered": 1, "covered->not_delivered": 1, "covered->timeout": 1,
                          "not_delivered->covered": 2, "not_delivered->not_delivered": 1},
                         figures["coverage_a_to_b"])
        self.assertEqual({"b_more": 2, "equal": 2, "b_fewer": 1}, figures["b_against_a"]["records"])
        self.assertEqual({"b_more": 2, "equal": 2, "b_fewer": 1}, figures["b_against_a"]["delivered_span_bytes"])
        self.assertEqual([(90, 45), (75, 45)], [(figures["arms"][arm]["span_bytes"],
                                                 figures["arms"][arm]["delivered_span_bytes"]) for arm in "AB"])
        self.assertEqual({"covered": 3, "not_delivered": 2, "timeout": 1}, figures["arms"]["B"]["coverage"])

    def test_one_arm_figures_make_no_paired_claim(self):
        first = hit("other.txt", "first", 1, 1)
        rows = [{"case": "Q1", "exposed": False, "A": arm([record("search", [WHOLE, first])])},
                {"case": "Q2", "exposed": False, "A": arm([record("search", [first, OTHER])])},
                {"case": "Q3", "exposed": False, "A": arm(state="cli_error")},
                {"case": "Q0", "exposed": True, "A": arm([record("search", [first]), record("context", [WHOLE])])}]
        freeze = {"evidence_limits": ["synthetic"]}
        summary = scorer["single_arm_report"](rows, freeze)
        self.assertEqual(("A", False, 0, 3, ["Q0"]),
                         (summary["arm"], summary["arm_b_issued"], summary["paired_outcomes"],
                          summary["population"]["judged_cases"], summary["population"]["exposed_cases"]))
        judged = summary["judged_figures"]
        self.assertEqual({"cli_error": 1, "covered": 1, "not_delivered": 1}, judged["coverage"])
        self.assertEqual({"rank_1_4": 1, "rank_5_8": 0, "absent": 1}, judged["initial_search"]["gold_document_rank"])
        self.assertEqual((2, 0), (judged["answer_elapsed_ms"]["n"], judged["focus"]["pairs_scored"]))
        self.assertEqual({"covered": 1}, summary["exposed_figures"]["coverage"])
        self.assertEqual({"rank_1_4": 0, "rank_5_8": 0, "absent": 1},
                         summary["exposed_figures"]["initial_search"]["gold_document_rank"])
        self.assertNotIn("by_case", summary)

    def test_paired_report_keeps_the_judged_figures_and_reports_only_complete_groups(self):
        covered = arm([record("search", [OTHER]), record("context", [WHOLE])])
        missed = arm([record("search", [OTHER])])
        rows = [paired("Q1", missed, covered), paired("Q2", covered, covered), paired("Q3", covered, missed)]
        rows.append(dict(paired("Q0", missed, missed), exposed=True))
        freeze = {"measurement_rule": {"pass_line": None}, "evidence_limits": ["synthetic"], "groups": [
            {"group": "G01", "cases": ["Q0"], "exposed": True},
            {"group": "G02", "cases": ["Q1", "Q2"], "exposed": False},
            {"group": "G03", "cases": ["Q3", "Q9"], "exposed": False}]}
        summary = scorer["report"](rows, freeze)
        self.assertEqual({"judged_cases": 3, "judged_groups": 1, "incomplete_groups_not_reported": 1,
                          "exposed_cases": ["Q0"]}, summary["population"])
        self.assertEqual((1, 1, 1, 0), tuple(summary["by_case"][key] for key in ("both", "a_only", "b_only", "net_b_minus_a")))
        self.assertEqual([("G02", 2, 1)], [(row["group"], row["denominator"], row["net_b_minus_a"])
                                           for row in summary["by_group"]["rows"]])
        # The population figures are a block of counts, never replaced by a flag.
        self.assertEqual(3, summary["judged_figures"]["cases"])
        self.assertEqual({"covered": 2, "not_delivered": 1}, summary["judged_figures"]["arms"]["B"]["coverage"])
        self.assertEqual(("neither", "Q0"), (summary["exposed"]["rows"][0]["category"],
                                             summary["exposed"]["rows"][0]["case"]))

    def test_nearest_rank_percentile(self):
        self.assertEqual(19, scorer["nearest_rank"](list(range(1, 21)), 95))
        self.assertEqual(73, scorer["nearest_rank"](list(range(76, 0, -1)), 95))
        self.assertEqual(7, scorer["nearest_rank"]([7], 95))
        self.assertEqual({"n": 4, "median": 2.5, "p95_nearest_rank": 4.0, "max": 4.0},
                         scorer["distribution"]([1000, 2000, 3000, 4000], 1000))

    def test_rule_has_no_pass_line_and_names_the_six_categories(self):
        rule = runner["RULE"]
        self.assertIsNone(rule["pass_line"])
        self.assertEqual(["both", "a_only", "b_only", "neither", "unresolved", "error"], rule["categories"])
        self.assertEqual(tuple(rule["categories"]), scorer["CATEGORIES"])


class FocusCensus(unittest.TestCase):
    RECIPE = {"INSTRUCTION": "rank", "PREFIX": "p p", "SUFFIX": "s"}

    def test_only_the_first_candidates_are_tokenised_and_long_pairs_are_expected_refusals(self):
        encode, view = str.split, str.upper
        # Two prefix tokens, one suffix token, five framing and query tokens, then the document tokens.
        self.assertEqual(11, scorer["pair_tokens"](encode, self.RECIPE, view, "q", "a b c"))
        long_hit = dict(OTHER, text="w " * 20)
        answer = packet([record("search", [WHOLE, OTHER, long_hit])])
        short = scorer["census_row"](encode, self.RECIPE, view, answer, 20, 2)
        self.assertEqual(([13, 9], False), (short["pair_tokens"], short["expected_refusal"]))
        every = scorer["census_row"](encode, self.RECIPE, view, answer, 20, 4)
        self.assertEqual(([13, 9, 28], True), (every["pair_tokens"], every["expected_refusal"]))
        self.assertEqual([23, 6, 40], every["hit_bytes"])


class Runner(unittest.TestCase):
    def test_groups_join_cases_through_shared_documents(self):
        menus = {"Q3": ["d9"], "Q1": ["d1", "d2"], "Q2": ["d2", "d3"], "Q4": ["d3"]}
        self.assertEqual([["Q1", "Q2", "Q4"], ["Q3"]], runner["document_groups"](menus))

    def test_two_ordered_batches_stay_inside_the_native_enrollment_bound(self):
        cases = [f"DEV_Q{number:03}" for number in range(76, 0, -1)]
        plan = runner["batches"](cases)
        self.assertEqual([("b1", 38, "DEV_Q001"), ("b2", 38, "DEV_Q039")],
                         [(batch["name"], len(batch["cases"]), batch["cases"][0]) for batch in plan])
        with self.assertRaises(ValueError):
            runner["batches"](cases, 65)
        with self.assertRaises(ValueError):
            runner["batches"](["Q1", "Q1"])

    def test_request_keeps_the_native_template_and_budget(self):
        template = {"schema": "airk.arbiter.context.request/2", "question": "native", "response_mode": "full",
                    "include_status": True,
                    "budget": {"calls": 8, "candidates": 32, "deadline_ms": 120000, "fanout": 8,
                               "input_tokens": 8192, "max_bytes": 262144, "rounds": 3}}
        request = json.loads(runner["request_bytes"](template, "Title\n\nBody"))
        self.assertEqual(template | {"question": "Title\n\nBody"}, request)
        for change in ({"budget": dict(template["budget"], calls=9)}, {"response_mode": "terse"}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                runner["request_bytes"](template | change, "Title")
        with self.assertRaises(ValueError):
            runner["request_bytes"](template, " ")

    def test_process_listings(self):
        listing = '"cargo.exe","1","Console","1","9 K"\n"RUSTC.EXE","2","Console","1","9 K"\n' \
                  '"rustc.exe","3","Console","1","9 K"\n"python.exe","4","Console","1","9 K"\n'
        self.assertEqual({"cargo.exe": 1, "rustc.exe": 2}, runner["build_processes"](listing))
        self.assertEqual({}, runner["build_processes"]('"airk.exe","5","Console","1","9 K"\n'))
        gpu = "3336, [Insufficient Permissions], [N/A]\n16628, C:\\Windows\\explorer.exe, [N/A]\n\n"
        self.assertEqual([3336, 16628], runner["gpu_process_ids"](gpu))
        self.assertEqual([], runner["gpu_process_ids"]("No running processes found\n"))

    def test_private_target_names_select_the_plan(self):
        campaign = runner["CAMPAIGN"]
        self.assertEqual(("run-1", "run-2", "dry"),
                         (runner["plan_name"](campaign / "techqa-stage-arms-run-1-r1"),
                          runner["plan_name"](campaign / "techqa-stage-arms-run-2-r3"),
                          runner["plan_name"](campaign / "techqa-stage-arms-dry-r12")))
        # The retired single-lifetime name stays readable for its closed run.
        self.assertEqual("run", runner["plan_name"](campaign / "techqa-stage-arms-run-r1"))
        for foreign in (campaign / "techqa-stage-arms-run-r0", campaign / "other" / "techqa-stage-arms-run-1-r1",
                        campaign / "techqa-stage-arms-run-0-r1", campaign / "techqa-stage-arms-inputs-r1"):
            with self.subTest(foreign=foreign.name), self.assertRaises(ValueError):
                runner["plan_name"](foreign)

    def test_one_fixture_lifetime_refuses_more_than_64_indexes(self):
        cases = [f"DEV_Q{number:03}" for number in range(1, 77)]
        bound = runner["catalog_bound"]
        self.assertEqual(cases[:64], bound(cases[:64]))
        for refused in (cases[:65], cases, [], ["DEV_Q001", "DEV_Q001"]):
            with self.subTest(size=len(refused)), self.assertRaisesRegex(ValueError, "at most 64 indexes"):
                bound(refused)
        # The frozen split keeps every lifetime inside the bound.
        self.assertEqual([38, 38], [len(bound(batch["cases"])) for batch in runner["batches"](cases)])

    def test_answer_failures_are_recorded_states_and_never_retried(self):
        with tempfile.TemporaryDirectory() as directory:
            folder, calls = Path(directory), []

            def run(outcome):
                def issue(command, target, label):
                    calls.append(label)
                    if outcome == "timeout":
                        raise TimeoutError("client deadline")
                    if outcome == "exit":
                        (target / f"{label}.command.json").write_text("{}", encoding="utf-8")
                        raise ValueError("failed")
                    if outcome == "deadline":
                        raise ValueError("reached the original deadline")
                    path = target / f"{label}.stdout"
                    text = "not json" if outcome == "garbled" else json.dumps({"isError": outcome == "tool"})
                    path.write_text(text, encoding="utf-8")
                    return path, 5
                return issue

            call = runner["answer_call"]
            self.assertEqual({"state": "ok", "elapsed_ns": 5}, call(run("ok"), folder, "a", []))
            self.assertEqual({"state": "tool_error", "elapsed_ns": 5}, call(run("tool"), folder, "b", []))
            self.assertEqual({"state": "timeout"}, call(run("timeout"), folder, "c", []))
            self.assertEqual({"state": "cli_error"}, call(run("exit"), folder, "d", []))
            with self.assertRaisesRegex(ValueError, "original deadline"):
                call(run("deadline"), folder, "e", [])
            self.assertEqual({"state": "tool_error", "elapsed_ns": 5}, call(run("garbled"), folder, "f", []))
            self.assertEqual(["a", "b", "c", "d", "e", "f"], calls)

    def test_duplicate_weights_are_removed_only_after_every_original_is_authenticated(self):
        with tempfile.TemporaryDirectory() as directory:
            root, profiles = Path(directory).resolve(), []
            for number, payload in enumerate((b"focus weights", b"context weights")):
                assets = root / f"assets-{number}"
                assets.mkdir()
                (assets / "model.onnx.data").write_bytes(payload)
                profiles.append({"manifest": str(number) * 64, "assets_directory": str(assets), "assets": [
                    {"name": "model.onnx", "role": "model", "bytes": 1, "sha256": "unused"},
                    {"name": "model.onnx.data", "role": "external_weights", "bytes": len(payload),
                     "sha256": sha(payload)}]})
            freeze, dispose = {"runtime": {"profiles": profiles}}, runner["dispose_weights"]

            def installation(name, settled=True):
                output = root / name
                for profile in profiles:
                    folder = output / "fixture/control/arbiter-v3/assets" / profile["manifest"]
                    folder.mkdir(parents=True)
                    source = Path(profile["assets_directory"]) / "model.onnx.data"
                    (folder / "model.onnx.data").write_bytes(source.read_bytes())
                (output / "owned.json").write_text(json.dumps({
                    "owned_descendants_settled": settled, "owned_job_final_active_processes": 0,
                    "owned_job_final_member_pids": []}), encoding="utf-8")
                return output

            def installed(output):
                return sorted(path.parent.name[0] for path in output.rglob("model.onnx.data"))

            with self.assertRaisesRegex(ValueError, "settled"):
                dispose(installation("unsettled", settled=False), freeze)
            last = Path(profiles[1]["assets_directory"]) / "model.onnx.data"
            last.write_bytes(b"context weightz")
            changed = installation("changed-original")
            with self.assertRaisesRegex(ValueError, "retained original"):
                dispose(changed, freeze)
            self.assertEqual(["0", "1"], installed(changed))
            last.write_bytes(b"context weights")
            foreign = installation("foreign-installed")
            (foreign / "fixture/control/arbiter-v3/assets" / ("1" * 64) / "model.onnx.data").write_bytes(b"context weightz")
            with self.assertRaisesRegex(ValueError, "verified duplicate"):
                dispose(foreign, freeze)
            self.assertEqual(["0", "1"], installed(foreign))
            closed = installation("closed")
            receipt = dispose(closed, freeze)
            self.assertEqual((28, 2, []), (receipt["reclaimed_bytes"], len(receipt["files"]), installed(closed)))
            self.assertEqual([b"focus weights", b"context weights"],
                             [(Path(profile["assets_directory"]) / "model.onnx.data").read_bytes()
                              for profile in profiles])
            self.assertTrue((closed / "weights-disposed.json").is_file())
            with self.assertRaisesRegex(ValueError, "settled"):
                dispose(closed, freeze)


if __name__ == "__main__":
    unittest.main()
