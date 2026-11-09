# SPDX-License-Identifier: Apache-2.0
"""Cheap campaign contracts; mocked child outputs do not qualify a native host."""
import copy
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("raw_clients_campaign", ROOT / "scripts/check-raw-https-clients.py")
CAMPAIGN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAMPAIGN)


class RawHttpsClientContracts(unittest.TestCase):
    def test_full_envelope_retains_future_fields_and_documented_timing_normalization(self):
        body = {"content": [{"type": "text", "text": '{"request_id":"old","future":{"kept":true}}'}],
                "structuredContent": {"future": {"kept": True}}, "isError": False,
                "future_envelope": {"not_reduced": [1, 2]}}
        actual = copy.deepcopy(body)
        actual["content"][0]["text"] = '{"request_id":"new","future":{"kept":true}}'
        CAMPAIGN.verify_result({"exit_code": 0, "body": actual}, body)
        actual["future_envelope"]["not_reduced"].pop()
        with self.assertRaisesRegex(AssertionError, "Complete"):
            CAMPAIGN.verify_result({"exit_code": 0, "body": actual}, body)

    def test_negative_outcome_requires_nonzero_exit_and_complete_native_body(self):
        body = {"isError": True, "structuredContent": {"error": "scope_denied", "retry": False}}
        CAMPAIGN.verify_result({"exit_code": 1, "body": body}, body, True)
        with self.assertRaisesRegex(AssertionError, "exit status"):
            CAMPAIGN.verify_result({"exit_code": 0, "body": body}, body, True)

    def test_catalog_checks_contract_and_duplicate_schema_names(self):
        catalog = {"schema": "airk.local-api/1", "contract_version": 1,
                   "tools": [{"name": "read_file", "inputSchema": {"future": True}}]}
        self.assertEqual(CAMPAIGN.catalog_tools(catalog)["read_file"]["inputSchema"], {"future": True})
        catalog["tools"] *= 2
        with self.assertRaisesRegex(AssertionError, "Duplicate"):
            CAMPAIGN.catalog_tools(catalog)

    def test_missing_client_fails_before_native_work(self):
        with patch.object(CAMPAIGN.os, "name", "nt"), patch.object(CAMPAIGN.shutil, "which", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "All three"):
                CAMPAIGN.commands()

    def test_catalog_parity_requires_every_supported_read_and_complete_schema(self):
        names = {tool.removeprefix("airk_") for tool in CAMPAIGN.clients.TOOLS} | {
            "projects", "list_templates", "get_template", "validate_draft", "proposal_status", "get_handoff"}
        mcp = {name: {"name": name, "inputSchema": {"type": "object"}, "future_field": True} for name in names}
        catalog = {"schema": "airk.local-api/1", "contract_version": 1, "tools": list(mcp.values())}
        CAMPAIGN.verify_catalog(catalog, mcp)
        missing = copy.deepcopy(catalog)
        missing["tools"].pop()
        with self.assertRaisesRegex(AssertionError, "catalog|schemas"):
            CAMPAIGN.verify_catalog(missing, mcp)
        missing_mcp = copy.deepcopy(mcp)
        missing_mcp.pop(next(iter(missing_mcp)))
        with self.assertRaisesRegex(AssertionError, "catalog"):
            CAMPAIGN.verify_catalog(catalog, missing_mcp)
        extra_mcp = copy.deepcopy(mcp)
        extra_mcp["unknown_future_read"] = {"name": "unknown_future_read"}
        with self.assertRaisesRegex(AssertionError, "catalog"):
            CAMPAIGN.verify_catalog(catalog, extra_mcp)
        drift = copy.deepcopy(mcp)
        drift[next(iter(drift))]["inputSchema"]["future_schema"] = True
        with self.assertRaisesRegex(AssertionError, "schemas"):
            CAMPAIGN.verify_catalog(catalog, drift)

    def test_actual_example_process_receives_one_exact_argument_object_no_retry(self):
        output = {"content": [], "structuredContent": {"future": "retained"}, "isError": False}
        result = subprocess.CompletedProcess([], 0, json.dumps(output), "")
        with patch.object(CAMPAIGN.subprocess, "run", return_value=result) as run:
            observed = CAMPAIGN.invoke(["python", "example.py"], ("--connection", "--tool", "--arguments"),
                                       "private.json", "read_file", {"path": "fixture.py"}, "private-bearer", {})
        self.assertEqual(run.call_count, 1)
        self.assertEqual(json.loads(run.call_args.args[0][-1]), {"path": "fixture.py"})
        self.assertEqual(observed["body"], output)
        self.assertEqual((observed["dispatches"], observed["retries"]), (1, 0))

    def test_accidental_credential_echo_is_never_returned_as_retained_evidence(self):
        result = subprocess.CompletedProcess([], 1, '{"error":"private-bearer"}', "")
        with patch.object(CAMPAIGN.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(AssertionError, "output withheld"):
                CAMPAIGN.invoke(["python"], ("--connection", "--tool", "--arguments"),
                                "private.json", None, None, "private-bearer", {})

    def test_selected_cases_include_full_reads_and_scope_escape(self):
        self.assertGreater(len(CAMPAIGN.CASES), 0)
        self.assertTrue(all(arguments["response_mode"] == "full" for _, arguments, _ in CAMPAIGN.CASES))
        self.assertTrue(any(negative and arguments.get("path", "").startswith("../")
                            for _, arguments, negative in CAMPAIGN.CASES))

    def test_relocation_preserves_credential_and_authority_and_changes_instance_and_url(self):
        previous = {"schema": 1, "connection_id": "one", "credential_file": "credential", "ca_certificate_file": "ca",
                    "selection": {"kind": "scoped", "selection": {"project": "original", "expected_catalog_revision": 3}}, "endpoint": {
            "url": "https://127.0.0.1:1234/mcp", "service_instance": "before", "generation": 1,
            "installation": "install", "user": "owner", "namespace": "namespace",
            "ca_sha256": "ca", "executable_sha256": "exe"}}
        current = copy.deepcopy(previous)
        current["endpoint"].update(url="https://127.0.0.1:4321/mcp", service_instance="after", generation=2)
        CAMPAIGN.verify_relocated(previous, current, "same", "same")
        for field in current["endpoint"]:
            changed = copy.deepcopy(current)
            changed["endpoint"][field] = previous["endpoint"][field] if field in ("url", "service_instance") else "crossed"
            with self.subTest(field=field), self.assertRaises(AssertionError):
                CAMPAIGN.verify_relocated(previous, changed, "same", "same")
        with self.assertRaises(AssertionError):
            CAMPAIGN.verify_relocated(previous, current, "same", "changed")
        changed = copy.deepcopy(current)
        changed["selection"]["selection"]["project"] = "crossed"
        with self.assertRaisesRegex(AssertionError, "complete selected scope"):
            CAMPAIGN.verify_relocated(previous, changed, "same", "same")
        current["selection"]["selection"]["expected_catalog_revision"] = 4
        CAMPAIGN.verify_relocated(previous, current, "same", "same")
        current["selection"]["selection"]["expected_catalog_revision"] = 2
        with self.assertRaisesRegex(AssertionError, "complete selected scope"):
            CAMPAIGN.verify_relocated(previous, current, "same", "same")

    def test_relocation_requires_monotonic_endpoint_generation(self):
        previous = {"schema": 1, "connection_id": "one", "credential_file": "credential", "ca_certificate_file": "ca",
                    "selection": {}, "endpoint": {"url": "before", "service_instance": "before", "generation": 2,
                    "installation": "install", "user": "owner", "namespace": "namespace", "ca_sha256": "ca", "executable_sha256": "exe"}}
        current = copy.deepcopy(previous)
        current["endpoint"].update(url="after", service_instance="after")
        with self.assertRaisesRegex(AssertionError, "generation"):
            CAMPAIGN.verify_relocated(previous, current, "same", "same")
        current["endpoint"]["generation"] = 1
        with self.assertRaisesRegex(AssertionError, "generation"):
            CAMPAIGN.verify_relocated(previous, current, "same", "same")

    def test_existing_connection_uses_read_only_getter_and_full_native_validation(self):
        from unittest.mock import Mock
        connection = {"future": "full metadata"}
        fixture = Mock(control="control", install="install")
        fixture.run.return_value.stdout = json.dumps(connection)
        selection = {"kind": "scoped", "selection": {"full": True}}
        with patch.object(CAMPAIGN.transport, "validate_enrollment", return_value=connection) as validate:
            self.assertEqual(CAMPAIGN.existing_connection(fixture, selection, "existing-id"), connection)
        fixture.run.assert_called_once_with("runtime", "http-connection-info", "--install-dir", "install", "--connection-id", "existing-id")
        validate.assert_called_once_with(connection, "control", selection, "existing-id")

    def test_credential_projection_retains_generation_but_never_verifier(self):
        record = {"id": "one", "generation": 1, "selection": {"full": True}, "client": "cli",
                  "expires_unix_ms": 5000, "revoked": False, "verifier": "must-not-export"}
        with patch.object(CAMPAIGN.transport, "private_bytes", return_value=json.dumps({"records": [record]}).encode()):
            observation = CAMPAIGN.credential_observation({"connection_id": "one"}, ROOT)
        self.assertEqual(observation["generation"], 1)
        self.assertNotIn("verifier", observation)

    def test_complete_mocked_campaign_executes_all_clients_before_and_after_relocation(self):
        def result(tool, arguments):
            return {"content": [], "structuredContent": {"tool": tool, "arguments": arguments,
                                                         "future": {"retained": True}},
                    "isError": arguments.get("path", "").startswith("../")}

        catalog = {"schema": "airk.local-api/1", "contract_version": 1,
                   "tools": [{"name": tool, "inputSchema": {"type": "object"}} for tool in sorted(CAMPAIGN.READ_NAMES)]}

        class Fixture:
            instances = []
            fail_start = False

            def __init__(self, runtime, owner):
                self.runtime, self.owner = runtime, owner
                self.control = owner / "control"
                (self.control / "http-1").mkdir(parents=True)
                self.single_image, self.managed_stop, self.environment = True, None, {}
                self.scopes, self.install, self.restarted, self.close_count = {}, owner / "install", False, 0
                self.instances.append(self)

            def __enter__(self):
                return self

            def __exit__(self, *_):
                self.close()

            def register(self, name, *_):
                self.scopes[name] = {"namespace": "namespace"}

            def job(self, *_):
                pass

            def rpc(self, *_):
                return {"observation": {}}

            def arguments(self, *_):
                return []

            def run(self, *arguments, **_):
                if arguments[:2] == ("runtime", "start"):
                    if self.managed_stop is not None or self.managed_health is not None:
                        raise AssertionError("Old Stop/health evidence survived restart dispatch")
                    self.restarted = True
                    if self.fail_start:
                        raise TimeoutError("uncertain start")
                    return subprocess.CompletedProcess([], 0, '{}', '')
                return subprocess.CompletedProcess([], 0, json.dumps(result(arguments[2], json.loads(arguments[4]))), '')

            def close(self):
                self.close_count += 1
                self.managed_stop = {"complete": True, "ordinal": self.close_count}

            def enroll_http(self, selection, connection_id, *_):
                return {"schema": 1, "connection_id": connection_id, "credential_file": str(self.control / "credential"),
                        "ca_certificate_file": str(self.control / "ca.pem"),
                        "selection": selection, "endpoint": {
                            "url": "https://127.0.0.1:" + ("4321" if self.restarted else "1234") + "/mcp",
                            "service_instance": "after" if self.restarted else "before",
                            "generation": 2 if self.restarted else 1,
                            "installation": "install", "user": "owner", "namespace": "namespace",
                            "ca_sha256": "ca", "executable_sha256": "exe"}}

        class Network:
            def __init__(self, *_):
                self.pending = {}

            def send(self, request):
                if "id" in request:
                    value = {"tools": catalog["tools"]} if request["method"] == "tools/list" else (
                        result(request["params"]["name"], request["params"]["arguments"])
                        if request["method"] == "tools/call" else {})
                    self.pending[request["id"]] = {"result": value}

            def receive(self, identifier):
                return self.pending.pop(identifier)

            def api_catalog(self):
                return catalog

            def api_read(self, tool, arguments):
                if tool in ("index", "missing_fixture_tool"):
                    raise CAMPAIGN.transport.HttpRefused(404, {"error": {"code": "unknown_tool"}})
                return result(tool, arguments)

            def close(self):
                pass

        class Occupant:
            def __enter__(self):
                return self

            def __exit__(self, *_):
                pass

            def bind(self, *_): pass
            def listen(self): pass
            def setsockopt(self, *_): pass
            def setblocking(self, *_): pass
            def accept(self): raise BlockingIOError()

        def invoke(_command, _flags, _connection, tool, arguments, *_):
            body = catalog if tool is None else ({"error": {"code": "unknown_tool"}}
                    if tool in ("index", "missing_fixture_tool") else result(tool, arguments))
            return {"exit_code": int(tool in ("index", "missing_fixture_tool") or body.get("isError", False)), "body": body}

        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            runtime, owner = base / "runtime", base / "owner"
            runtime.mkdir(); owner.mkdir()
            (runtime / "airk-install.json").write_text('{}')
            (runtime / "airk-build.json").write_text('{"source":{"sha256":"original"}}')
            @contextmanager
            def owned():
                yield str(owner)
            with patch.object(CAMPAIGN, "commands", return_value={name: ([], []) for name in ("python", "node", "powershell")}), \
                    patch.object(CAMPAIGN.clients, "owned_client_fixture", owned), \
                    patch.object(CAMPAIGN.clients, "Fixture", Fixture), \
                    patch.object(CAMPAIGN.transport, "McpClient", Network), \
                    patch.object(CAMPAIGN.transport, "validate_native_identity"), \
                    patch.object(CAMPAIGN.transport, "private_bytes", return_value=b'private-bearer'), \
                    patch.object(CAMPAIGN, "credential_observation", return_value={"generation": 1}), \
                    patch.object(CAMPAIGN, "existing_connection", side_effect=lambda fixture, selection, identifier: fixture.enroll_http(selection, identifier)), \
                    patch.object(CAMPAIGN.socket, "socket", Occupant), \
                    patch.object(CAMPAIGN, "invoke", invoke):
                receipt = CAMPAIGN.qualify(runtime, base / "report.json")
                owner = base / "failed-owner"
                owner.mkdir()
                Fixture.fail_start = True
                with self.assertRaisesRegex(TimeoutError, "uncertain start"):
                    CAMPAIGN.qualify(runtime, base / "failed-report.json")
            self.assertTrue(receipt["passed"])
            self.assertEqual(receipt["selected_observation_count"], 62)
            self.assertEqual(Fixture.instances[0].close_count, 2)
            self.assertEqual(len(receipt["endpoints"]), 2)
            self.assertFalse(receipt["occupant_contacted"])
            failed = json.loads((base / "failed-report.json").read_text())
            self.assertFalse(failed["passed"])
            self.assertEqual(failed["failure_type"], "TimeoutError")
            self.assertEqual(Fixture.instances[1].close_count, 2)


if __name__ == "__main__":
    unittest.main()
