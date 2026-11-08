# SPDX-License-Identifier: Apache-2.0
"""Execute the Node/Windows PowerShell examples against isolated actual TLS."""
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import shutil
import ssl
import subprocess
import threading
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("raw_examples_tls_base", ROOT / "scripts/tests/test_http_fixture.py")
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)
NODE = shutil.which("node")
PWSH = shutil.which("pwsh")


def protect_file(path):
    path.chmod(0o600)
    if os.name == "nt" and PWSH:
        # Only files created by this isolated fixture; never alter a user's
        # existing directory or trust store to qualify the example.
        script = "$path=[Console]::In.ReadToEnd();$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User;$acl=[Security.AccessControl.FileSecurity]::new();$acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false);$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'));Set-Acl -LiteralPath $path -AclObject $acl"
        subprocess.run([PWSH, "-NoProfile", "-NonInteractive", "-Command", script], input=str(path), text=True, capture_output=True, check=True, timeout=10)


@unittest.skipUnless(BASE.OPENSSL and NODE, "Node/OpenSSL needed for actual TLS examples")
class LocalHttpsExamplesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        BASE.ActualHttpsFixtureTests.setUpClass.__func__(cls)
        # Same issuer, wrong IP: this must fail name validation before any header.
        (cls.root / "wrong.conf").write_text("basicConstraints=critical,CA:FALSE\nsubjectAltName=IP:127.0.0.2\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n")
        subprocess.run([BASE.OPENSSL, "x509", "-req", "-in", "leaf.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-out", "wrong.pem", "-days", "1", "-sha256", "-extfile", "wrong.conf"], cwd=cls.root, check=True, capture_output=True, timeout=30)
        subprocess.run([BASE.OPENSSL, "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "1", "-config", "ca.conf", "-keyout", "other.key", "-out", "other.pem"], cwd=cls.root, check=True, capture_output=True, timeout=30)
        (cls.root / "other.pem").chmod(0o600)
        for name in ("ca.pem", "other.pem", "credential.token"):
            protect_file(cls.root / name)

    tearDownClass = classmethod(BASE.ActualHttpsFixtureTests.tearDownClass.__func__)

    def test_generated_wrong_ip_and_same_named_other_ca_have_explicit_key_identities(self):
        issuer = BASE.certificate_key_identifier(self.root / "ca.pem", "Subject Key Identifier")
        other = BASE.certificate_key_identifier(self.root / "other.pem", "Subject Key Identifier")
        self.assertNotEqual(issuer, other)
        self.assertEqual(BASE.certificate_key_identifier(self.root / "other.pem", "Authority Key Identifier"), other)
        self.assertEqual(BASE.certificate_key_identifier(self.root / "wrong.pem", "Authority Key Identifier"), issuer)
        self.assertEqual(BASE.certificate_key_identifier(self.root / "wrong.pem", "Subject Key Identifier"),
                         BASE.certificate_key_identifier(self.root / "leaf.pem", "Subject Key Identifier"))

    @contextmanager
    def server(self, mode="full", wrong_ip=False):
        observed = []

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args):
                pass

            def handle(self):
                try:
                    super().handle()
                except (ConnectionResetError, BrokenPipeError):
                    pass

            def reply(self, arguments=None):
                observed.append((self.command, self.path, self.headers.get("Authorization"), arguments))
                if mode == "redirect":
                    self.send_response(302)
                    self.send_header("Location", "https://127.0.0.1:1/api/v1/tools/search")
                    data = b'{"error":{"code":"fixture_redirect"}}'
                elif mode == "huge":
                    self.send_response(200)
                    data = b'"' + b"x" * 1048576 + b'"'
                else:
                    self.send_response(403 if mode == "refused" else 200)
                    data = json.dumps({"error": {"code": "fixture_refused", "future_detail": {"retained": True}}} if mode == "refused" else {"content": [], "structuredContent": {"arguments": arguments, "future_field": {"retained": True}}, "isError": mode == "tool_error"}).encode()
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                self.reply()

            def do_POST(self):
                self.reply(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(self.root / ("wrong.pem" if wrong_ip else "leaf.pem"), self.root / "leaf.key")
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.02}, daemon=True)
        thread.start()
        digest = lambda name: hashlib.sha256(ssl.PEM_cert_to_DER_cert((self.root / name).read_text())).hexdigest()
        try:
            yield {"schema": 1, "connection_id": self.identity, "selection": {"kind": "scoped", "selection": {"namespace": "33" * 16}}, "endpoint": {"schema": 1, "url": "https://127.0.0.1:" + str(server.server_port) + "/mcp", "state": "ready", "ca_sha256": digest("ca.pem")}, "ca_certificate_file": str(self.root / "ca.pem"), "credential_file": str(self.root / "credential.token")}, observed
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            self.assertFalse(thread.is_alive())

    def clients(self):
        yield "node", [NODE, str(ROOT / "examples/local_https_client.cjs")], ["--connection", "--tool", "--arguments"]
        if os.name == "nt" and PWSH:
            yield "pwsh", [PWSH, "-NoLogo", "-NoProfile", "-File", str(ROOT / "examples/local_https_client.ps1")], ["-Connection", "-Tool", "-Arguments"]
        else:
            with self.subTest(client="pwsh"):
                self.skipTest("PowerShell Windows protected-file reader requires Windows and pwsh")

    def run_client(self, client, connection, arguments=None, env=None):
        name, command, flags = client
        run_id = uuid.uuid4().hex
        saved = self.root / (name + "-" + run_id + "-connection.json")
        saved.write_text(json.dumps(connection), encoding="utf-8")
        protect_file(saved)
        command = [*command, flags[0], str(saved)]
        if arguments is not None:
            encoded = json.dumps(arguments)
            if len(encoded) > 20000:
                argument_file = self.root / (name + "-" + run_id + "-arguments.json")
                argument_file.write_text(encoded, encoding="utf-8")
                protect_file(argument_file)
                command += [flags[1], "search", "--arguments-file" if name == "node" else "-ArgumentsFile", str(argument_file)]
            else:
                command += [flags[1], "search", flags[2], encoded]
        environment = {**os.environ, "HTTP_PROXY": "http://127.0.0.1:1", "HTTPS_PROXY": "http://127.0.0.1:1", **(env or {})}
        result = subprocess.run(command, capture_output=True, text=True, timeout=20, env=environment)
        self.assertNotIn((self.root / "credential.token").read_text(), result.stdout + result.stderr)
        return result

    def test_real_tls_complete_get_post_refusal_and_tool_error_without_replay(self):
        for client in self.clients():
            for mode in ("full", "refused", "tool_error"):
                with self.subTest(client=client[0], mode=mode), self.server(mode) as (connection, observed):
                    args = {"query": "original fictional evidence", "response_mode": "full"}
                    result = self.run_client(client, connection, args)
                    self.assertEqual(result.returncode, 0 if mode == "full" else 1, result.stderr)
                    body = json.loads(result.stdout)
                    if mode == "refused":
                        self.assertEqual(body, {"error": {"code": "fixture_refused", "future_detail": {"retained": True}}})
                    else:
                        self.assertEqual(body, {"content": [], "structuredContent": {"arguments": args, "future_field": {"retained": True}}, "isError": mode == "tool_error"})
                    self.assertEqual(observed, [("POST", "/api/v1/tools/search", "Bearer " + (self.root / "credential.token").read_text(), args)])
            with self.subTest(client=client[0], mode="catalog"), self.server() as (connection, observed):
                result = self.run_client(client, connection)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(observed[0][:2], ("GET", "/api/v1/tools"))
                self.assertEqual(len(observed), 1)

    def test_ca_and_credential_identity_crosses_and_wrong_ip_send_no_headers(self):
        for client in self.clients():
            for mode in ("ca_digest", "credential", "untrusted", "ip"):
                with self.subTest(client=client[0], mode=mode), self.server(wrong_ip=mode == "ip") as (connection, observed):
                    if mode == "ca_digest":
                        connection["endpoint"]["ca_sha256"] = "00" * 32
                    elif mode == "credential":
                        connection["connection_id"] = "44" * 16
                    elif mode == "untrusted":
                        connection["ca_certificate_file"] = str(self.root / "other.pem")
                        connection["endpoint"]["ca_sha256"] = hashlib.sha256(ssl.PEM_cert_to_DER_cert((self.root / "other.pem").read_text())).hexdigest()
                    result = self.run_client(client, connection, {})
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(observed, [])

    def test_redirect_not_followed_and_reply_and_request_budgets_hold(self):
        for client in self.clients():
            for mode in ("redirect", "huge", "request_limit"):
                with self.subTest(client=client[0], mode=mode), self.server(mode) as (connection, observed):
                    result = self.run_client(client, connection, {"query": "x" * 65536} if mode == "request_limit" else {})
                    self.assertEqual(result.returncode, 1)
                    self.assertEqual(len(observed), 0 if mode == "request_limit" else 1)
                    if mode == "redirect":
                        self.assertEqual(json.loads(result.stdout), {"error": {"code": "fixture_redirect"}})

    def test_node_preserves_existing_extra_ca_roots(self):
        # Trust the actual fixture issuer through an existing Node extra-root
        # source while the native metadata names a different valid local CA.
        with self.server() as (connection, observed):
            connection["ca_certificate_file"] = str(self.root / "other.pem")
            connection["endpoint"]["ca_sha256"] = hashlib.sha256(ssl.PEM_cert_to_DER_cert((self.root / "other.pem").read_text())).hexdigest()
            result = self.run_client(next(self.clients()), connection, {}, {"NODE_EXTRA_CA_CERTS": str(self.root / "ca.pem")})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(observed), 1)

    @unittest.skipUnless(os.name == "nt", "Native Windows namespace path aliases")
    def test_native_windows_namespace_file_references(self):
        for client in self.clients():
            with self.subTest(client=client[0]), self.server() as (connection, observed):
                for key in ("ca_certificate_file", "credential_file"):
                    connection[key] = "\\\\?\\" + connection[key]
                result = self.run_client(client, connection, {})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(len(observed), 1)


if __name__ == "__main__":
    unittest.main()
