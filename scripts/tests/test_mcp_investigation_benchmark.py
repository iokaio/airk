# SPDX-License-Identifier: Apache-2.0
"""Fixture identity, replay safety, counting units and concurrent wire accounting."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

SPEC = importlib.util.spec_from_file_location("mcp_investigation_benchmark", Path(__file__).parents[1] / "benchmark-mcp-investigation.py")
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)

SERVER = r'''
import json, os, sys, threading
lock = threading.Lock()
def respond(request):
    name = request.get("params", {}).get("name")
    if request["method"] == "initialize":
        result = {"serverInfo":{"version":"1.8.2"}}
    elif request["method"] == "tools/list":
        result = {"tools":[]}
    else:
        value = {"publication":{"token":"same-publication"},"index_state":"available",
                 "discovery_readiness":{"state":"ready"},"semantic_readiness":{"state":"ready"}}
        result = {"content":[{"type":"text","text":json.dumps(value)}]}
    with lock:
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":result}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    if os.environ.get("BENCH_CALL_LOG"):
        with lock:
            with open(os.environ["BENCH_CALL_LOG"],"a",encoding="utf-8") as log:
                log.write(json.dumps({"method":request["method"],"tool":request.get("params",{}).get("name")})+"\n")
    if "id" in request:
        # Deliberately allow replies in arbitrary order.
        threading.Thread(target=respond,args=(request,)).start()
'''


class InvestigationBenchmarkTests(unittest.TestCase):
    def test_original_attempt_rows_and_summary_are_compared_without_rewriting(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "record.md"
            text = "The cost was 2 MCP calls. Five calls were rejected as busy.\n| 1 | status | Busy: no |\n| 2 | status | OK |\n| 2 | symbols | OK |\n"
            path.write_text(text, encoding="utf-8")
            result = BENCH.reconcile_record(path)
            self.assertEqual((result["table_call_rows"],result["table_busy_rows"]),(3,1))
            self.assertEqual((result["summary_calls"],result["summary_busy"]),(2,5))
            self.assertFalse(result["counts_agree"])
            self.assertEqual(path.read_text(encoding="utf-8"),text)

    def test_fixture_frozen_oracle_counts_lines_separately_from_occurrences(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "fixture"
            created = BENCH.create_fixture(destination)
            root, manifest = BENCH.load_fixture(destination)
            self.assertEqual(created, manifest)
            self.assertEqual(len(manifest["oracle"]["definition_paths"]), 6)
            self.assertEqual(len(manifest["oracle"]["matching_lines"]), 24)
            self.assertEqual(manifest["oracle"]["identifier_occurrences"], 36)
            (root / "src/alpha.rs").write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "fixture_identity_mismatch"):
                BENCH.load_fixture(destination)

    def test_only_explicit_typed_unsubmitted_errors_authorize_retry(self):
        def envelope(value):
            return {"isError":True,"structuredContent":value}
        error = {"code":"direct_query_admission","query_submitted":False,"retryable":True,"retry_after_ms":25}
        self.assertEqual(BENCH.typed_retry(envelope({"error":error})), .025)
        for replacement in ({"query_submitted":True}, {"retry_after_ms":501}, {"retryable":False}):
            self.assertIsNone(BENCH.typed_retry(envelope({"error":{**error, **replacement}})))
        self.assertIsNone(BENCH.typed_retry({"isError":True,"content":[{"type":"text","text":"AIRK bridge is busy"}]}))
        self.assertIsNone(BENCH.typed_retry(envelope({"error":{"retryable":True,"retry_after_ms":25}})))

    def test_counts_and_errors_are_content_free_and_publication_is_hashed(self):
        value = {"isError":True,"structuredContent":{"error":{"code":"busy", "message":"private path and source"},
            "publication":{"token":"exact-opaque-token"}, "secret":"private body"}}
        observed = BENCH.observation(value)
        self.assertEqual(observed["code"], "busy")
        self.assertEqual(observed["publication_token_sha256"], BENCH.digest_bytes(b"exact-opaque-token"))
        serialized = json.dumps(observed)
        self.assertNotIn("private", serialized)
        self.assertNotIn("exact-opaque-token", serialized)
        self.assertIsNone(BENCH.observation({"structuredContent":{"freshness":None}})["freshness_state"])

    def test_scope_and_launcher_digest_checked_before_process_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / "source"
            source.mkdir()
            configuration = root / "config.json"
            value = {"command":str(Path(sys.executable).resolve()), "args":["--root",str(root / "other"),"mcp"],
                "launcher_sha256":BENCH.digest_bytes(Path(sys.executable).read_bytes()),
                "route":"installed-launcher", "expected_version":"1.8.2"}
            configuration.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "configuration_fixture_scope_mismatch"):
                BENCH.load_configuration(configuration, source)
            value["args"][1] = str(source)
            value["launcher_sha256"] = "0"*64
            configuration.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "launcher_identity_mismatch"):
                BENCH.load_configuration(configuration, source)

    def test_concurrent_reader_dispatch_keeps_wire_bytes_and_identity(self):
        from concurrent.futures import ThreadPoolExecutor
        with tempfile.TemporaryDirectory() as temporary:
            server = Path(temporary) / "fake.py"
            server.write_text(SERVER, encoding="utf-8")
            configuration = {"command":sys.executable,"args":[str(server)],"expected_version":"1.8.2"}
            with BENCH.Session(configuration, 5) as session:
                with ThreadPoolExecutor(max_workers=8) as executor:
                    results = list(executor.map(session.tool, [("status",{})]*16))
                self.assertEqual(len(results), 16)
                self.assertGreater(session.tx_bytes, 0)
                self.assertGreater(session.rx_bytes, 0)
                self.assertTrue(all(not result[0].get("isError") for result in results))
                self.assertTrue(all(result[1]["attempts"][0]["publication_token_sha256"] ==
                    BENCH.digest_bytes(b"same-publication") for result in results))
                with self.assertRaisesRegex(ValueError, "non_read_tool_refused"):
                    session.tool(("draft_search",{}))

    def test_quantiles_use_even_median_and_nearest_rank_p95(self):
        self.assertEqual(BENCH.distribution([4,1,3,2]), {"samples":4,"median":2.5,"p95":4})

    def test_compact_file_bound_definitions_count_as_first_useful_evidence(self):
        self.assertTrue(BENCH.has_evidence({"items":[{"name":"Session","file":0}],"files":[{"path":"src/alpha.rs","content_hash":"hash"}]}))
        self.assertTrue(BENCH.has_evidence({"minimal":{"items":[{"name":"Session"}],"files":[{"path":"src/alpha.rs"}]}}))
        self.assertFalse(BENCH.has_evidence({"items":[],"files":[{"path":"src/alpha.rs"}]}))
        self.assertFalse(BENCH.has_evidence({"index_state":"available"}))

    def test_matching_paths_and_lines_do_not_hide_a_stale_indexed_hash(self):
        oracle={"matching_lines":[["src/alpha.rs",3]],"definition_paths":["src/alpha.rs"]}
        symbols={"structuredContent":{"items":[{"name":"Session"}],"files":[{"path":"src/alpha.rs","content_hash":"expected"}]}}
        refs={"structuredContent":{"evidence":{"hits":[{"citation":{"path":"src/alpha.rs","start_line":3,"content_hash":"stale"}}],"truncated":False}}}
        # Deliberately reverse insertion order: coverage binds task identities,
        # never assumed response order or two ambiguous search tool names.
        results={"references":(refs,{}),"definitions":(symbols,{})}
        before=BENCH.coverage(results,oracle,{"src/alpha.rs":"expected"})
        self.assertFalse(before["complete"])
        self.assertEqual(before["source_hash_mismatches"],1)
        refs["structuredContent"]["evidence"]["hits"][0]["citation"]["content_hash"]="expected"
        self.assertTrue(BENCH.coverage(results,oracle,{"src/alpha.rs":"expected"})["complete"])

    def test_unprimed_connection_never_submits_hidden_status_before_the_batch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            server=root/"fake.py"
            server.write_text(SERVER,encoding="utf-8")
            for prime in [False,True]:
                log=root/f"calls-{prime}.jsonl"
                config={"command":sys.executable,"args":[str(server)],"expected_version":"1.8.2","env":{"BENCH_CALL_LOG":str(log)}}
                with BENCH.Session(config,5,prime_status=prime) as session:
                    observed=[json.loads(line) for line in log.read_text().splitlines()]
                    tools=[row["tool"] for row in observed if row["method"]=="tools/call"]
                    self.assertEqual(tools,["status"] if prime else [])
                    self.assertEqual(session.next_id,3 if prime else 2)
                    metadata=BENCH.connection_metadata(session,opened=True)
                    self.assertEqual(metadata["status_primed"],prime)
                    self.assertEqual(metadata["first_status"] is None,not prime)
                    self.assertEqual(metadata["initialize_through_status_ms"] is None,not prime)
                    self.assertEqual(metadata["first_status_available"],True if prime else None)
                self.assertTrue(session.cleanup["graceful_eof"])

    def test_report_separates_unprimed_nullable_startup_from_identical_fixed_batches(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner=Path(temporary).resolve()
            fixture=owner/"fixture"
            BENCH.create_fixture(fixture)
            source,manifest=BENCH.load_fixture(fixture)
            server=owner/"fake.py"
            server.write_text(SERVER,encoding="utf-8")
            log=owner/"calls.jsonl"
            config={"command":str(Path(sys.executable).resolve()),"args":[str(server),"--root",str(source),"mcp"],
                "expected_version":"1.8.2","launcher_sha256":BENCH.digest_bytes(Path(sys.executable).read_bytes()),
                "route":"isolated-approved-runtime","env":{"BENCH_CALL_LOG":str(log)}}
            path=owner/"launch.json"
            path.write_text(json.dumps(config),encoding="utf-8")
            args=SimpleNamespace(configuration=path,fixture=fixture,output=owner/"report",label="baseline",record=None,
                concurrency=[1],samples=2,timeout=5)
            # This fake transport intentionally returns no source evidence;
            # protocol/report checks must not turn that into coverage success.
            self.assertFalse(BENCH.run(args))
            report=json.loads((args.output/"report.json").read_text())
            self.assertEqual(report["status"],"complete")
            self.assertTrue(report["source_unchanged"])
            self.assertEqual(report["fixture_sha256"],manifest["fixture_sha256"])
            groups={group["connection"]:group for group in report["cohorts"]}
            self.assertEqual(set(groups),{row[0] for row in BENCH.COHORTS})
            expected=("definitions","references","literal-search","code-facts","outline","excerpt","context","batch-status","identifier-question")
            self.assertEqual(BENCH.TASK_IDS,expected)
            for cohort,prime,reuse in BENCH.COHORTS:
                group=groups[cohort]
                self.assertEqual(group["status_primed"],prime)
                self.assertEqual(group["connections_started"],1 if reuse else 2)
                self.assertEqual(group["first_status_connection_count"],(1 if reuse else 2) if prime else 0)
                for trial in group["trials"]:
                    self.assertEqual(tuple(call["task_id"] for call in trial["calls"]),expected)
                    self.assertEqual(trial["batch_status"]["task_id"],"batch-status")
                    self.assertGreaterEqual(trial["batch_status_from_session_start_ms"],0)
                    self.assertEqual(trial["initialize_through_status_ms"] is None,not prime)
                    self.assertEqual(trial["first_status"] is None,not prime)
                    self.assertEqual(trial["connection_opened"],not reuse)
                    self.assertGreaterEqual(trial["task_with_connection_ms"],trial["task_ms"])
                    cleanup=group.get("frontend_cleanup",trial.get("frontend_cleanup"))
                    self.assertTrue(cleanup["graceful_eof"])
                    self.assertFalse(cleanup["forced_owned_frontend_reap"])
            reused=groups["reused-connection-status-first"]
            self.assertTrue(reused["connection_start"]["status_primed"])
            self.assertEqual(tuple(call["task_id"] for call in reused["warmup"]["calls"]),expected)
            observed=[json.loads(line) for line in log.read_text().splitlines()]
            self.assertEqual(sum(row["tool"]=="status" for row in observed),10,
                "three explicit status primes plus seven identical fixed batches (including one retained warmup)")


if __name__ == "__main__":
    unittest.main()
