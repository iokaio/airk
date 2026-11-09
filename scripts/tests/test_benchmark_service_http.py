# SPDX-License-Identifier: Apache-2.0
"""Owned producer/credential contracts and real isolated TLS; no AIRK execution."""
import copy
from contextlib import ExitStack
import hashlib
import importlib.util
import importlib.machinery
import json
from pathlib import Path
import tempfile
import shutil
from types import SimpleNamespace
import unittest
from unittest import mock

import test_http_fixture as TLS


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SERVICE = load("service_http_benchmark_test", "benchmark-service.py")
INVESTIGATION = load("owned_benchmark_test", "benchmark-mcp-investigation.py")
VERIFICATION = load("verification_benchmark_test", "benchmark-source-verification.py")


class ProducerTests(unittest.TestCase):
    def test_uncertain_exchange_is_not_replayed_or_deleted_and_cleanup_preserves_primary(self):
        network = mock.Mock(session="retained-native-session")
        network.receive.return_value = {"result": {"fixture": True}}
        observed = []
        with mock.patch.object(SERVICE.http_fixture, "McpClient", return_value=network):
            session = SERVICE.HttpSession({}, Path("owned-control"), observer=observed.append)
        network.send.reset_mock()
        primary = ConnectionAbortedError("uncertain original exchange")
        network.send.side_effect = primary
        with self.assertRaises(ConnectionAbortedError) as caught:
            session.request("tools/call", {"name": "search", "arguments": {"query": "original"}})
        self.assertIs(caught.exception, primary)
        with self.assertRaisesRegex(RuntimeError, "cannot be replayed"):
            session.request("tools/call", {"name": "search", "arguments": {"query": "original"}})
        network.send.assert_called_once()
        network.close.side_effect = RuntimeError("secondary teardown failure")
        with self.assertRaises(ConnectionAbortedError) as caught:
            with ExitStack() as stack:
                stack.push(session)
                raise primary
        self.assertIs(caught.exception, primary)
        network.close.assert_called_once_with(delete=False)
        self.assertTrue(session.closed)
        self.assertFalse(session.delete_completed)
        self.assertTrue(observed[-1]["uncertain_exchange"])
        self.assertEqual(observed[-1]["cleanup_error"]["type"], "RuntimeError")
        self.assertTrue(any("secondary teardown" in note for note in primary.__notes__))

    def test_constructor_failure_retains_original_instead_of_secondary_delete_failure(self):
        network = mock.Mock()
        primary = ConnectionAbortedError("initial exchange unknown")
        network.send.side_effect = primary
        network.close.side_effect = RuntimeError("cleanup unknown")
        with mock.patch.object(SERVICE.http_fixture, "McpClient", return_value=network):
            with self.assertRaises(ConnectionAbortedError) as caught:
                SERVICE.HttpSession({}, Path("owned-control"))
        self.assertIs(caught.exception, primary)
        network.send.assert_called_once()
        network.close.assert_called_once_with(delete=False)

    def test_failure_after_submission_is_not_reissued_on_fresh_tcp_or_hidden_by_delete(self):
        network = mock.Mock(session="original-session")
        network.receive.return_value = {"result": {"fixture": True}}
        with mock.patch.object(SERVICE.http_fixture, "McpClient", return_value=network):
            session = SERVICE.HttpSession({}, Path("owned-control"))
        network.send.reset_mock()
        network.receive.reset_mock()
        primary = ConnectionResetError("reply missing after submission")
        network.receive.side_effect = primary
        with self.assertRaises(ConnectionResetError) as caught:
            session.request("tools/call", {"name": "search", "arguments": {"query": "same-original-query"}})
        self.assertIs(caught.exception, primary)
        with self.assertRaisesRegex(RuntimeError, "cannot be replayed"):
            session.request("tools/call", {"name": "search", "arguments": {"query": "same-original-query"}})
        network.send.assert_called_once()
        network.receive.assert_called_once()
        session.close()
        network.close.assert_called_once_with(delete=False)
        self.assertFalse(session.delete_completed)

    def test_tls_establishment_budget_is_released_before_sending_or_execution(self):
        network = mock.Mock(session="original-session")
        network.receive.return_value = {"result": {"fixture": True}}
        with mock.patch.object(SERVICE.http_fixture, "McpClient", return_value=network):
            session = SERVICE.HttpSession({}, Path("owned-control"))
        active = [False]
        class HandshakeLease:
            def __enter__(self):
                if active[0]:
                    raise AssertionError("Handshake lease was already held")
                active[0] = True
            def __exit__(self, *_):
                active[0] = False
        def connect():
            self.assertTrue(active[0])
        def send(_request):
            self.assertFalse(active[0], "Handshake lease must not serialize request execution")
        network.connection.connect.side_effect = connect
        network.send.side_effect = send
        with mock.patch.object(SERVICE.HttpSession, "handshake_slots", HandshakeLease()):
            session.request("tools/call", {"name": "search", "arguments": {}})
            session.close()
        self.assertFalse(active[0])
        self.assertEqual(network.connection.connect.call_count, 2)

    def test_failed_case_retains_completed_phase_and_primary_failure_without_complete_case(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "cohort.json"
            primary = ConnectionAbortedError("settled request unknown")
            with self.assertRaises(ConnectionAbortedError) as caught:
                with SERVICE.CaseEvidence(path, 1, 64, 1, True) as journal:
                    journal.note("source_update_accepted", receipt={"job": {"id": "same-original-job"}})
                    journal.note("settled_started", rounds=1)
                    raise primary
            self.assertIs(caught.exception, primary)
            retained = json.loads(path.read_text())
            self.assertFalse(retained["passed"])
            self.assertEqual(retained["clients"], 64)
            self.assertEqual(retained["failure"]["type"], "ConnectionAbortedError")
            self.assertEqual([event["phase"] for event in retained["events"]],
                             ["source_update_accepted", "settled_started", "failed"])
            self.assertEqual(retained["events"][0]["receipt"]["job"]["id"], "same-original-job")

    def test_owned_benchmark_preparation_does_not_precreate_native_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checked = []
            def boundary(runtime, owner):
                self.assertFalse((owner / "state").exists())
                self.assertTrue((owner / "index-data").is_dir())
                checked.append(owner)
                raise RuntimeError("verified native state boundary")
            original = importlib.machinery.SourceFileLoader.exec_module
            def load_fixture(loader, module):
                original(loader, module)
                if module.__name__ == "investigation_clients":
                    module.Fixture = boundary
                    module.owned_client_fixture = tempfile.TemporaryDirectory
            with mock.patch.object(importlib.machinery.SourceFileLoader, "exec_module", load_fixture):
                with self.assertRaisesRegex(RuntimeError, "verified native state"):
                    INVESTIGATION.owned(SimpleNamespace(runtime_dir=root, output=root / "output"))
            self.assertEqual(len(checked), 1)

    def test_scoped_source_work_keeps_privacy_and_full_nonclock_management_identity(self):
        project = "owned"
        progress = {"status": "progress", "project": project, "pending_job": None, "queued_jobs": [],
                    "observation": None, "queue_truncated": False, "project_paused": False, "observed_unix_ms": 1}
        watch = {"status": "watch", "project": project, "observation": {"enabled": False, "failure": None,
                 "automatic_retries_exhausted": False, "automatic_retry_canceled": False,
                 "requested_revision": 2, "completed_revision": 2, "observed_unix_ms": 1, "future": {"retained": True}}}
        service = SimpleNamespace(projects={"verification": {"binding": project}}, rpc=mock.Mock(side_effect=[progress, watch]))
        clients = SimpleNamespace(assert_scoped_freshness_private=mock.Mock())
        settled, stable, raw = VERIFICATION.scoped_source_work(clients, service, {"freshness": "opaque"}, 123)
        self.assertTrue(settled)
        clients.assert_scoped_freshness_private.assert_called_once_with({"freshness": "opaque"})
        self.assertEqual(raw, {"progress": progress, "watch": watch})
        changed = copy.deepcopy(watch); changed["observation"]["observed_unix_ms"] = 2
        service.rpc = mock.Mock(side_effect=[{**progress, "observed_unix_ms": 2}, changed])
        self.assertEqual(VERIFICATION.scoped_source_work(clients, service, {}, 123)[1], stable)
        changed["observation"]["future"] = {}
        service.rpc = mock.Mock(side_effect=[progress, changed])
        self.assertNotEqual(VERIFICATION.scoped_source_work(clients, service, {}, 123)[1], stable)
        for field, value in (("pending_job", {"project": project}), ("queue_truncated", True), ("project_paused", True)):
            service.rpc = mock.Mock(side_effect=[{**progress, field: value}, watch])
            self.assertFalse(VERIFICATION.scoped_source_work(clients, service, {}, 123)[0])
        for changed_progress, changed_watch in (({**progress, "project": "foreign"}, watch),
                (progress, {**watch, "project": "foreign"}),
                (progress, {**watch, "observation": {**watch["observation"], "enabled": True}}),
                (progress, {**watch, "observation": {**watch["observation"], "completed_revision": 3}})):
            service.rpc = mock.Mock(side_effect=[changed_progress, changed_watch])
            with self.assertRaises(AssertionError):
                VERIFICATION.scoped_source_work(clients, service, {}, 123)

    def test_source_verification_preparation_keeps_native_state_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            (runtime / "airk-runtime.json").write_text(json.dumps({"schema": 2, "version": "1.8.14"}))
            (runtime / "airk-build.json").write_text(json.dumps({"source": {"fixture": True}}))
            checked = []
            def boundary(selected_runtime, owner):
                self.assertFalse((owner / "state").exists())
                self.assertTrue((owner / "index-data").is_dir())
                checked.append(owner)
                raise RuntimeError("verified_state_boundary")
            clients = SimpleNamespace(runtime_root=lambda _: runtime, owned_client_fixture=tempfile.TemporaryDirectory, Fixture=boundary)
            builder = SimpleNamespace(source_identity=lambda: {"fixture": True}, digest=lambda _: "fixture-digest")
            with mock.patch.object(VERIFICATION, "load", side_effect=[clients, SimpleNamespace(), builder]), \
                    mock.patch.object(VERIFICATION, "verify_payload", return_value={}):
                self.assertFalse(VERIFICATION.run(SimpleNamespace(runtime_dir=runtime, output=runtime / "output", samples=1)))
            self.assertEqual(len(checked), 1)

    def test_only_exact_known_http_admission_refusal_is_counted_without_retry(self):
        body = {"schema": "airk.local-api-error/1", "error": {"code": "query_admission_busy"}, "future": {"retained": True}}
        session = SimpleNamespace(request=mock.Mock(side_effect=SERVICE.http_fixture.HttpRefused(429, body)))
        result = SERVICE.request_attempt(session, "search", {"query": "original"})
        self.assertEqual(result["http_status"], 429)
        self.assertEqual(result["body"], body)
        self.assertGreaterEqual(result["rejected_ms"], 0)
        session.request.assert_called_once()
        for error in [SERVICE.http_fixture.HttpRefused(403, body),
                      SERVICE.http_fixture.HttpRefused(429, {"error": {"code": "query_admission_busy"}}),
                      SERVICE.http_fixture.HttpRefused(429, {**body, "error": {"code": "unknown"}}),
                      ConnectionResetError("uncertain exchange")]:
            session.request = mock.Mock(side_effect=error)
            with self.assertRaises(type(error)):
                SERVICE.request_attempt(session, "search", {})
            session.request.assert_called_once()

    def test_unclassified_http_503_retains_response_and_request_in_wave_terminal_and_cleanup(self):
        body = {"schema": "airk.local-api-error/1", "error": {"code": "native_delivery_fenced"},
                "future": {"unreduced": ["complete", 1]}}
        headers = [["Retry-After", "1"], ["X-Future", "one"], ["X-Future", "two"]]
        request = {"connection_id": "owned-connection", "jsonrpc_id": 2,
                   "jsonrpc_method": "tools/call", "request_sha256": "ab" * 32}
        raw = json.dumps(body).encode()
        refused = SERVICE.http_fixture.HttpRefused(503, body, headers=headers, request=request, raw_body=raw)
        session = SimpleNamespace(request=mock.Mock(side_effect=refused))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "failed-cohort.json"
            with self.assertRaises(SERVICE.http_fixture.HttpRefused) as caught:
                with SERVICE.CaseEvidence(path, 1, 64, 1, False) as journal:
                    try:
                        SERVICE.request_attempt(session, "search", {"query": "original"})
                    except SERVICE.http_fixture.HttpRefused as error:
                        journal.note("first_wave", observations=[{"client": 17, "error": SERVICE.error_observation(error)}])
                        raise
            self.assertIs(caught.exception, refused)
            session.request.assert_called_once()
            evidence = json.loads(path.read_text())
            for retained in (evidence["failure"]["http_response"],
                             evidence["events"][0]["observations"][0]["error"]["http_response"]):
                self.assertEqual(retained, refused.evidence())
                self.assertEqual(retained["body"], body)
                self.assertEqual(retained["headers"], headers)
                self.assertEqual(retained["request"], request)
                self.assertEqual(retained["error_code"], "native_delivery_fenced")
        network = mock.Mock(session="original-session")
        network.close.side_effect = refused
        observed = []
        owned = SERVICE.HttpSession.__new__(SERVICE.HttpSession)
        owned.network, owned.closed, owned.uncertain, owned.delete_completed = network, False, False, False
        owned.observer = observed.append
        primary = RuntimeError("original operation failed")
        owned.__exit__(RuntimeError, primary, None)
        self.assertEqual(observed[0]["cleanup_error"]["http_response"], refused.evidence())
        network.close.assert_called_once_with(delete=True)

    def test_observed_http_503_admission_stages_are_retained_and_counted_without_replaying(self):
        replies = []
        for code, stage in (("authentication_busy", "authentication"), ("native_validation_busy", "native_validation")):
            body = {"schema": "airk.local-api-error/1", "error": {"code": code}, "future": {"retained": [1]}}
            refused = SERVICE.http_fixture.HttpRefused(503, body, headers=[["X-Future", "unreduced"]],
                request={"jsonrpc_id": 2, "request_sha256": "ab" * 32}, raw_body=json.dumps(body).encode())
            session = SimpleNamespace(request=mock.Mock(side_effect=refused))
            observed = SERVICE.request_attempt(session, "search", {"query": "original"})
            session.request.assert_called_once_with("tools/call", {"name": "search", "arguments": {"query": "original"}})
            self.assertEqual(observed["http_response"], refused.evidence())
            self.assertEqual((observed["admission_code"], observed["admission_stage"]), (code, stage))
            self.assertNotIn("success_ms", observed)
            self.assertNotIn("envelope", observed)
            replies.append(observed)
        replies += [replies[0], {"success_ms": 1, "semantic_complete": True},
                    {"rejected_ms": 2, "tool_result": {"isError": True, "future_native_hint": True}}]
        self.assertEqual(SERVICE.http_admission_counts(replies), [
            {"stage": "authentication", "code": "authentication_busy", "http_status": 503, "count": 2},
            {"stage": "native_validation", "code": "native_validation_busy", "http_status": 503, "count": 1}])

    def test_http_admission_accounting_refuses_unknown_schema_status_fences_and_submitted_work(self):
        original = {"schema": "airk.local-api-error/1", "error": {"code": "authentication_busy"}}
        cases = [(401, original), (429, original), (503, {**original, "schema": "airk.local-api-error/2"}),
                 (503, {"error": original["error"]})]
        for code in ("unknown", "credential_refused", "credential_authentication_epoch_retired", "native_delivery_fenced"):
            cases.append((503, {**original, "error": {"code": code}}))
        for submitted in (True, None, "false", 0):
            cases += [(503, {**original, "query_submitted": submitted}),
                      (503, {**original, "error": {**original["error"], "query_submitted": submitted}})]
        for status, body in cases:
            refused = SERVICE.http_fixture.HttpRefused(status, body)
            session = SimpleNamespace(request=mock.Mock(side_effect=refused))
            with self.subTest(status=status, body=body), self.assertRaises(SERVICE.http_fixture.HttpRefused) as caught:
                SERVICE.request_attempt(session, "search", {})
            self.assertIs(caught.exception, refused)
            session.request.assert_called_once()
        body = {**original, "query_submitted": False, "error": {**original["error"], "query_submitted": False}}
        session = SimpleNamespace(request=mock.Mock(side_effect=SERVICE.http_fixture.HttpRefused(503, body)))
        self.assertEqual(SERVICE.request_attempt(session, "search", {})["admission_stage"], "authentication")
        session.request.assert_called_once()

    def test_unknown_native_stop_retains_the_exact_owned_install_and_state(self):
        owners = []
        class FailingNativeFixture:
            def __init__(self, runtime, owner, **_):
                self.owner = owner
                owners.append(owner)
            def __enter__(self):
                (self.owner / "unresolved-native-state").write_bytes(b"retain")
                return self
            def __exit__(self, *_):
                raise RuntimeError("native_stop_uncertain")
            def rpc(self, _):
                raise RuntimeError("observation_failed")
        try:
            with mock.patch.object(SERVICE.clients, "Fixture", FailingNativeFixture):
                with self.assertRaisesRegex(RuntimeError, "native_stop_uncertain"):
                    SERVICE.campaign(Path("unused-mocked-runtime"), 0, 0, 1)
            self.assertEqual(len(owners), 1)
            self.assertEqual((owners[0] / "unresolved-native-state").read_bytes(), b"retain")
        finally:
            for owner in owners:
                self.assertEqual(owner.parent.resolve(), Path(tempfile.gettempdir()).resolve())
                self.assertTrue(owner.name.startswith("airk-service-clients-"))
                self.assertFalse(owner.is_symlink())
                shutil.rmtree(owner)

    def test_owned_modern_configuration_has_exact_scope_control_and_no_launch_args(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            binary, control = root / "airk", root / "control"
            binary.write_bytes(b"approved-main"); control.mkdir()
            (control / "http-1").mkdir()
            (root / "airk-runtime.json").write_text(json.dumps({"version": "1.8.14"}))
            selected = {"namespace": "native", "grant_id": "frozen", "expected_catalog_revision": 4}
            service = SimpleNamespace(single_image=True, binary=binary, control=control,
                projects={"owned": {"root": str(root)}}, scopes={"owned": selected},
                enroll_http=mock.Mock(return_value={"native": "exact-enrolled-metadata"}),
                arguments=mock.Mock(side_effect=AssertionError("Modern config must not acquire a stdio launch")))
            result = INVESTIGATION.owned_configuration(service, "owned", root)
            self.assertEqual(result["schema"], 2); self.assertEqual(result["transport"], "https")
            self.assertEqual(result["args"], [])
            self.assertEqual(result["control"], str(control)); self.assertEqual(result["source_root"], str(root))
            self.assertEqual(result["selection"], {"kind": "scoped", "selection": selected})
            service.enroll_http.assert_called_once_with({"kind": "scoped", "selection": selected}, mock.ANY, "unknown")
            roster = json.loads(Path(result["connections_file"]).read_bytes())
            self.assertEqual(roster, {"schema": 1, "client": "unknown", "connections": [{"native": "exact-enrolled-metadata"}]})
            result["selection"]["selection"]["grant_id"] = "changed"
            self.assertEqual(selected["grant_id"], "frozen")
            service.single_image = False
            service.arguments = lambda _: ["--root", str(root)]
            (root / "airk-runtime.json").write_text(json.dumps({"version": "1.8.12"}))
            legacy = INVESTIGATION.owned_configuration(service, "owned", root)
            self.assertEqual(legacy["args"], ["--root", str(root), "mcp"])
            self.assertNotIn("transport", legacy)

    def test_sixty_four_clients_enroll_distinct_credentials_with_exact_immutable_authority(self):
        selected = {"namespace": "native", "grant_id": "same-reviewed-grant", "expected_catalog_revision": 1}
        methods = []
        health = {"instance": "service", "user": "user", "executable_sha256": "execution", "catalog_clock": {"namespace": "native"}}
        def rpc(method):
            self.assertEqual(method, "health", "Console must not perform private HTTP enrollment")
            return {"status": "health", "observation": health}
        def enroll_http(selection, identity, client):
            methods.append((copy.deepcopy(selection), identity, client))
            return {"connection_id": identity, "selection": copy.deepcopy(selection),
                "endpoint": {"service_instance": "service", "user": "user", "namespace": "native", "executable_sha256": "execution"}}
        fixture = SimpleNamespace(scopes={"owned": selected}, rpc=rpc, enroll_http=enroll_http, managed_health=health)
        connections = [SERVICE.enroll(fixture, "owned") for _ in range(64)]
        self.assertEqual(len({info["connection_id"] for info in connections}), 64)
        self.assertEqual(len(methods), 64)
        self.assertTrue(all(selection == {"kind": "scoped", "selection": selected} and client == "unknown"
                            for selection, _, client in methods))
        self.assertEqual(selected["expected_catalog_revision"], 1)

    def test_payload_verification_never_hashes_a_modern_helper_but_requires_legacy_helper(self):
        binary = Path("/owned/airk")
        modern = {"schema": 2, "version": "1.8.14", "os": "linux", "arch": "x86_64", "airk_sha256": "main"}
        builder = SimpleNamespace(digest=mock.Mock(return_value="main"))
        result = VERIFICATION.verify_payload(builder, binary, modern, {"runtime": modern})
        self.assertIsNone(result["helper_sha256"])
        self.assertEqual(result["transport"], "https")
        builder.digest.assert_called_once_with(binary)
        legacy = {**modern, "schema": 1, "version": "1.8.12", "embed_sha256": "helper"}
        builder.digest = mock.Mock(side_effect=["main", "helper"])
        self.assertEqual(VERIFICATION.verify_payload(builder, binary, legacy, {"runtime": legacy})["helper_sha256"], "helper")
        self.assertEqual(builder.digest.call_args_list[-1].args, (binary.with_name("airk-embed"),))
        for malformed in [{**modern, "embed_sha256": None}, {**modern, "schema": 1}]:
            with self.assertRaises(ValueError):
                VERIFICATION.verify_payload(builder, binary, malformed, {"runtime": malformed})


class ActualHttpsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        TLS.ActualHttpsFixtureTests.setUpClass()

    @classmethod
    def tearDownClass(cls):
        TLS.ActualHttpsFixtureTests.tearDownClass()

    def test_initialized_real_tls_session_retains_only_hashed_session_identity_and_deletes(self):
        fixture = TLS.ActualHttpsFixtureTests()
        with fixture.server() as (connection, observed):
            session = SERVICE.HttpSession(connection, fixture.root)
            try:
                self.assertEqual(session.session_sha256, hashlib.sha256(session.network.session.encode()).hexdigest())
                self.assertEqual(len(session.session_sha256), 64)
                self.assertTrue(session.request("ping", {})["fixture"])
            finally:
                session.close()
            session.close()
            self.assertTrue(session.closed)
            self.assertEqual(sum(method == "initialize" for method, _ in observed), 1)
            self.assertEqual(sum(method == "DELETE" for method, _ in observed), 1)

    def test_new_request_retires_stale_tcp_and_keeps_the_original_mcp_session_and_credential(self):
        fixture = TLS.ActualHttpsFixtureTests()
        with fixture.server() as (connection, observed):
            session = SERVICE.HttpSession(connection, fixture.root)
            original_session = session.network.session
            original_peer = session.network.connection.sock
            # Model an idle socket that is no longer usable without waiting for
            # a real 15-second native header timer. No submitted call is replayed.
            original_peer.close()
            try:
                self.assertTrue(session.request("ping", {})["fixture"])
                self.assertIsNot(session.network.connection.sock, original_peer)
                self.assertEqual(session.network.session, original_session)
            finally:
                session.close()
            self.assertTrue(session.delete_completed)
            self.assertEqual([method for method, _ in observed], ["initialize", "notifications/initialized", "ping", "DELETE"])
            self.assertEqual(len({authorization for _, authorization in observed}), 1)


class SemanticLoadContracts(unittest.TestCase):
    def test_initialization_refusals_keep_available_clients_running_and_count_missing_work(self):
        for refused_clients, refused_read in (({0}, None), ({0, 1}, None), (set(), "first"), (set(), "parity"), (set(), "tool_error")):
            with self.subTest(refused_clients=refused_clients, refused_read=refused_read):
                fixture = None
                class Native:
                    single_image = True
                    managed_health = {"instance": "same"}
                    control = Path("fictional-owned-control")
                    def __init__(self, runtime, owner, **kwargs):
                        nonlocal fixture
                        fixture, self.owner, self.projects = self, owner, {}
                    def __enter__(self): return self
                    def __exit__(self, *_): return False
                    def register(self, name, root, data): self.projects[name] = {"binding": name}
                    def rpc(self, value):
                        return {"status": "health", "observation": self.managed_health} if value == "health" else {"job": {"state": "succeeded"}}
                    def configure(self, *args): pass
                    def job(self, *args): pass
                    def enqueue(self, *args): return {"job": {"id": "saved"}}
                    def wait(self, *args): pass
                    def arguments(self, name): return [name]
                    def run(self, *args, **kwargs): return SimpleNamespace(returncode=0, stdout=json.dumps(envelope()))
                def envelope():
                    path = fixture.owner / "load-0" / "file-0.txt"
                    value = {"collection": "load-0", "hits": [{
                        "text": "projectmarker0", "citation": {"path": "file-0.txt", "content_hash": SERVICE.hashlib.sha256(path.read_bytes()).hexdigest()}}]}
                    return {"content": [{"type": "text", "text": json.dumps(value)}], "isError": False, "structuredContent": value}
                class Session:
                    def __init__(self, connection, control, observer=None):
                        if connection["index"] in refused_clients:
                            raise SERVICE.http_fixture.HttpRefused(503, {"schema": "airk.local-api-error/1", "error": {"code": "authentication_busy"}})
                        self.session_sha256, self.closed, self.delete_completed = connection["connection_id"], False, False
                        self.read_count = 0
                        def api_read(*args):
                            if refused_read == "parity":
                                raise SERVICE.http_fixture.HttpRefused(503, {"schema": "airk.local-api-error/1", "error": {"code": "native_validation_busy"}})
                            return envelope()
                        self.network = SimpleNamespace(api_read=api_read,
                            response_error=lambda status, data, *args, **kwargs: SERVICE.http_fixture.HttpResponseError(status, json.loads(data), failure=kwargs["failure"]))
                        self.sequence = 0
                    def request(self, *args):
                        self.read_count += 1
                        if refused_read == "first" and self.read_count == 1:
                            raise SERVICE.http_fixture.HttpRefused(503, {"schema": "airk.local-api-error/1", "error": {"code": "authentication_busy"}})
                        if refused_read == "tool_error":
                            value = {"status": "retry", "code": "selection_busy", "retryable": True, "retry_after_ms": 50}
                            return {"content": [{"type": "text", "text": json.dumps(value)}], "isError": True, "structuredContent": value}
                        return envelope()
                    def close(self): self.closed = self.delete_completed = True
                    def __exit__(self, *_): self.close(); return False
                counter = iter(range(2))
                def enroll(*args):
                    index = next(counter)
                    return {"index": index, "connection_id": str(index)}
                observed = {"resources": {"observation": {"resident_workers": 0}}, "embedding_status": {"loads": 1}}
                with mock.patch.object(SERVICE.clients, "Fixture", Native), \
                        mock.patch.object(SERVICE.clients, "owned_client_fixture", tempfile.TemporaryDirectory), \
                        mock.patch.object(SERVICE, "retained_snapshot", return_value=observed), \
                        mock.patch.object(SERVICE, "enroll", side_effect=enroll), \
                        mock.patch.object(SERVICE.clients, "read_envelope", side_effect=AssertionError("no harness replay")), \
                        mock.patch.object(SERVICE, "HttpSession", Session):
                    if refused_read == "tool_error":
                        with self.assertRaises(SERVICE.http_fixture.HttpResponseError) as caught:
                            SERVICE.campaign(Path("unused-fixture"), 1, 2, 1)
                        self.assertEqual(caught.exception.failure, "tool_error")
                        self.assertEqual(SERVICE.http_fixture.error_outcome(caught.exception), "failed")
                        continue
                    result = SERVICE.campaign(Path("unused-fixture"), 1, 2, 1)
                phases = result["phases"]
                self.assertEqual(phases["initialize"]["refused"], len(refused_clients))
                for phase in ("first", "idle", "during", "settled"):
                    self.assertEqual(phases[phase]["succeeded"], 0 if phase == "first" and refused_read == "first" else 2-len(refused_clients))
                    self.assertEqual(phases[phase]["not_executed"], len(refused_clients))
                if refused_read == "first":
                    self.assertTrue(phases["first"]["passed"])
                    self.assertEqual(phases["first"]["refused"], 2)
                if refused_read == "parity":
                    self.assertEqual(phases["parity"]["refused"], 1)
                    self.assertEqual(result["parity_coverage"]["unverified_projects"], ["load-0"])
                    self.assertFalse(result["qualified"])
                    self.assertFalse(result["parity_coverage"]["verified"])
                elif refused_clients:
                    self.assertFalse(result["qualified"])
                else:
                    self.assertTrue(result["qualified"])
                self.assertEqual(phases["client_close"]["succeeded"], 2-len(refused_clients))

    def test_phase_journal_keeps_future_phases_unexecuted_after_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "phases.json"
            with self.assertRaisesRegex(RuntimeError, "stop"):
                with SERVICE.CaseEvidence(path, 1, 64, 1, True) as journal:
                    journal.plan_phase("first", 64)
                    journal.plan_phase("idle", 64)
                    for _ in range(64):
                        journal.record_outcome("first", "refused", "authentication_busy")
                    journal.record_outcome("idle", "failed")
                    raise RuntimeError("stop")
            phases = json.loads(path.read_text())["phases"]
            self.assertTrue(phases["first"]["passed"])
            self.assertEqual(phases["idle"]["not_executed"], 63)
            self.assertFalse(phases["idle"]["passed"])

    pipeline = "ab" * 32

    def result(self):
        return {"execution":{"requested_mode":"vector","served_mode":"vector","incomplete":False},
                "semantic_readiness":{"state":"ready","completed_artifacts":16,"selected_artifacts":16,
                                      "pipeline_digest":self.pipeline}}

    def owner(self, loads=1, cached=True):
        return {"embedding_status":{"available":True,"process_id":None,"loads":loads,"sessions":0,
                "provider":{"schema":1,"kind":"in_process","admission_open":True,
                            "queued_outstanding":0,"running_outstanding":0,"callback_outstanding":0,"delivery_outstanding":0,
                            "model_references_outstanding":1 if cached else 0,
                            "provider":{"service_instance":"owned","factory_generation":"same"}}}}

    def test_first_existing_warmup_keeps_cached_baseline_or_exactly_reloads_observed_uncached_state(self):
        cached = SERVICE.post_first_warmup_baseline(self.owner(), self.owner())
        self.assertEqual(cached["kind"], "no_reload")
        self.assertEqual(cached["loads_after"], 1)
        cold = SERVICE.post_first_warmup_baseline(self.owner(cached=False), self.owner(loads=2))
        self.assertEqual(cold["kind"], "observed_uncached_reload")
        self.assertTrue(cold["idle_uncached_proven"])
        self.assertEqual((cold["loads_before"], cold["loads_after"]), (1, 2))
        # Every later client/wave still uses the strict, unchanged owner check.
        with self.assertRaisesRegex(AssertionError, "additional model"):
            SERVICE.stable_semantic_owner(self.owner(loads=2), self.owner(loads=3))

    def test_reload_requires_complete_idle_proof_and_rejects_replacement_or_multiple_loads(self):
        for field in ("queued_outstanding", "running_outstanding", "callback_outstanding", "delivery_outstanding"):
            unproven = self.owner(cached=False)
            del unproven["embedding_status"]["provider"][field]
            with self.subTest(field=field), self.assertRaises(AssertionError):
                SERVICE.post_first_warmup_baseline(unproven, self.owner(loads=2))
            unproven["embedding_status"]["provider"][field] = 1
            with self.subTest(outstanding=field), self.assertRaises(AssertionError):
                SERVICE.post_first_warmup_baseline(unproven, self.owner(loads=2))
        wrong = self.owner(loads=2)
        wrong["embedding_status"]["provider"]["provider"]["factory_generation"] = "replacement"
        for old, observed in ((self.owner(), self.owner(loads=2)),
                              (self.owner(cached=False), self.owner(loads=3)),
                              (self.owner(cached=False), self.owner(loads=2, cached=False)),
                              (self.owner(cached=False), wrong)):
            with self.subTest(old=old, observed=observed), self.assertRaises(AssertionError):
                SERVICE.post_first_warmup_baseline(old, observed)
        for unproven_sessions in (None, False, 1):
            unproven = self.owner(cached=False)
            unproven["embedding_status"]["sessions"] = unproven_sessions
            with self.subTest(sessions=unproven_sessions), self.assertRaises(AssertionError):
                SERVICE.post_first_warmup_baseline(unproven, self.owner(loads=2))

    def test_failed_semantic_counter_or_snapshot_validation_retains_raw_native_reply_first(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "cohort.json"
            observed = {"resources": {"complete_future_field": [1]}, **self.owner(loads=3)}
            fixture = SimpleNamespace(single_image=False, rpc=mock.Mock(side_effect=[observed["resources"], observed["embedding_status"]]))
            with self.assertRaisesRegex(AssertionError, "additional model"):
                with SERVICE.CaseEvidence(path, 1, 64, 1, True) as journal:
                    actual = SERVICE.retained_snapshot(fixture, journal, "after_warmup")
                    SERVICE.stable_semantic_owner(self.owner(), actual)
            events = json.loads(path.read_text())["events"]
            self.assertEqual(events[0]["phase"], "after_warmup_raw")
            self.assertEqual(events[0]["native"], observed)
            self.assertEqual(events[1]["native"]["embedding_status"]["loads"], 3)
            path = Path(temporary) / "invalid.json"
            observed["embedding_status"]["loads"] = "invalid-native-counter"
            fixture.rpc = mock.Mock(side_effect=[observed["resources"], observed["embedding_status"]])
            with self.assertRaisesRegex(AssertionError, "load count"):
                with SERVICE.CaseEvidence(path, 1, 64, 1, True) as journal:
                    SERVICE.retained_snapshot(fixture, journal, "invalid_snapshot")
            events = json.loads(path.read_text())["events"]
            self.assertEqual(events[0]["native"]["embedding_status"]["loads"], "invalid-native-counter")

    def test_semantic_arguments_preserve_full_supported_inputs_and_lexical_default(self):
        lexical = SERVICE.search_arguments(3)
        semantic = SERVICE.search_arguments(3, True)
        self.assertEqual(lexical["mode"], "lexical")
        self.assertEqual(semantic, {**lexical,"mode":"vector"})
        self.assertEqual(semantic["response_mode"], "full")

    def test_no_fallback_or_pipeline_substitution_even_during_partial_semantic_coverage(self):
        self.assertTrue(SERVICE.semantic_result(self.result(), self.pipeline))
        for mutation in ("fallback","pipeline","missing_execution","overreported"):
            value = self.result()
            if mutation == "fallback": value["execution"]["served_mode"] = "lexical"
            elif mutation == "pipeline": value["semantic_readiness"]["pipeline_digest"] = "cd"*32
            elif mutation == "missing_execution": del value["execution"]
            else: value["semantic_readiness"]["completed_artifacts"] = 17
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                SERVICE.semantic_result(value, self.pipeline, require_ready=False)
        partial = self.result()
        partial["execution"]["incomplete"] = True
        partial["semantic_readiness"].update(state="partial",completed_artifacts=8)
        self.assertFalse(SERVICE.semantic_result(partial,self.pipeline,require_ready=False))
        with self.assertRaises(AssertionError):
            SERVICE.semantic_result(partial,self.pipeline)

    def test_shared_owner_load_count_is_not_client_count_or_a_global_process_absence_claim(self):
        baseline = self.owner()
        SERVICE.stable_semantic_owner(baseline, self.owner())
        for changed in (self.owner(64), self.owner(0)):
            with self.assertRaises(AssertionError):
                SERVICE.stable_semantic_owner(baseline, changed)
        changed = self.owner(); changed["embedding_status"]["provider"]["provider"]["factory_generation"] = "replacement"
        with self.assertRaises(AssertionError):
            SERVICE.stable_semantic_owner(baseline, changed)
        changed = self.owner(); changed["embedding_status"]["process_id"] = 123
        with self.assertRaises(AssertionError):
            SERVICE.semantic_provider(changed)
        changed = self.owner(); changed["embedding_status"]["provider"]["admission_open"] = False
        with self.assertRaises(AssertionError):
            SERVICE.semantic_provider(changed)

    def test_semantic_pipeline_comes_from_the_approved_embedded_cpu_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk-runtime.json").write_text('{"schema":2}')
            smoke = {"source":"embedded","engine":"onnxruntime-cpu/1.22.1","dimensions":384,
                     "repeat_equal":True,"profile":"minilm-l6-v2-u8-v1","pipeline_digest":self.pipeline,"legacy_identity":"ef"*32}
            (root / "airk-build.json").write_text(json.dumps({"embedded_model_smoke":smoke}))
            self.assertEqual(SERVICE.semantic_pipeline(root),self.pipeline)
            for key, value in (("source","ambient"),("profile","foreign"),("pipeline_digest","invalid"),("repeat_equal",False)):
                (root / "airk-build.json").write_text(json.dumps({"embedded_model_smoke":{**smoke,key:value}}))
                with self.subTest(key=key), self.assertRaises(AssertionError):
                    SERVICE.semantic_pipeline(root)
            (root / "airk-runtime.json").write_text('{"schema":1}')
            with self.assertRaises(AssertionError):
                SERVICE.semantic_pipeline(root)

    def test_setup_submits_each_model_and_vector_owner_job_once_and_waits_its_exact_receipt(self):
        fixture = SimpleNamespace(projects={"first":{"binding":"a"},"second":{"binding":"b"}})
        calls = []
        def job(name, command):
            calls.append((name,command))
            return {"id":name+command,"project":fixture.projects[name]["binding"],"state":"succeeded"}
        fixture.job = job
        fixture.configure = lambda name, draft: job(name, "vectors_enabled")
        fixture.enqueue = lambda name, kind: {"job":job(name,kind)}
        fixture.wait = lambda receipt: receipt["job"]
        fixture.published_status = lambda name, deadline: self.result()
        with mock.patch.object(SERVICE,"snapshot",side_effect=[self.owner(0),self.owner(1)]):
            result = SERVICE.prepare_semantic(fixture,["first","second"],self.pipeline)
        self.assertEqual(calls,[(name,command) for name in ("first","second") for command in ("setup-model","vectors_enabled","vectors")])
        self.assertEqual(result["model_load_delta"],1)
        self.assertEqual(len(result["jobs"]),2)
        for row in result["jobs"]:
            self.assertEqual(row["vectors_accepted"]["job"]["id"],row["vectors_completed"]["id"])
        with mock.patch.object(SERVICE,"snapshot",side_effect=[self.owner(1),self.owner(1)]):
            warmed = SERVICE.prepare_semantic(fixture,["first","second"],self.pipeline)
        self.assertEqual(warmed["model_load_delta"],0,"Already loaded reviewed MiniLM must not be loaded again for measurement")
        with mock.patch.object(SERVICE,"snapshot",side_effect=[self.owner(1),self.owner(2)]):
            with self.assertRaises(AssertionError):
                SERVICE.prepare_semantic(fixture,["first","second"],self.pipeline)


if __name__ == "__main__":
    unittest.main()
