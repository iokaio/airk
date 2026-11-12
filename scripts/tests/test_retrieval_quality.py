# SPDX-License-Identifier: Apache-2.0
"""Retrieval qualification rejects false-positive rankings and identities."""
import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock
from types import SimpleNamespace
import time
import threading


SPEC = importlib.util.spec_from_file_location(
    "retrieval_quality", Path(__file__).resolve().parents[1] / "check-retrieval-quality.py")
QUALITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QUALITY)


class RetrievalQualityTests(unittest.TestCase):
    def test_additional_root_requires_native_review_and_updated_grant_without_legacy_fallback(self):
        selected = {"grant_id": "original", "index_id": "same-index"}
        service = SimpleNamespace(single_image=True, catalog_authority=True, scopes={"qualification": selected}, configure=Mock())
        complete = {"completion": {"catalog_revision": 7}, "future_evidence": {"retained": True}}
        def apply(change):
            self.assertEqual(change, {"operation": "add_root", "alias": "specs", "path": str(Path("original-alias"))})
            service.scopes["qualification"] = {"grant_id": "new-reviewed", "index_id": "same-index"}
            return complete
        reviewer = Mock()
        reviewer.source_change.side_effect = apply
        with patch.object(QUALITY.reviewed_source, "SourceReview", return_value=reviewer) as constructor:
            self.assertEqual(QUALITY.add_fixture_root(service, "qualification", "specs", Path("original-alias")), complete)
            constructor.assert_called_once_with(service, "qualification")
            self.assertEqual(selected, {"grant_id": "original", "index_id": "same-index"})
            self.assertEqual(service.scopes["qualification"]["grant_id"], "new-reviewed")
            reviewer.source_change.side_effect = RuntimeError("unknown source admission")
            with self.assertRaisesRegex(RuntimeError, "unknown source admission"):
                QUALITY.add_fixture_root(service, "qualification", "specs", Path("original-alias"))
        service.configure.assert_not_called()
        service.single_image = False
        with self.assertRaisesRegex(ValueError, "Historical catalog source-root review protocol"):
            QUALITY.add_fixture_root(service, "qualification", "specs", Path("original-alias"))
        service.configure.assert_not_called()
        service.catalog_authority = False
        with patch.object(QUALITY.reviewed_source, "SourceReview", side_effect=AssertionError("legacy cannot use modern grant")):
            QUALITY.add_fixture_root(service, "qualification", "specs", Path("original-alias"))
        service.configure.assert_called_once_with("qualification", {"operation": "add_root", "alias": "specs", "path": str(Path("original-alias"))})

    def test_both_workloads_leave_private_native_state_creation_to_installer(self):
        for representative in (False, True):
            with self.subTest(representative=representative), tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary) / "fixture"
                binary = Path(temporary) / "airk"
                def before_native_start(runtime, owner, cli=None):
                    self.assertIsNone(cli, "No --airkctl was given")
                    self.assertEqual(owner, base)
                    self.assertFalse((owner / "state").exists(), "Native state must not inherit an ordinary fixture-directory ACL")
                    self.assertTrue((owner / "index-data").is_dir())
                    self.assertTrue((owner / "root").is_dir())
                    raise RuntimeError("verified separate roots before native start")
                with patch.object(QUALITY.clients, "Fixture", side_effect=before_native_start) as start:
                    with self.assertRaisesRegex(RuntimeError, "verified separate roots"):
                        if representative:
                            QUALITY.representative_fixture(binary, base, {"sources": {"file.txt": "original"}}, "fixture-hash", 2, "balanced", Path("model"))
                        else:
                            QUALITY.fixture(binary, base, 2, 2, Mock())
                    start.assert_called_once()

    def test_modern_session_uses_exact_native_scope_https_and_same_ranking_contract(self):
        selected = {"namespace": "native", "expected_catalog_revision": 3, "grant_id": "exact", "index_id": "one"}
        health = {"instance": "owner"}
        service = SimpleNamespace(single_image=True, scopes={"fixture": selected}, control=Path("control"), managed_health=health)
        requests = []
        def rpc(method):
            self.assertEqual(method, "health")
            return {"observation": health}
        def enroll(selection, identity):
            requests.append(selection)
            return {"connection_id": identity, "selection": {"kind": "scoped", "selection": {**selected, "expected_catalog_revision": 4}}}
        service.rpc = rpc
        service.enroll_http = enroll
        network = Mock()
        network.connection = SimpleNamespace(timeout=None, sock=Mock())
        network.session = "actual-session"
        def receive(identifier):
            return {"jsonrpc": "2.0", "id": identifier, "result": {"content": [{"text": json.dumps({"publication": {"token": "fixed"}, "hits": [self.hit("wanted.txt")]})}]}}
        network.receive.side_effect = receive
        with patch.object(QUALITY.http_fixture, "McpClient", return_value=network), patch.object(QUALITY.http_fixture, "validate_native_identity") as validate, patch.object(QUALITY.subprocess, "Popen", side_effect=AssertionError("no stdio child")):
            session = QUALITY.open_session(service, "fixture")
            self.assertEqual(session.search(self.case())["recall_at_5"], 1)
            session.close(); session.close()
        self.assertEqual(requests, [{"kind": "scoped", "selection": selected}])
        validate.assert_called_once()
        network.close.assert_called_once()
        self.assertTrue(all(call.args[0].get("params", {}).get("arguments", {}).get("response_mode", "full") == "full" for call in network.send.call_args_list))

    def test_http_enrollment_cannot_silently_change_reviewed_grant(self):
        selected = {"expected_catalog_revision": 1, "grant_id": "original"}
        service = SimpleNamespace(scopes={"fixture": selected})
        def enroll(selection, identity):
            return {"connection_id": identity, "selection": {"kind": "scoped", "selection": {"expected_catalog_revision": 2, "grant_id": "broader"}}}
        service.enroll_http = enroll
        with patch.object(QUALITY.http_fixture, "McpClient") as network, self.assertRaisesRegex(AssertionError, "changed reviewed scope"):
            QUALITY.HttpSession(service, "fixture")
        network.assert_not_called()

    def test_http_request_absolute_deadline_interrupts_slow_body(self):
        session = QUALITY.HttpSession.__new__(QUALITY.HttpSession)
        session.closed, session.sequence = False, 0
        stopped = threading.Event()
        sock = Mock()
        sock.shutdown.side_effect = lambda *_: stopped.set()
        session.network = Mock()
        session.network.connection = SimpleNamespace(timeout=None, sock=sock)
        session.network.send.side_effect = lambda *_: self.assertTrue(stopped.wait(1))
        session.network.receive.return_value = {"id": 1, "result": {}}
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            session.request("tools/list", {}, started + 0.03)
        self.assertLess(time.monotonic() - started, 0.5)
        sock.shutdown.assert_called_once()
        before = session.network.send.call_count
        with self.assertRaises(QUALITY.clients.ObservationTimeout):
            session.request("tools/list", {}, time.monotonic() - 1)
        self.assertEqual(session.network.send.call_count, before)

    def test_explicit_historical_fixture_retains_stdio_adapter(self):
        service = SimpleNamespace(single_image=False, binary=Path("old-airk"), arguments=lambda _: ["--service-project", "old"])
        with patch.object(QUALITY, "Session") as historical, patch.object(QUALITY, "HttpSession", side_effect=AssertionError("wrong transport")):
            QUALITY.open_session(service, "fixture")
        historical.assert_called_once_with(["old-airk", "--service-project", "old"])

    def test_combined_publication_refs_preserve_raw_and_reject_wrong_member(self):
        value, sources = self.combined()
        for number, rank in enumerate(value["ranking"]):
            rank["origin"].pop("publication")
            rank["origin"]["publication_ref"] = {"member": number}
        raw = json.dumps(value, sort_keys=True)
        self.assertEqual(QUALITY.judge_combined(value, sources, {key: 3 for key in sources})["recall_at_10"], 1)
        self.assertEqual(json.dumps(value, sort_keys=True), raw)
        for invalid in (1, -1, True, 32):
            value["ranking"][0]["origin"]["publication_ref"] = {"member": invalid}
            with self.assertRaises(AssertionError):
                QUALITY.judge_combined(value, sources, {key: 3 for key in sources})

    def combined(self):
        projects = ["prj_" + "03" * 16]
        indexes = ["idx_" + "01" * 16, "idx_" + "02" * 16]
        root = "src_" + "04" * 16
        sources = {(index, root, "same.rs"): {"sha256": "a" * 64, "project_ids": projects}
                   for index in indexes}
        value = {"schema": "airk.combined-read/1", "members": [{"index_id": index,
            "result": {"structuredContent": {"hits": [self.hit("same.rs")]}}} for index in indexes],
            "publication_vector": {"members": [{"index_id": index, "publication": {"token": f"fixture-{index}"}} for index in indexes]},
            "ranking": [{"index_id": index,
                "result_pointer": f"/members/{number}/result/structuredContent/hits/0",
                "origin": {"index_id": index, "source_root_id": root, "source_relative_path": "same.rs",
                           "project_ids": projects, "publication": {"token": f"fixture-{index}"}}} for number, index in enumerate(indexes)]}
        return value, sources

    def test_combined_relevance_keeps_identical_paths_in_independent_indexes(self):
        value, sources = self.combined()
        grades = {key: 3 for key in sources}
        result = QUALITY.judge_combined(value, sources, grades)
        self.assertEqual(result["relevant_files"], 2)
        self.assertEqual(result["recall_at_10"], 1)
        self.assertEqual(result["ndcg_at_10"], 1)
        value["ranking"].pop()
        self.assertEqual(QUALITY.judge_combined(value, sources, grades)["recall_at_10"], .5)

    def test_combined_relevance_rejects_false_project_or_member_origin(self):
        value, sources = self.combined()
        grades = {key: 3 for key in sources}
        value["ranking"][0]["origin"]["project_ids"] = []
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            QUALITY.judge_combined(value, sources, grades)
        value, sources = self.combined()
        value["ranking"][0]["result_pointer"] = "/members/1/result/structuredContent/hits/0"
        with self.assertRaisesRegex(AssertionError, "member differs"):
            QUALITY.judge_combined(value, sources, grades)

    def test_combined_relevance_duplicate_chunks_do_not_boost_shared_index_gain(self):
        value, sources = self.combined()
        grades = {key: 3 for key in sources}
        value["ranking"].insert(1, value["ranking"][0].copy())
        result = QUALITY.judge_combined(value, sources, grades)
        self.assertEqual(result["recall_at_10"], 1)
        self.assertLess(result["ndcg_at_10"], 1)
        self.assertEqual(result["returned_chunks"], 3)

    def test_combined_relevance_rejects_cross_member_publication_substitution(self):
        value, sources = self.combined()
        value["ranking"][0]["origin"]["publication"] = value["ranking"][1]["origin"]["publication"]
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            QUALITY.judge_combined(value, sources, {key: 3 for key in sources})

    def test_only_unsubmitted_busy_rejections_are_retried(self):
        for submitted in (False, True):
            session = self.session([self.hit("wanted.txt")])
            accepted = session.request()
            replies = [{"isError":True,"content":[{"type":"text","text":json.dumps({"error":{
                "code":"direct_query_admission","reason":"busy","query_submitted":submitted,"retryable":True,"fresh_brokerage_required":False,"retry_after_ms":25}})}]}, accepted]
            def request(*_):
                return replies.pop(0)
            session.request = request
            with patch.object(QUALITY.time,"sleep") as sleep:
                if submitted:
                    with self.assertRaises(AssertionError):
                        session.search(self.case())
                    self.assertEqual(len(replies),1)
                    sleep.assert_not_called()
                else:
                    self.assertEqual(session.search(self.case())["admission_retries"],1)
                    self.assertEqual(replies,[])
                    sleep.assert_called_once_with(0.025)

    def session(self, hits, publication="fixture-revision"):
        session = QUALITY.Session.__new__(QUALITY.Session)
        session.publication = None
        session.request = lambda *_: {"content": [{"text": json.dumps({
            "publication": {"token": publication}, "hits": hits})}]}
        return session

    def hit(self, path, text="evidence", source_hash="a" * 64):
        return {"citation": {"path": path, "content_hash": source_hash}, "text": text}

    def case(self, **extra):
        return {"name": "fixture", "query": "needle", "mode": "hybrid",
                "expected": "wanted.txt", "source_sha256": "a" * 64, **extra}

    def test_repeated_paths_keep_their_original_chunk_rank(self):
        session = self.session([self.hit("other.txt"), self.hit("other.txt"), self.hit("wanted.txt")])
        self.assertEqual(session.search(self.case())["reciprocal_rank_at_5"], 1 / 3)

    def test_long_line_requires_the_expected_chunk_text(self):
        session = self.session([self.hit("wanted.txt", "earlier chunk"),
                                self.hit("wanted.txt", "tailmarker")])
        self.assertEqual(session.search(self.case(expected_text="tailmarker"))["reciprocal_rank_at_5"], 1 / 2)
        with self.assertRaises(AssertionError):
            session.search(self.case(expected_text="absent marker"))

    def test_citation_hash_must_match_the_fixture_source_bytes(self):
        session = self.session([self.hit("wanted.txt", source_hash="b" * 64)])
        with self.assertRaises(AssertionError):
            session.search(self.case())

    def test_session_cannot_change_publication(self):
        session = self.session([self.hit("wanted.txt")])
        session.publication = "previous-revision"
        with self.assertRaises(AssertionError):
            session.search(self.case())

    def test_graded_search_keeps_relevance_misses_and_checks_every_hit_identity(self):
        session = self.session([self.hit("other.txt")])
        case = {"query": "needle", "mode": "lexical", "grades": {"wanted.txt": 3}}
        result = session.judged_search(case, {"other.txt": "a" * 64, "wanted.txt": "a" * 64})
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["recall_at_10"], 0)
        self.assertEqual(result["ndcg_at_10"], 0)
        session = self.session([self.hit("other.txt", source_hash="b" * 64)])
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            session.judged_search(case, {"other.txt": "a" * 64})

    def test_graded_search_rejects_scope_leak_and_requests_ten_chunks(self):
        session = self.session([self.hit("outside.txt")])
        original = session.request
        calls = []
        def request(*arguments):
            calls.append(arguments)
            return original()
        session.request = request
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            session.judged_search({"query": "needle", "mode": "lexical", "grades": {},
                                   "path_prefix": "docs/"}, {"outside.txt": "a" * 64})
        self.assertEqual(calls[0][1]["arguments"]["limit"], 10)

    def test_setup_failure_writes_a_failed_receipt_without_serializing_error_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "airk"
            binary.write_bytes(b"fixture identity only")
            output = Path(temporary) / "receipt.json"
            with patch("sys.argv", ["check-retrieval-quality", "--binary", str(binary),
                                    "--output", str(output), "--trials", "2"]), \
                    patch.object(QUALITY, "source_identity", return_value={"fixture": "fixed"}), \
                    patch.object(QUALITY.subprocess, "check_output", return_value="airk fixture"), \
                    patch.object(QUALITY.platform, "platform", return_value="fixture platform"), \
                    patch.dict(os.environ, {"AIRK_PRIVATE_CENSUS_BASE": temporary}), \
                    patch.object(QUALITY, "fixture", side_effect=RuntimeError("private path and source")), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(SystemExit, "qualification failed"):
                    QUALITY.main()
            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertFalse(receipt["passed"])
            self.assertEqual(receipt["failure_class"], "RuntimeError")
            self.assertTrue(1 <= len(receipt["failure_frames"]) <= 12)
            self.assertTrue(all(set(frame) == {"file", "line"} and Path(frame["file"]).name == frame["file"]
                                and type(frame["line"]) is int and frame["line"] > 0 for frame in receipt["failure_frames"]))
            self.assertEqual(receipt["platform"], "fixture platform")
            self.assertNotIn("private path and source", output.read_text(encoding="utf-8"))
            self.assertEqual(Path(receipt["retained_fixture"]).name, receipt["retained_fixture"])
            self.assertTrue((Path(temporary) / receipt["retained_fixture"]).is_dir())

    def test_stage_diagnostic_filesystem_failure_preserves_primary_failed_receipt(self):
        stage = {}
        exec(compile("def run(*args):\n    raise AssertionError('original stage refusal private/source')\n",
                     "check-service-clients.py", "exec"), stage)
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "airk"
            binary.write_bytes(b"fixture identity only")
            private = Path(temporary) / "private"
            private.mkdir(mode=0o700)
            output = Path(temporary) / "receipt.json"
            original_mkdtemp = QUALITY.tempfile.mkdtemp
            def fail_diagnostic(suffix=None, prefix=None, dir=None):
                if prefix == "airk-retrieval-stage-diagnostic-":
                    raise PermissionError("diagnostic private path must not escape")
                return original_mkdtemp(suffix=suffix, prefix=prefix, dir=dir)
            def failed_stage(*_, **__):
                stage["run"]("runtime", "stage")
            with patch("sys.argv", ["check-retrieval-quality", "--binary", str(binary),
                                    "--output", str(output), "--trials", "2"]), \
                    patch.dict(os.environ, {"AIRK_PRIVATE_CENSUS_BASE": str(private)}), \
                    patch.object(QUALITY, "source_identity", return_value={"fixture": "fixed"}), \
                    patch.object(QUALITY.subprocess, "check_output", return_value="airk fixture"), \
                    patch.object(QUALITY.platform, "platform", return_value="fixture platform"), \
                    patch.object(QUALITY, "fixture", side_effect=failed_stage), \
                    patch.object(QUALITY.tempfile, "mkdtemp", side_effect=fail_diagnostic), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(SystemExit, "qualification failed"):
                    QUALITY.main()
            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertFalse(receipt["passed"])
            self.assertEqual(receipt["failure_class"], "AssertionError")
            self.assertEqual(receipt["private_stage_diagnostic_error"], "PermissionError")
            self.assertNotIn("private_stage_diagnostic", receipt)
            self.assertTrue(1 <= len(receipt["failure_frames"]) <= 12)
            public = output.read_text(encoding="utf-8")
            self.assertNotIn("original stage refusal private/source", public)
            self.assertNotIn("diagnostic private path must not escape", public)


if __name__ == "__main__":
    unittest.main()
