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


def top_score(body: dict, model: str = "context_compact", precision: str = "float16") -> dict:
    """The reply as a gate arm would produce it: no abstention, every model decision by top score."""
    body = copy.deepcopy(body)
    for decision in body["decisions"]:
        if decision["source"]["kind"] == "abstained":
            decision["source"] = {"kind": "model", "mode": "top_score"}
        if decision.get("model"):
            decision.update(model=model, precision=precision)
    body["degradations"] = [d for d in body["degradations"] if d["kind"] != "context_abstained"]
    return body


def focus_only(body: dict) -> dict:
    body = copy.deepcopy(body)
    for decision in body["decisions"]:
        for key in ("model", "device", "precision", "card_slot", "receipt", "top", "runner_up", "margin"):
            decision.pop(key, None)
        decision["batched_with"] = []
        decision["source"] = {"kind": "deterministic", "rule": "top_focus_span"}
    body["degradations"] = [{"kind": "context_absent"}]
    return body


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
            configuration=None, check=None, memory=None) -> dict:
    rows = []
    for sid, body in bodies.items():
        raw = tmp / arm / sid / "canonical.stdout.json"
        common.write_bytes(raw, envelope(body))
        call = {"ok": True, "elapsed_ms": elapsed, "final_attempt_ms": elapsed, "bytes": len(json.dumps(body)),
                "digest": evidence.evidence_digest(body), "result_path": str(raw), "result_sha256": "x"}
        row = gate.reply_row(scenarios[sid], call)
        row["arm_problems"] = loop_reply.check_arm(body, check) if check is not None else []
        rows.append(row)
    return {"schema": gate.GATE_SCHEMA, "kind": "arm-replays", "arm": arm, "configuration": configuration or arm,
            "rows": rows, "memory": {"peak_used_mib": memory or {}}, "arm_ok": all(not r["arm_problems"] for r in rows)}


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
        self.assertIn("I recommend dropping Large", text)
        self.assertFalse(figures["bars"]["C-L"]["met"])
        for section in text.split("#### bookbuilder-C1")[1:]:
            self.assertNotIn(secret, section.split("\n## ")[0].split("\n#### ")[0])
        self.assertIn("[quoted text omitted]", text)
        evidence_lines = [line for line in text.splitlines() if " evidence: " in line]
        self.assertTrue(evidence_lines)
        self.assertFalse(any(secret in line for line in evidence_lines))  # evidence is located, never quoted
        self.assertIn("src/scheduler.rs:1-16", text)
        self.assertIn("## Receipts", text)
        self.assertIn("No stage is sensitive to precision", text)
        self.assertIn("proposed bar; the decision rule is the user's", text)

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
        self.assertIn("continuing Large", gate_report.large_recommendation(rubric))

    def test_redact(self):
        self.assertEqual(gate_report.redact('uses `fn x()` and "a b" here'),
                         "uses [quoted text omitted] and [quoted text omitted] here")
        self.assertTrue(gate_report.redact("x" * 500).endswith("…"))
        self.assertEqual(gate_report.reason_for("airk", "keep `code`"), "keep `code`")


# ---------------------------------------------------------------- specs and plans


class Specs(TempDir):
    def test_gate_spec_arms(self):
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, list(common.REPOS), {})
        arms = {e["arm"]: e for e in spec["results"]}
        self.assertEqual(sorted(arms), sorted([*make_loop_spec.JUDGED_ARMS, *make_loop_spec.PRECISION_ARMS]))
        self.assertEqual(len(spec["scenario_ids"]), 20)
        self.assertEqual(arms["F"]["arm_check"], {"context": None})
        self.assertEqual(arms["L"]["arm_check"]["context"], "context_large")
        self.assertEqual(arms["L32"]["arm_check"]["precision"], "float32")
        self.assertTrue(all(e["replay_only"] for k, e in arms.items() if k in make_loop_spec.PRECISION_ARMS))
        self.assertTrue(all(e["reuse"] is False and e["device"] == "gpu" for e in arms.values()))
        self.assertEqual(spec["answer_defaults"]["budget"], common.ANSWER_BUDGET_DEFAULT)
        self.assertEqual(len({run_pass.Pass.result_name(e) for e in spec["results"]}), len(spec["results"]))
        for _stage, fp32, fp16 in make_loop_spec.PRECISION_PAIRS:
            self.assertIn(fp32, arms)
            self.assertIn(fp16, arms)

    def test_gate_plan_switches_before_each_arm(self):
        spec = make_loop_spec.gate_spec("lg", self.tmp, make_loop_spec.PROFILES, ["airk"], {})
        plan = make_loop_spec.gate_plan(self.tmp / "spec.json", spec, self.tmp)
        kinds = [next(iter(step)) for step in plan]
        for index, step in enumerate(plan):
            if "gate" in step and step["gate"]["command"] == "replays":
                self.assertEqual(kinds[index - 1], "switch_context")
        sessions = [s["sessions"] for s in plan if "sessions" in s]
        self.assertEqual([g[0][2] for g in (s["groups"] for s in sessions)], ["compact-gpu", "large-gpu", "focus-gpu"])
        self.assertTrue(all(s["stager"] == "cuda" for s in sessions))
        self.assertEqual(plan[0]["switch_context"]["context"], make_loop_spec.PROFILES["compact"])
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


class PassGate(TempDir):
    def test_replay_only_entries_wrong_arms_and_gate_second_jobs(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        spec = make_loop_spec.gate_spec("lg", self.tmp / "R", make_loop_spec.PROFILES, ["airk"], {})
        spec.update(scenarios_dir=str(paths["scenario"].parent.parent), freeze=None, handle=str(paths["handle"]),
                    prompts_freeze=None, pilot=False, start_retry_seconds=0)
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
