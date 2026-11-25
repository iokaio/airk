# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

import initial_search_depth as diagnostic


ARGS = {"query": "find sample", "limit": 6, "max_bytes": 4608, "full_chunks": False}
TOKEN = "synthetic-publication"


def search_out(hits=(), *, token=TOKEN, focus=None):
    body = {"publication": {"token": token}, "hits": list(hits), "focus": focus,
            "execution": {"served_mode": "hybrid"}, "truncated": False}
    envelope = {"structuredContent": body}
    return diagnostic.common.CallOutcome(True, 0, json.dumps(envelope).encode(), b"", 1, envelope, None)


def cap_out(code):
    envelope = {"structuredContent": {"error": {"code": code}}, "isError": True}
    return diagnostic.common.CallOutcome(False, 1, json.dumps(envelope).encode(), b"", 2, envelope, "exit")


def hit(path):
    return {"citation": {"path": path, "start_line": 1, "end_line": 10}, "text": "sample indexed text"}


class InitialSearchDepthTests(unittest.TestCase):
    def test_retains_only_first_round_search_uses_and_refuses_unexpected_arguments(self):
        body = {"schema": diagnostic.loop_reply.RESPONSE_SCHEMA, "records": [
            {"id": "initial", "operation": "search", "arguments": ARGS},
            {"id": "follow", "operation": "search", "arguments": ARGS},
            {"id": "structural", "operation": "symbols", "arguments": {}},
        ], "record_uses": [
            {"record": "initial", "part": 1, "round": 1, "cause": "part_search"},
            {"record": "initial", "part": 2, "need": 1, "round": 1, "cause": "shared_read"},
            {"record": "initial", "part": 2, "need": 1, "round": 2, "cause": "shared_read"},
            {"record": "follow", "part": 1, "need": 1, "round": 2, "cause": "follow_up"},
            {"record": "structural", "part": 1, "need": 1, "round": 1, "cause": "need_search"},
        ]}
        rows = diagnostic.initial_searches(body)
        self.assertEqual([record["id"] for record, _ in rows], ["initial"])
        self.assertEqual([use["cause"] for use in rows[0][1]], ["part_search", "shared_read"])
        body["records"][0]["arguments"] = {**ARGS, "index": "unapproved"}
        with self.assertRaisesRegex(diagnostic.common.HarnessError, "unexpected retained"):
            diagnostic.initial_searches(body)

    def test_measures_projection_matched_deep_and_unobserved_independently(self):
        scenario = {"notes": [{"id": path, "path": path, "lines": [2, 4], "mark": "essential"}
                              for path in ("shown.rs", "matched.rs", "deep.rs", "absent.rs")]}
        projection = {"projection": "airk.arbiter.loop-evidence/1", "spans": [
            {"path": "shown.rs", "start_line": 1, "end_line": 10, "text": "sample"}]}
        narrow = {"hits": [hit("matched.rs")]}
        deep = {"hits": [hit("matched.rs"), hit("deep.rs")]}
        rows = diagnostic.measure_notes(scenario, projection, narrow, deep)
        self.assertEqual([r["placement"] for r in rows], ["answer_projection", "matched_search_only",
                         "widened_search_only", "absent_from_observed_searches"])
        self.assertEqual(rows[2]["widened"]["rule2"], 2)
        refused = diagnostic.measure_notes(scenario, projection, None, deep)
        self.assertEqual(refused[2]["placement"], "widened_search_matched_unobserved")
        self.assertEqual(refused[3]["placement"], "search_unobserved")

    def test_exact_args_reuse_and_typed_scope_cap(self):
        caller = Mock(side_effect=[search_out(), cap_out("scope_reply_budget"), search_out([hit("deep.rs")])])
        engine = diagnostic.SearchReplay(object(), {"bookbuilder": TOKEN}, call_limit=3, timeout=10,
                                         wall_time=30, call=caller)
        record = {"arguments": ARGS}
        result = engine.pair("bookbuilder", record, 100, 65536)
        self.assertEqual(engine.pair("bookbuilder", record, 100, 65536), result)
        self.assertEqual(caller.call_count, 3)
        self.assertEqual([call.args[3] for call in caller.call_args_list], [
            {**ARGS, "verification": "off", "response_mode": "full"},
            {**ARGS, "verification": "off", "response_mode": "full", "limit": 100, "max_bytes": 65536},
            {**ARGS, "verification": "off", "response_mode": "full", "limit": 100, "max_bytes": 49152},
        ])
        self.assertEqual(len(result[3]["hits"]), 1)
        self.assertEqual(len(result[2]), 2)
        self.assertEqual(engine.attempts[result[2][0]]["error_code"], "scope_reply_budget")

    def test_other_error_does_not_retry_and_result_token_is_fenced(self):
        engine = diagnostic.SearchReplay(object(), {"bookbuilder": TOKEN}, call_limit=2, timeout=10,
                                         wall_time=30, call=Mock(side_effect=[search_out(), cap_out("read_failed")]))
        self.assertIsNone(engine.pair("bookbuilder", {"arguments": ARGS}, 100, 65536)[3])
        with self.assertRaisesRegex(diagnostic.common.HarnessError, "budget exhausted"):
            engine.one("bookbuilder", {**ARGS, "limit": 7})
        for bad in (search_out(token="changed"), search_out(focus={"applied": True})):
            engine = diagnostic.SearchReplay(object(), {"bookbuilder": TOKEN}, call_limit=3, timeout=10,
                                             wall_time=30, call=Mock(return_value=bad))
            with self.assertRaisesRegex(diagnostic.common.HarnessError, "publication.*Focus"):
                engine.one("bookbuilder", ARGS)

    def test_canonical_first_body_is_bound_to_retained_digest(self):
        body = {"schema": diagnostic.loop_reply.RESPONSE_SCHEMA, "records": [], "record_uses": []}
        receipt = json.dumps({"structuredContent": body}).encode()
        expected = diagnostic.evidence.evidence_digest(body)
        row = {"key": "bookbuilder-C1/focus-gpu"}
        with patch.object(diagnostic, "under", side_effect=lambda path, _root: path), \
                patch.object(Path, "read_bytes", return_value=receipt):
            _, pin = diagnostic.verified_first(Path("pass"), row, {}, {row["key"]: {"ok": True, "digest": expected}},
                                               "canonical")
            self.assertEqual(pin["raw_sha256"], diagnostic.common.sha256_bytes(receipt))
            with self.assertRaisesRegex(diagnostic.common.HarnessError, "digest differs"):
                diagnostic.verified_first(Path("pass"), row, {}, {row["key"]: {"ok": True, "digest": "wrong"}},
                                          "canonical")



    def test_selected_pass_writes_only_scoped_report_and_private_exact_attempts(self):
        root = Path.cwd()
        pass_dir, out, freeze_path = root / "pass-synthetic", root / "out-synthetic", root / "freeze.json"
        row = {"key": "bookbuilder-C1/focus-gpu", "repo": "bookbuilder", "slot": "C1", "arm": "F",
               "sample": 1, "path": "answer", "scenario_id": "synthetic"}
        index = {"schema": "airk.private.answer-eval-pass-index/1", "series": "synthetic",
                 "freeze": {"path": str(freeze_path), "sha256": "pin"}, "results": [row]}
        scenario_path = root / "scenarios/bookbuilder/C1.json"
        note = {"id": "n1", "mark": "essential", "path": "inside.rs", "lines": [2, 4]}
        scenario = {"id": "synthetic", "_sha256": "scenario-pin", "notes": [note]}
        body = {"schema": diagnostic.loop_reply.RESPONSE_SCHEMA,
                "scope": {"index_id": "index", "publication": {"token": TOKEN}},
                "records": [{"id": "record", "operation": "search", "arguments": ARGS,
                             "canonical_result": {"projection": "airk.arbiter.loop-evidence/1",
                                                  "hits_total": 1, "spans": []}}],
                "record_uses": [{"record": "record", "round": 1, "part": 1, "cause": "part_search"}]}
        status = {"repo": "bookbuilder", "publication_token": TOKEN, "settled": True}
        handle = Mock(sha256="pin")
        handle.index_id.return_value = "index"
        handle.live_stages.return_value = {"focus": None, "context": None}
        reads = {pass_dir / "pass-index.json": index, pass_dir / "digests.json": {},
                 freeze_path: {"files": {scenario_path.relative_to(root).as_posix(): "scenario-pin"}}}
        catalog = {"airk_search": {"inputSchema": {"properties": {
            "limit": {"maximum": 100}, "max_bytes": {"maximum": 65536}}}}}
        cli = ["--pass-dir", str(pass_dir), "--handle", str(root / "handle.json"), "--handle-sha256", "pin",
               "--catalog", str(root / "catalog.json"), "--catalog-sha256", "pin",
               "--out", str(out), "--repo", "bookbuilder", "--slot", "C1", "--arm", "F"]
        with patch.object(Path, "resolve", autospec=True, side_effect=lambda path, **_kw: path), \
                patch.object(Path, "is_dir", return_value=True), patch.object(Path, "iterdir", return_value=iter(())), \
                patch.object(diagnostic.common, "read_json", side_effect=reads.__getitem__), \
                patch.object(diagnostic.common, "sha256_file", return_value="pin"), \
                patch.object(diagnostic.common, "load_scenario", return_value=scenario), \
                patch.object(diagnostic.common, "load_handle", return_value=handle), \
                patch.object(diagnostic.common, "load_catalog", return_value=catalog), \
                patch.object(diagnostic.common, "call_tool", side_effect=[search_out(), search_out([hit("inside.rs")])]), \
                patch.object(diagnostic.common, "write_json") as write, \
                patch.object(diagnostic, "verified_first", return_value=(body, {"raw_sha256": "first"})), \
                patch.object(diagnostic.settled, "snapshot", return_value=status) as snapshot:
            self.assertEqual(diagnostic.main(cli), 0)
        self.assertEqual(snapshot.call_count, 2)
        files = {call.args[0].name: call.args[1] for call in write.call_args_list}
        rows = files["initial-search-depth.json"]["rows"]
        self.assertEqual(rows[0]["notes"][0]["placement"], "widened_search_only")
        self.assertNotIn(ARGS["query"], json.dumps(files["initial-search-depth.json"]))
        self.assertIn(ARGS["query"], json.dumps(files["queries.private.json"]))


    def test_actual_first_tool_turn_checks_scenario_and_raw_hash(self):
        body = {"schema": diagnostic.loop_reply.RESPONSE_SCHEMA, "records": [], "record_uses": []}
        receipt = json.dumps({"structuredContent": body}).encode()
        raw_sha = diagnostic.common.sha256_bytes(receipt)
        row = {"key": "bookbuilder-C1/focus-gpu", "repo": "bookbuilder", "slot": "C1",
               "scenario_id": "fixture", "session_dir": "session"}
        scenario = {"_sha256": "scenario-pin", "instruction": "sample question"}
        session = {"scenario_id": "fixture", "sample": 1, "scenario_sha256": "scenario-pin",
                   "repo": "bookbuilder", "slot": "C1", "path": "answer"}
        turn = {"turn": 1, "tool": "airk_answer", "ok": True, "rejected": False,
                "first_call": {"rule": "canonical"}, "sent_arguments": {"question": "sample question"},
                "result_path": "session/wrapper/first.stdout.json", "result_sha256": raw_sha}
        digests = {row["key"]: {"ok": True, "digest": "separate canonical pin"}}
        with patch.object(diagnostic, "under", side_effect=lambda path, _root: path), \
                patch.object(diagnostic.common, "read_json", return_value=session), \
                patch.object(diagnostic.common, "read_jsonl", return_value=[turn]), \
                patch.object(Path, "read_bytes", return_value=receipt):
            _, pin = diagnostic.verified_first(Path("pass"), row, scenario, digests, "actual")
            self.assertEqual(pin["raw_sha256"], raw_sha)
            turn["result_sha256"] = "tampered"
            with self.assertRaisesRegex(diagnostic.common.HarnessError, "receipt hash differs"):
                diagnostic.verified_first(Path("pass"), row, scenario, digests, "actual")

if __name__ == "__main__":
    unittest.main()
