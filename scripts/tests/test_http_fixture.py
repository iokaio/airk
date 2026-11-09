# SPDX-License-Identifier: Apache-2.0
"""Actual private TLS fixture: normal CA/IP checks precede bearer delivery."""
from contextlib import contextmanager
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
import re
from pathlib import Path
import shutil
import ssl
import subprocess
import tempfile
import threading
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("http_fixture", Path(__file__).parents[1] / "http_fixture.py")
HTTP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HTTP)
OPENSSL = shutil.which("openssl") or ("C:/Program Files/Git/usr/bin/openssl.exe" if Path("C:/Program Files/Git/usr/bin/openssl.exe").is_file() else None)


class ResponseEvidenceTests(unittest.TestCase):
    def client(self, status, data, headers=()):
        from unittest.mock import Mock
        client = HTTP.McpClient.__new__(HTTP.McpClient)
        client._token, client.session = "fictional-token", "fictional-session"
        client._connection_id, client._certificate_sha256 = "owned", hashlib.sha256(b"leaf").hexdigest()
        client.protocol, client.initialized = "2025-11-25", True
        response = Mock(status=status)
        response.read.return_value = data
        response.getheaders.return_value = list(headers)
        response.getheader.side_effect = lambda key: dict(headers).get(key)
        client.connection = Mock()
        client.connection.sock.getpeercert.return_value = b"leaf"
        client.connection.getresponse.return_value = response
        client.pending, client.unsolicited = {}, []
        return client

    def test_refusal_redacts_sessions_in_bodies_and_arbitrary_headers(self):
        raw = b'{"echo":"fictional-session fictional-token"}'
        client = self.client(503, raw, [("X-Echo", "fictional-session"), ("Mcp-Session-Id", "new-session")])
        with self.assertRaises(HTTP.HttpRefused) as caught:
            client._request("POST", {"id": 4, "method": "tools/call"})
        evidence = caught.exception.evidence()
        self.assertNotIn("fictional-session", json.dumps(evidence))
        self.assertNotIn("fictional-token", json.dumps(evidence))
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
        for secret in (b"fictional-token", b"fictional-session"):
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


def certificate_key_identifier(path, kind):
    """Inspect actual generated extensions with OpenSSL/LibreSSL common flags."""
    result = subprocess.run([OPENSSL, "x509", "-in", str(path), "-noout", "-text"],
                            check=True, capture_output=True, text=True, timeout=30)
    matched = re.search(r"X509v3 " + re.escape(kind) + r":[^\n]*\n\s*(?:keyid:)?([0-9a-f]{2}(?::[0-9a-f]{2})+)",
                        result.stdout, re.IGNORECASE)
    if not matched:
        raise AssertionError("Generated certificate lacks explicit " + kind)
    return bytes.fromhex(matched[1].replace(":", ""))


@unittest.skipUnless(OPENSSL, "OpenSSL required for an isolated actual TLS fixture")
class ActualHttpsFixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temporary.name).resolve()
        (cls.root / "ca.conf").write_text("[req]\nprompt=no\ndistinguished_name=dn\nx509_extensions=ca\n[dn]\nCN=AIRK fictional test issuer\n[ca]\nbasicConstraints=critical,CA:TRUE,pathlen:0\nkeyUsage=critical,keyCertSign,cRLSign\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n")
        (cls.root / "leaf.conf").write_text("basicConstraints=critical,CA:FALSE\nsubjectAltName=IP:127.0.0.1\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n")
        for command in (["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "1", "-config", "ca.conf", "-keyout", "ca.key", "-out", "ca.pem"],
                        ["req", "-new", "-newkey", "rsa:2048", "-nodes", "-sha256", "-subj", "/CN=AIRK fictional server", "-keyout", "leaf.key", "-out", "leaf.csr"],
                        ["x509", "-req", "-in", "leaf.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-out", "leaf.pem", "-days", "1", "-sha256", "-extfile", "leaf.conf"]):
            subprocess.run([OPENSSL, *command], cwd=cls.root, check=True, capture_output=True, timeout=30)
        cls.identity = "11" * 16
        (cls.root / "credential.token").write_text("airk1." + cls.identity + "." + "22" * 32)
        for name in ("ca.pem", "credential.token"):
            (cls.root / name).chmod(0o600)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_generated_chain_identifies_exact_issuer_and_preserves_strict_verification(self):
        issuer = certificate_key_identifier(self.root / "ca.pem", "Subject Key Identifier")
        self.assertEqual(len(issuer), 20)
        self.assertEqual(certificate_key_identifier(self.root / "ca.pem", "Authority Key Identifier"), issuer)
        self.assertEqual(certificate_key_identifier(self.root / "leaf.pem", "Authority Key Identifier"), issuer)
        self.assertEqual(len(certificate_key_identifier(self.root / "leaf.pem", "Subject Key Identifier")), 20)
        # This real handshake still requires the normal CA/IP checks and strict
        # RFC certificate validation, including on older Python defaults.
        with self.server() as (connection, observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                self.assertTrue(client.connection._context.verify_flags & ssl.VERIFY_X509_STRICT)
                self.assertTrue(client.connection._context.check_hostname)
                self.assertEqual(client.connection._context.verify_mode, ssl.CERT_REQUIRED)
                self.assertEqual(observed, [])
            finally:
                client.close()

    @contextmanager
    def server(self, wire=None, refusal_status=403, reflect_credential=False):
        observed = []
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def handle(self):
                try:
                    super().handle()
                except ConnectionResetError:
                    # A rejected leaf/IP pin closes TLS before authentication.
                    # Clients still assert failure and zero observed requests.
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
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(self.root / "leaf.pem", self.root / "leaf.key")
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.02}, daemon=True)
        thread.start()
        try:
            digest = lambda name: hashlib.sha256(ssl.PEM_cert_to_DER_cert((self.root / name).read_text())).hexdigest()
            yield {"schema": 1, "connection_id": self.identity, "selection": {"kind": "scoped", "selection": {"namespace": "33" * 16}},
                   "endpoint": {"schema": 1, "url": "https://127.0.0.1:" + str(server.server_port) + "/mcp", "state": "ready", "namespace": "33" * 16,
                                "ca_sha256": digest("ca.pem"), "certificate_sha256": digest("leaf.pem")},
                   "ca_certificate_file": str(self.root / "ca.pem"), "credential_file": str(self.root / "credential.token")}, observed
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=5)
            self.assertFalse(thread.is_alive())

    def test_actual_tls_jsonrpc_notification_session_delete_and_proxy_isolation(self):
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
            self.assertTrue(all(value == "Bearer " + (self.root / "credential.token").read_text() for _, value in observed))

    def test_actual_tls_sse_preserves_unsolicited_messages_and_matching_reply(self):
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

    def test_actual_tls_headers_preserve_requested_and_negotiated_versions(self):
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
                self.assertEqual((request["http_method"], request["path"], request["connection_id"]),
                                 ("POST", "/api/v1/tools/refused", connection["connection_id"]))
                self.assertEqual(request["request_sha256"], hashlib.sha256(b"{}").hexdigest())
            finally:
                client.close()

    def test_actual_http_503_preserves_diagnostics_but_redacts_reflected_credentials_and_session(self):
        with self.server(refusal_status=503, reflect_credential=True) as (connection, observed):
            client = HTTP.McpClient(connection, self.root)
            try:
                with self.assertRaises(HTTP.HttpRefused) as caught:
                    client.api_read("refused", {"future_original_arguments": [1, 2]})
                evidence = caught.exception.evidence()
                self.assertEqual(evidence["status"], 503)
                self.assertEqual(evidence["body"]["error"]["future_detail"], {"retained": True})
                self.assertTrue(evidence["body_redacted"])
                self.assertEqual(evidence["header_redactions"], ["Mcp-Session-Id", "X-Fixture-Token"])
                self.assertEqual(dict(evidence["headers"])["Mcp-Session-Id"], "[REDACTED]")
                self.assertNotIn(client._token, json.dumps(evidence))
                self.assertNotIn("private-fixture-session", json.dumps(evidence))
                raw = base64.b64decode(evidence["raw_body_base64"])
                self.assertNotIn(client._token.encode(), raw)
                self.assertEqual(evidence["raw_body_sha256"], hashlib.sha256(raw).hexdigest())
                self.assertEqual(len(observed), 1, "A complete refusal is never replayed")
            finally:
                client.close()

    def test_peer_mismatch_sends_no_bearer_and_root_escape_is_rejected(self):
        with self.server() as (connection, observed):
            connection["endpoint"]["certificate_sha256"] = "00" * 32
            with self.assertRaisesRegex(ValueError, "peer differs"):
                HTTP.McpClient(connection, self.root)
            self.assertEqual(observed, [])
            with tempfile.TemporaryDirectory() as outside:
                with self.assertRaisesRegex(ValueError, "escaped"):
                    HTTP.private_bytes(connection["credential_file"], outside, 103)

    def test_wrong_ip_certificate_is_rejected_by_normal_hostname_validation(self):
        # Use the actual issuer but a different literal IP as the TLS name. This
        # directly exercises the context's required normal hostname validation.
        with self.server() as (connection, observed):
            context = ssl.create_default_context()
            context.load_verify_locations(cafile=connection["ca_certificate_file"])
            import socket
            with socket.create_connection(("127.0.0.1", int(connection["endpoint"]["url"].split(":")[-1].split("/")[0])), timeout=5) as socket_:
                with self.assertRaises(ssl.SSLCertVerificationError):
                    context.wrap_socket(socket_, server_hostname="127.0.0.2")
            self.assertEqual(observed, [])


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
