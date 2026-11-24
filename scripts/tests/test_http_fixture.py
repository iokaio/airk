# SPDX-License-Identifier: Apache-2.0
"""Actual loopback HTTP fixture (local trust plan C): no CA, no certificate pin, no credential (U110)."""
from contextlib import contextmanager
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import socket
import tempfile
import threading
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("http_fixture", Path(__file__).parents[1] / "http_fixture.py")
HTTP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HTTP)


class ResponseEvidenceTests(unittest.TestCase):
    def client(self, status, data, headers=()):
        from unittest.mock import Mock
        client = HTTP.McpClient.__new__(HTTP.McpClient)
        client.session, client.scope = "fictional-session", {}
        client.protocol, client.initialized = "2025-11-25", True
        response = Mock(status=status)
        response.read.return_value = data
        response.getheaders.return_value = list(headers)
        response.getheader.side_effect = lambda key: dict(headers).get(key)
        client.connection = Mock()
        client.connection.getresponse.return_value = response
        client.pending, client.unsolicited = {}, []
        return client

    def test_refusal_redacts_sessions_in_bodies_and_arbitrary_headers(self):
        raw = b'{"echo":"fictional-session new-session"}'
        client = self.client(503, raw, [("X-Echo", "fictional-session"), ("Mcp-Session-Id", "new-session")])
        with self.assertRaises(HTTP.HttpRefused) as caught:
            client._request("POST", {"id": 4, "method": "tools/call"})
        evidence = caught.exception.evidence()
        self.assertNotIn("fictional-session", json.dumps(evidence))
        self.assertNotIn("new-session", json.dumps(evidence))
        self.assertTrue(evidence["body_redacted"])
        self.assertEqual(evidence["header_redactions"], ["X-Echo", "Mcp-Session-Id"])
        self.assertEqual(evidence["request"]["jsonrpc_id"], 4)

    def test_oversize_response_is_typed_bounded_uncertain_evidence(self):
        client = self.client(200, b"fictional-session " + b"x" * HTTP.MAX_RESPONSE_BYTES)
        with self.assertRaises(HTTP.HttpResponseError) as caught:
            client._request("POST", {"id": 8, "method": "tools/call"})
        evidence = caught.exception.evidence()
        self.assertEqual(evidence["failure"], "response_limit")
        self.assertTrue(evidence["body_truncated"])
        self.assertEqual(evidence["submission"], "uncertain")
        self.assertLessEqual(len(caught.exception.raw_body), HTTP.MAX_RESPONSE_BYTES)
        self.assertEqual(evidence["request"]["jsonrpc_id"], 8)

    def test_oversize_response_redacts_secret_prefix_cut_by_actual_read_bound(self):
        for secret in (b"fictional-session",):
            with self.subTest(secret=secret):
                data = b"x" * (HTTP.MAX_RESPONSE_BYTES-4) + secret[:5]
                client = self.client(200, data)
                with self.assertRaises(HTTP.HttpResponseError) as caught:
                    client._request("POST", {"id": 8, "method": "tools/call"})
                self.assertTrue(secret[:4] not in caught.exception.raw_body, "retained prefix contains secret bytes")
                self.assertTrue(caught.exception.body_redacted)

    def test_truncation_without_secret_is_not_reported_as_redaction(self):
        client = self.client(200, b"x" * (HTTP.MAX_RESPONSE_BYTES+1))
        with self.assertRaises(HTTP.HttpResponseError) as caught:
            client._request("POST", {"id": 8, "method": "tools/call"})
        self.assertTrue(caught.exception.body_truncated)
        self.assertFalse(caught.exception.body_redacted)

    def test_http_200_rpc_error_retains_response_and_request(self):
        raw = json.dumps({"jsonrpc": "2.0", "id": 9, "error": {"code": -32000, "data": "fictional-session"}}).encode()
        client = self.client(200, raw, [("Content-Type", "application/json")])
        with self.assertRaises(HTTP.HttpResponseError) as caught:
            client.send({"jsonrpc": "2.0", "id": 9, "method": "tools/call"})
        evidence = caught.exception.evidence()
        self.assertEqual(evidence["failure"], "jsonrpc_error")
        self.assertEqual(evidence["body"]["error"]["code"], -32000)
        self.assertEqual(evidence["request"]["jsonrpc_id"], 9)
        self.assertNotIn("fictional-session", json.dumps(evidence))

    def test_scope_is_added_only_to_evidence_reads_that_name_none(self):
        scope = {"index_ids": ["idx_fixture"]}
        self.assertEqual(HTTP.scoped_arguments("search", {"query": "q"}, scope), {"query": "q", "index_ids": ["idx_fixture"]})
        self.assertEqual(HTTP.scoped_arguments("airk_read_file", {}, scope), {"index_ids": ["idx_fixture"]})
        for named in ({"index_ids": ["other"]}, {"index_set_ids": ["set"]}, {"workspace": "C:/work"}):
            self.assertIs(HTTP.scoped_arguments("search", named, scope), named)
        for unscoped in ("help", "airk_help", "list_scopes", "list_templates", "get_template", "draft_search", "suggest_view"):
            arguments = {}
            self.assertIs(HTTP.scoped_arguments(unscoped, arguments, scope), arguments)
        original = {"query": "q"}
        HTTP.scoped_arguments("search", original, scope)
        self.assertEqual(original, {"query": "q"})
        self.assertIs(HTTP.scoped_arguments("search", original, {}), original)

    def test_scope_value_requires_bounded_names_or_an_absolute_workspace(self):
        self.assertEqual(HTTP.scope_value(index_ids=["a", "b"]), {"index_ids": ["a", "b"]})
        self.assertEqual(HTTP.scope_value(index_set_ids=["s"]), {"index_set_ids": ["s"]})
        absolute = str(Path(tempfile.gettempdir()).resolve())
        self.assertEqual(HTTP.scope_value(workspace=absolute), {"workspace": absolute})
        for invalid in ({"index_ids": ["x"] * 33}, {"index_ids": [""]}, {"index_ids": "idx"},
                        {"workspace": "relative/folder"}):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                HTTP.scope_value(**invalid)

    def test_tools_call_gets_the_client_scope_without_changing_the_callers_request(self):
        client = self.client(200, b"")
        client.scope = {"index_ids": ["idx_fixture"]}
        request = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search", "arguments": {"query": "q"}}}
        self.assertEqual(client.with_scope(request)["params"]["arguments"], {"query": "q", "index_ids": ["idx_fixture"]})
        self.assertEqual(request["params"]["arguments"], {"query": "q"})
        listing = {"jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {}}
        self.assertIs(client.with_scope(listing), listing)
        scopes = {"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "list_scopes", "arguments": {}}}
        self.assertIs(client.with_scope(scopes), scopes)

    def test_shared_classifier_rejects_unknown_status_and_submitted_outcomes(self):
        for status, code in ((429, "query_admission_busy"), (503, "authentication_busy"), (503, "native_validation_busy")):
            body = {"schema": "airk.local-api-error/1", "error": {"code": code}}
            self.assertIsNotNone(HTTP.classify_refusal(HTTP.HttpRefused(status, body)))
            self.assertIsNone(HTTP.classify_refusal(HTTP.HttpRefused(401, body)))
            body["query_submitted"] = True
            self.assertIsNone(HTTP.classify_refusal(HTTP.HttpRefused(status, body)))

    def test_phase_accounting_has_no_success_threshold_and_keeps_unexecuted_calls(self):
        outcomes = [{"outcome": "refused", "code": "authentication_busy"}] * 64
        phase = HTTP.phase_counts(outcomes, 64)
        self.assertTrue(phase["passed"])
        self.assertEqual(phase["succeeded"], 0)
        self.assertEqual(phase["refused_by_code"], {"authentication_busy": 64})
        self.assertEqual(HTTP.phase_counts(outcomes[:1], 64)["not_executed"], 63)
        self.assertFalse(HTTP.phase_counts(outcomes + [{"outcome": "uncertain"}], 65)["passed"])
        self.assertFalse(HTTP.phase_counts(outcomes + [{"outcome": "failed"}], 65)["passed"])
        with self.assertRaisesRegex(ValueError, "not documented"):
            HTTP.phase_counts([{"outcome": "refused", "code": "unknown"}], 1)

    def test_unknown_refusals_are_failures_and_response_limit_is_uncertain(self):
        body = {"schema": "airk.local-api-error/1", "error": {"code": "native_delivery_fenced"}}
        self.assertEqual(HTTP.error_outcome(HTTP.HttpRefused(503, body)), "failed")
        self.assertEqual(HTTP.error_outcome(HTTP.HttpResponseError(200, {}, submission="uncertain")), "uncertain")
        self.assertEqual(HTTP.error_outcome(ConnectionResetError()), "uncertain")


class ActualHttpFixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temporary.name).resolve()
        cls.identity = "11" * 16

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_plain_http_client_holds_no_tls_context_and_sends_nothing_on_construction(self):
        with self.server() as (connection, observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                self.assertIsInstance(client.connection, HTTP.http.client.HTTPConnection)
                self.assertNotIsInstance(client.connection, HTTP.http.client.HTTPSConnection)
                self.assertEqual((client.connection.host, client.connection.port),
                                 ("127.0.0.1", int(connection["endpoint"]["url"].split(":")[-1].split("/")[0])))
                self.assertIsNotNone(client.connection.sock, "The endpoint is reached before any request")
                self.assertEqual(observed, [])
            finally:
                client.close()
            self.assertEqual(observed, [], "No MCP session was created, so nothing is deleted")

    @contextmanager
    def server(self, wire=None, refusal_status=403, reflect_credential=False):
        observed = []
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def handle(self):
                try:
                    super().handle()
                except ConnectionResetError:
                    # A client that refuses an endpoint may drop the
                    # connection; tests still assert zero observed requests.
                    pass
            def log_message(self, *_args):
                pass
            def do_POST(self):
                value = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if wire is not None:
                    wire.append((value, self.headers.get("MCP-Protocol-Version")))
                observed.append((value.get("method"), self.headers.get("Authorization")))
                if self.path == "/mcp" and (self.headers.get("MCP-Protocol-Version") or "") >= "2026-07-28":
                    expected = HTTP.standard_headers(value, self.headers["MCP-Protocol-Version"])
                    if any(self.headers.get(key) != item for key, item in expected.items()):
                        data = b'{"error":"missing or mismatched standard routing header"}'
                        self.send_response(400); self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data); return
                if self.path.startswith("/api/v1/tools/"):
                    if self.path.endswith("/refused"):
                        body = {"error":{"code":"fixture_refused","future_detail":{"retained":True}}}
                        if reflect_credential:
                            body["reflected"] = self.headers.get("Authorization")
                        data = json.dumps(body).encode()
                        self.send_response(refusal_status); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(data)))
                        self.send_header("X-Fixture-Future", "original-first"); self.send_header("X-Fixture-Future", "original-second")
                        if reflect_credential:
                            self.send_header("Mcp-Session-Id", "private-fixture-session")
                            if self.headers.get("Authorization") is not None:
                                self.send_header("X-Fixture-Token", self.headers.get("Authorization"))
                        self.end_headers(); self.wfile.write(data); return
                    data = json.dumps({"content": [], "structuredContent": {"arguments": value, "future_field": {"retained": True}}, "isError": False}).encode()
                    self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data); return
                if "id" not in value:
                    self.send_response(202); self.send_header("Content-Length", "0"); self.end_headers(); return
                requested = value.get("params", {}).get("protocolVersion")
                negotiated = "2024-11-05" if requested == "2024-11-05" else "2025-11-25"
                data = json.dumps({"jsonrpc": "2.0", "id": value["id"], "result": {"fixture": True, "protocolVersion": negotiated, "serverInfo": {"version": "1.8.14"}}}).encode()
                self.send_response(200)
                mime = "application/json"
                if value.get("method") == "initialize":
                    # rmcp's real stateful initialize is SSE even when later
                    # tool calls use the JSON-response configuration.
                    data = b"event: message\ndata: " + data + b"\n\n"
                    mime = "text/event-stream; charset=utf-8"
                elif value.get("method") == "sse-multiple":
                    notification = {"jsonrpc":"2.0","method":"notifications/message","params":{"complete":{"retained":True}}}
                    data = b": heartbeat\r\n\r\ndata: " + json.dumps(notification).encode() + b"\r\n\r\ndata: " + data + b"\r\n\r\n"
                    mime = "text/event-stream"
                self.send_header("Content-Type", mime)
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Mcp-Session-Id", "owned-fixture-session")
                self.end_headers(); self.wfile.write(data)
            def do_DELETE(self):
                observed.append(("DELETE", self.headers.get("Authorization")))
                self.send_response(204); self.send_header("Content-Length", "0"); self.end_headers()
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.02}, daemon=True)
        thread.start()
        try:
            # The legacy (pre-U110) connection shape other suites still consume;
            # McpClient reads only its embedded endpoint.
            yield {"schema": 1, "connection_id": self.identity, "selection": {"kind": "scoped", "selection": {"namespace": "33" * 16}},
                   "endpoint": {"schema": 1, "url": "http://127.0.0.1:" + str(server.server_port) + "/mcp", "state": "ready",
                                "namespace": "33" * 16}}, observed
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=5)
            self.assertFalse(thread.is_alive())

    def test_actual_http_jsonrpc_notification_session_delete_and_proxy_isolation(self):
        with self.server() as (connection, observed), patch.dict(os.environ, {"HTTP_PROXY": "http://127.0.0.1:1", "HTTPS_PROXY": "http://127.0.0.1:1", "ALL_PROXY": "http://127.0.0.1:1"}):
            client = HTTP.McpClient(connection, self.root)
            try:
                client.send({"jsonrpc": "2.0", "id": 1, "method": "initialize"})
                self.assertTrue(client.receive(1)["result"]["fixture"])
                client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
                client.send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
                self.assertEqual(client.receive(2)["id"], 2)
            finally:
                client.close()
            self.assertEqual([row[0] for row in observed], ["initialize", "notifications/initialized", "tools/list", "DELETE"])
            # U110: no request carries Authorization.
            self.assertEqual([value for _, value in observed], [None] * 4)

    def test_endpoint_record_alone_connects_tokenless_with_per_call_scope(self):
        with self.server(wire := []) as (connection, observed), tempfile.TemporaryDirectory() as directory:
            control = Path(directory).resolve()
            (control / "http-1").mkdir()
            (control / "http-1" / "endpoint.json").write_text(json.dumps(connection["endpoint"]), encoding="utf-8")
            (control / "http-1" / "endpoint.json").chmod(0o600)
            self.assertEqual([path.name for path in (control / "http-1").iterdir()], ["endpoint.json"])
            endpoint = HTTP.read_endpoint(control)
            self.assertEqual(endpoint, connection["endpoint"])
            client = HTTP.McpClient(endpoint, control, scope={"index_ids": ["idx_fixture"]})
            self.assertEqual(client.connection.timeout, 125, "120-second native operation plus five transport seconds")
            try:
                client.send({"jsonrpc": "2.0", "id": 1, "method": "initialize"})
                client.receive(1)
                client.send({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "search", "arguments": {"query": "q"}}})
                client.receive(2)
                client.send({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search", "arguments": {}}}, scoped=False)
                client.receive(3)
                self.assertEqual(client.api_read("search", {"query": "q"})["structuredContent"]["arguments"],
                                 {"query": "q", "index_ids": ["idx_fixture"]})
                self.assertEqual(client.api_read("search", {}, scoped=False)["structuredContent"]["arguments"], {})
                self.assertEqual(client.api_read("list_scopes", {})["structuredContent"]["arguments"], {})
            finally:
                client.close()
            short = HTTP.McpClient(endpoint, control, timeout=3, scope={"index_ids": ["idx_fixture"]})
            try:
                self.assertEqual(short.connection.timeout, 3, "An explicit deadline must not be extended")
            finally:
                short.close()
            calls = [value for value, _ in wire if isinstance(value, dict) and value.get("method") == "tools/call"]
            self.assertEqual([call["params"]["arguments"] for call in calls], [{"query": "q", "index_ids": ["idx_fixture"]}, {}])
            self.assertTrue(all(value is None for _, value in observed))

    def test_https_legacy_unready_or_named_host_endpoint_is_refused_before_connecting(self):
        with self.server() as (connection, observed):
            endpoint = connection["endpoint"]
            for invalid in ({**endpoint, "state": "stopped"}, {**endpoint, "url": endpoint["url"].replace("127.0.0.1", "localhost")},
                            {**endpoint, "url": endpoint["url"].replace("http://", "https://")}, {**endpoint, "schema": 2}):
                with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, "invalid or unavailable"):
                    HTTP.McpClient(invalid, self.root)
            self.assertEqual(observed, [])

    def test_actual_http_sse_preserves_unsolicited_messages_and_matching_reply(self):
        with self.server() as (connection, _observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                client.send({"jsonrpc":"2.0","id":1,"method":"initialize"})
                self.assertEqual(client.receive(1)["result"]["protocolVersion"], "2025-11-25")
                client.send({"jsonrpc":"2.0","method":"notifications/initialized"})
                client.send({"jsonrpc":"2.0","id":2,"method":"sse-multiple"})
                self.assertTrue(client.receive(2)["result"]["fixture"])
                self.assertEqual(client.unsolicited,[{"jsonrpc":"2.0","method":"notifications/message","params":{"complete":{"retained":True}}}])
            finally:
                client.close()

    def test_standard_headers_preserve_method_name_and_encoded_values(self):
        body = {"method": "tools/call", "params": {"name": "airk_search"}}
        self.assertEqual(HTTP.standard_headers(body, "2025-11-25"), {})
        self.assertEqual(HTTP.standard_headers(body, "2026-07-28"), {"Mcp-Method": "tools/call", "Mcp-Name": "airk_search"})
        self.assertEqual(HTTP.standard_headers({"method": "resources/read", "params": {"uri": "\u00e9"}}, "2026-07-28")["Mcp-Name"], "=?base64?w6k=?=")
        self.assertEqual(body, {"method": "tools/call", "params": {"name": "airk_search"}})

    def test_actual_http_headers_preserve_requested_and_negotiated_versions(self):
        for requested, negotiated in [("2024-11-05", "2024-11-05"), ("2025-11-25", "2025-11-25"), ("2026-07-28", "2025-11-25")]:
            with self.subTest(requested=requested), self.server(wire := []) as (connection, _observed):
                client = HTTP.McpClient(connection, self.root)
                initialize = {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":requested,"capabilities":{},"clientInfo":{"name":"header-fixture","version":"1"}}}
                try:
                    client.send(initialize)
                    self.assertEqual(client.receive(1)["result"]["protocolVersion"], negotiated)
                    client.send({"jsonrpc":"2.0","id":2,"method":"ping"})
                    client.receive(2)
                    inline = {"jsonrpc":"2.0","id":3,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}
                    client.send(inline)
                    client.receive(3)
                    client.send({"jsonrpc":"2.0","id":4,"method":"tools/list"})
                    client.receive(4)
                    self.assertEqual(wire, [(initialize, None), ({"jsonrpc":"2.0","id":2,"method":"ping"}, negotiated), (inline, "2026-07-28"), ({"jsonrpc":"2.0","id":4,"method":"tools/list"}, negotiated)])
                    self.assertEqual(client.protocol, negotiated)
                finally:
                    client.close()

    def test_discovery_session_does_not_pretend_initialize_already_negotiated(self):
        with self.server(wire := []) as (connection, _observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                client.send({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}})
                client.receive(1)
                self.assertIsNotNone(client.session)
                self.assertFalse(client.initialized)
                initialize = {"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}
                client.send(initialize)
                self.assertEqual(client.receive(2)["result"]["protocolVersion"], "2024-11-05")
                self.assertEqual(wire[-1], (initialize, None))
                self.assertTrue(client.initialized)
            finally:
                client.close()

    def test_actual_http_refusal_preserves_full_body_without_headers_in_error_text(self):
        with self.server() as (connection, _observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                with self.assertRaises(HTTP.HttpRefused) as caught:
                    client.api_read("refused", {})
                self.assertEqual(caught.exception.status,403)
                self.assertEqual(caught.exception.body,{"error":{"code":"fixture_refused","future_detail":{"retained":True}}})
                self.assertNotIn("airk1.",str(caught.exception))
                evidence = caught.exception.evidence()
                self.assertEqual(evidence["error_code"], "fixture_refused")
                self.assertEqual([value for name,value in evidence["headers"] if name == "X-Fixture-Future"],
                                 ["original-first", "original-second"])
                self.assertEqual(json.loads(base64.b64decode(evidence["raw_body_base64"])), caught.exception.body)
                request = evidence["request"]
                self.assertEqual((request["http_method"], request["path"]), ("POST", "/api/v1/tools/refused"))
                self.assertNotIn("connection_id", request)
                self.assertEqual(request["request_sha256"], hashlib.sha256(b"{}").hexdigest())
            finally:
                client.close()

    def test_actual_http_503_preserves_diagnostics_and_redacts_the_reflected_session(self):
        with self.server(refusal_status=503, reflect_credential=True) as (connection, observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                with self.assertRaises(HTTP.HttpRefused) as caught:
                    client.api_read("refused", {"future_original_arguments": [1, 2]})
                evidence = caught.exception.evidence()
                self.assertEqual(evidence["status"], 503)
                self.assertEqual(evidence["body"]["error"]["future_detail"], {"retained": True})
                # U110: nothing to reflect; no Authorization was sent.
                self.assertIsNone(evidence["body"]["reflected"])
                self.assertEqual(evidence["header_redactions"], ["Mcp-Session-Id"])
                self.assertEqual(dict(evidence["headers"])["Mcp-Session-Id"], "[REDACTED]")
                self.assertNotIn("private-fixture-session", json.dumps(evidence))
                raw = base64.b64decode(evidence["raw_body_base64"])
                self.assertEqual(evidence["raw_body_sha256"], hashlib.sha256(raw).hexdigest())
                self.assertEqual(len(observed), 1, "A complete refusal is never replayed")
            finally:
                client.close()

    def test_endpoint_root_escape_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            control = Path(directory).resolve()
            (control / "http-1").mkdir()
            (control / "http-1" / "endpoint.json").write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "escaped"):
                HTTP.private_bytes(control / "http-1" / "endpoint.json", outside, 8192)

    def test_unreachable_endpoint_fails_on_construction(self):
        with socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
            endpoint = {"schema": 1, "url": "http://127.0.0.1:" + str(port) + "/mcp", "state": "ready"}
            with self.assertRaises(OSError):
                HTTP.McpClient(endpoint, self.root, timeout=3)


class ResponseParserTests(unittest.TestCase):
    def test_invalid_explicit_protocol_never_reaches_http(self):
        for invalid in [None, 2026, "", "bad\r\nheader", "x" * 65]:
            with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, "protocol header"):
                HTTP.request_protocol({"params":{"_meta":{"io.modelcontextprotocol/protocolVersion":invalid}}}, True, "2025-11-25")
        self.assertEqual(HTTP.request_protocol(None, True, "2024-11-05"), "2024-11-05")

    def test_multiline_crlf_priming_and_all_events_are_preserved(self):
        wire = b": keepalive\r\nretry: 1000\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notification\"}\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\r\ndata: \"id\":1,\"result\":{\"future\":true}}\r\n\r\n"
        self.assertEqual(HTTP.response_messages(wire, "text/event-stream"), [
            {"jsonrpc":"2.0","method":"notification"}, {"jsonrpc":"2.0","id":1,"result":{"future":True}}])
        self.assertEqual(HTTP.response_messages(b'{"full":{"retained":true}}', "application/json; charset=utf-8"), [{"full":{"retained":True}}])
        self.assertEqual(HTTP.response_messages(b"", None), [])

    def test_malformed_truncated_and_excessive_messages_refuse(self):
        for wire, mime in [(b"data: {bad}\n\n", "text/event-stream"),
                           (b'data: {"id":1}', "text/event-stream"),
                           (b"data: \xff\n\n", "text/event-stream"),
                           (b'{}', "text/plain"),
                           (b"data: {}\n\n" * 129, "text/event-stream"),
                           (b" " * 1_048_577, "application/json")]:
            with self.subTest(wire_bytes=len(wire), mime=mime), self.assertRaises((ValueError, UnicodeError)):
                HTTP.response_messages(wire, mime)


if __name__ == "__main__":
    unittest.main()
