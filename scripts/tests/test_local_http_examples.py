# SPDX-License-Identifier: Apache-2.0
"""Execute the Node/Windows PowerShell examples against isolated loopback HTTP (local trust plan C; U110: no token)."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[2]
NODE = shutil.which("node")
PWSH = shutil.which("pwsh")


def protect_file(path):
    path.chmod(0o600)
    if os.name == "nt" and PWSH:
        # Only files created by this isolated fixture; never alter a user's
        # existing directory to qualify the example.
        script = "$path=[Console]::In.ReadToEnd();$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User;$acl=[Security.AccessControl.FileSecurity]::new();$acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false);$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'));Set-Acl -LiteralPath $path -AclObject $acl"
        subprocess.run([PWSH, "-NoProfile", "-NonInteractive", "-Command", script], input=str(path), text=True, capture_output=True, check=True, timeout=10)


@unittest.skipUnless(NODE, "Node needed for the actual HTTP examples")
class LocalHttpExamplesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temporary.name).resolve()

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    @contextmanager
    def server(self, mode="full"):
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
                    self.send_header("Location", "http://127.0.0.1:1/api/v1/tools/search")
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
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.02}, daemon=True)
        thread.start()
        try:
            # The published endpoint descriptor fields the examples read from
            # <control>/http-1/endpoint.json; nothing else is published there.
            yield {"endpoint": {"schema": 1, "url": "http://127.0.0.1:" + str(server.server_port) + "/mcp", "state": "ready"}}, observed
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            self.assertFalse(thread.is_alive())

    def clients(self):
        yield "node", [NODE, str(ROOT / "examples/local_http_client.cjs")], ["--control-dir", "--tool", "--arguments", "--index", "--set", "--workspace"]
        if os.name == "nt" and PWSH:
            yield "pwsh", [PWSH, "-NoLogo", "-NoProfile", "-File", str(ROOT / "examples/local_http_client.ps1")], ["-ControlDir", "-Tool", "-Arguments", "-Index", "-Set", "-Workspace"]
        else:
            with self.subTest(client="pwsh"):
                self.skipTest("PowerShell Windows protected-file reader requires Windows and pwsh")

    def control(self, connection, prefix=""):
        """A fresh service control directory holding http-1/endpoint.json alone."""
        control = self.root / ("control-" + uuid.uuid4().hex)
        native = control / "http-1"
        native.mkdir(parents=True)
        (native / "endpoint.json").write_text(json.dumps(connection["endpoint"]), encoding="utf-8")
        protect_file(native / "endpoint.json")
        return prefix + str(control)

    def run_client(self, client, connection, arguments=None, env=None, scope=None, prefix=""):
        name, command, flags = client
        run_id = uuid.uuid4().hex
        command = [*command, flags[0], self.control(connection, prefix)]
        for flag, values in zip(flags[3:], scope or ()):
            for value in ([values] if isinstance(values, str) else values):
                command += [flag, value]
        if arguments is not None:
            encoded = json.dumps(arguments)
            if len(encoded) > 20000:
                argument_file = self.root / (name + "-" + run_id + "-arguments.json")
                argument_file.write_text(encoded, encoding="utf-8")
                protect_file(argument_file)
                command += [flags[1], "search", "--arguments-file" if name == "node" else "-ArgumentsFile", str(argument_file)]
            else:
                command += [flags[1], "search", flags[2], encoded]
        # A proxy that would refuse every connection: the examples must not use one.
        environment = {**os.environ, "HTTP_PROXY": "http://127.0.0.1:1", "HTTPS_PROXY": "http://127.0.0.1:1",
                       "ALL_PROXY": "http://127.0.0.1:1", **(env or {})}
        return subprocess.run(command, capture_output=True, text=True, timeout=20, env=environment)

    def test_real_http_complete_get_post_refusal_and_tool_error_without_replay(self):
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
                    self.assertEqual(observed, [("POST", "/api/v1/tools/search", None, args)])
            with self.subTest(client=client[0], mode="catalog"), self.server() as (connection, observed):
                result = self.run_client(client, connection)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(observed, [("GET", "/api/v1/tools", None, None)])

    def test_scope_options_are_merged_into_tool_arguments(self):
        workspace = str(self.root)
        for client in self.clients():
            # PowerShell -File binds one value per parameter; Node repeats flags.
            indexes = ["alpha"] if client[0] == "pwsh" else ["alpha", "beta"]
            with self.subTest(client=client[0]), self.server() as (connection, observed):
                args = {"query": "original fictional evidence", "index_ids": ["alpha"]}
                result = self.run_client(client, connection, args, scope=(indexes, "team", workspace))
                self.assertEqual(result.returncode, 0, result.stderr)
                expected = {"query": "original fictional evidence", "index_ids": indexes, "index_set_ids": ["team"], "workspace": workspace}
                self.assertEqual(observed, [("POST", "/api/v1/tools/search", None, expected)])
            for scope in ((["alpha"], (), "relative-folder"), ((), (), str(self.root) + "-other")):
                with self.subTest(client=client[0], refused=scope), self.server() as (connection, observed):
                    result = self.run_client(client, connection, {"workspace": str(self.root)}, scope=scope)
                    self.assertEqual(result.returncode, 1)
                    self.assertEqual(observed, [])
            with self.subTest(client=client[0], refused="scope without tool"), self.server() as (connection, observed):
                result = self.run_client(client, connection, scope=(["alpha"],))
                self.assertEqual(result.returncode, 1)
                self.assertEqual(observed, [])

    def test_legacy_https_unready_or_named_host_endpoint_sends_no_request(self):
        for client in self.clients():
            for mode in ("legacy_https", "not_ready", "named_host"):
                with self.subTest(client=client[0], mode=mode), self.server() as (connection, observed):
                    endpoint = connection["endpoint"]
                    if mode == "legacy_https":
                        endpoint["url"] = endpoint["url"].replace("http://", "https://")
                    elif mode == "not_ready":
                        endpoint["state"] = "stopping"
                    else:
                        endpoint["url"] = endpoint["url"].replace("127.0.0.1", "localhost")
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

    @unittest.skipUnless(os.name == "nt", "Native Windows namespace path aliases")
    def test_native_windows_namespace_control_directory(self):
        for client in self.clients():
            with self.subTest(client=client[0]), self.server() as (connection, observed):
                result = self.run_client(client, connection, {}, prefix="\\\\?\\")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(len(observed), 1)


if __name__ == "__main__":
    unittest.main()
