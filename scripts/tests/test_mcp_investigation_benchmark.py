# SPDX-License-Identifier: Apache-2.0
"""Fixture identity, replay safety, counting units and concurrent wire accounting."""
import importlib.util
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

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

PAIRED_SERVER = r'''
import json, os, sys
calls = 0
for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    if request["method"] == "initialize":
        result = {"serverInfo":{"version":"1.8.2"}}
    elif request["method"] == "tools/list":
        result = {"tools":[]}
    else:
        calls += 1
        if os.environ.get("BENCH_BUSY_FIRST") == "1" and calls == 1:
            result = {"isError":True,"structuredContent":{"error":{"code":"fixture_busy","query_submitted":False,"retryable":True,"retry_after_ms":0}}}
        else:
            result = json.loads(os.environ["BENCH_REPLY"])
    print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":result}),flush=True)
'''


class InvestigationBenchmarkTests(unittest.TestCase):
    def test_http_cleanup_failure_retains_original_and_complete_secondary_evidence(self):
        session = BENCH.Session({"transport": "https"}, 5)
        primary = BENCH.http_fixture.HttpResponseError(200, "bounded prefix", submission="uncertain")
        secondary = BENCH.http_fixture.HttpRefused(503, {"schema": "airk.local-api-error/1", "error": {"code": "native_delivery_fenced"}},
                                                  headers=[["X-Future", "complete"]], request={"http_method": "DELETE"})
        session.networks = [mock.Mock()]
        session.networks[0].close.side_effect = secondary
        self.assertFalse(session.__exit__(type(primary), primary, None))
        self.assertEqual(session.cleanup["errors"][0]["http_response"], secondary.evidence())
        self.assertTrue(any("cleanup also failed" in note for note in primary.__notes__))

    def test_modern_tool_hint_is_observed_once_without_harness_retry(self):
        session = BENCH.Session({"transport": "https"}, 5)
        envelope = {"isError": True, "structuredContent": {"error": {
            "code": "selection_busy", "query_submitted": False, "retryable": True, "retry_after_ms": 50}}}
        session.request = mock.Mock(return_value=(envelope, {"elapsed_ms": 1, "completed_at": BENCH.time.perf_counter()}))
        with mock.patch.object(BENCH.time, "sleep", side_effect=AssertionError("no harness retry")):
            result, metadata = session.tool(("search", {"query": "original"}))
        self.assertIs(result, envelope)
        self.assertEqual(metadata["retries"], 0)
        session.request.assert_called_once()

    def test_http_failures_keep_full_redacted_evidence_and_never_replay_uncertain(self):
        for code, status, outcome in (("authentication_busy", 503, "refused"),
                                      ("native_delivery_fenced", 503, "failed"),
                                      ("response_limit", 200, "uncertain")):
            body = {"schema": "airk.local-api-error/1", "error": {"code": code}, "future": {"retained": [1]}}
            error_type = BENCH.http_fixture.HttpResponseError if outcome == "uncertain" else BENCH.http_fixture.HttpRefused
            error = error_type(status, body, headers=[["X-Future", "one"], ["X-Future", "two"]],
                               request={"jsonrpc_id": 1, "connection_id": "owned"},
                               raw_body=json.dumps(body).encode(), header_redactions=["Mcp-Session-Id"],
                               body_redacted=True, submission="uncertain" if outcome == "uncertain" else "observed")
            session = BENCH.RetainedSession({"transport": "https"}, 5)
            session.local.network = network = mock.Mock()
            session.networks = [network]
            network._request.side_effect = error
            with self.subTest(code=code), self.assertRaises(error_type) as caught:
                session.request("tools/call", {"name": "search", "arguments": {}})
            self.assertIs(caught.exception, error)
            self.assertEqual(session.query_replies[0]["http_response"], error.evidence())
            self.assertEqual(session.query_replies[0]["outcome"], outcome)
            if outcome == "uncertain":
                with self.assertRaisesRegex(RuntimeError, "transport_uncertain"):
                    session.request("tools/call", {"name": "search", "arguments": {}})
                network._request.assert_called_once()
            session.__exit__(None, None, None)
            network.close.assert_called_once_with(delete=outcome != "uncertain")

    def test_all_refused_trial_accounts_every_call_and_leaves_coverage_unobserved(self):
        error = BENCH.http_fixture.HttpRefused(503, {"schema": "airk.local-api-error/1", "error": {"code": "authentication_busy"}})
        session = SimpleNamespace(tool=mock.Mock(side_effect=error), started=BENCH.time.perf_counter())
        result = BENCH.run_trial(session, 4, {}, {})
        selected = len(BENCH.task_workload())
        self.assertEqual(session.tool.call_count, selected)
        self.assertEqual(result["accounting"]["refused"], selected)
        self.assertEqual(result["accounting"]["succeeded"], 0)
        self.assertTrue(result["accounting"]["passed"])
        self.assertFalse(result["coverage"]["available"])
        self.assertFalse(result["coverage"]["complete"])
        self.assertTrue(all(call["retries"] == 0 and call["submission"] == "not_submitted" for call in result["calls"]))

    def test_partial_combined_group_retains_failed_and_unexecuted_samples(self):
        group = {"trials": [{"sample": 0, "validated": False, "outcome": "uncertain"}],
                 "frontend_cleanup": {"frontend_process_created": False, "http_session_delete": False}}
        BENCH.combined_phase_evidence(group, 3)
        self.assertEqual(group["accounting"]["uncertain"], 1)
        self.assertEqual(group["accounting"]["not_executed"], 2)
        self.assertEqual(group["assertion_coverage"]["unverified_samples"], [0, 1, 2])
        self.assertFalse(group["qualified"])

    def legacy_metadata_fixture(self, temporary):
        owner = Path(temporary).resolve()
        control, source = owner / "control", owner / "source"
        control.mkdir()
        source.mkdir()
        namespace = "01" * 16
        configuration = {"command": str(Path(sys.executable).resolve()),
                         "args": ["--service-runtime-dir", str(control), "--combined-session", json.dumps({"namespace": namespace}), "mcp"],
                         "route": "isolated-approved-runtime", "expected_version": "fixture"}
        case = {"index_ids": ["idx_" + "02" * 16]}
        registration = {"binding": "b" * 64, "root": str(source), "data_root": str(owner / "data"), "collection": "original-fixture"}
        epoch = {"status": "request_epoch", "namespace": namespace, "epoch": 0}
        eligibility = {"status": "catalog_legacy_binding", "observation": {"schema": 1, "namespace": namespace,
                        "project": registration["binding"], "eligibility": {"kind": "imported_snapshot", "registration": registration}}}
        return configuration, case, source, registration, epoch, eligibility

    def metadata_response(self, value):
        return SimpleNamespace(returncode=0, stdout=json.dumps(value).encode())

    def test_owned_legacy_lookup_requires_opt_in_and_never_uses_a_global_service(self):
        with tempfile.TemporaryDirectory() as temporary:
            configuration, case, source, *_ = self.legacy_metadata_fixture(temporary)
            with mock.patch.dict(BENCH.os.environ, {BENCH.LEGACY_COMPARE_ENV: "0"}), mock.patch.object(BENCH.subprocess, "run") as run:
                with self.assertRaisesRegex(BENCH.LegacyComparisonError, "legacy_comparison_not_authorized"):
                    BENCH.owned_legacy_configuration(configuration, case, source, 5)
                run.assert_not_called()
            configuration["args"] = ["--combined-session", "{}", "mcp"]
            with mock.patch.dict(BENCH.os.environ, {BENCH.LEGACY_COMPARE_ENV: "1"}), mock.patch.object(BENCH.subprocess, "run") as run:
                with self.assertRaisesRegex(BENCH.LegacyComparisonError, "explicit_owned_control_required"):
                    BENCH.owned_legacy_configuration(configuration, case, source, 5)
                run.assert_not_called()

    def test_owned_legacy_lookup_uses_exact_native_pages_and_unique_explicit_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            configuration, case, source, registration, epoch, eligibility = self.legacy_metadata_fixture(temporary)
            unselected = {**registration, "binding": "a" * 64, "root": str(source.parent / "unselected-never-opened")}
            responses = [epoch,
                         {"status": "projects", "page": {"revision": 7, "projects": [{"registration": unselected}], "after": unselected["binding"]}},
                         {"status": "projects", "page": {"revision": 7, "projects": [{"registration": registration}], "after": None}},
                         eligibility, epoch]
            with mock.patch.dict(BENCH.os.environ, {BENCH.LEGACY_COMPARE_ENV: "1"}), mock.patch.object(BENCH.subprocess, "run", side_effect=[self.metadata_response(row) for row in responses]) as run:
                legacy, evidence = BENCH.owned_legacy_configuration(configuration, case, source, 5)
            self.assertEqual(legacy["args"], ["--service-runtime-dir", str(source.parent / "control"), "--service-project", registration["binding"], "mcp"])
            methods = [json.loads(call.kwargs["input"]) for call in run.call_args_list]
            self.assertEqual(methods[2], {"projects": {"after": unselected["binding"], "revision": 7, "limit": 32}})
            self.assertEqual(methods[3], {"catalog_legacy_binding": {"project": registration["binding"], "expected_catalog_revision": None}})
            self.assertTrue(all(call.args[0][1:3] == ["service", "rpc"] for call in run.call_args_list))
            self.assertEqual(evidence["metadata_requests"], 5)
            self.assertEqual(evidence["legacy_eligibility"], "imported_snapshot")
            self.assertNotIn(str(source), json.dumps(evidence))

    def test_owned_legacy_lookup_refuses_missing_ambiguous_changed_or_unimported_authority(self):
        for condition, code in [("missing", "legacy_registration_missing"), ("ambiguous", "legacy_registration_ambiguous"),
                                ("changed", "legacy_metadata_authority_changed"), ("unimported", "legacy_compatibility_snapshot_unavailable")]:
            with self.subTest(condition=condition), tempfile.TemporaryDirectory() as temporary:
                configuration, case, source, registration, epoch, eligibility = self.legacy_metadata_fixture(temporary)
                rows = [] if condition == "missing" else [{"registration": registration}]
                if condition == "ambiguous":
                    rows.append({"registration": {**registration, "binding": "c" * 64}})
                if condition == "unimported":
                    eligibility["observation"]["eligibility"] = {"kind": "unavailable"}
                final_epoch = {**epoch, "namespace": "ff" * 16} if condition == "changed" else epoch
                responses = [epoch, {"status": "projects", "page": {"revision": 7, "projects": rows, "after": None}}, eligibility, final_epoch]
                with mock.patch.dict(BENCH.os.environ, {BENCH.LEGACY_COMPARE_ENV: "1"}), mock.patch.object(BENCH.subprocess, "run", side_effect=[self.metadata_response(row) for row in responses]):
                    with self.assertRaisesRegex(BENCH.LegacyComparisonError, code):
                        BENCH.owned_legacy_configuration(configuration, case, source, 5)

    def test_one_index_comparison_preserves_full_hits_and_does_not_claim_body_equivalence(self):
        envelope, case, _ = self.combined()
        member = envelope["structuredContent"]["members"][0]["result"]["structuredContent"]
        member["publication"] = {"token": "exact-private-token"}
        member["scope_observation"] = {"qualifier": "selected_subset"}
        legacy = {"structuredContent": {"hits": copy.deepcopy(member["hits"]), "publication": member["publication"],
                                        "semantic_readiness": {"state": "disabled"}}}
        original = copy.deepcopy([envelope, legacy])
        self.assertTrue(BENCH.compare_one_index(envelope, legacy, case)["full_hit_records_equal"])
        self.assertEqual([envelope, legacy], original)
        legacy["structuredContent"]["hits"][0]["text"] = "different text"
        with self.assertRaisesRegex(BENCH.LegacyComparisonError, "one_index_hit_records_differ"):
            BENCH.compare_one_index(envelope, legacy, case)
        legacy["structuredContent"]["hits"] = copy.deepcopy(member["hits"])
        legacy["structuredContent"]["publication"] = {"token": "changed"}
        with self.assertRaisesRegex(BENCH.LegacyComparisonError, "one_index_publication_differ"):
            BENCH.compare_one_index(envelope, legacy, case)

    def test_one_index_comparison_materializes_hidden_header_publication_before_matching(self):
        envelope, case, _ = self.combined()
        combined = envelope["structuredContent"]
        hits = combined["members"][0]["result"]["structuredContent"]["hits"]
        template = {"fields": {"publication": {"member": None}},
                    "slots": [{"path": ["publication", "member"], "kind": "integer"}]}
        identity = BENCH.combined_evidence.framed_digest("airk/shared-native-header-template/1", template)
        combined["shared_values"] = {"schema": BENCH.combined_evidence.HEADER_SCHEMA, "values": {identity: template}}
        combined["members"][0]["result"] = {"hits": hits, BENCH.combined_evidence.HEADER_REFERENCE: {"id": identity, "values": [0]}}
        legacy = {"structuredContent": {"hits": copy.deepcopy(hits), "publication": {"token": "exact-private-token"}}}
        raw = copy.deepcopy(envelope)
        self.assertTrue(BENCH.compare_one_index(envelope, legacy, case)["full_hit_records_equal"])
        self.assertEqual(envelope, raw)
        combined["members"][0]["result"][BENCH.combined_evidence.HEADER_REFERENCE]["values"] = [True]
        with self.assertRaisesRegex(ValueError, "combined_header_slot_kind"):
            BENCH.compare_one_index(envelope, legacy, case)

    def test_opted_in_paired_protocol_retains_every_raw_attempt_and_fails_on_changed_hit(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary)
            source = owner / "source"
            (source / "src").mkdir(parents=True)
            (source / "src/shared.rs").write_text("needle\n", encoding="utf-8")
            inventory = BENCH.combined_source_inventory(source)
            envelope, case, _ = self.combined()
            case["arguments"].update(query="needle", mode="lexical", path_prefix="src/", verification="off")
            member = envelope["structuredContent"]["members"][0]["result"]["structuredContent"]
            member["hits"][0]["citation"]["content_hash"] = inventory[0]["sha256"]
            member["publication"] = {"token": "exact-private-token"}
            legacy = {"structuredContent": copy.deepcopy(member)}
            legacy["structuredContent"]["semantic_readiness"] = {"state": "disabled"}
            server = owner / "paired.py"
            server.write_text(PAIRED_SERVER, encoding="utf-8")
            configuration = {"command": str(Path(sys.executable).resolve()),
                             "args": [str(server), "--combined-session", json.dumps({"project_ids": case["project_ids"]}), "mcp"],
                             "launcher_sha256": BENCH.digest_bytes(Path(sys.executable).read_bytes()),
                             "route": "isolated-approved-runtime", "expected_version": "1.8.2", "env": {"BENCH_REPLY": json.dumps(envelope)}}
            legacy_configuration = {**configuration, "args": [str(server), "--service-project", "b" * 64, "mcp"],
                                    "env": {"BENCH_REPLY": json.dumps(legacy), "BENCH_BUSY_FIRST": "1"}}
            config_path, scenario_path = owner / "configuration.json", owner / "scenario.json"
            config_path.write_text(json.dumps(configuration), encoding="utf-8")
            scenario_path.write_text(json.dumps({"schema": BENCH.COMBINED_SCHEMA,
                "indexes": [{"index_id": case["index_ids"][0], "source_root": str(source), "inventory": inventory}], "cases": [case]}), encoding="utf-8")
            args = SimpleNamespace(configuration=config_path, scenario=scenario_path, output=owner / "matched", samples=2, fanouts=[1], timeout=5)
            with mock.patch.dict(BENCH.os.environ, {BENCH.LEGACY_COMPARE_ENV: "1"}), mock.patch.object(BENCH, "owned_legacy_configuration", return_value=(legacy_configuration, {"unique_explicit_root_match": True})):
                self.assertTrue(BENCH.combined(args))
            report = json.loads((args.output / "report.json").read_text())
            comparison = report["legacy_one_index"]
            self.assertTrue(comparison["passed"] and comparison["source_unchanged"])
            self.assertEqual(comparison["errors"], 1)
            self.assertEqual(comparison["retries"], 1)
            self.assertEqual(comparison["legacy_latency_ms"]["samples"], 2)
            self.assertNotIn("exact-private-token", json.dumps(report))
            raw = [json.loads(line) for line in (args.output / comparison["raw_outcomes"]).read_text().splitlines()]
            self.assertEqual(len(raw), 5, "both complete responses plus the complete busy retry remain available")
            self.assertEqual([row["attempt"] for row in raw if row["route"] == "legacy" and row["sample"] == 0], [1, 2])
            self.assertIn("exact-private-token", json.dumps(raw))
            legacy["structuredContent"]["hits"][0]["text"] = "changed native hit text"
            legacy_configuration["env"] = {"BENCH_REPLY": json.dumps(legacy)}
            args.output = owner / "mismatched"
            with mock.patch.dict(BENCH.os.environ, {BENCH.LEGACY_COMPARE_ENV: "1"}), mock.patch.object(BENCH, "owned_legacy_configuration", return_value=(legacy_configuration, {})):
                self.assertFalse(BENCH.combined(args))
            failed = json.loads((args.output / "report.json").read_text())
            self.assertEqual(failed["failure_code"], "one_index_hit_records_differ")
            self.assertFalse(failed["legacy_one_index"]["passed"])
            self.assertTrue(failed["source_unchanged"])
            self.assertTrue(failed["legacy_one_index"]["frontend_cleanup"]["graceful_eof"])

    def combined(self):
        index = "idx_" + "01" * 16
        project = "prj_" + "02" * 16
        source = "src_" + "03" * 16
        hit = {"citation": {"path": "src/shared.rs", "content_hash": "a" * 64}, "text": "needle"}
        result = {"schema": "airk.combined-read/1", "members": [{"index_id": index, "outcome": "complete",
            "allocation": {"limit": 2, "max_bytes": 100, "verification_max_sources": 0,
                           "verification_max_bytes": 0, "verification_deadline_ms": 0},
            "result": {"structuredContent": {"hits": [hit]}}}],
            "ranking": [{"index_id": index, "result_pointer": "/members/0/result/structuredContent/hits/0",
                "origin": {"index_id": index, "project_ids": [project], "source_root_id": source}}],
            "publication_vector": {"members": [{"index_id": index, "publication": {"token": "exact-private-token"}}]},
            "partial": False, "truncated": False}
        result["ranking"][0]["origin"]["publication"] = {"token": "exact-private-token"}
        case = {"index_ids": [index], "project_ids": [project], "arguments": {"limit": 2, "max_bytes": 100},
                "expected": [{"index_id": index, "path": "src/shared.rs", "project_ids": [project], "source_root_id": source}]}
        return {"structuredContent": result}, case, {(index, "src/shared.rs"): "a" * 64}

    def test_combined_observation_retains_identity_proof_without_private_tokens_or_paths(self):
        envelope, case, hashes = self.combined()
        observed = BENCH.combined_observation(envelope, case, hashes)
        self.assertEqual(observed["selected_indexes"], 1)
        self.assertTrue(observed["source_hashes_and_attribution_match"])
        self.assertIsNone(observed["unselected_native_work"], "response membership cannot prove zero worker access")
        self.assertNotIn("exact-private-token", json.dumps(observed))
        self.assertNotIn("src/shared.rs", json.dumps(observed))

    def test_combined_observation_resolves_publication_refs_without_rewriting_raw(self):
        envelope, case, hashes = self.combined()
        origin = envelope["structuredContent"]["ranking"][0]["origin"]
        origin.pop("publication")
        origin["publication_ref"] = {"member": 0}
        raw = copy.deepcopy(envelope)
        self.assertTrue(BENCH.combined_observation(envelope, case, hashes)["source_hashes_and_attribution_match"])
        self.assertEqual(envelope, raw)
        for reference in ({"member": True}, {"member": 1}, {"member": 0, "extra": 1}):
            origin["publication_ref"] = reference
            with self.assertRaisesRegex(ValueError, "combined_origin_or_hash_mismatch"):
                BENCH.combined_observation(envelope, case, hashes)

    def test_combined_observation_rejects_duplicate_or_unselected_members(self):
        envelope, case, hashes = self.combined()
        envelope["structuredContent"]["members"] *= 2
        with self.assertRaisesRegex(ValueError, "combined_member_selection_mismatch"):
            BENCH.combined_observation(envelope, case, hashes)

    def test_combined_observation_rejects_wrong_shared_project_attribution(self):
        envelope, case, hashes = self.combined()
        envelope["structuredContent"]["ranking"][0]["origin"]["project_ids"] += ["prj_" + "04" * 16]
        with self.assertRaisesRegex(ValueError, "combined_origin_or_hash_mismatch"):
            BENCH.combined_observation(envelope, case, hashes)

    def test_combined_observation_preserves_separate_index_origins_and_hash_validation(self):
        envelope, case, hashes = self.combined()
        envelope["structuredContent"]["members"][0]["result"]["structuredContent"]["hits"][0]["citation"]["content_hash"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "combined_origin_or_hash_mismatch"):
            BENCH.combined_observation(envelope, case, hashes)

    def test_combined_observation_detects_multiplied_total_budgets(self):
        envelope, case, hashes = self.combined()
        envelope["structuredContent"]["members"][0]["allocation"]["verification_max_sources"] = 9
        with self.assertRaisesRegex(ValueError, "combined_budget_multiplied"):
            BENCH.combined_observation(envelope, case, hashes)

    def test_combined_fixture_snapshot_refuses_capacity_excess_before_reading_content(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "oversized.txt"
            with path.open("wb") as source:
                source.truncate(16 * 1024 * 1024 + 1)
            with self.assertRaisesRegex(ValueError, "combined_fixture_size_limit"):
                BENCH.combined_source_inventory(root)

    def test_combined_configuration_rejects_implicit_or_mixed_legacy_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "config.json"
            value = {"command": str(Path(sys.executable).resolve()), "args": ["--root", temporary, "mcp"],
                     "launcher_sha256": BENCH.digest_bytes(Path(sys.executable).read_bytes()),
                     "route": "isolated-approved-runtime", "expected_version": "fixture"}
            path.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "explicit_combined_scope_required"):
                BENCH.load_combined_configuration(path)
            session = json.dumps({"project_ids": ["prj_" + "02" * 16]})
            value["args"] = ["--combined-session", session, "mcp"]
            path.write_text(json.dumps(value), encoding="utf-8")
            accepted, _ = BENCH.load_combined_configuration(path)
            self.assertEqual(accepted, value)
            value["args"] += ["--additional-root", temporary, "mcp"]
            path.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "explicit_combined_scope_required"):
                BENCH.load_combined_configuration(path)

    def test_combined_replay_keeps_failed_attempt_and_actual_frontend_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary)
            fixture = owner / "fixture"
            BENCH.create_fixture(fixture)
            root, manifest = BENCH.load_fixture(fixture)
            server = owner / "fake.py"
            server.write_text(SERVER, encoding="utf-8")
            index = "idx_" + "01" * 16
            project = "prj_" + "02" * 16
            config = {"command": str(Path(sys.executable).resolve()),
                      "args": [str(server), "--combined-session", json.dumps({"project_ids": [project]}), "mcp"],
                      "launcher_sha256": BENCH.digest_bytes(Path(sys.executable).read_bytes()),
                      "route": "isolated-approved-runtime", "expected_version": "1.8.2"}
            config_path = owner / "configuration.json"
            config_path.write_text(json.dumps(config), encoding="utf-8")
            scenario = {"schema": BENCH.COMBINED_SCHEMA,
                        "indexes": [{"index_id": index, "source_root": str(root), "inventory": manifest["inventory"]}],
                        "cases": [{"index_ids": [index], "project_ids": [project],
                                   "arguments": {"query": "Session", "mode": "lexical", "limit": 1, "max_bytes": 100},
                                   "expected": [{"index_id": index, "path": "src/alpha.rs", "source_root_id": "src_" + "03" * 16,
                                                 "project_ids": [project]}]}]}
            scenario_path = owner / "scenario.json"
            scenario_path.write_text(json.dumps(scenario), encoding="utf-8")
            args = SimpleNamespace(configuration=config_path, scenario=scenario_path, output=owner / "output",
                                   samples=1, fanouts=[1], timeout=5)
            with mock.patch.dict(BENCH.os.environ, {BENCH.COMBINED_RAW_ENV: "0", BENCH.LEGACY_COMPARE_ENV: "0"}):
                self.assertFalse(BENCH.combined(args), "a successful protocol reply without combined evidence is a failed measurement")
            report = json.loads((args.output / "report.json").read_text())
            trial = report["cases"][0]["trials"][0]
            self.assertEqual(trial["failure_class"], "ValueError")
            self.assertFalse(trial["validated"])
            self.assertEqual(len(trial["attempts"]), 1)
            self.assertGreater(trial["response_bytes"], 0)
            self.assertTrue(report["cases"][0]["frontend_cleanup"]["graceful_eof"])
            self.assertTrue(report["source_unchanged"])

            class RefusedSession:
                def __init__(self, *args, **kwargs):
                    self.cleanup = {"frontend_process_created": False, "http_session_delete": True}
                    self.calls = 0
                def __enter__(self): return self
                def __exit__(self, *_): return False
                def tool(self, *args):
                    self.calls += 1
                    raise BENCH.http_fixture.HttpRefused(503, {"schema": "airk.local-api-error/1", "error": {"code": "authentication_busy"}})
            args.output, args.samples = owner / "refused-output", 2
            with mock.patch.dict(BENCH.os.environ, {BENCH.COMBINED_RAW_ENV: "0", BENCH.LEGACY_COMPARE_ENV: "0"}), \
                    mock.patch.object(BENCH, "Session", RefusedSession):
                self.assertFalse(BENCH.combined(args), "unobserved oracle validation cannot qualify")
            report = json.loads((args.output / "report.json").read_text())
            group = report["cases"][0]
            self.assertEqual(len(group["trials"]), 2, "documented refusal does not stop later samples")
            self.assertEqual(group["accounting"]["refused"], 2)
            self.assertTrue(group["accounting"]["passed"])
            self.assertFalse(group["qualified"])
            self.assertEqual(group["assertion_coverage"]["unverified_samples"], [0, 1])
            self.assertNotIn("raw_outcomes", report)
            self.assertFalse((args.output / "raw.jsonl").exists())
            # A typed native size failure remains an exact full raw outcome;
            # the positive evidence oracle and its original budgets still fail.
            failed_envelope = {"isError": True, "structuredContent": {"error": {"code": "combined_reply_budget", "retryable": False},
                "future_error_observation": {"complete_budget_data": [1, 2, 4, 32]}}}
            server.write_text(PAIRED_SERVER, encoding="utf-8")
            config["env"] = {"BENCH_REPLY": json.dumps(failed_envelope)}
            config_path.write_text(json.dumps(config), encoding="utf-8")
            args.output, args.samples = owner / "raw-output", 1
            with mock.patch.dict(BENCH.os.environ, {BENCH.COMBINED_RAW_ENV: "1", BENCH.LEGACY_COMPARE_ENV: "0"}):
                self.assertFalse(BENCH.combined(args))
            report = json.loads((args.output / "report.json").read_text())
            raw = [json.loads(line) for line in (args.output / "raw.jsonl").read_text().splitlines()]
            self.assertEqual(len(raw), 1)
            self.assertEqual(raw[0]["envelope"], failed_envelope)
            self.assertEqual(raw[0]["selected_indexes"], 1)
            self.assertFalse(report["cases"][0]["trials"][0]["validated"])
            self.assertTrue(report["cases"][0]["trials"][0]["observation"]["is_error"])
            self.assertTrue(report["source_unchanged"])

    def test_raw_retention_keeps_a_received_jsonrpc_failure_without_fabricating_a_result(self):
        error = RuntimeError("protocol_error")
        error.raw_response = {"jsonrpc": "2.0", "id": 7, "error": {"code": -32000, "data": {"future": True}}}
        session = BENCH.RetainedSession({}, 5)
        with mock.patch.object(BENCH.Session, "request", side_effect=error):
            with self.assertRaisesRegex(RuntimeError, "protocol_error"):
                session.request("tools/call", {"name": "search"})
        retained = io.StringIO()
        BENCH.retain_query_replies(retained, session, 0, "combined", selected_indexes=32)
        row = json.loads(retained.getvalue())
        self.assertEqual(row["envelope"], {"jsonrpc_response": error.raw_response})
        self.assertEqual(row["selected_indexes"], 32)
        self.assertEqual(session.query_replies, [])

    def phase_report(self):
        return {"schema": BENCH.SCHEMA, "status": "complete", "source_unchanged": True,
                "route": "isolated-approved-runtime",
                "fixture_sha256": "5c0ddbe2197afd930d64d6f3dbef844e3247fb52ebb2b35175ca9597397da256",
                "cohorts": [{"connection": name, "concurrency": 1, "coverage_complete": True,
                    "cleanup_clean": True, "task_ms": {"median": 0, "p95": 0},
                    "trials": [{"coverage": {"complete": True}, "publication_consistent": True,
                        "task_ms": 100, "task_with_connection_ms": 120,
                        "calls": [{"retries": 0, "attempts": [{"is_error": False, "source_verification": None}]}
                                  for _ in range(9)]} for _ in range(30)]}
                    for name, _, _ in BENCH.COHORTS]}

    def test_phase_gate_input_recomputes_latency_and_preserves_fixed_sample_requirements(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "report.json"
            report = self.phase_report()
            path.write_text(json.dumps(report), encoding="utf-8")
            groups, counts = BENCH.phase_measurement(path, "off")
            self.assertEqual(next(iter(groups.values()))["task_ms"]["median"], 100,
                             "an edited summary must not replace retained trial timings")
            self.assertEqual(counts["calls"], 810)
            report["cohorts"][0]["trials"].pop()
            path.write_text(json.dumps(report), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "phase_sample_or_coverage_contract"):
                BENCH.phase_measurement(path, "off")

    def test_phase_gate_inputs_reject_retry_or_off_source_checks(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "report.json"
            for mutation, expected in (({"retries": 1}, "phase_error_or_retry"),
                                       ({"attempts": [{"is_error": False, "source_verification": {"mode": "report"}}]}, "phase_off_source_checks")):
                report = self.phase_report()
                report["cohorts"][0]["trials"][0]["calls"][0].update(mutation)
                path.write_text(json.dumps(report), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, expected):
                    BENCH.phase_measurement(path, "off")

    def test_phase_compare_keeps_unchanged_absolute_and_relative_allowances(self):
        def groups(median, p95):
            return {"cohort": {metric: {"median": median, "p95": p95}
                               for metric in ("task_ms", "task_with_connection_ms")}}
        gates = BENCH.phase_compare(groups(125, 150), groups(100, 100), "control")
        self.assertTrue(all(gate["passed"] for gate in gates))
        gates = BENCH.phase_compare(groups(125.01, 150.01), groups(100, 100), "control")
        self.assertFalse(any(gate["passed"] for gate in gates))
        gates = BENCH.phase_compare(groups(1100, 1150), groups(1000, 1000), "control")
        self.assertTrue(all(gate["passed"] for gate in gates))
        self.assertEqual({gate["bound"] for gate in gates}, {1100, 1150})

    def test_verification_observations_keep_cost_without_source_locations_or_prose(self):
        receipt = {"schema": "airk.saved-source-verification/1", "mode": "report", "status": "partial",
                   "counts": {"bytes_read": 4096, "matched_sources": 1, "requested_sources": None},
                   "sources": [{"path": "private-source.txt", "reason": "private prose"}]}
        observed = BENCH.observation({"structuredContent": {"source_verification": receipt}})["source_verification"]
        self.assertEqual(observed["counts"]["bytes_read"], 4096)
        self.assertEqual(observed["counts"]["matched_sources"], 1)
        self.assertIsNone(observed["counts"]["requested_sources"])
        self.assertNotIn("private", json.dumps(observed))

    def test_verification_override_preserves_workload_and_metadata_arguments(self):
        session = BENCH.Session({"verification_override": "off"}, 5)
        observed = []
        def request(method, params, deadline):
            observed.append(params)
            return {"content": [{"type": "text", "text": "{}"}]}, {}
        session.request = request
        arguments = {"query": "Session"}
        session.tool(("search", arguments))
        session.tool(("status", {}))
        self.assertEqual(observed[0]["arguments"], {"query": "Session", "verification": "off"})
        self.assertEqual(observed[1]["arguments"], {})
        self.assertEqual(arguments, {"query": "Session"})

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

    @unittest.skipUnless(sys.platform == "win32", "Windows extended native path identity")
    def test_native_extended_root_matches_only_the_same_frozen_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary).resolve() / "source"
            source.mkdir()
            configuration = source.parent / "config.json"
            value = {"command": str(Path(sys.executable).resolve()),
                     "args": ["--root", "\\\\?\\" + str(source), "mcp"],
                     "launcher_sha256": BENCH.digest_bytes(Path(sys.executable).read_bytes()),
                     "route": "isolated-approved-runtime", "expected_version": "fixture"}
            configuration.write_text(json.dumps(value), encoding="utf-8")
            accepted, _ = BENCH.load_configuration(configuration, source)
            self.assertEqual(accepted, value)
            other = source.parent / "other"
            other.mkdir()
            with self.assertRaisesRegex(ValueError, "configuration_fixture_scope_mismatch"):
                BENCH.load_configuration(configuration, other)

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
