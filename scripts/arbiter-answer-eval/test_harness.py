# SPDX-License-Identifier: Apache-2.0
"""Unit tests for the Answer evaluation harness. Synthetic data only.

Run: python -I -B -m unittest discover -s scripts/arbiter-answer-eval
"""

from __future__ import annotations

import contextlib
import csv
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import evidence  # noqa: E402
import grade  # noqa: E402
import judge  # noqa: E402
import depth  # noqa: E402
import mcp_wrapper  # noqa: E402
import prompts_freeze  # noqa: E402
import rescore  # noqa: E402
import settled  # noqa: E402
import run_pass  # noqa: E402
import run_plan  # noqa: E402
import run_session  # noqa: E402
import search_replay  # noqa: E402
import synthetic  # noqa: E402
import verify_restart  # noqa: E402


def hit(path, start, end, text="x"):
    return {"citation": {"path": path, "start_line": start, "end_line": end}, "text": text, "truncated": False}


def envelope(body) -> bytes:
    return json.dumps({"content": [{"type": "text", "text": json.dumps(body)}]}).encode()


class TempDir(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()


# ---------------------------------------------------------------- wrapper


class FakeBackend:
    def __init__(self, body=None, ok=True):
        self.calls = []
        self.body = body if body is not None else {"hits": [hit("src/scheduler.rs", 1, 5)]}
        self.ok = ok

    def __call__(self, tool, args):
        self.calls.append((tool, args))
        raw = envelope(self.body)
        return common.CallOutcome(self.ok, 0 if self.ok else 1, raw, b"", 7, json.loads(raw),
                                  None if self.ok else "CLI exit 1")


class WrapperRules(TempDir):
    def make(self, path="answer", backend=None, answer_defaults=None):
        catalog = common.load_catalog(self.write_catalog())
        return mcp_wrapper.Wrapper(handle=None, repo="airk", path=path, instruction=synthetic.INSTRUCTION,
                                   catalog=catalog, log_dir=self.tmp / "log", answer_defaults=answer_defaults,
                                   backend=backend or FakeBackend())

    def write_catalog(self):
        path = self.tmp / "catalog.json"
        common.write_json(path, synthetic.catalog())
        return path

    def log(self):
        return common.read_jsonl(self.tmp / "log" / "turns.jsonl")

    def test_tool_list_per_path_and_hidden_arguments(self):
        for path, first in (("answer", "airk_answer"), ("focus", "airk_answer"), ("noModel", "airk_answer"),
                            ("search", "airk_search"), ("parse", "airk_parse")):
            tools = self.make(path).list_tools()
            self.assertEqual(tools[0]["name"], first)
            self.assertEqual([t["name"] for t in tools], common.PATH_TOOLS[path])
        answer = {t["name"]: t for t in self.make("answer").list_tools()}["airk_answer"]["inputSchema"]
        self.assertNotIn("index", answer["properties"])
        self.assertNotIn("budget", answer["properties"])
        self.assertNotIn("schema", answer["properties"])
        self.assertEqual(answer["required"], ["question"])
        self.assertEqual(len(self.make("parse").list_tools()), 1)

    def test_first_call_must_be_primary_with_instruction_verbatim(self):
        backend = FakeBackend()
        w = self.make(backend=backend)
        self.assertTrue(w.call("airk_read_file", {"path": "a", "start_line": 1, "end_line": 2})["isError"])
        self.assertTrue(w.call("airk_answer", {"question": "something else"})["isError"])
        self.assertTrue(w.call("airk_answer", {"question": synthetic.INSTRUCTION[:-1]})["isError"])
        self.assertEqual(backend.calls, [])
        reply = w.call("airk_answer", {"question": synthetic.INSTRUCTION})
        self.assertNotIn("isError", reply)
        rows = self.log()
        self.assertEqual([r["turn"] for r in rows], [None, None, None, 1])
        self.assertTrue(all(r["rejected"] for r in rows[:3]))
        self.assertFalse(rows[3]["rejected"])
        # After the first call, any tool of the path may follow.
        w.call("airk_read_file", {"path": "a", "start_line": 1, "end_line": 2})
        self.assertEqual(self.log()[-1]["turn"], 2)

    def test_tools_outside_the_path_are_refused(self):
        w = self.make("search")
        w.call("airk_search", {"query": synthetic.INSTRUCTION})
        for tool in ("airk_answer", "airk_parse", "airk_status", "Bash"):
            self.assertTrue(w.call(tool, {"query": "x"})["isError"])
        self.assertEqual(w.counted, 1)
        parse = self.make("parse")
        parse.call("airk_parse", {"text": synthetic.INSTRUCTION})
        self.assertTrue(parse.call("airk_read_file", {"path": "a", "start_line": 1, "end_line": 1})["isError"])

    def test_turn_cap(self):
        backend = FakeBackend()
        w = self.make(backend=backend)
        w.call("airk_answer", {"question": synthetic.INSTRUCTION})
        for _ in range(11):
            self.assertNotIn("isError", w.call("airk_outline", {"path": "src/a.rs"}))
        refused = w.call("airk_outline", {"path": "src/a.rs"})
        self.assertTrue(refused["isError"])
        self.assertIn("12-call limit", refused["content"][0]["text"])
        self.assertEqual(len(backend.calls), 12)
        rows = self.log()
        self.assertEqual([r["turn"] for r in rows if not r["rejected"]], list(range(1, 13)))
        self.assertTrue(rows[-1]["rejected"])

    def test_forced_answer_arguments_and_parse_defaults(self):
        backend = FakeBackend()
        w = self.make(backend=backend, answer_defaults={"budget": {"calls": 4}, "response_mode": "full"})
        w.call("airk_answer", {"question": synthetic.INSTRUCTION, "index": "evil", "budget": {"calls": 12},
                               "schema": "x"})
        tool, sent = backend.calls[0]
        self.assertNotIn("index", sent)
        self.assertEqual(sent["schema"], common.ANSWER_SCHEMA)
        self.assertEqual(sent["budget"], dict(common.ANSWER_BUDGET_DEFAULT, calls=4))
        self.assertEqual(sent["response_mode"], "full")
        parse_backend = FakeBackend()
        p = self.make("parse", backend=parse_backend)
        p.call("airk_parse", {"text": synthetic.INSTRUCTION})
        self.assertEqual(parse_backend.calls[0][1], {"text": synthetic.INSTRUCTION, "mode": "execute",
                                                     "schema": common.PARSE_SCHEMA})
        p.call("airk_parse", {"text": "/search x", "mode": "prepare"})
        self.assertEqual(parse_backend.calls[1][1]["mode"], "prepare")

    def test_log_format_and_raw_results(self):
        w = self.make()
        w.call("airk_answer", {"question": synthetic.INSTRUCTION})
        row = self.log()[0]
        for key in ("turn", "tool", "arguments", "started", "elapsed_ms", "bytes", "ok", "result_path", "result_sha256"):
            self.assertIn(key, row)
        raw = Path(row["result_path"]).read_bytes()
        self.assertEqual(common.sha256_bytes(raw), row["result_sha256"])
        self.assertEqual(row["bytes"], len(json.loads(raw)["content"][0]["text"].encode()))

    def test_typed_retries_are_honoured_within_one_turn(self):
        """Baseline-bookbuilder finding: typed retries cost the agent turns; the wrapper repeats them."""
        def env(body, error=True):
            return json.dumps({"content": [{"type": "text", "text": json.dumps(body)}], "isError": error}).encode()
        busy = env({"action": "repeat_same_request", "code": "selection_busy", "retry_after_ms": 1,
                    "retryable": True, "status": "retry"})
        unsure = env({"error": {"code": "scoped_observation_unavailable", "retryable": True,
                                "query_submitted": None, "fresh_brokerage_required": True}})
        submitted = env({"error": {"code": "scoped_observation_unavailable", "retryable": True,
                                   "query_submitted": True}})
        good = env({"hits": [hit("src/a.rs", 1, 5)]}, error=False)
        replies = [busy, unsure, good]

        def backend(tool, args):
            raw = replies.pop(0)
            ok = raw is good
            return common.CallOutcome(ok, 0 if ok else 1, raw, b"", 10, json.loads(raw), None if ok else "x")
        w = self.make(backend=backend)
        w.MAX_RETRY_WAIT_MS = 5
        reply = w.call("airk_answer", {"question": synthetic.INSTRUCTION})
        self.assertNotIn("isError", reply)
        row = self.log()[0]
        self.assertEqual((row["turn"], row["ok"]), (1, True))
        self.assertEqual([r["code"] for r in row["typed_retries"]], ["selection_busy", "scoped_observation_unavailable"])
        self.assertTrue(all(Path(r["result_path"]).is_file() for r in row["typed_retries"]))
        self.assertGreaterEqual(row["elapsed_ms"], 30)
        self.assertEqual(w.counted, 1)
        # Uncertain work is never replayed, and an ordinary error is not a typed retry.
        self.assertFalse(common.typed_retry(json.loads(submitted))[0])
        self.assertFalse(common.typed_retry(json.loads(env({"error": {"code": "read_failed", "retryable": False}})))[0])
        self.assertFalse(common.typed_retry(json.loads(good))[0])
        always = self.make(backend=lambda t, a: common.CallOutcome(False, 1, busy, b"", 1, json.loads(busy), "x"))
        always.MAX_RETRY_WAIT_MS = 1
        self.assertTrue(always.call("airk_answer", {"question": synthetic.INSTRUCTION})["isError"])
        last = common.read_jsonl(self.tmp / "log" / "turns.jsonl")[-1]
        self.assertEqual(len(last["typed_retries"]), always.MAX_TYPED_RETRIES)

    def test_failed_backend_counts_as_a_turn_and_reports_error(self):
        w = self.make(backend=FakeBackend(ok=False))
        self.assertTrue(w.call("airk_answer", {"question": synthetic.INSTRUCTION})["isError"])
        self.assertEqual(self.log()[0]["turn"], 1)
        self.assertFalse(self.log()[0]["ok"])


class WrapperProtocol(TempDir):
    """The real wrapper process over stdio, against the mock CLI."""

    def start(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        instruction = self.tmp / "instruction.txt"
        common.write_text(instruction, synthetic.INSTRUCTION)
        return subprocess.Popen([sys.executable, "-I", "-B", str(HERE / "mcp_wrapper.py"), "--handle",
                                 str(paths["handle"]), "--repo", "airk", "--path", "answer", "--instruction-file",
                                 str(instruction), "--log-dir", str(self.tmp / "log")],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def exchange(self, proc, messages, framing="line"):
        data = b""
        for m in messages:
            payload = json.dumps(m).encode()
            data += (b"Content-Length: %d\r\n\r\n" % len(payload) + payload) if framing == "header" else payload + b"\n"
        out, err = proc.communicate(data, timeout=60)
        self.assertEqual(proc.returncode, 0, err.decode())
        if framing == "line":
            return [json.loads(line) for line in out.splitlines() if line.strip()]
        replies, stream = [], io.BytesIO(out)
        while True:
            read = mcp_wrapper.read_message(stream)
            if read is None:
                return replies
            replies.append(read[0])

    def session(self, framing):
        proc = self.start()
        replies = self.exchange(proc, [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t"}}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
            {"jsonrpc": "2.0", "id": 3, "method": "ping"},
            {"jsonrpc": "2.0", "id": 4, "method": "tools/call",
             "params": {"name": "airk_answer", "arguments": {"question": synthetic.INSTRUCTION}}},
            {"jsonrpc": "2.0", "id": 5, "method": "tools/call",
             "params": {"name": "airk_read_file", "arguments": {"path": "src/format.rs", "start_line": 1, "end_line": 2}}},
            {"jsonrpc": "2.0", "id": 6, "method": "resources/list"},
        ], framing)
        self.assertEqual([r["id"] for r in replies], [1, 2, 3, 4, 5, 6])
        self.assertEqual(replies[0]["result"]["protocolVersion"], "2025-06-18")
        self.assertEqual([t["name"] for t in replies[1]["result"]["tools"]], common.PATH_TOOLS["answer"])
        self.assertEqual(replies[2]["result"], {})
        body = json.loads(replies[3]["result"]["content"][0]["text"])
        self.assertEqual(body["schema"], "airk.arbiter.context/2")
        self.assertIn("FORMAT_VERSION", replies[4]["result"]["content"][0]["text"])
        self.assertEqual(replies[5]["error"]["code"], -32601)
        rows = common.read_jsonl(self.tmp / "log" / "turns.jsonl")
        self.assertEqual([r["turn"] for r in rows], [1, 2])
        self.assertTrue(all(r["ok"] for r in rows))
        self.assertTrue((self.tmp / "log" / "wrapper.json").is_file())

    def test_newline_framing(self):
        self.session("line")

    def test_content_length_framing(self):
        self.session("header")


# ---------------------------------------------------------------- evidence and coverage


NOTE = {"id": "N1", "mark": "essential", "path": "src/a.rs", "lines": [10, 20], "symbol": "alpha", "fact": "f",
        "alternatives": [{"path": "docs/a.md", "lines": [1, 3], "symbol": None}]}


class Coverage(unittest.TestCase):
    def test_rule_two_needs_min_five_lines_of_overlap(self):
        """Decision 3: a span covers when it overlaps min(5, note line count) lines (NOTE spans 11 lines)."""
        cov = lambda *span: evidence.covers(evidence.extract({"hits": [hit("src/a.rs", *span)]}), NOTE)  # noqa: E731
        self.assertTrue(cov(16, 30))   # 16-20: five lines
        self.assertFalse(cov(17, 30))  # 17-20: four lines
        self.assertTrue(cov(1, 14))    # 10-14: five lines
        self.assertFalse(cov(1, 13))
        self.assertFalse(cov(15, 15))  # a single references line no longer covers
        self.assertFalse(cov(21, 30))
        self.assertFalse(evidence.covers(evidence.extract({"hits": [hit("src/b.rs", 10, 20)]}), NOTE))
        short = dict(NOTE, lines=[7, 8], alternatives=[])  # a 2-line note needs both lines
        self.assertTrue(evidence.covers(evidence.extract({"hits": [hit("src/a.rs", 1, 8)]}), short))
        self.assertFalse(evidence.covers(evidence.extract({"hits": [hit("src/a.rs", 8, 9)]}), short))
        one = dict(NOTE, lines=[7, 7], alternatives=[])
        self.assertTrue(evidence.covers(evidence.extract({"hits": [hit("src/a.rs", 7, 7)]}), one))
        self.assertEqual(evidence.required_overlap([10, 20]), 5)
        self.assertEqual(evidence.required_overlap([7, 8]), 2)

    def test_rule_one_kept_for_comparison(self):
        one_line = evidence.extract({"hits": [hit("src/a.rs", 20, 30)]})
        self.assertTrue(evidence.covers(one_line, NOTE, rule=1))
        self.assertFalse(evidence.covers(one_line, NOTE, rule=2))
        self.assertFalse(evidence.covers(evidence.extract({"hits": [hit("src/a.rs", 21, 30)]}), NOTE, rule=1))

    def test_alternative_and_path_normalization(self):
        # The alternative (docs/a.md:1-3) needs min(5, 3) = 3 lines on its own path.
        self.assertTrue(evidence.covers(evidence.extract({"hits": [hit("docs/a.md", 1, 4)]}), NOTE))
        self.assertFalse(evidence.covers(evidence.extract({"hits": [hit("docs/a.md", 2, 4)]}), NOTE))
        self.assertFalse(evidence.covers(evidence.extract({"hits": [hit("src/a.rs", 1, 3)]}), NOTE))
        self.assertTrue(evidence.covers(evidence.extract({"hits": [hit("D:\\repo\\src\\a.rs", 12, 16)]}), NOTE))
        self.assertTrue(evidence.covers(evidence.extract({"hits": [hit("./src/a.rs", 12, 16)]}), NOTE))
        self.assertFalse(evidence.covers(evidence.extract({"hits": [hit("xsrc/a.rs", 12, 16)]}), NOTE))

    def test_path_without_lines_covers_only_by_symbol(self):
        with_symbol = {"page": {"files": [{"path": "src/a.rs", "count": 1, "name": "alpha"}]}}
        without = {"page": {"files": [{"path": "src/a.rs", "count": 1}]}}
        self.assertTrue(evidence.covers(evidence.extract(with_symbol), NOTE))
        self.assertFalse(evidence.covers(evidence.extract(without), NOTE))
        self.assertFalse(evidence.covers(evidence.extract(with_symbol), dict(NOTE, symbol=None, alternatives=[])))

    def test_fact_rows_count_only_when_enabled(self):
        compact = {"representation": "minimal", "files": [{"path": "src/z.rs"}, {"path": "src/a.rs"}],
                   "items": [{"name": "alpha", "lines": [10, 16], "file": 1}]}
        spans = evidence.extract(compact)
        self.assertIn(evidence.Span("src/a.rs", 10, 16, "alpha", "fact"), spans)
        self.assertFalse(evidence.covers(spans, NOTE))
        self.assertTrue(evidence.covers(spans, NOTE, count_fact_rows=True))
        full = {"items": [{"name": "alpha", "kind": "fn", "citation": {"path": "src/a.rs", "start_line": 10,
                                                                        "end_line": 12}}]}
        self.assertEqual(evidence.extract(full)[0].kind, "fact")

    def test_answer_records_count_canonical_results_not_arguments(self):
        body = {"records": [{"operation": "read_evidence",
                             "arguments": {"path": "src/a.rs", "start_line": 10, "end_line": 20},
                             "canonical_result": {"hits": []}},
                            {"operation": "search", "arguments": {"query": "q"},
                             "canonical_result": {"hits": [hit("docs/a.md", 1, 3)]}}],
                "paragraphs": [{"text": "p", "evidence": ["r1"]}]}
        spans = evidence.extract(body)
        self.assertEqual([(s.path, s.start) for s in spans if s.kind == "text"], [("docs/a.md", 1)])
        self.assertTrue(evidence.covers(spans, NOTE))
        self.assertFalse(evidence.covers(evidence.extract(body["records"][0]), NOTE))

    def test_references_parse_and_combined_shapes(self):
        refs = {"resolution": "lexical", "evidence": {"hits": [hit("src/a.rs", n, n, "alpha()") for n in (15, 16)]}}
        self.assertEqual(len(evidence.extract(refs)), 2)
        self.assertFalse(evidence.covers(evidence.extract(refs), NOTE))  # single lines never reach five
        self.assertTrue(evidence.covers(evidence.extract(refs), NOTE, rule=1))
        parse = {"steps": [{"arguments": {"path": "src/a.rs", "start_line": 10, "end_line": 20}, "result": None}],
                 "evidence": [{"citation": {"path": "src/a.rs", "start_line": 11, "end_line": 16},
                               "text": "alpha", "fact": {"name": "alpha", "kind": "fn"}, "step": "s1"}]}
        self.assertEqual(evidence.extract(parse)[0].kind, "fact")
        self.assertFalse(evidence.covers(evidence.extract(parse), NOTE))
        parse["evidence"][0]["fact"] = hit("src/a.rs", 11, 16)
        self.assertTrue(evidence.covers(evidence.extract(parse), NOTE))
        combined = {"schema": "airk.combined-read/1", "members": [{"index_id": "i", "result": {"hits": [
            hit("src/a.rs", 10, 14)]}}]}
        self.assertTrue(evidence.covers(evidence.extract(combined), NOTE))

    def test_pointer_without_text_does_not_cover(self):
        body = {"hits": [], "definitions": [{"name": "alpha", "matches": [
            {"citation": {"path": "src/a.rs", "start_line": 10, "end_line": 20}, "symbol_id": "s"}]}]}
        self.assertFalse(evidence.covers(evidence.extract(body), NOTE))

    def test_digest_ignores_receipts_and_respects_order(self):
        a = {"publication": {"token": "1"}, "execution": {"ms": 5}, "hits": [hit("a", 1, 2, "x"), hit("b", 3, 4, "y")]}
        b = {"publication": {"token": "2"}, "execution": {"ms": 9}, "hits": [hit("a", 1, 2, "x"), hit("b", 3, 4, "y")]}
        c = {"hits": [hit("b", 3, 4, "y"), hit("a", 1, 2, "x")]}
        self.assertEqual(evidence.evidence_digest(a), evidence.evidence_digest(b))
        self.assertNotEqual(evidence.evidence_digest(a), evidence.evidence_digest(c))

    def test_cited_locations(self):
        text = "Edit `src/a.rs:10-20` and docs/a.md:3, see crates/x-y/src/lib.rs:5\u20139; not http://h:80."
        found = evidence.cited_locations(text)
        self.assertIn({"path": "src/a.rs", "lines": [10, 20]}, found)
        self.assertIn({"path": "docs/a.md", "lines": [3, 3]}, found)
        self.assertIn({"path": "crates/x-y/src/lib.rs", "lines": [5, 9]}, found)

    def test_session_coverage_summary(self):
        scenario = {"id": "s", "notes": [
            dict(NOTE, alternatives=[]),
            {"id": "N2", "mark": "essential", "path": "src/b.rs", "lines": [1, 2], "symbol": None, "fact": "f"},
            {"id": "N3", "mark": "helpful", "path": "src/c.rs", "lines": [1, 2], "symbol": None}]}
        bodies = {1: {"hits": [hit("src/a.rs", 10, 14)]}, 2: {"hits": [hit("src/c.rs", 1, 2)]},
                  3: {"hits": [hit("src/b.rs", 1, 3)]}}
        turns = [{"turn": n, "tool": "t", "ok": True, "bytes": 10, "elapsed_ms": 5} for n in (1, 2, 3)]
        turns.append({"turn": None, "rejected": True, "tool": "x"})
        result = coverage.compute(scenario, turns, body_loader=lambda row: bodies[row["turn"]])
        self.assertEqual(result["coverage_rule"], 2)
        self.assertEqual(result["first_call_yield"], 0.5)
        self.assertEqual(result["all_essential_turn"], 3)
        self.assertEqual(result["turns"][1]["new"], ["N3"])
        self.assertEqual((result["total_bytes"], result["total_elapsed_ms"]), (30, 15))
        failed = [dict(t, ok=False) if t["turn"] == 3 else t for t in turns]
        result = coverage.compute(scenario, failed, body_loader=lambda row: bodies[row["turn"]])
        self.assertIsNone(result["all_essential_turn"])
        self.assertEqual(result["essential_uncovered"], ["N2"])


# ---------------------------------------------------------------- grading


def cov(turn, uncovered=(), essential=("N1", "N2", "N3")):
    return {"essential": list(essential), "essential_uncovered": list(uncovered), "all_essential_turn": turn}


def verdict(wrong=(), missing=(), misleading=False, essential=("N1", "N2", "N3")):
    return {"notes": [{"id": n, "verdict": "wrong" if n in wrong else "missing" if n in missing else "correct",
                       "reason": ""} for n in essential], "misleading_followed": misleading, "misleading_reason": "",
            "rubric_b": {k: {"score": 2, "reason": ""} for k in judge.RUBRIC_B_KEYS}}


class RubricA(unittest.TestCase):
    def test_grade_table(self):
        for turn, expected in ((1, 4), (2, 3), (3, 2), (4, 2), (5, 1), (12, 1)):
            g = grade.grade_one(cov(turn), verdict(), "complete")
            self.assertEqual((g["grade"], g["correct_finish"]), (expected, True), turn)

    def test_one_fact_short_and_worse(self):
        self.assertEqual(grade.grade_one(cov(1), verdict(wrong=["N2"]), "complete")["grade"], 1)
        self.assertEqual(grade.grade_one(cov(None, ["N3"]), verdict(), "complete")["grade"], 1)
        self.assertEqual(grade.grade_one(cov(None, ["N3"]), verdict(missing=["N3"]), "complete")["grade"], 1)
        self.assertEqual(grade.grade_one(cov(None, ["N3"]), verdict(wrong=["N1"]), "complete")["grade"], 0)
        self.assertEqual(grade.grade_one(cov(1), verdict(missing=["N1", "N2"]), "complete")["grade"], 0)
        self.assertEqual(grade.grade_one(cov(1), verdict(misleading=True), "complete")["grade"], 0)
        for status in ("failed", "timeout", "discarded"):
            self.assertEqual(grade.grade_one(cov(1), verdict(), status)["grade"], 0)
        self.assertFalse(grade.grade_one(cov(1), verdict(wrong=["N2"]), "complete")["correct_finish"])

    def test_adjudication(self):
        g = lambda n, f=True: {"grade": n, "correct_finish": f, "reason": ""}  # noqa: E731
        self.assertEqual(grade.adjudicate(g(3), g(3), None)["source"], "agree")
        lower = grade.adjudicate(g(3), g(1, False), None)
        self.assertEqual((lower["grade"], lower["source"]), (None, "pending"))
        adjacent = grade.adjudicate(g(2), g(1, False), None)
        self.assertEqual((adjacent["grade"], adjacent["correct_finish"]), (1, False))
        override = grade.adjudicate(g(4), g(0, False), {"grade": 3, "correct_finish": True, "reason": "r"})
        self.assertEqual((override["grade"], override["source"]), (3, "adjudicated"))


def row(turns=None, grade_=None, repo="airk", kind="coding", **extra):
    finished = turns is not None
    return {"correct_finish": finished, "turns_to_finish": turns,
            "adjudicated_grade": grade_ if grade_ is not None else (
                (4 if turns == 1 else 3 if turns == 2 else 2 if turns in (3, 4) else 1) if finished else 0),
            "repo": repo, "kind": kind, **extra}


class CaseTable(unittest.TestCase):
    def test_cases(self):
        C = grade.case
        self.assertEqual(C(row(2), row(3))["case"], "better (one turn sooner)")
        self.assertFalse(C(row(2), row(3))["substantial"])
        self.assertEqual(C(row(2), row(2))["case"], "tie")
        self.assertTrue(C(row(3), row(2))["worse"])
        self.assertTrue(C(row(1), row(3))["substantial"])
        self.assertTrue(C(row(2), row(4))["substantial"])
        self.assertTrue(C(row(5), None)["substantial"])
        self.assertTrue(C(row(5), row(None))["substantial"])
        self.assertTrue(C(row(None, 1), row(4))["worse"])
        self.assertTrue(C(row(None, 0), row(12))["worse"])
        neither = C(row(None, 1), row(None, 0))
        self.assertFalse(neither["substantial"] or neither["worse"])
        self.assertEqual(C(row(3, grade_=None) | {"adjudicated_grade": None}, row(2))["case"], "pending")
        self.assertTrue(C(row(2), row(4))["close_call"])
        self.assertFalse(C(row(1), row(4))["close_call"])

    def test_better_of_parse_and_search(self):
        p, s = row(3, configuration="parse"), row(2, configuration="search")
        self.assertIs(grade.better_of([p, s]), s)
        self.assertIs(grade.better_of([None, p]), p)
        # Tie rule (22 Nov 2026): finish at fewer turns; else more essential covered; then fewer turns; then Search.
        p2, s2 = row(2, configuration="parse"), row(2, configuration="search")
        self.assertIs(grade.better_of([p2, s2]), s2)
        p0 = row(None, 1, configuration="parse", essential_covered=2, session_turns=5)
        s0 = row(None, 0, configuration="search", essential_covered=3, session_turns=9)
        self.assertIs(grade.better_of([p0, s0]), s0)  # coverage beats grade when neither finishes
        s1 = dict(s0, essential_covered=2)
        self.assertIs(grade.better_of([p0, s1]), p0)  # equal coverage: fewer turns
        s3 = dict(s1, session_turns=5)
        self.assertIs(grade.better_of([p0, s3]), s3)  # all equal: Search

    def test_samples(self):
        win = grade.case(row(1), row(3))
        better = grade.case(row(2), row(3))
        self.assertEqual(grade.combine_samples([win, win])["case"], "substantial-win")
        self.assertEqual(grade.combine_samples([win, better])["case"], "undecided (samples differ)")
        self.assertFalse(grade.combine_samples([win, better])["substantial"])

    def test_configuration_rule(self):
        def per(n_wins, n_worse, repo_split=(6, 6), kinds_ok=True):
            rows = []
            for i in range(20):
                repo = "airk" if i < 10 else "bookbuilder"
                wins_here = repo_split[0] if repo == "airk" else repo_split[1]
                idx = i if repo == "airk" else i - 10
                if idx < wins_here:
                    c = {"case": "substantial-win", "substantial": True, "worse": False}
                elif idx < wins_here + (n_worse // 2 + (n_worse % 2 if repo == "airk" else 0)):
                    c = {"case": "worse", "substantial": False, "worse": True}
                else:
                    c = {"case": "tie", "substantial": False, "worse": False}
                rows.append({"repo": repo, "kind": "coding" if i % 2 else "writing", "case": c,
                             "answer_grade": 3 if kinds_ok else 0, "best_grade": 2})
            return rows
        self.assertTrue(grade.configuration_rule(per(12, 2))["substantially_improves"])
        self.assertFalse(grade.configuration_rule(per(12, 2, (8, 4)))["substantially_improves"])
        self.assertFalse(grade.configuration_rule(per(12, 3))["substantially_improves"])
        self.assertFalse(grade.configuration_rule(per(10, 0, (5, 5)))["substantially_improves"])
        self.assertFalse(grade.configuration_rule(per(12, 0, kinds_ok=False))["substantially_improves"])

    def test_headroom(self):
        best = {"a": row(1), "b": row(2), "c": row(3), "d": row(None, 1), "e": None}
        room = grade.headroom(best)
        self.assertEqual((room["count"], room["scenarios"]), (3, ["c", "d", "e"]))
        self.assertFalse(room["rule_reachable"])

    def test_step_comparison(self):
        a = {"s1": row(1, elapsed_ms=100), "s2": row(2, elapsed_ms=100), "s3": row(None, elapsed_ms=100)}
        b = {"s1": row(3, elapsed_ms=50), "s2": row(None, elapsed_ms=50), "s3": row(2, elapsed_ms=50)}
        step = grade.step_comparison(a, b)
        self.assertEqual((step["finishes_added"], step["gained"], step["lost"]), (0, ["s2"], ["s3"]))
        self.assertEqual((step["turns_saved"], step["time_added_ms_mean"]), (2, 50))

    def test_csv_columns(self):
        expected = ("series,repo,slot,kind,configuration,device,session_id,reused_from,judge1_grade,judge2_grade,"
                    "adjudicated_grade,correct_finish,turns_to_finish,first_call_yield,essential_covered,"
                    "essential_total,bytes,elapsed_ms,rubric_b_j1,rubric_b_j2,cost_usd")
        text = grade.csv_text([{"series": "s", "repo": "airk", "turns_to_finish": None, "extra": 1}])
        self.assertEqual(text.splitlines()[0], expected)
        parsed = list(csv.DictReader(io.StringIO(text)))
        self.assertEqual(parsed[0]["turns_to_finish"], "")
        self.assertNotIn("\r", text)


# ---------------------------------------------------------------- session audit and judge


class SessionAudit(TempDir):
    def test_discard_and_citation_audit(self):
        result = self.tmp / "turn-01.stdout.json"
        common.write_bytes(result, envelope({"hits": [hit("src/a.rs", 10, 20)]}))
        turns = [{"turn": 1, "result_path": str(result)}]
        allowed = [f"mcp__airk__{t}" for t in common.PATH_TOOLS["answer"]]
        good = {"init": {"tools": allowed, "mcp_servers": [{"name": "airk", "status": "connected"}]},
                "tool_uses": [{"name": "mcp__airk__airk_answer"}], "result": {}}
        checks = run_session.audit(good, "answer", turns, "Work in src/a.rs:12-14 and src/b.rs:1-2.")
        self.assertFalse(checks["discard"])
        self.assertEqual(checks["citations"]["unbacked"], [{"path": "src/b.rs", "lines": [1, 2]}])
        self.assertEqual(checks["citations"]["unaudited_bare_refs"], 0)
        bare = run_session.audit(good, "answer", turns, "In src/a.rs (lines 14-26), the struct at 236.")
        self.assertEqual(bare["citations"]["unaudited_bare_refs"], 2)
        self.assertTrue(checks["tool_use_count"]["agree"])
        bad = dict(good, tool_uses=[{"name": "Bash"}, {"name": "mcp__airk__airk_answer"}])
        self.assertTrue(run_session.audit(bad, "answer", turns, "")["discard"])
        offered = dict(good, init={"tools": [*allowed, "Read"], "mcp_servers": good["init"]["mcp_servers"]})
        self.assertTrue(run_session.audit(offered, "answer", turns, "")["discard"])
        extra_server = dict(good, init={"tools": allowed, "mcp_servers": [{"name": "airk"}, {"name": "other"}]})
        self.assertTrue(run_session.audit(extra_server, "answer", turns, "")["discard"])
        refused = turns + [{"turn": None, "rejected": True}]
        two_uses = dict(good, tool_uses=[{"name": "mcp__airk__airk_answer"}, {"name": "mcp__airk__airk_outline"}])
        count = run_session.audit(two_uses, "answer", refused, "")["tool_use_count"]
        self.assertEqual((count["agree"], count["counted"], count["refused"]), (True, 1, 1))  # pilot bug: refused call
        wrong_path = dict(good, tool_uses=[{"name": "mcp__airk__airk_search"}])
        self.assertTrue(run_session.audit(wrong_path, "answer", turns, "")["discard"])

    def test_command_lines(self):
        command = run_session.claude_command("claude.exe", common.AGENT_MODEL, Path("m.json"), "search", None)
        self.assertEqual(command[command.index("--setting-sources") + 1], "")
        self.assertEqual(command[command.index("--model") + 1], "claude-haiku-5-5")  # user decision, 21 Nov
        self.assertEqual(command[command.index("--effort") + 1], "medium")
        sonnet = run_session.claude_command("claude.exe", "claude-sonnet-5-5", Path("m.json"), "search", None, "high")
        self.assertEqual((sonnet[sonnet.index("--model") + 1], sonnet[sonnet.index("--effort") + 1]),
                         ("claude-sonnet-5-5", "high"))
        with self.assertRaises(common.HarnessError):
            run_session.claude_command("claude.exe", "m", Path("m.json"), "search", None, "extreme")
        self.assertIn("--strict-mcp-config", command)
        tools = command[command.index("--tools") + 1].split(",")
        self.assertEqual(tools, [f"mcp__airk__{t}" for t in common.PATH_TOOLS["search"]])
        self.assertEqual(command[command.index("--permission-mode") + 1], "dontAsk")
        self.assertIn('"airk_search"', command[command.index("--system-prompt") + 1].replace("airk_search", '"airk_search"'))
        jc = judge.judge_command("claude.exe", common.JUDGE_MODEL)
        self.assertEqual(jc[jc.index("--tools") + 1], "")
        self.assertEqual(jc[jc.index("--model") + 1], "claude-sonnet-5-5")
        self.assertEqual(jc[jc.index("--effort") + 1], "medium")


class Judging(TempDir):
    def test_neutralize(self):
        text = 'primary airk_answer {"schema":"airk.arbiter.context/2"} then mcp__airk__airk_read_file'
        out = judge.neutralize(text, "airk_answer")
        self.assertNotIn("airk_answer", out)
        self.assertNotIn("airk.arbiter", out)
        self.assertIn("read_file", out)
        self.assertNotIn("airk_search", judge.neutralize("airk_search", "airk_answer"))

    def test_parse_and_validate(self):
        scenario = synthetic.scenario()
        good = verdict(essential=("N1", "N2", "N3", "N4"))
        self.assertEqual(judge.parse_verdict({"structured_output": good}), good)
        self.assertEqual(judge.parse_verdict({"result": "```json\n" + json.dumps(good) + "\n```"}), good)
        self.assertEqual(judge.validate(good, scenario), [])
        short = dict(good, notes=good["notes"][:2])
        self.assertTrue(judge.validate(short, scenario))
        bad_score = json.loads(json.dumps(good))
        bad_score["rubric_b"]["find"]["score"] = 5
        self.assertTrue(judge.validate(bad_score, scenario))
        self.assertEqual(judge.validate(None, scenario), ["no JSON verdict"])

    def test_packet_is_blind(self):
        directory = self.tmp / "s"
        (directory / "wrapper" / "results").mkdir(parents=True)
        raw = directory / "wrapper" / "results" / "turn-01.stdout.json"
        common.write_bytes(raw, envelope({"schema": "airk.arbiter.context/2", "records": []}))
        common.write_json(directory / "session.json", {"path": "focus", "status": "complete"})
        common.write_text(directory / "brief.md", "I called airk_answer then airk_read_file.")
        common.append_jsonl(directory / "wrapper" / "turns.jsonl", {"turn": 1, "tool": "airk_answer", "ok": True,
                                                                    "arguments": {"question": "q"}, "result_path": str(raw)})
        packet = judge.build_packet(synthetic.scenario(), directory, "airk-C1/focus-cpu")
        text = json.dumps(packet)
        for word in ("airk_answer", "focus", "airk.arbiter"):
            self.assertNotIn(word, text)
        self.assertEqual(packet["calls"][0]["tool"], "primary_tool")
        self.assertTrue(packet["first_reply"]["present"])


# ---------------------------------------------------------------- pilot regressions (real shapes, 21 Nov 2026)


class PilotRegressions(TempDir):
    def test_catalog_without_prefix_is_aliased(self):
        raw = synthetic.catalog()
        for tool in raw["tools"]:
            tool["name"] = tool["name"].removeprefix("airk_")
            tool["inputSchema"]["properties"].pop("index")
            tool["inputSchema"]["required"].remove("index")
            tool["inputSchema"]["properties"]["project"] = {"type": "string"}
        path = self.tmp / "catalog.json"
        common.write_json(path, raw)
        catalog = common.load_catalog(path)
        self.assertIn("airk_answer", catalog)
        self.assertEqual(catalog["airk_answer"]["catalog_name"], "answer")
        w = mcp_wrapper.Wrapper(handle=None, repo="airk", path="search", instruction="q", catalog=catalog,
                                log_dir=self.tmp / "log", backend=FakeBackend())
        tools = w.list_tools()
        self.assertEqual(tools[0]["name"], "airk_search")
        self.assertNotIn("project", tools[0]["inputSchema"]["properties"])

    def test_handle_readiness_gate(self):
        base = {"cli": "x.exe", "control": "c", "environment": {"AIRK_MCP_RESPONSE_MODE": "full"},
                "service_environment": {"AIRK_SECRET_SERVICE_ONLY": "1"},
                "indexes": {"airk": {"index_id": "idx_a"}}}
        for state, queryable, plain, pilot in (("ready", True, True, True), ("indexing", None, False, False),
                                               ("indexing", True, False, True), (None, None, True, True)):
            raw = dict(base, **({} if state is None else {"state": state}), **({} if queryable is None else
                                                                             {"queryable": queryable}))
            path = self.tmp / f"h-{state}-{queryable}.json"
            common.write_json(path, raw)
            for accept, expected in ((False, plain), (True, pilot)):
                if expected:
                    handle = common.load_handle(path, accept_queryable=accept)
                    self.assertEqual(handle.env, {"AIRK_MCP_RESPONSE_MODE": "full"})
                    self.assertEqual(handle.index_id("airk"), "idx_a")
                    self.assertIn("semantic", handle.readiness())
                else:
                    with self.assertRaises(common.HarnessError):
                        common.load_handle(path, accept_queryable=accept)

    def test_real_answer_envelope_shape(self):
        """Answer as the o2-fix2 image returns it: content + structuredContent, records[].canonical_result."""
        body = {"schema": "airk.arbiter.context/2", "records": [
            {"operation": "search", "role": "unknown",
             "arguments": {"query": "q", "limit": 8, "max_bytes": 4096, "full_chunks": True},
             "canonical_result": {"collection": "c", "publication": {"token": "t"}, "semantic_readiness": {},
                                  "scope_observation": {"members": [{"index_id": "i"}]},
                                  "hits": [hit("src/a.rs", 12, 14, "alpha")]}},
            {"operation": "context", "role": "unknown",
             "arguments": {"after": 32, "before": 0, "line": 10, "path": "src/a.rs"},
             "canonical_result": {"read": {"status": "content", "requested_path": "src/a.rs"},
                                  "hits": [hit("src/a.rs", 10, 20, "alpha body")]}}],
            "paragraphs": [{"text": "The initial indexed evidence includes (src/a.rs:10–20)", "evidence": ["x"]}],
            "source_verification": {"sources": [{"path": "src/b.rs"}]}, "scope_observation": {}}
        spans = evidence.extract(body)
        self.assertEqual([(s.path, s.start, s.end) for s in spans], [("src/a.rs", 12, 14), ("src/a.rs", 10, 20)])
        text = json.dumps(body)
        raw = json.dumps({"content": [{"type": "text", "text": text}], "isError": False,
                          "structuredContent": body}).encode()

        def backend(tool, args):
            return common.CallOutcome(True, 0, raw, b"", 5, json.loads(raw), None)
        w = mcp_wrapper.Wrapper(handle=None, repo="airk", path="answer", instruction="q",
                                catalog=common.load_catalog(WrapperRules.write_catalog(self)),
                                log_dir=self.tmp / "log", backend=backend)
        reply = w.call("airk_answer", {"question": "q"})
        self.assertEqual(len(reply["content"]), 1)
        self.assertNotIn("isError", reply)
        row = common.read_jsonl(self.tmp / "log" / "turns.jsonl")[0]
        self.assertEqual(row["bytes"], len(text.encode("utf-8")))  # structuredContent is not counted twice

    def test_parse_requiring_input_has_no_evidence(self):
        body = {"schema": "airk.workbench.result/1", "executed": False, "evidence": [], "steps": [],
                "plan": {"steps": [{"args": {"name": "x", "path_prefix": ""}, "id": "query", "tool": "symbols"}]},
                "requires_input": [{"field": "name", "step": "query"}]}
        self.assertEqual(evidence.extract(body), [])

    def test_error_envelope_is_a_failed_turn(self):
        raw = json.dumps({"content": [{"type": "text", "text": json.dumps(
            {"error": {"code": "read_failed", "message": "line range exceeds indexed source"}})}],
            "isError": True}).encode()

        def backend(tool, args):
            return common.CallOutcome(False, 1, raw, b"", 5, json.loads(raw), "CLI exit 1")
        w = mcp_wrapper.Wrapper(handle=None, repo="airk", path="answer", instruction="q",
                                catalog=common.load_catalog(WrapperRules.write_catalog(self)),
                                log_dir=self.tmp / "log", backend=backend)
        w.call("airk_answer", {"question": "q"})
        reply = w.call("airk_read_file", {"path": "a", "start_line": 1, "end_line": 9999})
        self.assertTrue(reply["isError"])
        self.assertIn("line range exceeds", reply["content"][0]["text"])

    def test_declared_stages_must_be_live(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        handle = common.load_handle(paths["handle"])
        self.assertEqual(handle.stages(), {"focus": None, "context": None})
        log = paths["handle"].parent / "stages.jsonl"
        common.append_jsonl(log, {"profile": "focus-static1024-cpu", "enabled": True})
        common.append_jsonl(log, {"profile": "context-compact-cpu", "enabled": True})
        common.append_jsonl(log, {"profile": "context-compact-cpu", "enabled": False})
        self.assertEqual(handle.stages(), {"focus": "focus-static1024-cpu", "context": None})
        spec = {"series": "st", "pilot": True, "scenarios_dir": str(self.tmp / "fx" / "scenarios"), "results": [
            {"configuration": "noModel", "path": "noModel", "handle": str(paths["handle"]), "stages": {}},
            {"configuration": "focus", "path": "focus", "device": "cpu", "handle": str(paths["handle"]),
             "stages": {"focus": "focus-static1024-cpu"}}]}
        flow = run_pass.Pass(spec, self.tmp / "R")
        with self.assertRaises(common.HarnessError):
            flow.check_stages(spec["results"][0])  # noModel while Focus is enabled is refused
        flow.check_stages(spec["results"][1])

    def test_freeze_and_scenario_subset(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        freeze = self.tmp / "fx" / "freeze.json"
        rel = "scenarios/airk/C1.json"
        common.write_json(freeze, {"files": {rel: common.sha256_file(paths["scenario"])}})
        spec = {"series": "f", "pilot": True, "scenarios_dir": str(self.tmp / "fx" / "scenarios"), "freeze": str(freeze),
                "scenario_ids": ["airk-C1"], "results": []}
        flow = run_pass.Pass(spec, self.tmp / "R")
        self.assertEqual([s["id"] for s in flow.scenarios], ["airk-C1"])
        common.write_json(freeze, {"files": {rel: "0" * 64}})
        with self.assertRaises(common.HarnessError):
            run_pass.Pass(spec, self.tmp / "R")
        with self.assertRaises(common.HarnessError):  # an empty subset is refused, never run as zero scenarios
            run_pass.Pass(dict(spec, freeze=None, scenario_ids=["nope"]), self.tmp / "R")


# ---------------------------------------------------------------- pass orchestration


class FakeRuns:
    """Session and judge runners that write synthetic records, for run_pass."""

    def __init__(self, bodies):
        self.bodies = bodies  # result name -> list of bodies per turn
        self.sessions, self.judged = [], []

    def session(self, *, scenario, path, series, sessions_root, configuration, device, sample, rerun=False,
                model=common.AGENT_MODEL, effort=common.AGENT_EFFORT, **_):
        name = run_session.result_name(configuration, device)
        directory = run_session.session_dir(sessions_root, series, scenario["repository"], scenario["slot"], name, sample)
        (directory / "wrapper" / "results").mkdir(parents=True, exist_ok=True)
        for n, body in enumerate(self.bodies[name], start=1):
            raw = directory / "wrapper" / "results" / f"turn-{n:02d}.stdout.json"
            common.write_bytes(raw, envelope(body))
            common.append_jsonl(directory / "wrapper" / "turns.jsonl", {
                "turn": n, "tool": common.PATH_TOOLS[path][0 if n == 1 else -1], "ok": True, "bytes": 100,
                "elapsed_ms": 10, "arguments": {}, "result_path": str(raw), "result_sha256": ""})
        common.write_text(directory / "brief.md", "brief")
        record = {"status": "complete", "path": path, "session_id": f"sid-{name}", "cost_usd": 0.5,
                  "model_requested": model, "effort_requested": effort}
        common.write_json(directory / "session.json", record)
        self.sessions.append(name)
        return record

    def judge(self, *, scenario, key, out_dir, judge, **_):
        record = {"key": key, "judge": judge, "status": "valid", "cost_usd": 0.1,
                  "verdict": verdict(essential=[n["id"] for n in scenario["notes"] if n["mark"] == "essential"])}
        common.write_json(Path(out_dir) / judge / "verdict.json", record)
        self.judged.append((key, judge))
        return record


class PassFlow(TempDir):
    def test_reuse_coverage_judging_and_grading(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        all_hits = {"hits": [hit("src/scheduler.rs", 1, 16), hit("docs/scheduler/CONTRACT.md", 1, 5),
                             hit("src/format.rs", 1, 3)]}
        partial = {"hits": [hit("src/scheduler.rs", 1, 16)]}
        rest = {"hits": [hit("docs/scheduler/CONTRACT.md", 1, 5), hit("src/format.rs", 1, 3)]}
        runs = FakeRuns({"parse": [partial, rest], "search": [partial, partial, rest],
                         "focus-cpu": [all_hits], "compact-cpu": [all_hits]})
        digests = {"parse": "p", "search": "s", "focus-cpu": "same", "compact-cpu": "same"}
        frozen = self.tmp / "prompts-freeze.json"
        prompts_freeze.write(frozen)
        spec = {"series": "t1", "scenarios_dir": str(self.tmp / "fx" / "scenarios"), "repos": ["airk"],
                "prompts_freeze": str(frozen), "concurrency": 2, "start_retry_seconds": 0, "results": [
                    {"configuration": "parse", "path": "parse", "device": "none", "handle": str(paths["handle"])},
                    {"configuration": "search", "path": "search", "device": "none", "handle": str(paths["handle"])},
                    {"configuration": "focus", "path": "focus", "device": "cpu", "handle": str(paths["handle"])},
                    {"configuration": "compact", "path": "compact", "device": "cpu", "handle": str(paths["handle"])}]}
        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=runs.session, judge_runner=runs.judge,
                             digest_fn=lambda s, e: {"digest": digests[run_pass.Pass.result_name(e)], "ok": True},
                             settle_fn=settled_fake())
        flow.sessions()
        self.assertEqual([s["phase"] for s in common.read_jsonl(flow.pass_dir / "settled.jsonl")], ["start", "end"])
        self.assertFalse((flow.pass_dir / "INVALID.json").exists())
        self.assertEqual(sorted(runs.sessions), ["focus-cpu", "parse", "search"])
        flow.coverage()
        flow.judge()
        self.assertEqual(len(runs.judged), 6)
        summary = flow.grade()
        rows = list(csv.DictReader(io.StringIO((flow.pass_dir / "results.csv").read_text(encoding="utf-8"))))
        by = {r["configuration"]: r for r in rows}
        self.assertEqual(by["compact"]["reused_from"], "airk-C1/focus-cpu")
        self.assertEqual((by["focus"]["turns_to_finish"], by["focus"]["adjudicated_grade"]), ("1", "4"))
        self.assertEqual(by["parse"]["turns_to_finish"], "2")
        self.assertEqual(by["search"]["turns_to_finish"], "3")
        focus = summary["configurations"]["focus-cpu"]["scenarios"][0]
        self.assertEqual(focus["best_path"], "parse")
        self.assertFalse(focus["case"]["substantial"])  # 1 vs 2: better, not substantial
        self.assertEqual(summary["headroom"]["count"], 0)
        self.assertEqual(summary["reused"], [{"key": "airk-C1/compact-cpu", "reused_from": "airk-C1/focus-cpu"}])
        import pass_report
        with contextlib.redirect_stdout(io.StringIO()):
            pass_report.main(["--pass-dir", str(flow.pass_dir)])
        report = common.read_json(flow.pass_dir / "pass-report.json")
        self.assertEqual(report["totals"]["sessions"], 3)
        self.assertEqual(report["totals"]["agent_cost_usd"], 1.5)
        self.assertEqual(report["per_result_name"]["search"]["turns_mean"], 3)
        focus_row = next(f for f in report["results"] if f["key"] == "airk-C1/focus-cpu")
        self.assertEqual((focus_row["coverage_turn"], focus_row["adjudicated"]), (1, "4"))
        # A failed canonical call is never cached as a digest (pilot finding).
        failing = run_pass.Pass(dict(spec, series="t2"), self.tmp / "R", session_runner=runs.session,
                                judge_runner=runs.judge, digest_fn=lambda s, e: {"digest": None, "ok": False},
                                settle_fn=settled_fake())
        failing.digest(failing.scenarios[0], spec["results"][2])
        self.assertEqual(failing.digests, {})
        self.assertTrue((failing.pass_dir / "digest-failures.jsonl").is_file())
        # Resume: nothing runs again.
        again = run_pass.Pass(spec, self.tmp / "R", session_runner=runs.session, judge_runner=runs.judge,
                              digest_fn=lambda s, e: self.fail("digest recomputed"), settle_fn=settled_fake())
        again.sessions()
        self.assertEqual(len(runs.sessions), 3)


def settled_fake(tokens=None, settled_flags=None):
    """A settle_fn returning snapshots; tokens/settled_flags are consumed one per call (last value repeats)."""
    tokens = list(tokens or ["tok-1"])
    flags = list(settled_flags or [True])
    calls = {"n": 0}

    def snap(handle_path, repo):
        n = calls["n"]
        calls["n"] += 1
        ok = flags[min(n, len(flags) - 1)]
        return {"repo": repo, "settled": ok, "problems": [] if ok else ["vectors job running"],
                "publication_token": tokens[min(n, len(tokens) - 1)], "at": f"t{n}"}
    return snap


class Decisions(TempDir):
    """The primary agent's decisions of 21 Nov 2026 (settled index, default mode, rule 2, freezes, depth)."""

    def spec(self, **extra):
        paths = synthetic.write_fixture(self.tmp / "fx")
        frozen = self.tmp / "prompts-freeze.json"
        if not frozen.is_file():
            prompts_freeze.write(frozen)
        base = {"series": "d", "scenarios_dir": str(self.tmp / "fx" / "scenarios"), "prompts_freeze": str(frozen),
                "handle": str(paths["handle"]), "start_retry_seconds": 0,
                "results": [{"configuration": "search", "path": "search", "device": "none"}]}
        base.update(extra)
        return base

    def test_judged_specs_need_the_prompts_freeze_and_no_queryable_override(self):
        spec = self.spec()
        run_pass.Pass(spec, self.tmp / "R")
        self.assertEqual(spec["results"][0]["handle"], spec["handle"])  # spec-level default handle
        with self.assertRaises(common.HarnessError):
            run_pass.Pass(dict(self.spec(), prompts_freeze=None), self.tmp / "R")
        with self.assertRaises(common.HarnessError):
            run_pass.Pass(self.spec(accept_queryable=True), self.tmp / "R")
        run_pass.Pass(self.spec(accept_queryable=True, pilot=True, prompts_freeze=None), self.tmp / "R")

    def test_prompts_freeze_refuses_a_mismatch(self):
        frozen = self.tmp / "pf.json"
        record = prompts_freeze.write(frozen)
        self.assertEqual(len(record["agent_prompts"]["search"]["sha256"]), 64)
        self.assertEqual(set(record["files"]), set(prompts_freeze.HARNESS_FILES))
        prompts_freeze.verify(frozen)
        record["agent_prompts"]["search"]["sha256"] = "0" * 64
        record["files"]["evidence.py"] = "0" * 64
        common.write_json(frozen, record)
        with self.assertRaises(common.HarnessError) as caught:
            prompts_freeze.verify(frozen)
        self.assertIn("agent prompt for search", str(caught.exception))
        self.assertIn("file evidence.py", str(caught.exception))
        with self.assertRaises(common.HarnessError):
            run_pass.Pass(self.spec(prompts_freeze=str(frozen)), self.tmp / "R")

    def test_settled_decision(self):
        status = {"semantic_readiness": {"state": "ready"}, "publication": {"token": "t"}}
        search = {"execution": {"served_mode": "hybrid", "incomplete": False}, "publication": {"token": "t"}}
        idle = {"observation": {"job": None}, "pending_job": None, "queued_jobs": []}
        self.assertTrue(settled.evaluate(status, search, idle)["settled"])
        busy = {"observation": {"job": {"id": "j", "kind": "vectors", "state": "running"}}, "queued_jobs": []}
        self.assertFalse(settled.evaluate(status, search, busy)["settled"])
        queued = {"observation": {}, "queued_jobs": [{"id": "k", "kind": "index", "state": "pending"}]}
        self.assertFalse(settled.evaluate(status, search, queued)["settled"])
        other = {"observation": {"job": {"id": "m", "kind": "model_setup", "state": "running"}}}
        self.assertTrue(settled.evaluate(status, search, other)["settled"])
        done = {"observation": {"job": {"id": "j", "kind": "vectors", "state": "succeeded"}}}
        self.assertTrue(settled.evaluate(status, search, done)["settled"])
        partial = {"semantic_readiness": {"state": "partial"}, "publication": {"token": "t"}}
        self.assertFalse(settled.evaluate(partial, search, idle)["settled"])
        lexical = dict(search, execution={"served_mode": "lexical"})
        self.assertFalse(settled.evaluate(status, lexical, idle)["settled"])
        self.assertFalse(settled.evaluate(status, search, None)["settled"])
        self.assertTrue(settled.compare({"publication_token": "a"}, {"publication_token": "a"})["unchanged"])
        self.assertFalse(settled.compare({"publication_token": "a"}, {"publication_token": "b"})["unchanged"])

    def test_guard_refuses_unsettled_start_and_invalidates_on_token_change(self):
        ran = []

        def runner(**kwargs):
            ran.append(kwargs["configuration"])
            return {"status": "complete"}
        flow = run_pass.Pass(self.spec(), self.tmp / "R", session_runner=runner,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True},
                             settle_fn=settled_fake(settled_flags=[False]))
        with self.assertRaises(common.HarnessError):
            flow.sessions()
        self.assertEqual(ran, [])
        flow = run_pass.Pass(self.spec(series="d2"), self.tmp / "R", session_runner=runner,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True},
                             settle_fn=settled_fake(tokens=["a", "b"]))
        with self.assertRaises(common.HarnessError):
            flow.sessions()
        self.assertEqual(ran, ["search"])
        invalid = common.read_json(flow.pass_dir / "INVALID.json")
        self.assertEqual(invalid["changes"][0]["start_token"], "a")
        self.assertEqual(invalid["changes"][0]["end_token"], "b")
        # A later invocation compares with the pass's first start token, not its own.
        later = run_pass.Pass(self.spec(series="d2"), self.tmp / "R", session_runner=runner,
                              digest_fn=lambda s, e: {"digest": "d", "ok": True}, settle_fn=settled_fake(tokens=["b"]))
        with self.assertRaises(common.HarnessError):
            later.sessions()
        pilot = run_pass.Pass(self.spec(series="d3", pilot=True), self.tmp / "R", session_runner=runner,
                              digest_fn=lambda s, e: {"digest": "d", "ok": True},
                              settle_fn=settled_fake(settled_flags=[False]))
        pilot.sessions()  # a pilot records an unsettled index and runs
        self.assertFalse(common.read_jsonl(pilot.pass_dir / "settled.jsonl")[0]["settled"])

    def test_agent_calls_get_the_default_response_mode(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        raw = common.read_json(paths["handle"])
        raw["environment"] = {"AIRK_MCP_RESPONSE_MODE": "full", "AIRK_OTHER": "kept"}
        common.write_json(paths["handle"], raw)
        handle = common.load_handle(paths["handle"])
        self.assertEqual(common.tool_env(handle).get("AIRK_MCP_RESPONSE_MODE"), "full")
        agent_env = common.tool_env(handle, common.AGENT_STRIPPED_ENV)
        self.assertNotIn("AIRK_MCP_RESPONSE_MODE", agent_env)
        self.assertEqual(agent_env["AIRK_OTHER"], "kept")
        self.assertEqual(common.effective_response_mode(handle, {}, common.AGENT_STRIPPED_ENV),
                         "terse (product default)")
        self.assertEqual(common.effective_response_mode(handle, {}), "full")
        self.assertEqual(common.effective_response_mode(handle, {"response_mode": "full"}, common.AGENT_STRIPPED_ENV),
                         "full")
        catalog = common.load_catalog(paths["catalog"])
        w = mcp_wrapper.Wrapper(handle=handle, repo="airk", path="search", instruction=synthetic.INSTRUCTION,
                                catalog=catalog, log_dir=self.tmp / "log")
        w.call("airk_search", {"query": synthetic.INSTRUCTION})
        row = common.read_jsonl(self.tmp / "log" / "turns.jsonl")[0]
        self.assertTrue(row["ok"])
        self.assertEqual(row["response_mode"], "terse (product default)")
        self.assertEqual(row["env_stripped"], ["AIRK_MCP_RESPONSE_MODE"])

    def test_depth_ranks(self):
        scenario = synthetic.scenario()
        body = {"hits": [hit("src/other.rs", 1, 9), hit("src/scheduler.rs", 9, 9), hit("src/scheduler.rs", 9, 16),
                         hit("src/format.rs", 1, 3)]}
        row = depth.measure(scenario, body)
        by = {n["id"]: n for n in row["notes"]}
        self.assertEqual((by["N2"]["same_path"], by["N2"]["rule1"], by["N2"]["rule2"]), (2, 3, 3))
        self.assertEqual(by["N4"]["rule2"], 4)
        self.assertIsNone(by["N3"]["rule2"])
        self.assertEqual((row["hits_returned"], row["essential_in_pool"], row["deepest_rank"]), (4, 2, 4))
        catalog = common.load_catalog(synthetic.write_fixture(self.tmp / "fx")["catalog"])
        self.assertIsNone(depth.catalog_limit_max(catalog))
        self.assertEqual(depth.budgets(65536), [65536, 49152, 32768, 16384])
        self.assertEqual(depth.budgets(20000), [20000, 16384])
        refused = json.dumps({"content": [{"type": "text", "text": json.dumps(
            {"error": {"code": "scope_reply_budget", "maximum_bytes": 65536}})}], "isError": True}).encode()
        out = common.CallOutcome(False, 1, refused, b"", 1, json.loads(refused), "CLI exit 1")
        self.assertEqual(depth.error_code(out), "scope_reply_budget")
        self.assertEqual(depth.depth_query("q", 100, 65536), {"query": "q", "limit": 100, "max_bytes": 65536,
                                                            "verification": "off"})

    def test_reference_arm_resolves_the_better_baseline_and_never_reuses_another_agent(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        partial = {"hits": [hit("src/scheduler.rs", 1, 16)]}
        rest = {"hits": [hit("docs/scheduler/CONTRACT.md", 1, 5), hit("src/format.rs", 1, 3)]}
        runs = FakeRuns({"parse": [partial, rest], "search": [partial, partial, rest], "noModel": [partial, rest],
                         "reference-sonnet-best": [partial, rest], "reference-sonnet-noModel": [partial, rest]})
        spec = self.spec(series="ref", results=[
            {"configuration": "search", "path": "search"}, {"configuration": "parse", "path": "parse"},
            {"configuration": "noModel", "path": "noModel"},
            {"configuration": "reference-sonnet-noModel", "path": "noModel", "arm": "reference-sonnet",
             "model": "claude-sonnet-5-5", "effort": "medium"},
            {"configuration": "reference-sonnet-best", "path": "@better-of-search-parse", "arm": "reference-sonnet",
             "model": "claude-sonnet-5-5", "effort": "medium"}])
        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=runs.session, judge_runner=runs.judge,
                             digest_fn=lambda s, e: {"digest": "same-evidence", "ok": True}, settle_fn=settled_fake())
        with self.assertRaises(common.HarnessError):  # the marker needs the Haiku arm graded first
            flow.concrete(spec["results"][4], flow.scenarios[0])
        flow.sessions({"search", "parse", "noModel"})
        flow.coverage()
        flow.judge()
        flow.grade()
        self.assertEqual(flow.better_baseline(flow.scenarios[0]), "parse")
        flow.sessions({"reference-sonnet-noModel", "reference-sonnet-best"})
        self.assertIn("reference-sonnet-noModel", runs.sessions)  # same digest, other agent: not reused
        self.assertIn("reference-sonnet-best", runs.sessions)
        self.assertEqual(flow.reuse_map(), {})
        rows = {r["configuration"]: r for r in flow.index()["results"]}
        self.assertEqual(rows["reference-sonnet-best"]["path"], "parse")
        self.assertEqual(rows["reference-sonnet-best"]["agent_model"], "claude-sonnet-5-5")
        self.assertEqual(rows["noModel"]["agent_model"], "claude-haiku-5-5")
        record = common.read_json(Path(rows["reference-sonnet-best"]["session_dir"]) / "session.json")
        self.assertEqual(record["model_requested"], "claude-sonnet-5-5")

    def test_a_refused_start_is_never_the_reference_token(self):
        """airk baseline finding (22 Nov 2026): a start refused for a typed busy reply stored token None."""
        def runner(**kwargs):
            return {"status": "complete"}
        flow = run_pass.Pass(self.spec(series="ref1", start_attempts=1), self.tmp / "R", session_runner=runner,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True},
                             settle_fn=settled_fake(tokens=[None, "t", "t"], settled_flags=[False, True, True]))
        with self.assertRaises(common.HarnessError):
            flow.sessions()
        self.assertFalse((flow.pass_dir / "settled-start.json").exists())
        flow.sessions()  # the next start is settled and becomes the reference; the end matches it
        self.assertEqual(common.read_json(flow.pass_dir / "settled-start.json")["airk"]["publication_token"], "t")
        self.assertFalse((flow.pass_dir / "INVALID.json").exists())

    def test_an_unsettled_start_is_retried_and_never_a_token_change(self):
        """cs1-airk finding (22 Nov 2026): a start met scoped_connection_unavailable; it was compared as a change."""
        ran = []

        def runner(**kwargs):
            ran.append(kwargs["configuration"])
            return {"status": "complete"}
        flow = run_pass.Pass(self.spec(series="retry1"), self.tmp / "R", session_runner=runner,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True},
                             settle_fn=settled_fake(tokens=["t", "t", None, "t", "t"],
                                                    settled_flags=[True, True, False, True, True]))
        flow.sessions()  # start t, end t
        flow.sessions()  # start None (retried), then t; end t
        self.assertEqual(ran, ["search", "search"])
        self.assertFalse((flow.pass_dir / "INVALID.json").exists())
        phases = [s["phase"] for s in common.read_jsonl(flow.pass_dir / "settled.jsonl")]
        self.assertEqual(phases, ["start", "end", "start-retry", "start", "end"])

    def test_end_snapshot_needs_only_an_unchanged_token(self):
        """Baseline-bookbuilder finding: an unsettled end snapshot with the same token is recorded, not fatal."""
        ran = []

        def runner(**kwargs):
            ran.append(kwargs["configuration"])
            return {"status": "complete"}
        flow = run_pass.Pass(self.spec(series="end"), self.tmp / "R", session_runner=runner,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True},
                             settle_fn=settled_fake(tokens=["same"], settled_flags=[True, False]))
        flow.sessions()
        self.assertEqual(ran, ["search"])
        self.assertFalse((flow.pass_dir / "INVALID.json").exists())
        events = [e["event"] for e in common.read_jsonl(flow.pass_dir / "pass-log.jsonl")]
        self.assertIn("end-snapshot-not-settled", events)
        missing = run_pass.Pass(self.spec(series="end2"), self.tmp / "R", session_runner=runner,
                                digest_fn=lambda s, e: {"digest": "d", "ok": True},
                                settle_fn=settled_fake(tokens=["same", None]))
        with self.assertRaises(common.HarnessError):  # an end without any token is still refused
            missing.sessions()

    def test_live_stage_names_come_from_every_lifetime(self):
        fixture = self.tmp / "fixture"
        (fixture / "L1-a").mkdir(parents=True)
        common.append_jsonl(fixture / "L1-a" / "stages.jsonl", {"profile": "focus-x", "manifest": "m1", "enabled": True})
        common.write_json(fixture / "L2-b" / "handle.json", {"cli": [sys.executable, "-c",
                          "import json;print(json.dumps({'configuration':{'focus':{'enabled':True,"
                          "'selected_profile':'m1'},'context':{'enabled':False,'selected_profile':'m2'}}}))"],
                          "control": "c", "indexes": {"airk": "i"}})
        handle = common.load_handle(fixture / "L2-b" / "handle.json")
        self.assertEqual(handle.live_stages(), {"focus": "focus-x", "context": None})

    def test_index_aliases_serve_a_repository_from_another_index(self):
        fixture = self.tmp / "fixture"
        raw = {"state": "ready", "cli": "x", "control": "c", "indexes": {
            "bookbuilder": {"index_id": "idx_old", "binding": "b_old"},
            "bookbuilder-v2": {"index_id": "idx_new", "binding": "b_new"}, "airk": {"index_id": "idx_a"}}}
        common.write_json(fixture / "L3-x" / "handle.json", raw)
        plain = common.load_handle(fixture / "L3-x" / "handle.json")
        self.assertEqual((plain.index_id("bookbuilder"), plain.binding("bookbuilder")), ("idx_old", "b_old"))
        common.write_json(fixture / "index-aliases.json", {"aliases": {"bookbuilder": "bookbuilder-v2"}})
        aliased = common.load_handle(fixture / "L3-x" / "handle.json")
        self.assertEqual((aliased.index_id("bookbuilder"), aliased.binding("bookbuilder")), ("idx_new", "b_new"))
        self.assertEqual(aliased.index_id("bookbuilder@original"), "idx_old")
        self.assertEqual(aliased.index_id("airk"), "idx_a")
        self.assertEqual(aliased.aliases, {"bookbuilder": "bookbuilder-v2"})
        common.write_json(fixture / "index-aliases.json", {"aliases": {"bookbuilder": "missing"}})
        with self.assertRaises(common.HarnessError):
            common.load_handle(fixture / "L3-x" / "handle.json")

    def test_spec_level_answer_budget(self):
        import make_cs_spec
        spec = make_cs_spec.spec("airk", "baseline", ["A1"], "cs1-airk", "f", "c", "l", self.tmp, "")
        self.assertEqual(spec["answer_defaults"]["budget"], make_cs_spec.CS1_BUDGET)
        large = next(e for e in spec["results"] if e["configuration"] == "large")
        self.assertEqual(large["answer_defaults"]["budget"]["fanout"], 12)
        flow_spec = self.spec(answer_defaults={"budget": {"calls": 12}},
                              results=[{"configuration": "noModel", "path": "noModel"},
                                       {"configuration": "focus", "path": "focus", "answer_defaults": {}}])
        flow = run_pass.Pass(flow_spec, self.tmp / "R")
        self.assertEqual(flow.spec["results"][0]["answer_defaults"], {"budget": {"calls": 12}})
        self.assertEqual(flow.spec["results"][1]["answer_defaults"], {})  # an entry's own value wins

    def test_image_identity_is_recorded(self):
        image = {"label": "cs1b-8117a9c5", "service": {"path": "x", "sha256": "ab" * 32}, "git_head": "c" * 40}
        self.assertEqual(common.image_identity(image), {"label": "cs1b-8117a9c5", "service_sha256": "ab" * 32,
                                                        "git_head": "c" * 40})
        self.assertIsNone(common.image_identity(None))

    def test_second_sessions_follow_close_calls_and_the_only_filter(self):
        ran = []

        def runner(**kwargs):
            ran.append((kwargs["configuration"], kwargs["sample"]))
            return {"status": "complete"}
        spec = self.spec(series="sec", results=[{"configuration": "parse", "path": "parse"},
                                                {"configuration": "focus", "path": "focus", "device": "cpu"}])
        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=runner, settle_fn=settled_fake(),
                             digest_fn=lambda s, e: {"digest": "d", "ok": True})
        common.write_json(flow.pass_dir / "summary.json", {"configurations": {"focus-cpu": {
            "scenarios": [{"scenario": "airk-C1", "best_path": "parse"}], "close_calls": ["airk-C1"]}}})
        self.assertEqual(flow.second_sessions({"focus-cpu"}), ["airk-C1/focus-cpu.s2"])
        self.assertEqual(ran, [("focus", 2)])
        self.assertEqual(flow.second_sessions({"parse"}), ["airk-C1/parse.s2"])
        self.assertEqual(common.read_json(flow.pass_dir / "second-sessions.json"),
                         ["airk-C1/focus-cpu.s2", "airk-C1/parse.s2"])

    def test_newest_handle_and_settled_wait_check(self):
        fixture = self.tmp / "fixture"
        for name in ("L2-a", "L10-b", "L9-c"):
            common.write_json(fixture / name / "handle.json", {"state": "indexing", "cli": "x", "control": "c",
                                                                "indexes": {"airk": "i"}})
        self.assertEqual(common.newest_handle(fixture).parent.name, "L10-b")
        self.assertEqual(common.resolve_handle("@newest", fixture).parent.name, "L10-b")
        self.assertEqual(common.resolve_handle(str(fixture / "L2-a" / "handle.json"), fixture).parent.name, "L2-a")
        ok, snaps = settled.check("@newest", fixture, ["airk"], False)  # an unready handle is not settled
        self.assertFalse(ok)
        self.assertIn("not ready", snaps[0]["problems"][0])
        with self.assertRaises(common.HarnessError):
            common.newest_handle(self.tmp / "empty")

    def test_stage_switch_disables_the_live_focus_first(self):
        import run_baseline
        calls = []

        class FakeHandle:
            path = Path("L9-x") / "handle.json"
            state = {"focus": "f-old", "context": "c-old"}

            def stages(self):
                return dict(self.state)
        fake = FakeHandle()
        runner = run_baseline.Runner.__new__(run_baseline.Runner)
        runner.handle = lambda: fake
        runner.log = lambda **e: None

        def run(label, command):
            profile, enable = command[command.index("--profile") + 1], command[-1] == "--enable"
            calls.append((profile, enable))
            kind = "focus" if profile.startswith("f") else "context"
            fake.state[kind] = profile if enable else None
        runner.run = run
        runner.set_stages({"focus": "f-new", "context": None})
        self.assertEqual(calls, [("c-old", False), ("f-old", False), ("f-new", True)])

    def test_require_images_refuses_another_image(self):
        spec = self.spec(require_images=["baseline-1fe7b600-o2-fix3"])
        raw = common.read_json(Path(spec["handle"]))
        raw["image"] = {"label": "baseline-1fe7b600-o2-fix2", "service": {"sha256": "a" * 64}}
        common.write_json(Path(spec["handle"]), raw)
        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=lambda **k: {"status": "complete"},
                             settle_fn=settled_fake(), digest_fn=lambda s, e: {"digest": "d", "ok": True})
        with self.assertRaises(common.HarnessError):
            flow.sessions()
        raw["image"]["label"] = "baseline-1fe7b600-o2-fix3"
        common.write_json(Path(spec["handle"]), raw)
        flow.check_image(spec["results"][0])

    def test_lifetime_search_and_layers_must_match(self):
        spec = self.spec(require_search="search_v2", layers_by_configuration={"search": "all,pool_full=8"})
        raw = common.read_json(Path(spec["handle"]))
        raw["service_environment"] = {"AIRK_ARBITER_EVALUATION_SEARCH": "search_v2",
                                      "AIRK_ARBITER_EVALUATION_ANSWER_LAYERS": "all,pool_full=8,context_b"}
        common.write_json(Path(spec["handle"]), raw)
        flow = run_pass.Pass(spec, self.tmp / "R")
        with self.assertRaises(common.HarnessError):
            flow.check_lifetime(spec["results"][0])  # layers differ
        raw["service_environment"]["AIRK_ARBITER_EVALUATION_ANSWER_LAYERS"] = "all,pool_full=8"
        common.write_json(Path(spec["handle"]), raw)
        flow.check_lifetime(spec["results"][0])
        raw["service_environment"].pop("AIRK_ARBITER_EVALUATION_SEARCH")
        common.write_json(Path(spec["handle"]), raw)
        with self.assertRaises(common.HarnessError):
            flow.check_lifetime(spec["results"][0])  # search switch off

    def test_baseline_driver_groups_entries_by_stages(self):
        import run_baseline
        spec = common.read_json(Path(__file__).with_name("synthetic.py")) if False else {"results": [
            {"configuration": "search", "path": "search", "stages": {}},
            {"configuration": "focus", "path": "focus", "device": "cpu", "stages": {"focus": "focus-static1024-cpu"}},
            {"configuration": "large", "path": "large", "device": "cpu",
             "stages": {"focus": "focus-static1024-cpu", "context": "context-large-cpu"}},
            {"configuration": "noModel", "path": "noModel", "stages": {}},
            {"configuration": "reference-sonnet-best", "path": "@better-of-search-parse", "stages": {}},
            {"configuration": "compact", "path": "compact", "device": "cpu",
             "stages": {"focus": "focus-static1024-cpu", "context": "context-compact-cpu"}}]}
        got = [(g[0]["focus"], g[0]["context"], g[1]) for g in run_baseline.groups(spec)]
        self.assertEqual(got, [(None, None, ["search", "noModel"]),
                               ("focus-static1024-cpu", None, ["focus-cpu"]),
                               ("focus-static1024-cpu", "context-compact-cpu", ["compact-cpu"]),
                               ("focus-static1024-cpu", "context-large-cpu", ["large-cpu"])])

    def test_cs_replay_compares_the_recorded_first_answer(self):
        import cs_replay
        scenario = synthetic.scenario()
        session = self.tmp / "s"
        (session / "wrapper" / "results").mkdir(parents=True)
        raw = session / "wrapper" / "results" / "turn-02.stdout.json"
        common.write_bytes(raw, envelope({"hits": [hit("src/scheduler.rs", 1, 16)]}))
        common.append_jsonl(session / "wrapper" / "turns.jsonl", {"turn": None, "rejected": True, "tool": "airk_answer"})
        common.append_jsonl(session / "wrapper" / "turns.jsonl", {"turn": 1, "tool": "airk_search", "ok": True})
        common.append_jsonl(session / "wrapper" / "turns.jsonl", {
            "turn": 2, "tool": "airk_answer", "ok": True, "arguments": {"question": "q"},
            "sent_arguments": {"question": "q", "schema": common.ANSWER_SCHEMA}, "result_path": str(raw),
            "elapsed_ms": 9, "bytes": 3})
        self.assertEqual(cs_replay.recorded_first(session)["turn"], 2)
        new_body = {"hits": [hit("src/scheduler.rs", 1, 16), hit("src/format.rs", 1, 3)]}
        sent = []

        def fake_issue(handle, repo, tool, args, out, stem, timeout_s):
            sent.append(args)
            return {"ok": True, "error": None, "elapsed_ms": 5, "bytes": 7, "digest": evidence.evidence_digest(new_body),
                    "result_path": str(raw.with_name("new.json")), "typed_retries": 0}
        common.write_bytes(raw.with_name("new.json"), envelope(new_body))
        original = cs_replay.replay.issue
        cs_replay.replay.issue = fake_issue
        try:
            row = cs_replay.one(scenario, session, None, self.tmp / "out", 10)
        finally:
            cs_replay.replay.issue = original
        self.assertEqual(sent, [{"question": "q", "schema": common.ANSWER_SCHEMA}])
        self.assertEqual((row["recorded_essential_covered"], row["essential_covered"]), (2, 3))  # N1+N2, then + N4
        self.assertNotEqual(row["digest"], row["recorded_digest"])

    def test_depth_compare(self):
        import depth_compare
        raw = self.tmp / "r.json"
        common.write_bytes(raw, envelope({"hits": [hit("a", 1, 9)]}))
        row = {"scenario_id": "s", "ok": True, "hits_returned": 1, "deepest_rank": 1, "elapsed_ms": 10,
               "result_path": str(raw), "notes": [{"id": "N1", "rule2": 1, "rule1": 1, "same_path": 1},
                                                  {"id": "N2", "rule2": None, "rule1": None, "same_path": None}]}
        same = depth_compare.compare({"rows": [row]}, {"rows": [dict(row, elapsed_ms=30)]})
        self.assertTrue(same["identical"])
        self.assertEqual(same["totals"]["a"], {"rule2": 1, "rule1": 1, "same_path": 1, "essential": 2})
        self.assertEqual((same["latency_ms"]["a"]["mean"], same["latency_ms"]["b"]["mean"]), (10, 30))
        moved = dict(row, notes=[{"id": "N1", "rule2": 2, "rule1": 1, "same_path": 1}, row["notes"][1]])
        self.assertFalse(depth_compare.compare({"rows": [row]}, {"rows": [moved]})["identical"])

    def test_listing_restarts_after_an_expired_cursor(self):
        import credential_probe
        pages = []

        def env(body, ok=True):
            raw = json.dumps({"content": [{"type": "text", "text": json.dumps(body)}], "isError": not ok}).encode()
            return common.CallOutcome(ok, 0 if ok else 1, raw, b"", 1, json.loads(raw), None if ok else "CLI exit 1")
        script = [env({"files": [{"path": "a"}], "cursor": "c1"}),
                  env({"error": {"code": "read_failed", "message": "discovery cursor expired or filters changed"}},
                      ok=False),
                  env({"files": [{"path": "a"}], "cursor": "c2"}), env({"files": [{"path": "b"}]})]
        original = common.call_tool_retrying
        common.call_tool_retrying = lambda *a, **k: (pages.append(a[3]) or script.pop(0), 0)
        try:
            result = credential_probe.list_all(None, "bookbuilder")
        finally:
            common.call_tool_retrying = original
        self.assertEqual((result["paths"], result["complete"], result["restarts"]), (["a", "b"], True, 1))

    def test_credential_patterns(self):
        import credential_probe
        found = credential_probe.classify([
            "aikeys.json", "src/AIKeys.JSON", "app/appsettings.Development.json", "pro/client_secret.txt",
            ".env", "web/.env.local", "certs/dev.pfx", "ironspire/scripts/set-sendgrid-key.ps1",
            "docs/environment.md", "src/settings.json", "set-key.ps1", "src/envelope.cs"])
        self.assertEqual(found["aikeys.json"], ["aikeys.json", "src/AIKeys.JSON"])
        self.assertEqual(found["appsettings*.json"], ["app/appsettings.Development.json"])
        self.assertEqual(found["*secret*"], ["pro/client_secret.txt"])
        self.assertEqual(found[".env"], [".env"])
        self.assertEqual(found[".env.*"], ["web/.env.local"])
        self.assertEqual(found["*.pfx"], ["certs/dev.pfx"])
        self.assertEqual(found["set-*-key.ps1"], ["ironspire/scripts/set-sendgrid-key.ps1"])
        self.assertEqual(credential_probe.entries_of({"files": [{"path": "a"}, "b"]}), ["a", "b"])

    def test_rescore_writes_beside_the_original(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        runs = FakeRuns({"search": [{"hits": [hit("src/scheduler.rs", 2, 2), hit("src/scheduler.rs", 12, 12),
                                              hit("docs/scheduler/CONTRACT.md", 4, 4), hit("src/format.rs", 2, 2)]}]})
        spec = {"series": "rs", "pilot": True, "start_retry_seconds": 0, "scenarios_dir": str(self.tmp / "fx" / "scenarios"),
                "handle": str(paths["handle"]), "results": [{"configuration": "search", "path": "search"}]}
        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=runs.session, judge_runner=runs.judge,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True}, settle_fn=settled_fake())
        flow.sessions()
        flow.index()
        directory = Path(flow.index()["results"][0]["session_dir"])
        coverage.for_session(flow.scenarios[0], directory, rule=1)
        flow.judge()
        flow.grade()
        original = (flow.pass_dir / "results.csv").read_bytes()
        result = rescore.rescore(flow.pass_dir, 2, {s["id"]: s for s in flow.scenarios})
        self.assertEqual((flow.pass_dir / "results.csv").read_bytes(), original)
        rule1 = next(csv.DictReader(io.StringIO(original.decode())))
        rule2 = next(csv.DictReader(io.StringIO((flow.pass_dir / "results-rule2.csv").read_text(encoding="utf-8"))))
        self.assertEqual((rule1["essential_covered"], rule1["turns_to_finish"]), ("4", "1"))
        self.assertEqual((rule2["essential_covered"], rule2["turns_to_finish"]), ("2", ""))  # N2 needs 5 lines, N3 (2 lines) both
        self.assertEqual(result["summary"]["coverage_rule"], 2)
        self.assertTrue((directory / "coverage-rule2.json").is_file())


    def test_plan_lifetime_names_and_close_verdicts(self):
        self.assertEqual(run_plan.next_lifetime("L35-fix3", "cs2a"), "L36-cs2a")
        with self.assertRaises(common.HarnessError):
            run_plan.next_lifetime("fix3", "x")
        alive, dead = (lambda pid: True), (lambda pid: False)
        self.assertEqual(run_plan.closed_verdict({"state": "ready", "restartable": True}, dead), "open")
        self.assertEqual(run_plan.closed_verdict({"state": "closed", "restartable": True}, alive), "open")
        self.assertEqual(run_plan.closed_verdict({"state": "closed", "restartable": True}, dead), "closed")
        self.assertEqual(run_plan.closed_verdict({"state": "ready", "stop_reason": "stop file",
                                                  "restartable": False}, dead), "blocked")

    def test_plan_runs_steps_in_order_and_stops_at_a_failure(self):
        spec = self.tmp / "spec.json"
        spec.write_text(json.dumps({"series": "cs9"}), encoding="utf-8")
        plan_path = self.tmp / "plan.json"
        plan_path.write_text(json.dumps([
            {"replays": {"series": "baseline", "repos": "airk", "out": str(self.tmp / "rep"),
                         "plan": [["a", None, None, "noModel"]], "answer_defaults": {"budget": {"calls": 1}}}},
            {"sessions": {"spec": str(spec), "groups": [["f1", None, "noModel,focus-cpu"]]}},
            {"grade": {"spec": str(spec)}},
            {"second": {"spec": str(spec), "groups": [["f1", "c1", "compact-cpu"]]}},
            {"compare": {"a": "A", "b": "B", "out": "O"}},
        ]), encoding="utf-8")
        calls = []
        failing = {"label": None}

        def fake_run(self_, label, command, timeout=0):
            calls.append((label, command[2:]))
            common.require(label != failing["label"], f"{label} failed")

        original = run_plan.Plan.run
        run_plan.Plan.run = fake_run
        try:
            log = self.tmp / "plan-log.jsonl"
            with contextlib.redirect_stdout(io.StringIO()):
                code = run_plan.main(["--plan", str(plan_path), "--log", str(log), "--r-root", str(self.tmp / "R")])
            self.assertEqual(code, 0)
            labels = [label for label, _ in calls]
            self.assertEqual(labels, ["replays-airk-rep", "stages-f1-none", "sessions-noModel,focus-cpu",
                                      "coverage-" + self.tmp.name, "judge-" + self.tmp.name, "grade-" + self.tmp.name,
                                      "report-" + self.tmp.name, "summary-" + self.tmp.name,
                                      "stages-f1-c1", "second-compact-cpu", "compare-B"])
            replay = calls[0][1]
            self.assertEqual(json.loads(replay[replay.index("--answer-defaults") + 1]), {"budget": {"calls": 1}})
            self.assertEqual(calls[2][1][-4:], ["--stage", "sessions", "--only", "noModel,focus-cpu"])
            self.assertEqual(calls[9][1][-4:], ["--stage", "second", "--only-second", "compact-cpu"])
            self.assertIn(str(self.tmp / "R" / "passes" / "cs9"), calls[6][1])
            calls.clear()
            failing["label"] = "sessions-noModel,focus-cpu"
            with contextlib.redirect_stdout(io.StringIO()):
                code = run_plan.main(["--plan", str(plan_path), "--log", str(log), "--r-root", str(self.tmp / "R"),
                                      "--from-step", "2"])
            self.assertEqual(code, 1)
            self.assertEqual([label for label, _ in calls], ["stages-f1-none", "sessions-noModel,focus-cpu"])
            self.assertEqual(common.read_jsonl(log)[-1]["event"], "stopped")
        finally:
            run_plan.Plan.run = original

    def test_only_names_must_match_a_result(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        spec = {"series": "names", "pilot": True, "scenarios_dir": str(self.tmp / "fx" / "scenarios"),
                "handle": str(paths["handle"]), "results": [{"configuration": "focus", "path": "focus", "device": "cpu"}]}
        flow = run_pass.Pass(spec, self.tmp / "R", session_runner=None, judge_runner=None,
                             digest_fn=lambda s, e: {"digest": "d", "ok": True}, settle_fn=settled_fake())
        with self.assertRaisesRegex(common.HarnessError, r"unknown result names \['focus'\]"):
            flow.sessions({"focus"})
        with self.assertRaisesRegex(common.HarnessError, "unknown result names"):
            flow.second_sessions({"compact"})
        flow.check_names({"focus-cpu"})

    def test_search_replay_reissues_recorded_searches_and_combines_arms(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        scenario = common.load_scenario(paths["scenario"])
        handle = common.load_handle(paths["handle"], accept_queryable=True)
        session = self.tmp / "sessions" / "s" / scenario["id"] / "search"
        recorded = session / "wrapper" / "results" / "turn-01.stdout.json"
        recorded.parent.mkdir(parents=True)
        recorded.write_bytes(envelope({"hits": []}))
        turns = [{"turn": 1, "tool": "airk_search", "arguments": {"query": "q1"}, "sent_arguments": {"query": "q1"},
                  "ok": True, "result_path": str(recorded)},
                 {"turn": 2, "tool": "airk_search", "arguments": {"query": "bad"}, "rejected": "first-call rule"},
                 {"turn": 3, "tool": "airk_read_file", "arguments": {"path": "src/scheduler.rs"}, "ok": True},
                 {"turn": 4, "tool": "airk_search", "arguments": {"query": "q2"}, "sent_arguments": {"query": "q2"},
                  "ok": True, "result_path": str(recorded)}]
        for turn in turns:
            common.append_jsonl(session / "wrapper" / "turns.jsonl", turn)
        backend = FakeBackend({"hits": [hit("src/scheduler.rs", 1, 20), hit("docs/scheduler/CONTRACT.md", 1, 10)]})
        entry = search_replay.replay_one(session, scenario, handle, self.tmp / "out", 2, 5.0, backend)
        self.assertEqual([args for _, args in backend.calls], [{"query": "q1"}, {"query": "q2"}])
        self.assertEqual(entry["recorded_essential_covered"], 0)
        self.assertGreater(entry["essential_covered"], 0)
        for arm, covered in (("default", 3), ("search_v2", 1)):
            common.write_json(self.tmp / arm / "search-replay.json", {
                "arm": arm, "lifetime": "L1", "rule": 2, "tokens_start": {"airk": "t"},
                "series": {"b": [{"scenario_id": "airk-C1", "essential_total": 5, "essential_covered": covered,
                                  "recorded_essential_covered": 2}]}})
        combined = search_replay.combine([self.tmp / "default", self.tmp / "search_v2"])
        self.assertTrue(combined["same_publication"])
        self.assertEqual(combined["scenarios"]["airk-C1"], {"essential_total": 5, "b@default": 3, "b@search_v2": 1,
                                                            "b@recorded": 2})
        self.assertEqual(combined["totals"], {"b@default": 3, "b@recorded": 2, "b@search_v2": 1})

    def test_verify_restart_requires_exception_tokens_and_idle_stages(self):
        paths = synthetic.write_fixture(self.tmp / "fx")
        raw = common.read_json(paths["handle"])
        token = "airk-r1:abc:330:dd"
        snaps = [{"repo": "airk", "publication_token": token, "settled": True}]
        checker = lambda *a: (True, snaps)  # noqa: E731
        busy = {"focus": {"queued": 0, "running": 1}, "context": {"queued": 0, "running": 0}}
        quiet = {"focus": {"queued": 0, "running": 0}, "context": {"queued": 0, "running": 0}}
        sequence = iter([busy, quiet, busy, quiet, quiet, quiet])
        result = verify_restart.verify(paths["handle"], {"airk": token}, 100, sampler=lambda h: next(sequence),
                                       checker=checker, sleep=lambda s: None)
        self.assertEqual(result["problems"], ["the handle carries no restart_exception"])
        self.assertEqual(len(result["stage_samples"]), 6)
        common.write_json(paths["handle"], dict(raw, restart_exception={"receipt": {"path": "x"}}))
        result = verify_restart.verify(paths["handle"], {"airk": token}, 100, sampler=lambda h: quiet,
                                       checker=checker, sleep=lambda s: None)
        self.assertTrue(result["passed"])
        result = verify_restart.verify(paths["handle"], {"airk": "airk-r1:abc:331:dd"}, 100,
                                       sampler=lambda h: quiet, checker=checker, sleep=lambda s: None)
        self.assertFalse(result["passed"])
        self.assertEqual(len(result["problems"]), 2)  # token and generation
        result = verify_restart.verify(paths["handle"], {"airk": token}, 0, sampler=lambda h: busy,
                                       checker=checker, sleep=lambda s: None)
        self.assertIn("not idle", result["problems"][0])

    def test_refreeze_keeps_manifests_and_computes_prompts_changed(self):
        base = prompts_freeze.current()
        state = {"now": base}
        original = prompts_freeze.current
        prompts_freeze.current = lambda: state["now"]
        try:
            path = self.tmp / "design" / "prompts-freeze.json"
            first = prompts_freeze.refreeze(path, "first")
            self.assertIsNone(first["old_sha256"])
            self.assertIsNone(first["prompts_changed"])  # nothing to compare with
            changed_file = dict(base, files=dict(base["files"], **{"grade.py": "0" * 64}))
            state["now"] = changed_file
            second = prompts_freeze.refreeze(path, "harness file only")
            self.assertEqual((second["prompts_changed"], second["files_changed"]), (False, ["grade.py"]))
            self.assertEqual(second["old_sha256"], first["new_sha256"])
            state["now"] = dict(changed_file, judge_schema={"sha256": "1" * 64, "text": "x"})
            third = prompts_freeze.refreeze(path, "rubric")
            self.assertEqual((third["prompts_changed"], third["prompt_changes"]),
                             (True, ["rubric (judge verdict schema)"]))
            manifests = sorted(p.stem for p in (path.parent / "prompts-freeze-manifests").glob("*.json"))
            self.assertEqual(manifests, sorted({first["new_sha256"], second["new_sha256"], third["new_sha256"]}))
            for entry in (first, second, third):
                self.assertEqual(common.sha256_file(Path(entry["new_manifest"])), entry["new_sha256"])
            history = common.read_jsonl(path.parent / "prompts-freeze-history.jsonl")
            self.assertEqual([h["reason"] for h in history], ["first", "harness file only", "rubric"])
            with self.assertRaises(common.HarnessError):
                prompts_freeze.refreeze(path, " ")
        finally:
            prompts_freeze.current = original

if __name__ == "__main__":
    unittest.main()
