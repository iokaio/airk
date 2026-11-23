# SPDX-License-Identifier: Apache-2.0
"""Unit tests for the loop's harness parts: Answer /3 parsing, the gate tests, the gate report and the specs.

The Answer /3 shapes come from the Rust-emitted cases
``fixtures/split/arbiter/answer_loop_cases.json``; the scenario is the invented
"lantern" fixture of ``synthetic.py``. No model, service or session starts.

Run: python -I -B -m unittest discover -s scripts/arbiter-answer-eval
"""

from __future__ import annotations

import argparse
import contextlib
import copy
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import depth  # noqa: E402
import evidence  # noqa: E402
import gate  # noqa: E402
import gate_report  # noqa: E402
import grade  # noqa: E402
import loop_reply  # noqa: E402
import make_loop_spec  # noqa: E402
import mcp_wrapper  # noqa: E402
import prompts_freeze  # noqa: E402
import replay  # noqa: E402
import run_pass  # noqa: E402
import run_plan  # noqa: E402
import synthetic  # noqa: E402

CASES_PATH = common.REPO_ROOT / "fixtures" / "split" / "arbiter" / "answer_loop_cases.json"
CASES = json.loads(CASES_PATH.read_text(encoding="utf-8"))
SEARCH_ID = "1f" * 32
SYMBOLS_ID = "20" * 32


def case(name: str) -> dict:
    return copy.deepcopy(next(c["response"] for c in CASES["response_cases"] if c["id"] == name))


def hit(path, start, end, text="x"):
    return {"citation": {"path": path, "start_line": start, "end_line": end}, "text": text, "truncated": False}


def v3_reply(search_hits=(), symbol_hits=()) -> dict:
    """The fixture's valid two-part reply with evidence in its two records (search cited by part 2, symbols by
    part 1)."""
    body = case("two_parts_valid")
    for record in body["records"]:
        record["canonical_result"]["hits"] = list(search_hits if record["id"] == SEARCH_ID else symbol_hits)
    return body


def recount(body: dict) -> dict:
    """Keep the decision log's counts by source in step with the decisions (these fixtures include every
    logged decision), as the service writes them: one row per stage, source and model."""
    counts: dict[tuple, int] = {}
    for decision in body["decisions"]:
        key = (decision["stage"], decision["source"]["kind"], decision.get("model"))
        counts[key] = counts.get(key, 0) + 1
    body["decision_log"]["counts"] = [
        {"count": n, "source": source, "stage": stage, **({"model": model} if model else {})}
        for (stage, source, model), n in counts.items()]
    return body


def top_score(body: dict, model: str = "context_compact", precision: str = "float16") -> dict:
    """The reply as a gate arm would produce it: no abstention, every model decision by top score."""
    body = copy.deepcopy(body)
    for decision in body["decisions"]:
        if decision["source"]["kind"] == "abstained":
            decision["source"] = {"kind": "model", "mode": "top_score"}
        if decision.get("model"):
            decision.update(model=model, precision=precision)
    body["degradations"] = [d for d in body["degradations"] if d["kind"] != "context_abstained"]
    return recount(body)


def focus_only(body: dict) -> dict:
    body = copy.deepcopy(body)
    for decision in body["decisions"]:
        for key in ("model", "device", "precision", "card_slot", "receipt", "top", "runner_up", "margin"):
            decision.pop(key, None)
        decision["batched_with"] = []
        decision["source"] = {"kind": "deterministic", "rule": "top_focus_span"}
    body["degradations"] = [{"kind": "context_absent"}]
    return recount(body)


def starved(body: dict, reason: str = "token_budget") -> dict:
    """The reply of an arm whose model was present and never reached: every decision due to it was not
    dispatched. The service reports ``context_absent`` for it, as it does for a Focus-only reply."""
    body = top_score(body)
    for decision in body["decisions"]:
        if decision["source"]["kind"] == "model":
            decision["source"] = {"kind": "not_dispatched", "fallback": "top_focus_span", "reason": reason}
    body["degradations"] = [{"kind": "context_absent"}]
    return recount(body)


def _budgets(value) -> list[dict]:
    """Every object that carries a ``budget`` object, anywhere in a JSON document."""
    found = []
    if isinstance(value, dict):
        if isinstance(value.get("budget"), dict):
            found.append(value)
        for child in value.values():
            found += _budgets(child)
    elif isinstance(value, list):
        for child in value:
            found += _budgets(child)
    return found


def envelope(body) -> bytes:
    return json.dumps({"content": [{"type": "text", "text": json.dumps(body)}]}).encode()


def pointer_set(document, pointer: str, value):
    parts = [p.replace("~1", "/").replace("~0", "~") for p in pointer.split("/")[1:]]
    target = document
    for part in parts[:-1]:
        target = target[int(part)] if isinstance(target, list) else target[part]
    last = parts[-1]
    if isinstance(target, list):
        target[int(last)] = value
    else:
        target[last] = value


class TempDir(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()


# ---------------------------------------------------------------- contract constants


class ContractConstants(unittest.TestCase):
    def test_request_schema_and_default_budget_match_the_rust_cases(self):
        self.assertEqual(common.ANSWER_SCHEMA, CASES["request_schema"])
        self.assertEqual(common.ANSWER_BUDGET_DEFAULT, CASES["default_budget"])
        self.assertEqual(loop_reply.RESPONSE_SCHEMA, CASES["response_schema"])
        self.assertEqual(loop_reply.PACKET_SCHEMA, CASES["packet_schema"])
        self.assertEqual(loop_reply.REPLY_FRAME_BYTES, CASES["reply_frame_bytes"])
        self.assertFalse(CASES["qualification"])

    def test_the_default_budget_is_read_from_the_rust_cases_and_nowhere_copied(self):
        """On 22 November 2026 the product default moved to 196,608 input tokens while the harness still held
        a copied 131,072, which every gate Answer would have been sent."""
        source = common.answer_budget_source()
        self.assertEqual(source, {"path": "fixtures/split/arbiter/answer_loop_cases.json",
                                  "sha256": common.sha256_file(CASES_PATH), "default_budget": CASES["default_budget"]})
        # No harness source file spells the input-token default: a moved default cannot leave a stale copy.
        tokens = CASES["default_budget"]["input_tokens"]
        for name in prompts_freeze.HARNESS_FILES + ("make_loop_spec.py", "lg_lifetimes.py", "gate_report.py"):
            text = (HERE / name).read_text(encoding="utf-8")
            self.assertNotIn(str(tokens), text.replace("_", ""), name)
        # The documents that state the default follow the cases file.
        readme = (HERE / "README.md").read_text(encoding="utf-8")
        for key, unit in (("input_tokens", "input tokens"), ("calls", "calls"), ("candidates", "candidates")):
            self.assertIn(f"{CASES['default_budget'][key]:,} {unit}", readme, key)
        template = common.read_json(HERE / "l7_plan_template.json")
        budgets = [row["budget"] for row in _budgets(template)]
        self.assertTrue(budgets)
        for budget in budgets:
            self.assertEqual({k: budget[k] for k in CASES["default_budget"] if k != "deadline_ms"},
                             {k: v for k, v in CASES["default_budget"].items() if k != "deadline_ms"})
        # A file that is not this contract's cases, or that carries no whole budget, is refused at import.
        with tempfile.TemporaryDirectory() as folder:
            for number, cases in enumerate((dict(CASES, request_schema="airk.arbiter.context.request/2"),
                                            {k: v for k, v in CASES.items() if k != "default_budget"},
                                            dict(CASES, default_budget=dict(CASES["default_budget"], calls=0)),
                                            dict(CASES, default_budget=dict(CASES["default_budget"], calls="132")),
                                            dict(CASES, default_budget={}))):
                path = Path(folder) / f"cases-{number}.json"
                path.write_text(json.dumps(cases), encoding="utf-8")
                with self.assertRaises(common.HarnessError):
                    common.answer_default_budget(path)
            moved = Path(folder) / "moved.json"
            moved.write_text(json.dumps(dict(CASES, default_budget=dict(CASES["default_budget"], input_tokens=7))),
                             encoding="utf-8")
            self.assertEqual(common.answer_default_budget(moved)["input_tokens"], 7)

    def test_receipts_carry_the_exact_budget_sent(self):
        rows = [{"budget_sent": {"calls": 1}}, {"budget_sent": {"calls": 1}}, {"budget_sent": {"calls": 2}},
                {"tool": "airk_answer", "sent_arguments": {"budget": {"calls": 2}}},
                {"tool": "airk_search", "sent_arguments": {"query": "q"}}, {"budget_sent": None}]
        self.assertEqual(common.distinct_budgets(rows), [{"budget": {"calls": 1}, "calls": 2},
                                                         {"budget": {"calls": 2}, "calls": 2}])
        with tempfile.TemporaryDirectory() as folder:
            sent = []
            body = v3_reply()

            def backend(tool, args):
                sent.append(args)
                return Outcome(body, ms=5)

            entry = replay.replay_canonical(scen(), "compact", object(), Path(folder), timeout_s=5, backend=backend,
                                            answer_defaults={"budget": {"deadline_ms": 600000}})
        self.assertEqual(entry["call"]["budget_sent"], dict(CASES["default_budget"], deadline_ms=600000))
        self.assertEqual(entry["call"]["budget_sent"], sent[0]["budget"])

    def test_a_roots_tools_catalog_is_the_live_lifetimes_listing(self):
        """The wrapper serves agents from <root>/tools-catalog.json; nothing wrote it for a new root."""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            names = sorted({tool for tools in common.PATH_TOOLS.values() for tool in tools})

            def cli(tools):
                script = root / f"cli-{len(tools)}.py"
                script.write_text("import json, sys\nassert sys.argv[-3:] == ['tools', 'list', '--json'], sys.argv\n"
                                  f"print(json.dumps({{'tools': {[{'name': t, 'inputSchema': {}} for t in tools]!r}}}))\n",
                                  encoding="utf-8")
                return common.Handle(root / "handle.json", "0" * 64, (sys.executable, "-I", str(script)),
                                     str(root / "control"), {}, {})

            first = common.write_tools_catalog(cli(names), root)
            self.assertEqual((first["tools"], Path(first["path"]).name), (len(names), "tools-catalog.json"))
            self.assertEqual(common.write_tools_catalog(cli(names), root), first)  # the same listing is kept
            with self.assertRaisesRegex(common.HarnessError, "written by another image"):
                common.write_tools_catalog(cli(names + ["airk_new_tool"]), root)
            other = root / "other"
            other.mkdir()
            with self.assertRaisesRegex(common.HarnessError, "lacks"):
                common.write_tools_catalog(cli(names[:-1]), other)

    def test_prompts_freeze_pins_the_budget_and_its_source(self):
        now = prompts_freeze.current()
        self.assertEqual(now["answer_budget"], common.answer_budget_source())
        self.assertEqual(prompts_freeze.differences(copy.deepcopy(now), now), [])
        stale = copy.deepcopy(now)
        stale["answer_budget"]["default_budget"]["input_tokens"] = 131072
        self.assertEqual(prompts_freeze.differences(stale, now), ["answer default budget"])
        self.assertEqual(prompts_freeze.prompt_changes(stale, now), ["answer default budget"])
        before = copy.deepcopy(now)
        before.pop("answer_budget")  # a freeze from before the budget was pinned must be redone
        self.assertEqual(prompts_freeze.differences(before, now), ["answer default budget"])
        moved = copy.deepcopy(now)
        moved["answer_budget"]["sha256"] = "0" * 64  # the cases file changed, the budget did not
        self.assertEqual(prompts_freeze.differences(moved, now), ["answer default budget"])
        self.assertEqual(prompts_freeze.prompt_changes(moved, now), [])

    def test_canonical_first_call_sends_a_whole_v3_budget(self):
        tool, args = replay.canonical_first_call("compact", "Where is it?", {"budget": {"deadline_ms": 600000}})
        self.assertEqual(tool, "airk_answer")
        self.assertEqual(args["schema"], "airk.arbiter.context.request/3")
        self.assertEqual(set(args["budget"]), set(CASES["default_budget"]))
        self.assertEqual(args["budget"]["deadline_ms"], 600000)
        accepted = [c["budget"] for c in CASES["budget_cases"] if c["accepted"]]
        self.assertIn(common.ANSWER_BUDGET_DEFAULT, [dict(CASES["default_budget"], **b) for b in accepted])


# ---------------------------------------------------------------- loop_reply


class LoopReply(unittest.TestCase):
    def test_two_part_reply(self):
        body = case("two_parts_valid")
        self.assertTrue(loop_reply.is_v3(body))
        self.assertFalse(loop_reply.is_ticket(body))
        ds = loop_reply.decisions(body)
        self.assertEqual([d["ordinal"] for d in ds], [1, 2, 3, 4])
        self.assertEqual(ds[0]["key"], ("split", None, None, None))
        self.assertEqual(ds[1]["key"], ("coverage", 1, 1, 1))
        self.assertEqual(ds[1]["chosen"], (("A", "covered"),))
        self.assertEqual(ds[2]["source"], "abstained")
        self.assertEqual(len(loop_reply.current_decisions(body)), 4)
        needs = loop_reply.sub_needs(body)
        self.assertEqual([(n["part"], n["need"], n["coverage"]) for n in needs], [(1, 1, "covered"), (2, 1, "partial")])
        summary = loop_reply.summary(body)
        self.assertEqual(summary["stops"], ["covered", "no_progress"])
        self.assertEqual(summary["sub_needs"], {"covered": 1, "partial": 1, "missing": 0})
        self.assertEqual(summary["degradations"], ["context_abstained"])
        self.assertEqual(summary["rounds_per_part"], [1, 2])
        self.assertEqual(summary["decision_counts"]["coverage"], {"model": 1, "abstained": 1})
        self.assertEqual(summary["work"]["rounds"], 2)
        self.assertIsNone(summary["partial"])

    def test_decision_sources_count_model_fallback_and_no_model_per_stage(self):
        """The gate report must show how often a model decision fell back: a slower model may be shed more."""
        body = case("two_parts_valid")
        self.assertEqual([loop_reply.source_class(k) for k in ("model", "abstained", "unavailable", "not_dispatched",
                                                               "deterministic", "something_new")],
                         ["model", "fell_back", "fell_back", "not_dispatched", "no_model", "other"])
        self.assertEqual(loop_reply.fallback_reasons(body), {"coverage": {"abstained/abstained": 1}})
        self.assertEqual(loop_reply.summary(body)["fallback_reasons"], {"coverage": {"abstained/abstained": 1}})
        shed = copy.deepcopy(body)
        shed["decisions"][1]["source"] = {"kind": "unavailable", "fallback": "top_focus_span", "reason": "deadline"}
        shed["decision_log"]["counts"] = [
            {"count": 1, "source": "deterministic", "stage": "split"},
            {"count": 3, "model": "context_large", "source": "unavailable", "stage": "coverage"},
            {"count": 1, "model": "context_large", "source": "abstained", "stage": "coverage"},
            {"count": 2, "model": "context_large", "source": "model", "stage": "follow_up"},
            {"count": 4, "model": "context_large", "source": "not_dispatched", "stage": "follow_up"},
            {"count": 1, "source": "future_kind", "stage": "follow_up"}]
        shed["decision_log"].update(total=12, included=4, omitted=8)
        shed["decisions"][3]["source"] = {"kind": "not_dispatched", "fallback": "top_focus_span",
                                          "reason": "token_budget"}
        self.assertEqual(loop_reply.fallback_reasons(shed), {"coverage": {"abstained/abstained": 1,
                                                                          "unavailable/deadline": 1},
                                                             "follow_up": {"not_dispatched/token_budget": 1}})
        rows = [{"ok": True, "loop": loop_reply.summary(shed)}, {"ok": True, "loop": loop_reply.summary(body)},
                {"ok": False, "loop": loop_reply.summary(shed)}, {"ok": True, "loop": None},
                {"ok": True, "loop": loop_reply.summary(case("ticket_valid"))}]
        found = gate.decision_sources({"arm": "L", "rows": rows})
        self.assertEqual(found["replies"], 2)  # the failed call, the non-loop reply and the ticket do not count
        self.assertEqual(found["stages"]["coverage"],
                         {"model": 1, "fell_back": 5, "not_dispatched": 0, "no_model": 0, "other": 0})
        # Not dispatched is counted apart from the model's decisions and from the no-model path.
        self.assertEqual(found["stages"]["follow_up"],
                         {"model": 2, "fell_back": 0, "not_dispatched": 4, "no_model": 1, "other": 1})
        self.assertEqual(found["stages"]["split"],
                         {"model": 0, "fell_back": 0, "not_dispatched": 0, "no_model": 2, "other": 0})
        self.assertEqual(found["total"], {"model": 3, "fell_back": 5, "not_dispatched": 4, "no_model": 3, "other": 1})
        # Reasons come from the included decisions only: 4 of the 9 such decisions carry one here.
        self.assertEqual(found["fallback_reasons"], {"coverage": {"abstained/abstained": 2, "unavailable/deadline": 1},
                                                     "follow_up": {"not_dispatched/token_budget": 1}})
        self.assertEqual((found["fell_back_with_reason"], found["decisions_logged"], found["decisions_omitted"]),
                         (4, 16, 8))
        self.assertEqual(found["replies_without_log_counts"], 0)
        bare = loop_reply.summary(body) | {"decision_counts": {}}
        self.assertEqual(gate.decision_sources({"arm": "L", "rows": [{"ok": True, "loop": bare}]})[
            "replies_without_log_counts"], 1)

    def test_ticket_reply(self):
        body = case("ticket_valid")
        self.assertTrue(loop_reply.is_ticket(body))
        self.assertEqual(loop_reply.ticket_wait(body), (True, 90000, "50" * 32))
        summary = loop_reply.summary(body)
        self.assertEqual((summary["parts"], summary["partial"]["reason"], summary["ticket"]), (0, "queued", True))
        self.assertEqual(loop_reply.check_arm(body, {"context": "context_large"}), [])
        self.assertEqual(loop_reply.ticket_wait(case("two_parts_valid")), (False, 0, None))
        self.assertFalse(loop_reply.is_v3({"schema": "airk.arbiter.context/2"}))

    def test_every_rust_refusal_case_parses_without_error(self):
        bases = {c["id"]: c["response"] for c in CASES["response_cases"]}
        checked = 0
        for refusal in CASES["refusal_cases"]:
            if refusal.get("base") not in bases:
                continue
            body = copy.deepcopy(bases[refusal["base"]])
            try:
                for step in refusal["replace"]:
                    pointer_set(body, step["pointer"], step["value"])
            except (KeyError, IndexError, ValueError, TypeError):
                continue  # a replacement that adds a member this reader does not need
            loop_reply.summary(body)
            loop_reply.current_decisions(body)
            loop_reply.check_arm(body, {"context": "context_compact", "precision": "float16"})
            evidence.extract(body)
            checked += 1
        self.assertGreater(checked, 50)

    def test_an_arm_whose_focus_took_no_part_or_whose_reads_failed_is_refused(self):
        # s5 shakedown, 23 November 2026: every search read failed read_failed in the service, Focus ran no
        # call, and the Focus-only reply passed the old check.
        body = focus_only(case("two_parts_valid"))
        body["decision_log"]["focus_calls"] = 3
        arm = {"context": None, "focus": True}
        self.assertEqual(loop_reply.check_arm(body, arm), [])
        starved_focus = copy.deepcopy(body)
        starved_focus["decision_log"]["focus_calls"] = 0
        starved_focus["degradations"].append({"kind": "focus_absent"})
        self.assertTrue(any("Focus is staged and took no part" in p for p in loop_reply.check_arm(starved_focus, arm)))
        self.assertEqual(loop_reply.check_arm(starved_focus, {"context": None}), [])  # Focus not required
        failed = copy.deepcopy(body)
        error = {"error": {"code": "read_failed", "message": "embedding worker is unavailable", "retryable": False}}
        failed["records"][0]["canonical_result"] = {
            "failure": json.dumps({"content": [{"type": "text", "text": json.dumps(error)}], "isError": True})}
        op = failed["records"][0].get("operation") or "unknown"
        self.assertEqual(loop_reply.read_failures(failed), {f"{op}:read_failed": 1})
        self.assertEqual(loop_reply.summary(failed)["read_failures"], {f"{op}:read_failed": 1})
        self.assertTrue(any("reads failed in the service" in p for p in loop_reply.check_arm(failed, arm)))
        cut = failed["records"][0]["canonical_result"]["failure"][:100]  # the service keeps 512 characters
        failed["records"][0]["canonical_result"]["failure"] = cut
        self.assertEqual(loop_reply.read_failures(failed), {f"{op}:read_failed": 1})
        failed["records"][0]["canonical_result"]["failure"] = "not json"
        self.assertEqual(loop_reply.read_failures(failed), {f"{op}:unknown": 1})
        self.assertFalse(any("reads failed in the service" in p for p in loop_reply.check_arm(failed, arm)))
        # Any failed record still refuses the reply, named apart from service failures (invalid_arguments too).
        self.assertTrue(any("refused or failed otherwise" in p for p in loop_reply.check_arm(failed, arm)))

    def test_later_reads_are_classed_against_the_answer_rows(self):
        answer = {"schema": loop_reply.RESPONSE_SCHEMA, "records": [{"canonical_result": {
            "projection": evidence.LOOP_PROJECTION_PREFIX + "1", "spans": [
                {"path": "src/a.rs", "start_line": 10, "end_line": 20, "text": "x", "truncated": True},
                {"path": "src/b.rs", "start_line": 1, "end_line": 5, "text": "y", "truncated": False}]}}]}
        bodies = {1: {"hits": [hit("src/z.rs", 1, 2)]}, 2: answer, 3: {"hits": [hit("src/a.rs", 15, 30)]},
                  4: {"hits": [hit("src/b.rs", 4, 4)]}, 5: {"hits": [hit("src/c.rs", 1, 9)]}, 6: {"files": 3}}
        turns = [{"turn": n, "ok": True} for n in bodies]
        found = coverage.later_reads(turns, body_loader=lambda row: bodies[row["turn"]])
        self.assertEqual({k: found[k] for k in ("answer_replies", "rows", "rows_truncated", "reads_after",
                                                "to_truncated_row", "to_whole_row", "elsewhere", "no_location")},
                         {"answer_replies": 1, "rows": 2, "rows_truncated": 1, "reads_after": 4,
                          "to_truncated_row": 1, "to_whole_row": 1, "elsewhere": 1, "no_location": 1})
        self.assertEqual(found["turns_to_truncated_rows"], [3])

    def test_the_live_smoke_needs_a_search_record_focus_and_model_decisions(self):
        compact = {"context": "context_compact", "device": "cuda", "precision": "float16", "focus": True}
        good = top_score(case("two_parts_valid"))
        good["decision_log"]["focus_calls"] = 2
        self.assertTrue(any(r.get("operation") == "search" for r in loop_reply.records(good)))
        self.assertEqual(gate.smoke_problems(good, compact), [])
        no_focus = copy.deepcopy(good)
        no_focus["decision_log"]["focus_calls"] = 0
        self.assertIn("no Focus call while Focus is staged", gate.smoke_problems(no_focus, compact))
        self.assertTrue(any("no model-sourced decision" in p for p in gate.smoke_problems(focus_only(good), compact)))
        failed = copy.deepcopy(good)
        for record in failed["records"]:
            if record.get("operation") == "search":
                record["canonical_result"] = {"failure": '{"content":[{"text":"{\\"error\\":{\\"code\\":\\"read_failed\\"'}
        self.assertTrue(any("no search record without a failure" in p for p in gate.smoke_problems(failed, compact)))

    def test_arm_checks(self):
        raw = case("two_parts_valid")
        compact = {"context": "context_compact", "device": "cuda", "precision": "float16"}
        problems = loop_reply.check_arm(raw, compact)
        self.assertTrue(any("abstained" in p for p in problems))
        self.assertEqual(loop_reply.check_arm(top_score(raw), compact), [])
        large = loop_reply.check_arm(top_score(raw), {"context": "context_large", "precision": "float16"})
        self.assertTrue(any("not context_large" in p for p in large))
        fp32 = loop_reply.check_arm(top_score(raw), dict(compact, precision="float32"))
        self.assertTrue(any("float16, not float32" in p for p in fp32))
        self.assertTrue(loop_reply.check_arm(top_score(raw), {"context": None}))
        self.assertEqual(loop_reply.check_arm(focus_only(raw), {"context": None}), [])
        self.assertTrue(any("context_absent" in p for p in loop_reply.check_arm(focus_only(raw), compact)))
        with self.assertRaises(ValueError):
            loop_reply.arm_requirement({"context": "context_huge"})

    def test_a_model_that_was_present_and_never_reached_is_not_focus_only(self):
        """The reply sets context_absent both for a Focus-only run and for a model whose every call was not
        dispatched; the gate must tell them apart from the decision counts by source."""
        raw = case("two_parts_valid")
        compact = {"context": "context_compact", "device": "cuda", "precision": "float16"}
        took = loop_reply.context_presence(top_score(raw))
        self.assertEqual((took["state"], took["due"], took["models"], took["stages_without_model"], took["from"]),
                         ("model_took_decisions", 2, ["context_compact"], [], "decision_log.counts"))
        absent = loop_reply.context_presence(focus_only(raw))
        self.assertEqual((absent["state"], absent["due"], absent["models"]), ("no_model", 0, []))
        shed = starved(raw)
        presence = loop_reply.context_presence(shed)
        self.assertEqual((presence["state"], presence["due"], presence["models"]),
                         ("model_present_no_call_dispatched", 2, ["context_compact"]))
        self.assertEqual(presence["by_source"], {"model": 0, "abstained": 0, "unavailable": 0, "not_dispatched": 2})
        self.assertEqual(presence["stages_without_model"], ["coverage"])
        self.assertEqual(loop_reply.summary(shed)["context_presence"]["state"], "model_present_no_call_dispatched")
        # In the arm that has the model, such a reply fails the check and says which case it is.
        problems = loop_reply.check_arm(shed, compact)
        self.assertTrue(any("was present and no call was dispatched (2 decisions due)" in p for p in problems))
        self.assertTrue(any("took none of the 2 decisions due to it (not dispatched 2, unavailable 0" in p
                            for p in problems))
        self.assertFalse(any("no Context model took part" in p for p in problems))
        # In the Focus-only arm it is refused too: it carries the degradation and is not Focus only.
        focus = loop_reply.check_arm(shed, {"context": None})
        self.assertTrue(any("in a Focus-only arm: model_present_no_call_dispatched" in p for p in focus))
        # A real Focus-only reply in the model's arm keeps the plain statement.
        self.assertIn("context_absent: no Context model took part", loop_reply.check_arm(focus_only(raw), compact))
        # Unavailable at compile time (or later) is the other way to be present and take nothing.
        refused = copy.deepcopy(shed)
        refused["decisions"][1]["source"] = {"kind": "unavailable", "fallback": "top_focus_span", "reason": "compile"}
        recount(refused)
        self.assertEqual(loop_reply.context_presence(refused)["state"], "model_present_none_taken")
        self.assertTrue(any("was present and took no decision (not dispatched 1, unavailable 1, abstained 0)" in p
                            for p in loop_reply.check_arm(refused, compact)))
        # Counts cover every logged decision: a reply that omits its model decisions is still the arm's.
        omitted = top_score(raw)
        omitted["decisions"] = [d for d in omitted["decisions"] if d["source"]["kind"] != "model"]
        self.assertEqual(loop_reply.check_arm(omitted, compact), [])
        # Without counts in the log the included decisions are counted, and the reading says so.
        bare = starved(raw)
        bare["decision_log"]["counts"] = []
        self.assertEqual((loop_reply.context_presence(bare)["state"], loop_reply.context_presence(bare)["from"]),
                         ("model_present_no_call_dispatched", "included_decisions"))
        # A count row naming another model is not the arm's.
        other = top_score(raw)
        other["decision_log"]["counts"].append({"count": 1, "model": "context_large", "source": "not_dispatched",
                                                "stage": "follow_up"})
        self.assertTrue(any("counts decisions of context_large, not context_compact" in p
                            for p in loop_reply.check_arm(other, compact)))
        self.assertIsNone(loop_reply.context_presence(case("ticket_valid")))

    def test_a_stage_whose_model_decisions_were_all_shed_is_flagged_not_refused(self):
        raw = top_score(case("two_parts_valid"))
        compact = {"context": "context_compact", "device": "cuda", "precision": "float16"}
        self.assertEqual(loop_reply.arm_flags(raw, compact), [])
        raw["decision_log"]["counts"] += [
            {"count": 3, "model": "context_compact", "source": "not_dispatched", "stage": "follow_up"},
            {"count": 1, "model": "context_compact", "source": "unavailable", "stage": "follow_up"}]
        self.assertEqual(loop_reply.check_arm(raw, compact), [])  # the model took the coverage decisions
        self.assertEqual(loop_reply.arm_flags(raw, compact),
                         ["stage follow_up: the model took none of the 4 decisions due (not dispatched 3, "
                          "unavailable 1, abstained 0)"])
        self.assertEqual(loop_reply.context_presence(raw)["stages_without_model"], ["follow_up"])
        # No flag where there is nothing to flag: a Focus-only arm, a ticket, a reply the check refuses outright.
        self.assertEqual(loop_reply.arm_flags(focus_only(raw), {"context": None}), [])
        self.assertEqual(loop_reply.arm_flags(case("ticket_valid"), compact), [])
        self.assertEqual(loop_reply.arm_flags(starved(case("two_parts_valid")), compact), [])


# ---------------------------------------------------------------- evidence, coverage, depth


class V3Evidence(unittest.TestCase):
    def test_spans_come_from_records_not_bookkeeping(self):
        body = v3_reply(search_hits=[hit("README.md", 10, 20, "search section")],
                        symbol_hits=[hit("src/guard.rs", 3, 9, "fn restart_guard() {}")])
        body["parts"][1]["sub_needs"][0]["unrun_reads"].append(
            {"operation": "read_file", "reason": "not_chosen", "target": "x", "path": "src/never.rs", "start_line": 1})
        body["decisions"][0]["path"] = "src/decision.rs"
        spans = evidence.extract(body)
        self.assertEqual(sorted({s.path for s in spans}), ["README.md", "src/guard.rs"])
        self.assertTrue(all(s.kind == "text" for s in spans))

    def test_compact_span_with_excerpt_is_evidence_text(self):
        node = {"path": "src/guard.rs", "lines": [3, 9], "excerpt": "fn restart_guard() {}", "content_hash": "ab"}
        spans = evidence.extract({"parts": [{"sub_needs": [{"spans": [node]}]}]})
        self.assertEqual(spans, [evidence.Span("src/guard.rs", 3, 9, "fn restart_guard() {}", "text")])
        pointer = evidence.extract({"path": "src/guard.rs", "lines": [3, 9]})
        self.assertEqual(pointer[0].kind, "pointer")

    def projection(self, spans, rows=None):
        """A record's ``canonical_result`` as the kernel's Union builds it (``answer/union.rs`` ``project``)."""
        value = {"projection": "airk.arbiter.loop-evidence/1", "revision": "r1", "full_bytes": 4096,
                 "full_digest": "d" * 64, "hits_total": len(spans), "spans": spans}
        if rows is not None:
            value.update(rows_total=len(rows), rows=rows)
        return value

    def row(self, pointer, path, start, end, **carried):
        return {"pointer": pointer, "path": path, "start_line": start, "end_line": end, "content_hash": "c" * 64,
                "truncated": False, **carried}

    def test_loop_projection_counts_every_location_the_reply_delivered(self):
        """The reply carries each text once (change log entry 45): a row has ``text``, or ``same_as`` the row
        that carries it; ``also_at`` lists other places with the same text; ``overlaps`` marks a trimmed row.
        Coverage counts every such location, and trimming does not shrink a row's line range."""
        scenario = synthetic.scenario()
        plain = v3_reply(search_hits=[hit("docs/scheduler/CONTRACT.md", 1, 5)],
                         symbol_hits=[hit("src/scheduler.rs", 1, 16), hit("tests/scheduler_limits.rs", 1, 6)])
        expected = coverage.first_reply(scenario, plain, 2048)
        self.assertEqual(expected["essential_covered"], ["N1", "N2", "N3"])
        carrier = {"record": SYMBOLS_ID, "pointer": "/hits/0"}
        merged = v3_reply()
        for record in merged["records"]:
            record["canonical_result"] = self.projection(
                # The contract text is found only as a repeat: its own row names the carrier.
                [self.row("/hits/0", "docs/scheduler/CONTRACT.md", 1, 5, same_as=carrier)]
                if record["id"] == SEARCH_ID else
                [self.row("/hits/0", "src/scheduler.rs", 1, 16, text="fn schedule() { limit }", focus=0.91),
                 # Trimmed at its head: the note lies in lines 1-6, and the reply still delivered them.
                 self.row("/hits/1", "tests/scheduler_limits.rs", 1, 6, text="assert!(limited)",
                          overlaps=[dict(carrier, bytes=48, side="head")])])
        found = coverage.first_reply(scenario, merged, 2048)
        for key in ("essential_covered", "essential_missing", "supporting_covered"):
            self.assertEqual(found[key], expected[key], key)
        spans = evidence.extract(merged)
        self.assertEqual(sorted((s.path, s.start, s.end, s.kind) for s in spans),
                         [("docs/scheduler/CONTRACT.md", 1, 5, "text"), ("src/scheduler.rs", 1, 16, "text"),
                          ("tests/scheduler_limits.rs", 1, 6, "text")])
        # Only rows count. A place named in `also_at` alone is not read (every such place is a row of its own
        # in a real reply, and the string cannot be parsed safely); hits that are not rows (`hits_total` larger
        # than the row count) and records named only in `omitted_evidence` deliver nothing.
        listed = v3_reply()
        for record in listed["records"]:
            record["canonical_result"] = dict(self.projection(
                [] if record["id"] == SEARCH_ID else
                [self.row("/hits/0", "src/scheduler.rs", 1, 16, text="fn schedule() { limit }",
                          also_at=["docs/scheduler/CONTRACT.md:1-5"]),
                 self.row("/hits/1", "tests/scheduler_limits.rs", 1, 6, same_as=carrier, within=True)]),
                hits_total=9)
        listed["omitted_evidence"] = [{"id": "aa" * 32, "path": "docs/scheduler/CONTRACT.md", "start_line": 1,
                                       "end_line": 5}]
        also = coverage.first_reply(scenario, listed, 2048)
        self.assertNotIn("N3", also["essential_covered"])  # the contract note: named in also_at only
        self.assertEqual(sorted({s.path for s in evidence.extract(listed)}),
                         ["src/scheduler.rs", "tests/scheduler_limits.rs"])
        self.assertEqual(also["supporting_covered"], expected["supporting_covered"])  # the `within` row counts
        # A row with neither text nor a carrier delivers nothing, and name rows of a read without hits are
        # facts, not text.
        bare = v3_reply()
        for record in bare["records"]:
            record["canonical_result"] = self.projection(
                [self.row("/hits/0", "src/scheduler.rs", 1, 16)],
                rows=[{"path": "src/scheduler.rs", "start_line": 1, "end_line": 16, "text": "schedule"}])
        self.assertEqual(sorted({s.kind for s in evidence.extract(bare)}), ["fact", "pointer"])
        self.assertEqual(coverage.first_reply(scenario, bare, 2048)["essential_covered"], [])
        self.assertNotEqual(evidence.evidence_digest(merged), evidence.evidence_digest(listed))

    def test_digest_ignores_decisions_and_follows_records(self):
        body = v3_reply(symbol_hits=[hit("src/guard.rs", 3, 9)])
        same = copy.deepcopy(body)
        same["decisions"][1]["margin"] = 9.0
        same["decision_log"]["digest"] = "47" * 32
        same["elapsed_ms"] = 99
        self.assertEqual(evidence.evidence_digest(body), evidence.evidence_digest(same))
        other = v3_reply(symbol_hits=[hit("src/guard.rs", 3, 10)])
        self.assertNotEqual(evidence.evidence_digest(body), evidence.evidence_digest(other))

    def test_first_reply_and_attribution(self):
        scenario = synthetic.scenario()
        body = v3_reply(search_hits=[hit("docs/scheduler/CONTRACT.md", 1, 5)],
                        symbol_hits=[hit("src/scheduler.rs", 1, 16), hit("tests/scheduler_limits.rs", 1, 6)])
        first = coverage.first_reply(scenario, body, 2048)
        self.assertEqual(first["essential_covered"], ["N1", "N2", "N3"])
        self.assertEqual(first["essential_missing"], ["N4"])
        self.assertEqual(first["supporting_covered"], ["N5"])
        self.assertEqual(first["sub_needs"], {"covered": 1, "partial": 1, "missing": 0})
        self.assertIsNone(first["partial"])
        where = {n["id"]: n for n in depth.answer_attribution(scenario, body)["notes"]}
        self.assertEqual((where["N1"]["record_rank"], where["N1"]["operation"]), (2, "symbols"))
        self.assertEqual(where["N1"]["cited_by"], [[1, 1]])
        self.assertEqual(where["N1"]["uses"], [{"part": 1, "need": 1, "round": 1, "cause": "structural_read"}])
        self.assertEqual(where["N3"]["record_rank"], 1)
        self.assertEqual(where["N3"]["sub_needs_covering"], [[2, 1]])
        self.assertIsNone(where["N4"]["record_rank"])

    def test_coverage_turns_note_the_v3_reply(self):
        scenario = synthetic.scenario()
        body = v3_reply(symbol_hits=[hit("src/scheduler.rs", 1, 16)])
        result = coverage.compute(scenario, [{"turn": 1, "tool": "airk_answer", "ok": True, "bytes": 10}],
                                  body_loader=lambda row: body)
        self.assertEqual(result["turns"][0]["answer_v3"]["stops"], ["covered", "no_progress"])
        self.assertEqual(result["turns"][0]["covered"], ["N1", "N2"])


# ---------------------------------------------------------------- tickets


class Outcome:
    def __init__(self, body, ms=10):
        self.ok, self.exit_code, self.elapsed_ms, self.error = True, 0, ms, None
        self.stdout = envelope(body)
        self.stderr = b""
        self.envelope = json.loads(self.stdout)


class Tickets(TempDir):
    def test_replay_attaches_to_a_ticket(self):
        sent = []
        replies = [case("ticket_valid"), v3_reply(symbol_hits=[hit("src/scheduler.rs", 1, 16)])]

        def backend(tool, args):
            sent.append(args)
            return Outcome(replies[len(sent) - 1], ms=7)

        waits = []
        row = replay.issue(None, "airk", "airk_answer", {"question": "q"}, self.tmp, "c", 5, backend, waits.append)
        self.assertEqual([a.get("continuation") for a in sent], [None, "50" * 32])
        self.assertEqual(waits, [30.0])  # retry_after_ms 90,000 is capped at 30 s a wait
        self.assertEqual(row["tickets"][0]["queue_position"], 3)
        self.assertEqual(row["elapsed_ms"], 7 + 30000 + 7)
        self.assertEqual(row["final_attempt_ms"], 7)
        self.assertEqual(row["loop"]["stops"], ["covered", "no_progress"])

    def test_wrapper_attaches_to_a_ticket(self):
        sent = []
        replies = [case("ticket_valid"), v3_reply()]

        def backend(tool, args):
            sent.append(args)
            return Outcome(replies[len(sent) - 1])

        wrapper = mcp_wrapper.Wrapper(handle=None, repo="airk", path="compact", instruction="q",
                                      catalog={t["name"]: t for t in synthetic.catalog()["tools"]},
                                      log_dir=self.tmp / "w", backend=backend)
        with mock.patch.object(mcp_wrapper.time, "sleep") as sleep:
            reply = wrapper.call("airk_answer", {"question": "q"})
        self.assertNotIn("isError", reply)
        self.assertEqual(sent[1]["continuation"], "50" * 32)
        self.assertEqual(sent[1]["schema"], "airk.arbiter.context.request/3")
        sleep.assert_called_once_with(30.0)
        turn = common.read_jsonl(self.tmp / "w" / "turns.jsonl")[0]
        self.assertEqual(turn["typed_retries"][0]["code"], "ticket")
        self.assertIn("airk.arbiter.context/3", reply["content"][0]["text"])


# ---------------------------------------------------------------- gate tests


def arm_doc(tmp: Path, arm: str, bodies: dict[str, dict], scenarios: dict[str, dict], *, elapsed=1000,
            configuration=None, check=None, memory=None, budget=None) -> dict:
    rows = []
    for sid, body in bodies.items():
        raw = tmp / arm / sid / "canonical.stdout.json"
        common.write_bytes(raw, envelope(body))
        call = {"ok": True, "elapsed_ms": elapsed, "final_attempt_ms": elapsed, "bytes": len(json.dumps(body)),
                "digest": evidence.evidence_digest(body), "result_path": str(raw), "result_sha256": "x",
                "budget_sent": dict(CASES["default_budget"], **(budget or {}))}
        row = gate.reply_row(scenarios[sid], call)
        row["arm_problems"] = loop_reply.check_arm(body, check) if check is not None else []
        row["arm_flags"] = loop_reply.arm_flags(body, check) if check is not None else []
        rows.append(row)
    return {"schema": gate.GATE_SCHEMA, "kind": "arm-replays", "arm": arm, "configuration": configuration or arm,
            "coverage_rule": evidence.COVERAGE_RULE, "budgets_sent": common.distinct_budgets(rows),
            "rows": rows, "memory": {"peak_used_mib": memory or {}}, "arm_ok": all(not r["arm_problems"] for r in rows),
            "arm_flagged": sorted(r["scenario_id"] for r in rows if r["arm_flags"])}


def scen(sid="airk-C1", repo="airk"):
    scenario = synthetic.scenario()
    scenario["id"], scenario["repository"] = sid, repo
    scenario["slot"] = sid.split("-")[1]
    return scenario


class Agreement(TempDir):
    def test_agree_differ_diverged_and_favour(self):
        scenarios = {"airk-C1": scen()}
        good = v3_reply(search_hits=[hit("docs/scheduler/CONTRACT.md", 1, 5)],
                        symbol_hits=[hit("src/scheduler.rs", 1, 16)])
        a = arm_doc(self.tmp, "C", {"airk-C1": good}, scenarios)
        same = gate.agreement(a, arm_doc(self.tmp, "L", {"airk-C1": copy.deepcopy(good)}, scenarios), scenarios)
        self.assertEqual(same["by_stage"]["coverage"]["agree"], 2)
        self.assertEqual(same["by_stage"]["coverage"]["agreement_share"], 1.0)
        self.assertEqual(same["views"]["same_evidence"], 1)
        changed = copy.deepcopy(good)
        changed["decisions"][1]["chosen"] = [{"class": "read_more_here", "symbol": "B"}]  # same roster, other card
        changed["decisions"][3]["roster_digest"] = "99" * 32  # other cards presented
        del changed["decisions"][2]
        for record in changed["records"]:
            if record["id"] == SYMBOLS_ID:
                record["canonical_result"]["hits"] = []  # part 1's evidence is gone in arm L
        result = gate.agreement(a, arm_doc(self.tmp, "L", {"airk-C1": changed}, scenarios), scenarios)
        by_status = {(p["stage"], p["part"]): p for p in result["rows"][0]["pairs"]}
        self.assertEqual(by_status[("coverage", 1)]["status"], "differ")
        self.assertEqual(by_status[("coverage", 1)]["favoured"], "a")
        self.assertEqual(by_status[("coverage", 2)]["status"], "only_a")
        self.assertEqual(by_status[("follow_up", 2)]["status"], "diverged")
        self.assertEqual(by_status[("split", None)]["status"], "agree")
        self.assertEqual(result["by_stage"]["coverage"]["agreement_share"], 0.0)
        self.assertEqual(result["scenario_favoured"]["a"], 1)
        self.assertFalse(result["rows"][0]["same_evidence"])


class Precision(TempDir):
    def test_escalation_rule(self):
        scenarios = {"airk-C1": scen(), "airk-C2": scen("airk-C2")}
        body = top_score(v3_reply(symbol_hits=[hit("src/scheduler.rs", 1, 16)]))
        fp16 = arm_doc(self.tmp, "C", {"airk-C1": body, "airk-C2": body}, scenarios)
        quiet = gate.precision(arm_doc(self.tmp, "C32", {"airk-C1": body, "airk-C2": body}, scenarios), fp16,
                               "compact", scenarios)
        self.assertEqual((quiet["flips"], quiet["escalate"]), (0, False))
        self.assertEqual(quiet["decisions"], 8)
        flipped = copy.deepcopy(body)
        flipped["decisions"][3]["chosen"] = [{"class": "follow_up_query", "symbol": "B"}]
        loud = gate.precision(arm_doc(self.tmp, "C32", {"airk-C1": flipped, "airk-C2": body}, scenarios), fp16,
                              "compact", scenarios)
        self.assertEqual((loud["flips"], loud["flip_share"], loud["escalate"]), (1, 0.125, True))
        self.assertEqual(loud["by_decision_type"]["follow_up"]["flips"], 1)
        fewer = v3_reply()
        evidence_only = gate.precision(arm_doc(self.tmp, "C32", {"airk-C1": top_score(fewer), "airk-C2": body},
                                               scenarios), fp16, "compact", scenarios)
        self.assertEqual(evidence_only["essential_differs"], ["airk-C1"])
        self.assertTrue(evidence_only["escalate"])
        basis = copy.deepcopy(body)
        basis["decisions"][1]["basis"]["digest"] = "77" * 32
        focus = gate.precision(arm_doc(self.tmp, "CF32", {"airk-C1": basis, "airk-C2": body}, scenarios), fp16,
                               "focus", scenarios)
        self.assertEqual(focus["flips"], 1)  # Focus's choice is the presented basis
        with self.assertRaises(common.HarnessError):
            gate.precision(fp16, fp16, "context", scenarios)


class Outcomes(TempDir):
    def rows(self, grades_a, grades_b, turns_a, turns_b):
        def row(g, t, sid):
            return {"repo": "airk", "kind": "coding", "adjudicated_grade": g, "correct_finish": t is not None,
                    "turns_to_finish": t, "rubric_b_j1": 3.0, "rubric_b_j2": 2.0, "key": sid}
        a = {f"airk-C{i}": row(g, t, i) for i, (g, t) in enumerate(zip(grades_a, turns_a), start=1)}
        b = {f"airk-C{i}": row(g, t, i) for i, (g, t) in enumerate(zip(grades_b, turns_b), start=1)}
        return a, b

    def test_arm_compare_and_close_calls(self):
        a, b = self.rows([4, 2, 0, 3, 1], [3, 3, 2, 3, 2], [1, 3, None, 2, None], [2, 2, 4, 2, 4])
        result = grade.arm_compare(a, b)
        winners = [p["winner"] for p in result["scenarios"]]
        self.assertEqual(winners, ["a", "b", "b", "tie", "b"])
        self.assertEqual((result["b_wins"], result["b_losses"], result["ties"]), (3, 1, 1))
        self.assertEqual(result["finishes_added"], 2)
        self.assertEqual(result["close_calls"], ["airk-C1", "airk-C2", "airk-C4", "airk-C5"])
        self.assertEqual(result["a_rubric_b_mean"], 2.5)
        self.assertEqual(grade.rubric_a_winner(None, a["airk-C1"]), "pending")

    def test_outcomes_from_a_pass_and_second_jobs(self):
        pass_dir = self.tmp / "pass"
        common.write_json(pass_dir / "pass-index.json", {"series": "lg", "results": [
            {"key": "airk-C1/compact-gpu", "judges": []}, {"key": "airk-C1/large-gpu", "judges": []}]})
        rows = [{"key": "airk-C1/compact-gpu", "configuration": "compact", "device": "gpu", "repo": "airk",
                 "slot": "C1", "kind": "coding", "sample": 1, "adjudicated_grade": 3, "correct_finish": True,
                 "turns_to_finish": 2, "rubric_b_j1": None, "rubric_b_j2": None},
                {"key": "airk-C1/large-gpu", "configuration": "large", "device": "gpu", "repo": "airk",
                 "slot": "C1", "kind": "coding", "sample": 1, "adjudicated_grade": 4, "correct_finish": True,
                 "turns_to_finish": 1, "rubric_b_j1": None, "rubric_b_j2": None}]
        with mock.patch.object(grade, "build_rows", return_value=rows):
            result = gate.outcomes(pass_dir, "compact-gpu", "large-gpu")
        self.assertEqual(result["b_wins"], 1)
        self.assertEqual(result["second_sessions"], [["airk-C1", "compact-gpu"], ["airk-C1", "large-gpu"]])
        second = [dict(rows[0], sample=2, key="airk-C1/compact-gpu.s2", adjudicated_grade=4, turns_to_finish=1),
                  dict(rows[1], sample=2, key="airk-C1/large-gpu.s2", adjudicated_grade=3, turns_to_finish=2)]
        with mock.patch.object(grade, "build_rows", return_value=rows + second):
            settled_ = gate.outcomes(pass_dir, "compact-gpu", "large-gpu")
        self.assertEqual(settled_["scenarios"][0]["winner"], "undecided")
        self.assertEqual((settled_["b_wins"], settled_["undecided"], settled_["second_sessions"]), (0, 1, []))


class ArmRunsAndConcurrency(TempDir):
    def spec(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        spec = make_loop_spec.gate_spec("lg", self.tmp / "R", make_loop_spec.PROFILES, ["airk"], {})
        spec.update(scenarios_dir=str(paths["scenario"].parent.parent), freeze=None, handle=str(paths["handle"]))
        return spec

    def test_arm_replays_check_identity_and_sample_memory(self):
        spec = self.spec()
        body = top_score(v3_reply(symbol_hits=[hit("src/scheduler.rs", 1, 16)]))
        calls = []

        def backend(tool, args):
            calls.append(args)
            return Outcome(body, ms=40)

        readings = iter([{"0": 1000}, {"0": 7000}, {"0": 3000}])
        sampler = gate.MemorySampler(query=lambda: next(readings, {"0": 2000}), interval_s=60)
        with contextlib.redirect_stdout(io.StringIO()):
            doc = gate.arm_replays(spec, "C", self.tmp / "R", self.tmp / "out", handle=object(), backend=backend,
                                   sampler=sampler, sleep=lambda s: None)
        self.assertTrue(doc["arm_ok"])
        self.assertEqual(doc["memory"]["peak_used_mib"], {"0": 7000})
        self.assertEqual(calls[0]["budget"], common.ANSWER_BUDGET_DEFAULT)
        self.assertEqual(doc["rows"][0]["first_reply"]["essential_covered"], ["N1", "N2"])
        self.assertEqual(gate.load_arm(self.tmp / "out")["arm"], "C")
        with contextlib.redirect_stdout(io.StringIO()):
            wrong = gate.arm_replays(spec, "L", self.tmp / "R", self.tmp / "out-l", handle=object(), backend=backend,
                                     sampler=gate.MemorySampler(query=lambda: None), sleep=lambda s: None)
        self.assertFalse(wrong["arm_ok"])
        self.assertGreaterEqual(wrong["memory"]["failures"], 2)

    def test_concurrency_levels_and_tickets(self):
        spec = self.spec()
        body = top_score(v3_reply())
        ticket = case("ticket_valid")
        seen = []

        def backend(tool, args):
            seen.append(args.get("continuation"))
            return Outcome(ticket if len(seen) == 1 else body, ms=5)

        doc = gate.concurrency(spec, "C", self.tmp / "R", self.tmp / "conc", [1, 2], handle=object(),
                               backend=backend, sampler_factory=lambda: gate.MemorySampler(query=lambda: {"0": 5}),
                               sleep=lambda s: None)
        self.assertEqual([r["level"] for r in doc["runs"]], [1, 2])
        self.assertEqual(doc["runs"][0]["tickets"], 1)
        self.assertEqual(doc["runs"][1]["answers"], 1)
        self.assertIsNotNone(doc["runs"][1]["answers_per_minute"])
        perf = gate.performance({"C": arm_doc(self.tmp, "C", {"airk-C1": body}, {"airk-C1": scen()})},
                                {"C": doc}, [{"from": None, "to": {"context": "x"}, "release_s": 0, "warm_s": 2.5}])
        self.assertEqual(perf["arms"]["C"]["rounds"], {"2": 1})
        self.assertEqual(perf["arms"]["C"]["model_ms_per_call_median"], 0.003)
        self.assertEqual(perf["concurrency"]["C"][0]["tickets"], 1)
        self.assertEqual(perf["switches"][0]["warm_s"], 2.5)


# ---------------------------------------------------------------- report


class Report(TempDir):
    def test_failed_reads_are_counted_per_arm_and_kind(self):
        rows = [{"ok": True, "loop": {"read_failures": {"search:read_failed": 3}}},
                {"ok": True, "loop": {"read_failures": {}}}, {"ok": True, "loop": {"parts": 1}},
                {"ok": False, "loop": None}]
        lines, figures = gate_report.read_failure_section({"F": {"rows": rows}})
        self.assertEqual(figures["F"], {"replies": 3, "replies_with_failures": 1, "not_counted": 1,
                                        "by_kind": {"search:read_failed": 3}})
        self.assertTrue(any("search:read_failed x3" in line for line in lines))

    def build(self, large_rows, large_elapsed=1500):
        scenarios = {"airk-C1": scen(), "bookbuilder-C1": scen("bookbuilder-C1", "bookbuilder")}
        secret = "SECRET_BOOK_SOURCE_LINE"
        body = top_score(v3_reply(symbol_hits=[hit("src/scheduler.rs", 1, 16, secret)]))
        docs = {"C": arm_doc(self.tmp, "C", {k: body for k in scenarios}, scenarios, elapsed=1000),
                "L": arm_doc(self.tmp, "L", {k: top_score(body, "context_large") for k in scenarios}, scenarios,
                             elapsed=large_elapsed),
                "F": arm_doc(self.tmp, "F", {k: focus_only(body) for k in scenarios}, scenarios, elapsed=800)}
        inputs = {"series": "05-gate-lg", "arms": {}, "agreement": {}, "outcomes": {}, "precision": []}
        for arm, doc in docs.items():
            common.write_json(self.tmp / "gate" / arm / "arm-replays.json", doc)
            inputs["arms"][arm] = str(self.tmp / "gate" / arm)
        for a, b in (("C", "L"), ("F", "C")):
            path = self.tmp / "gate" / f"agreement-{a}-{b}.json"
            common.write_json(path, gate.agreement(docs[a], docs[b], scenarios))
            inputs["agreement"][f"{a}-{b}"] = str(path)
        judging = [{"judge": "j1", "valid": True, "misleading_followed": False, "misleading_reason": "",
                    "notes": [{"id": "N1", "verdict": "correct", "reason": f'it quotes `{secret}` and "{secret}"'}]}]
        outcome = {"scenarios": [{"scenario": sid, "winner": w, "close_call": False, "a_grade": 3, "b_grade": 3,
                                  "a_turns": 2, "b_turns": 2, "a_judging": judging, "b_judging": judging}
                                 for sid, w in large_rows],
                   "finishes_added": 0, "b_wins": sum(1 for _s, w in large_rows if w == "b"),
                   "b_losses": sum(1 for _s, w in large_rows if w == "a"), "ties": 0, "pending": 0, "undecided": 0,
                   "a_correct_finishes": 2, "b_correct_finishes": 2, "a_rubric_b_mean": 2.0, "b_rubric_b_mean": 2.0,
                   "close_calls": []}
        for pair in ("C-L", "F-C"):
            path = self.tmp / "gate" / f"outcomes-{pair}.json"
            common.write_json(path, outcome)
            inputs["outcomes"][pair] = str(path)
        prec = gate.precision(docs["C"], docs["C"], "compact", scenarios)
        common.write_json(self.tmp / "gate" / "precision.json", prec)
        inputs["precision"] = [str(self.tmp / "gate" / "precision.json")]
        common.write_json(self.tmp / "gate" / "performance.json", gate.performance(docs, {}, []))
        inputs["performance"] = str(self.tmp / "gate" / "performance.json")
        return gate_report.build(inputs), secret

    def test_report_recommends_dropping_large_without_an_outcome_condition(self):
        (text, figures), secret = self.build([("airk-C1", "tie"), ("bookbuilder-C1", "tie")])
        self.assertIn("The 12.7.1 bar indicates dropping Large: no outcome condition was met.", text)
        self.assertIn("The 12.7.2 bar indicates shipping the loop with Focus only", text)
        self.assertNotIn("I recommend", text)  # the report states what the bars indicate; the user decides
        self.assertFalse(figures["bars"]["C-L"]["met"])
        for section in text.split("#### bookbuilder-C1")[1:]:
            self.assertNotIn(secret, section.split("\n## ")[0].split("\n#### ")[0])
        self.assertIn(gate_report.WITHHELD, text)  # private repositories: reasons withheld entirely
        evidence_lines = [line for line in text.splitlines() if " evidence: " in line]
        self.assertTrue(evidence_lines)
        self.assertFalse(any(secret in line for line in evidence_lines))  # evidence is located, never quoted
        self.assertIn("src/scheduler.rs:1-16", text)
        self.assertIn("## Receipts", text)
        self.assertIn("No stage is sensitive to precision", text)
        self.assertIn("proposed bar; the decision rule is the user's", text)
        self.assertIn("recommendations; the user decides", text)
        limits = text.split("### Evidence limits")[1].split("\n## ")[0]
        self.assertIn("close calls", limits)
        self.assertIn("concurrency test (12.7.1 test 4) did not run", limits)
        self.assertIn("not run, so not passed", text)

    def test_report_states_the_coverage_rule_and_the_decision_sources(self):
        (text, figures), _ = self.build([("airk-C1", "tie"), ("bookbuilder-C1", "tie")])
        self.assertIn("Reference notes are counted under coverage rule 3 in every arm", text)
        self.assertIn("whole identifier", text.split("## What the bars indicate")[0])
        self.assertEqual(figures["coverage_rule"], {"arms": {"C": 3, "F": 3, "L": 3}, "uniform": True})
        section = text.split("## Decisions by source, per arm and stage")[1].split("\n## ")[0]
        # Each of the two fixture replies, as a gate arm produces it, logs split 1 deterministic, coverage 2
        # model (top score, so no abstention) and follow_up 1 deterministic. The report counts the decision log.
        self.assertIn("| C | coverage | 4 | 0 | 0 | 0 | 0 |", section)
        self.assertIn("| C | all stages | 4 | 0 | 0 | 4 | 0 |", section)
        self.assertIn("| F | all stages | 0 | 0 | 0 | 8 | 0 |", section)
        self.assertIn("| Not dispatched |", section)
        self.assertIn("never sent: no token budget, no time or no model", section)
        self.assertNotIn("with a reason in the reply", section)  # nothing fell back in these arms
        self.assertEqual(figures["decision_sources"]["C"]["total"],
                         {"model": 4, "fell_back": 0, "not_dispatched": 0, "no_model": 4, "other": 0})
        self.assertEqual(figures["decision_sources"]["C"]["replies"], 2)
        self.assertIn("| C | 2 | 2 | 0 | 0 | 0 |", section)  # replies by whether the model took part
        self.assertIn("| F | 2 | 0 | 0 | 0 | 2 |", section)
        self.assertIn("was sent this budget:", text.split("## What the bars indicate")[0])
        self.assertTrue(figures["budgets_sent"]["uniform"])

    def test_report_counts_a_starved_model_apart_from_focus_only_and_states_the_budget(self):
        scenarios = {"airk-C1": scen(), "airk-C2": scen("airk-C2")}
        compact = {"context": "context_compact", "device": "cuda", "precision": "float16"}
        body = top_score(v3_reply())
        partly = copy.deepcopy(body)
        partly["decision_log"]["counts"].append({"count": 2, "model": "context_compact", "source": "not_dispatched",
                                                 "stage": "follow_up"})
        docs = {"C": arm_doc(self.tmp, "C", {"airk-C1": starved(v3_reply()), "airk-C2": partly}, scenarios,
                             check=compact),
                "F": arm_doc(self.tmp, "F", {k: focus_only(body) for k in scenarios}, scenarios,
                             check={"context": None})}
        self.assertFalse(docs["C"]["arm_ok"])  # the starved reply is not the arm's
        self.assertTrue(docs["F"]["arm_ok"])
        self.assertEqual(docs["C"]["arm_flagged"], ["airk-C2"])
        found = gate.decision_sources(docs["C"])
        self.assertEqual(found["presence"], {"model_took_decisions": 1, "model_present_no_call_dispatched": 1,
                                             "model_present_none_taken": 0, "no_model": 0})
        self.assertEqual(found["presence_scenarios"], {"model_present_no_call_dispatched": ["airk-C1"]})
        self.assertEqual(found["stages_without_model"], {"coverage": ["airk-C1"], "follow_up": ["airk-C2"]})
        self.assertEqual(gate.decision_sources(docs["F"])["presence"]["no_model"], 2)
        lines, figures = gate_report.decision_source_section(docs)
        text = "\n".join(lines)
        self.assertIn("| Model present, no call dispatched |", text)
        self.assertIn("| C | 2 | 1 | 1 | 0 | 0 |", text)
        self.assertIn("| F | 2 | 0 | 0 | 0 | 2 |", text)
        self.assertIn("C, model present no call dispatched: airk-C1.", text)
        self.assertIn("C, stage follow_up: decisions were due to the model and it took none in 1 replies (airk-C2).",
                      text)
        self.assertIn("is not one: the arm had the model and did not reach it", text)
        # The starved reply includes its two not-dispatched decisions with their reason; the other reply's two
        # shed follow-up decisions are in the counts only.
        self.assertIn("with a reason in the reply: C: 2 of 4", text)
        self.assertIn("not_dispatched/token_budget 2", text)
        identity = "\n".join(gate_report.identity_section(docs))
        self.assertIn("| airk-C2 |", identity)
        self.assertIn("was present and no call was dispatched", identity)
        limits = "\n".join(gate_report.evidence_limits(docs, {}, [], None))
        self.assertIn("Context model was present and took no decision: C 1", limits)
        self.assertIn("one stage's model decisions were all shed: C 1", limits)
        # The budget every Answer was sent is stated; arms sent different budgets are named.
        same, figures = gate_report.budget_statement(docs)
        self.assertIn("Every Answer of C, F was sent this budget:", same[0])
        self.assertIn(f"input_tokens {CASES['default_budget']['input_tokens']}", same[0])
        self.assertTrue(figures["uniform"])
        docs["F"] = arm_doc(self.tmp, "F2", {k: focus_only(body) for k in scenarios}, scenarios,
                            budget={"input_tokens": 131072})
        different, figures = gate_report.budget_statement(docs)
        self.assertIn("different budgets, so their figures do not compare", different[0])
        self.assertIn("input_tokens 131072", different[0])
        self.assertFalse(figures["uniform"])
        self.assertIn("different budgets", "\n".join(gate_report.evidence_limits(docs, {}, [], None)))
        old = dict(docs["C"])
        old.pop("budgets_sent")
        unknown, figures = gate_report.budget_statement({"C": old})
        self.assertIn("not recorded for C", "\n".join(unknown))
        self.assertEqual(figures["not_recorded"], ["C"])

    def test_arms_scored_under_different_coverage_rules_are_named(self):
        scenarios = {"airk-C1": scen()}
        body = top_score(v3_reply())
        docs = {"C": arm_doc(self.tmp, "C", {"airk-C1": body}, scenarios),
                "L": dict(arm_doc(self.tmp, "L", {"airk-C1": body}, scenarios), coverage_rule=2)}
        lines, figures = gate_report.coverage_rule_statement(docs)
        self.assertIn("different coverage rules (C rule 3, L rule 2)", lines[0])
        self.assertFalse(figures["uniform"])
        self.assertIn("different coverage rules", "\n".join(gate_report.evidence_limits(docs, {}, [], None)))
        old, _ = gate_report.coverage_rule_statement({"C": dict(docs["C"], coverage_rule=2)})
        self.assertIn("coverage rule 2 in every arm", old[0])
        self.assertIn("substring match", old[0])
        self.assertIn("no arm replays", gate_report.coverage_rule_statement({})[0][0])

    def test_bar_conditions(self):
        outcome = {"finishes_added": 3, "b_wins": 0, "b_losses": 0, "pending": 0}
        scenarios = {"airk-C1": scen()}
        body = top_score(v3_reply(symbol_hits=[hit("src/scheduler.rs", 1, 16)]))
        a = arm_doc(self.tmp, "C", {"airk-C1": body}, scenarios, elapsed=1000)
        b = arm_doc(self.tmp, "L", {"airk-C1": body}, scenarios, elapsed=2000)
        self.assertTrue(gate_report.model_bar(outcome, a, b)["met"])
        slow = arm_doc(self.tmp, "L2", {"airk-C1": body}, scenarios, elapsed=2001)
        bar = gate_report.model_bar(outcome, a, slow)
        self.assertFalse(bar["met"])
        self.assertIn("time condition", gate_report.large_recommendation(bar))
        thin = arm_doc(self.tmp, "L3", {"airk-C1": top_score(v3_reply())}, scenarios, elapsed=1000)
        self.assertFalse(gate_report.model_bar(outcome, a, thin)["coverage_condition"]["airk"])
        rubric = gate_report.model_bar({"finishes_added": 0, "b_wins": 8, "b_losses": 3, "pending": 0}, a, b)
        self.assertTrue(rubric["rubric_a_condition"] and rubric["met"])
        self.assertIn("indicates continuing Large", gate_report.large_recommendation(rubric))

    def test_redact(self):
        self.assertEqual(gate_report.redact('uses `fn x()` and "a b" here'),
                         "uses [quoted text omitted] and [quoted text omitted] here")
        self.assertTrue(gate_report.redact("x" * 500).endswith("…"))
        self.assertEqual(gate_report.reason_for("airk", "keep `code`"), "keep `code`")


# ---------------------------------------------------------------- specs and plans


class Specs(TempDir):
    def test_gate_spec_arms(self):
        # The LG run is the judged arms only: the precision check is deferred, with its reason in the spec.
        judged = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), {})
        self.assertEqual(sorted(e["arm"] for e in judged["results"]), sorted(make_loop_spec.JUDGED_ARMS))
        self.assertEqual(judged["gate"]["precision"], {"run": False, "reason": make_loop_spec.PRECISION_DEFERRED})
        self.assertEqual(judged["gate"]["precision_pairs"], [])
        self.assertIn("distinct scenario question per concurrent call", judged["gate"]["concurrency_path"])
        self.assertEqual(make_loop_spec.PROFILES["focus"], "focus-1024-cuda")  # the gate is CUDA only
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), {},
                                        precision=True)
        arms = {e["arm"]: e for e in spec["results"]}
        self.assertEqual(sorted(arms), sorted([*make_loop_spec.JUDGED_ARMS, *make_loop_spec.PRECISION_ARMS]))
        self.assertEqual(len(spec["scenario_ids"]), 20)
        self.assertEqual(arms["F"]["arm_check"], {"context": None, "focus": True})
        self.assertTrue(all(e["arm_check"]["focus"] for e in arms.values()))  # every gate arm stages Focus
        self.assertEqual(arms["L"]["arm_check"]["context"], "context_large")
        self.assertEqual(arms["L32"]["arm_check"]["precision"], "float32")
        self.assertTrue(all(e["replay_only"] for k, e in arms.items() if k in make_loop_spec.PRECISION_ARMS))
        self.assertTrue(all(e["reuse"] is False and e["device"] == "gpu" for e in arms.values()))
        self.assertEqual(spec["answer_defaults"]["budget"], common.ANSWER_BUDGET_DEFAULT)
        self.assertEqual(len({run_pass.Pass.result_name(e) for e in spec["results"]}), len(spec["results"]))
        for _stage, fp32, fp16 in make_loop_spec.PRECISION_PAIRS:
            self.assertIn(fp32, arms)
            self.assertIn(fp16, arms)

    def test_gate_lifetimes_carry_a_dispatch_census_from_the_plan(self):
        # The LG lifetimes ended on the fixed 1,024-dispatch census (23 November 2026). A gate lifetime now gets a
        # bound from its plan: 20 scenarios and 2 repositories at the default budget's 24 Focus + 48 Context
        # calls: (2 smoke + 20 x 2 replays + 20 x 2 x 12 session Answers + 2 + 4 concurrent) x 72 = 38,016.
        self.assertEqual(make_loop_spec.lifetime_answers(20, 2, [2, 4]), 528)
        budget = {"focus_calls": 24, "context_calls": 48}
        self.assertEqual(make_loop_spec.dispatch_census(528, budget), 38016)
        self.assertEqual(make_loop_spec.dispatch_census(1, budget), 1024)  # never below the old bound
        with self.assertRaises(common.HarnessError):
            make_loop_spec.dispatch_census(1000, budget)  # 72,000 is above the service's 65,536
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), {})
        expected = make_loop_spec.dispatch_census(
            make_loop_spec.lifetime_answers(20, 2, [2, 4]), common.ANSWER_BUDGET_DEFAULT)
        self.assertEqual(spec["gate"]["dispatch_census"], expected)
        for row in spec["gate"]["lifetimes"].values():
            args = row["start_args"]
            self.assertEqual(args[args.index("--dispatch-census") + 1], str(expected))
        # A lifetime that names a smaller census, or turns the observer off, is refused.
        small = {"C": {"start_args": ["--dispatch-census", "2048"]}}
        with self.assertRaises(common.HarnessError):
            make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), {},
                                     lifetimes=make_loop_spec.gate_lifetimes("lg", self.tmp, small))
        off = {"C": {"start_args": ["--no-dispatch-observer"]}}
        with self.assertRaises(common.HarnessError):
            make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), {},
                                     lifetimes=make_loop_spec.gate_lifetimes("lg", self.tmp, off))

    def test_cuda_arms_carry_a_stage_guard_for_each_session(self):
        # Every session of a CUDA arm first checks its stages are active (gpu_fixture.py ensure).
        overrides = {a: {"launcher": "gpu_fixture.py"} for a in make_loop_spec.JUDGED_ARMS}
        lifetimes = make_loop_spec.gate_lifetimes("lg", self.tmp, overrides)
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, ["airk"], {}, lifetimes=lifetimes)
        arms = {e["arm"]: e for e in spec["results"]}
        guard = arms["C"]["stage_guard"]
        self.assertEqual(guard[guard.index("ensure") + 1:], ["--lifetime", "lg-C", "--profiles",
                                                             "focus-1024-cuda,context-compact-fp16-cuda"])
        self.assertEqual(arms["F"]["stage_guard"][-1], "focus-1024-cuda")
        shake = make_loop_spec.shakedown_spec("s", self.tmp, self.tmp / "o", str(self.tmp / "fx" / "life" / "handle.json"),
                                              ["munarium-A1"], make_loop_spec.PROFILES, {})
        self.assertEqual(shake["results"][0]["stage_guard"][-4:-2], ["--lifetime", "life"])

    def test_shakedown_spec_keeps_outputs_out_of_r(self):
        # Arm C alone on a named handle; scenarios and their freeze from R, the prompts freeze under the
        # shakedown's own root; the budget is the contract default.
        out = self.tmp / "shake"
        spec = make_loop_spec.shakedown_spec("s", self.tmp / "R", out, "H/handle.json", ["munarium-A1", "munarium-C2"],
                                             make_loop_spec.PROFILES, {})
        self.assertEqual([e["arm"] for e in spec["results"]], ["C"])
        self.assertEqual(spec["results"][0]["handle"], "H/handle.json")
        self.assertEqual(spec["repos"], ["munarium"])
        self.assertEqual(spec["scenario_ids"], ["munarium-A1", "munarium-C2"])
        self.assertEqual(spec["freeze"], str(self.tmp / "R" / "scenario-freeze.json"))
        self.assertEqual(spec["prompts_freeze"], str(out / "design" / "prompts-freeze.json"))
        self.assertEqual(spec["answer_defaults"]["budget"], common.ANSWER_BUDGET_DEFAULT)
        self.assertEqual(make_loop_spec.gate_report_inputs(spec, out)["arms"], {"C": str(out / "passes" / "s" / "gate" / "C")})
        with self.assertRaises(common.HarnessError):
            make_loop_spec.shakedown_spec("s", self.tmp, out, "h", ["nowhere-A1"], make_loop_spec.PROFILES, {})

    def test_gate_plan_switches_before_each_arm(self):
        # The LG plan: one launch of the judged arms, no precision phase, and the concurrency test after
        # every judged and second session, before the lifetimes stop.
        lg_spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, ["airk"], {})
        lg_plan = make_loop_spec.gate_plan(self.tmp / "spec.json", lg_spec, self.tmp)
        lg_kinds = [next(iter(step)) for step in lg_plan]
        self.assertEqual([[r["arm"] for r in s["lifetimes"]["start"]] for s in lg_plan if "lifetimes" in s],
                         [["C", "L", "F"]])
        lg_commands = [s["gate"]["command"] if "gate" in s else k for s, k in zip(lg_plan, lg_kinds)]
        self.assertNotIn("precision", lg_commands)
        first = lg_commands.index("concurrency")
        self.assertGreater(first, lg_commands.index("second"))
        self.assertGreater(first, lg_commands.index("sessions"))
        self.assertLess(first, lg_commands.index("stop_lifetimes"))
        self.assertEqual(lg_commands.count("concurrency"), 3)
        lg_inputs = make_loop_spec.gate_report_inputs(lg_spec, self.tmp)
        self.assertEqual((lg_inputs["precision"], lg_inputs["precision_note"]),
                         ([], make_loop_spec.PRECISION_DEFERRED))
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, ["airk"], {}, precision=True)
        plan = make_loop_spec.gate_plan(self.tmp / "spec.json", spec, self.tmp)
        kinds = [next(iter(step)) for step in plan]
        for index, step in enumerate(plan):
            if "parallel" in step and step["parallel"][0]["gate"]["command"] == "smoke":
                # Every launch is followed by the live smoke test, then the replays, on its own arms.
                self.assertEqual(kinds[index - 1], "lifetimes")
                replays_step = plan[index + 1]["parallel"]
                self.assertEqual([g["gate"]["command"] for g in replays_step], ["replays"] * len(replays_step))
                self.assertEqual([g["gate"]["arm"] for g in step["parallel"]],
                                 [r["arm"] for r in plan[index - 1]["lifetimes"]["start"]])
        smokes = [s for s in plan if "parallel" in s and s["parallel"][0]["gate"]["command"] == "smoke"]
        self.assertEqual(len(smokes), len([s for s in plan if "lifetimes" in s]))
        self.assertNotIn("switch_context", kinds)
        sessions = [s["sessions"] for s in plan if "sessions" in s]
        self.assertEqual([s["groups"] for s in sessions], [[["@any", "@any", "compact-gpu,large-gpu,focus-gpu"]]])
        launches = [s["lifetimes"]["start"] for s in plan if "lifetimes" in s]
        self.assertEqual([[r["arm"] for r in rows] for rows in launches], [["C", "L", "F"], ["C32"], ["L32"],
                                                                          ["CF16", "CF32"]])
        for rows in launches:  # every launch phase fits the 48 GiB A6000 with the 4 GiB reserve
            self.assertLessEqual(sum(r["device_gib"] for r in rows) + 4, 48)
        self.assertEqual(launches[0][0]["stages"], [{"stage": "focus", "profile": make_loop_spec.PROFILES["focus"]},
                                                    {"stage": "context", "profile": make_loop_spec.PROFILES["compact"]}])
        self.assertEqual(len({r["fixtures"] for rows in launches for r in rows}), 7)  # one fixture root per arm
        self.assertEqual(len({e["handle"] for e in spec["results"]}), 7)
        self.assertTrue(spec["parallel_entries"] and spec["settle_per_root"])
        self.assertEqual(kinds.index("lifetimes"), 0)
        self.assertLess(kinds.index("stop_lifetimes"), kinds.index("lifetimes", 1))
        conc = [s["gate"]["arm"] for s in plan if "gate" in s and s["gate"]["command"] == "concurrency"]
        self.assertEqual(conc, ["C", "L", "F"])
        self.assertLess(kinds.index("sessions"), kinds.index("stop_lifetimes"))
        commands = [s["gate"]["command"] for s in plan if "gate" in s]
        self.assertEqual(commands.count("precision"), len(make_loop_spec.PRECISION_PAIRS))
        self.assertLess(kinds.index("grade"), kinds.index("second"))
        inputs = make_loop_spec.gate_report_inputs(spec, self.tmp)
        self.assertEqual(sorted(inputs["outcomes"]), ["C-L", "F-C"])

    def test_l7_spec_has_no_cpu_cells_and_the_template_is_current(self):
        spec = make_loop_spec.l7_spec("l7", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), ["compact"], {})
        names = [run_pass.Pass.result_name(e) for e in spec["results"]]
        self.assertEqual(names, ["search", "parse", "focus-gpu", "compact-gpu"])
        groups = make_loop_spec.l7_plan(self.tmp / "spec.json", spec, self.tmp)[0]["sessions"]["groups"]
        self.assertEqual(groups, [["@any", "@any", "search,parse"]])
        self.assertFalse(any(e.get("device") == "cpu" for e in spec["results"]))
        with self.assertRaises(common.HarnessError):
            make_loop_spec.l7_spec("l7", self.tmp, make_loop_spec.PROFILES, ["airk"], ["huge"], {})
        committed = common.read_json(HERE / "l7_plan_template.json")
        self.assertEqual(committed, make_loop_spec.template(), "regenerate l7_plan_template.json (make_loop_spec.py template)")


class PlanHook(TempDir):
    def plan(self):
        args = argparse.Namespace(r_root=self.tmp, log=self.tmp / "plan.jsonl", plan=None, from_step=1, hold=False,
                                  live_cuda=None)
        plan = run_plan.Plan(args)
        plan.calls = []
        plan.lifetime_name = lambda: "L40-gate"
        plan.stage_command = lambda life, profile, enable, repos, stage: plan.calls.append((profile, enable, stage))
        plan.gpu_memory = lambda: {"0": 100}
        return plan

    def test_switch_releases_before_warming_and_skips_an_unchanged_switch(self):
        plan = self.plan()
        with contextlib.redirect_stdout(io.StringIO()):
            plan.switch_context({"focus": "f-w8", "context": "c16", "receipt": str(self.tmp / "s1.json")})
            plan.switch_context({"focus": "f-w8", "context": "l16", "receipt": str(self.tmp / "s2.json")})
            again = plan.switch_context({"focus": "f-w8", "context": "l16"})
            plan.switch_context({"focus": "f-w8", "context": None})
        self.assertEqual(plan.calls, [("f-w8", True, "focus"), ("c16", True, "context"),
                                      ("c16", False, "context"), ("l16", True, "context"),
                                      ("l16", False, "context")])
        self.assertTrue(again["unchanged"])
        receipt = common.read_json(self.tmp / "s2.json")
        self.assertEqual((receipt["from"]["context"], receipt["to"]["context"], receipt["ok"]), ("c16", "l16", True))
        self.assertEqual(receipt["lifetime"], "L40-gate")

    def test_resume_seeds_live_stages_and_gate_steps_build_commands(self):
        args = argparse.Namespace(r_root=self.tmp, log=self.tmp / "p.jsonl", plan=None, from_step=3, hold=False,
                                  live_cuda=json.dumps({"focus": "f", "context": "c16"}))
        plan = run_plan.Plan(args)
        self.assertEqual(plan.live_cuda, {"focus": "f", "context": "c16"})
        ran = []
        plan.run = lambda label, command, timeout=0: ran.append(command)
        plan.gate_step({"command": "performance", "arm_dir": ["C=a", "L=b"], "out": "p.json"})
        self.assertEqual(ran[0][-7:], ["performance", "--arm-dir", "C=a", "--arm-dir", "L=b", "--out", "p.json"])
        staged = []
        plan.switch_context = lambda body: staged.append(body)
        plan.stages = lambda *a: staged.append(("cpu", a[:2]))
        plan.stage_group("@any", "@any", self.tmp, "cuda")
        plan.stage_group("f", "c", self.tmp, "cuda")
        plan.stage_group("f", None, self.tmp, None)
        self.assertEqual(staged, [{"focus": "f", "context": "c"}, ("cpu", ("f", None))])


class SideBySide(TempDir):
    def plan(self, device=None, host=None):
        args = argparse.Namespace(r_root=self.tmp, log=self.tmp / "plan.jsonl", plan=None, from_step=1, hold=False,
                                  live_cuda=None)
        plan = run_plan.Plan(args)
        plan.device_memory = lambda: device
        plan.host_available_bytes = lambda: host
        plan.ran = []
        plan.run = lambda label, command, timeout=0: plan.ran.append(command)
        return plan

    def rows(self):
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, ["airk"], {})
        steps = make_loop_spec.gate_plan(self.tmp / "spec.json", spec, self.tmp)
        return steps[0]["lifetimes"]["start"]

    def test_memory_check_and_placeholders_refuse_a_launch(self):
        rows = self.rows()
        gib = 1 << 30
        plan = self.plan({"0": {"total_mib": 49140, "used_mib": 1000}}, 200 * gib)
        check = plan.memory_check(rows, 4, 16)
        self.assertEqual((check["need_device_gib"], check["need_host_gib"]), (30.0, 64.0))
        busy = self.plan({"0": {"total_mib": 49140, "used_mib": 30000}}, 200 * gib)
        with self.assertRaises(common.HarnessError):
            busy.memory_check(rows, 4, 16)
        with self.assertRaises(common.HarnessError):
            self.plan({"0": {"total_mib": 49140, "used_mib": 0}}, 40 * gib).memory_check(rows, 4, 16)
        with self.assertRaises(common.HarnessError):
            self.plan(None, 200 * gib).memory_check(rows, 4, 16)
        with self.assertRaises(common.HarnessError):  # L5C's start options are still placeholders
            plan.start_lifetimes({"start": rows})

    def test_stop_disables_cuda_context_before_stopping(self):
        rows = self.rows()
        plan = self.plan()
        for row in rows:
            handle = Path(row["fixtures"]) / row["lifetime"] / "handle.json"
            common.write_json(handle, {"state": "closed", "restartable": True})
        with contextlib.redirect_stdout(io.StringIO()):
            plan.stop_lifetimes({"stop": rows})
        verbs = [(c[c.index("--fixtures") + 1].rsplit("-", 1)[-1], c[c.index("--fixtures") + 2],
                  "--disable" in c) for c in plan.ran]
        self.assertEqual(verbs, [("C", "stage", True), ("C", "stop", False), ("L", "stage", True), ("L", "stop", False),
                                 ("F", "stop", False)])

    def test_parallel_steps_run_together(self):
        plan = self.plan()
        import threading
        barrier = threading.Barrier(3, timeout=10)
        seen = []
        plan.gate_step = lambda body: (barrier.wait(), seen.append(body["arm"]))
        plan.parallel([{"gate": {"command": "replays", "arm": a}} for a in ("C", "L", "F")])
        self.assertEqual(sorted(seen), ["C", "F", "L"])
        plan.gate_step = lambda body: (_ for _ in ()).throw(common.HarnessError("arm failed"))
        with self.assertRaises(common.HarnessError):
            plan.parallel([{"gate": {"command": "replays", "arm": "C"}}])

    def test_parallel_sessions_cap_each_arm_and_interleave(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        spec = make_loop_spec.gate_spec("lg", self.tmp / "R", make_loop_spec.PROFILES, ["airk"], {})
        spec.update(scenarios_dir=str(paths["scenario"].parent.parent), freeze=None, prompts_freeze=None, pilot=True)
        for entry in spec["results"]:
            entry["handle"] = str(paths["handle"])
            entry.pop("stage_guard", None)  # no live service here; the guard is tested on its own
        import threading
        lock, running, peak, order = threading.Lock(), {}, {}, []

        def session(**kwargs):
            name = kwargs["configuration"]
            with lock:
                order.append(name)
                running[name] = running.get(name, 0) + 1
                peak[name] = max(peak.get(name, 0), running[name])
            import time as t
            t.sleep(0.05)
            with lock:
                running[name] -= 1
            return {"status": "complete"}

        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=session,
                             digest_fn=lambda s, e: {"ok": True, "digest": e["arm"], "arm_problems": []},
                             settle_fn=lambda h, r: {"settled": True, "publication_token": "t", "repo": r})
        flow.sessions()
        # Interleaved: the first three sessions to start are one of each arm. Their start order among
        # themselves is the pool threads' and not fixed (this assertion raced on 23 November 2026).
        self.assertEqual(sorted(order[:3]), ["compact", "focus", "large"])
        self.assertTrue(all(v <= make_loop_spec.ARM_SESSIONS for v in peak.values()))
        self.assertEqual(flow.settle_key(str(paths["handle"]), "airk"), f"{paths['handle'].parent.parent.resolve()}::airk")
        self.assertEqual(flow.settle_key(str(self.tmp / "R" / "fixture" / "L1" / "handle.json"), "airk"), "airk")


class PassGate(TempDir):
    def test_replay_only_entries_wrong_arms_and_gate_second_jobs(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        spec = make_loop_spec.gate_spec("lg", self.tmp / "R", make_loop_spec.PROFILES, ["airk"], {})
        spec.update(scenarios_dir=str(paths["scenario"].parent.parent), freeze=None, handle=str(paths["handle"]),
                    prompts_freeze=None, pilot=False, start_retry_seconds=0)
        for entry in spec["results"]:
            entry["handle"] = str(paths["handle"])
            entry.pop("stage_guard", None)  # no live service here; the guard is tested on its own
        spec["pilot"] = True  # a judged spec pins prompts_freeze; the pilot flag lets the test build it
        ran = []

        def session(**kwargs):
            ran.append(kwargs["configuration"])
            return {"status": "complete"}

        flow = run_pass.Pass(copy.deepcopy(spec), self.tmp / "R", session_runner=session,
                             digest_fn=lambda s, e: {"ok": True, "digest": e["arm"], "arm_problems": []},
                             settle_fn=lambda h, r: {"settled": True, "publication_token": "t", "repo": r})
        flow.sessions()
        self.assertEqual(sorted(ran), ["compact", "focus", "large"])
        spec["pilot"] = False
        strict = run_pass.Pass.__new__(run_pass.Pass)
        strict.__dict__.update(flow.__dict__)
        strict.pilot = False
        strict.digests = {}
        strict.digest_fn = lambda s, e: {"ok": True, "digest": "d", "arm_problems": ["not context_large"]}
        strict.series = "lg2"
        strict.pass_dir = self.tmp / "R" / "passes" / "lg2"
        strict.pass_dir.mkdir(parents=True)
        strict.digests_path = strict.pass_dir / "digests.json"
        large = next(e for e in strict.spec["results"] if e["arm"] == "L")
        with self.assertRaises(common.HarnessError):
            strict.run_one(strict.scenarios[0], large)
        jobs = self.tmp / "jobs.json"
        common.write_json(jobs, {"second_sessions": [["airk-C1", "large-gpu"]]})
        ran.clear()
        self.assertEqual(flow.second_sessions(None, jobs), ["airk-C1/large-gpu.s2"])
        self.assertEqual(ran, ["large"])
        common.write_json(jobs, {"second_sessions": [["airk-C1", "huge-gpu"]]})
        with self.assertRaises(common.HarnessError):
            flow.second_sessions(None, jobs)


class FreezeSet(unittest.TestCase):
    def test_loop_files_are_runtime_files(self):
        for name in ("loop_reply.py", "gate.py"):
            self.assertIn(name, prompts_freeze.HARNESS_FILES)
            self.assertTrue((HERE / name).is_file())
        for name in prompts_freeze.HARNESS_FILES:
            self.assertTrue((HERE / name).is_file(), name)


if __name__ == "__main__":
    unittest.main()
