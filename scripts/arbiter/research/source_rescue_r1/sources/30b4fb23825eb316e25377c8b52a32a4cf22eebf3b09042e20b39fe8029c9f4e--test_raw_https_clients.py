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
        # U110: the local agent session lists scopes; there is no bound project.
        names = {tool.removeprefix("airk_") for tool in CAMPAIGN.clients.TOOLS} | {
            "list_scopes", "list_templates", "get_template", "validate_draft", "proposal_status", "get_handoff"}
        self.assertNotIn("projects", CAMPAIGN.READ_NAMES)
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
        flags = ("--control-dir", "--tool", "--arguments", "--index", "--workspace")
        with patch.object(CAMPAIGN.subprocess, "run", return_value=result) as run:
            observed = CAMPAIGN.invoke(["python", "example.py"], flags, "C:/owned/control", "read_file",
                                       {"path": "fixture.py"}, {}, index="idx_fixture")
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args.args[0], ["python", "example.py", "--control-dir", "C:/owned/control", "--tool", "read_file",
                                                 "--arguments", '{"path":"fixture.py"}', "--index", "idx_fixture"])
        self.assertEqual(observed["body"], output)
        self.assertEqual((observed["dispatches"], observed["retries"]), (1, 0))

    def test_examples_take_the_control_directory_and_scope_flags_without_a_token(self):
        with patch.object(CAMPAIGN.os, "name", "nt"), patch.object(CAMPAIGN.shutil, "which", side_effect=lambda name: name):
            selected = CAMPAIGN.commands()
        self.assertEqual(selected["python"][1], ("--control-dir", "--tool", "--arguments", "--index", "--workspace"))
        self.assertEqual(selected["node"][1], ("--control-dir", "--tool", "--arguments", "--index", "--workspace"))
        self.assertEqual(selected["powershell"][1], ("-ControlDir", "-Tool", "-Arguments", "-Index", "-Workspace"))
        result = subprocess.CompletedProcess([], 0, "{}", "")
        with patch.object(CAMPAIGN.subprocess, "run", return_value=result) as run:
            CAMPAIGN.invoke(["pwsh"], selected["powershell"][1], "C:/owned/control", None, None, {})
            self.assertEqual(run.call_args.args[0], ["pwsh", "-ControlDir", "C:/owned/control"])
            CAMPAIGN.invoke(["pwsh"], selected["powershell"][1], "C:/owned/control", "search", {}, {}, workspace="C:/owned")
            self.assertEqual(run.call_args.args[0][-2:], ["-Workspace", "C:/owned"])
        with self.assertRaisesRegex(ValueError, "catalog call takes no scope"):
            CAMPAIGN.invoke(["pwsh"], selected["powershell"][1], "C:/owned/control", None, None, {}, index="idx")

    def test_selected_cases_include_full_reads_and_scope_escape(self):
        self.assertGreater(len(CAMPAIGN.CASES), 0)
        self.assertEqual(CAMPAIGN.CASES[0], ("query_scope", {"view":"projects"}, False))
        self.assertTrue(all(arguments["response_mode"] == "full" for tool, arguments, _ in CAMPAIGN.CASES if tool != "query_scope"))
        self.assertTrue(all(arguments.get("start_line") == 1 and arguments.get("end_line") == 2
                            for tool, arguments, _ in CAMPAIGN.CASES if tool == "read_file"))
        self.assertTrue(any(tool == "read_file" and negative and arguments.get("path", "").startswith("../")
                            for tool, arguments, negative in CAMPAIGN.CASES))
        tool, arguments = CAMPAIGN.SCOPE_CASE
        self.assertEqual(tool, "search")
        self.assertFalse(any(key in arguments for key in ("index_ids", "index_set_ids", "workspace")))

    def endpoint(self, **changes):
        return {"schema": 1, "url": "https://127.0.0.1:1234/mcp", "service_instance": "before", "generation": 1,
                "installation": "install", "user": "owner", "namespace": "namespace",
                "ca_sha256": "ca", "certificate_sha256": "leaf", "executable_sha256": "exe", **changes}

    def test_relocation_keeps_installation_authority_and_ca_and_changes_instance_and_url(self):
        previous = self.endpoint()
        current = self.endpoint(url="https://127.0.0.1:4321/mcp", service_instance="after", generation=2, certificate_sha256="renewed")
        CAMPAIGN.verify_relocated(previous, current)
        for field in ("url", "service_instance", "schema", "installation", "user", "namespace", "ca_sha256", "executable_sha256"):
            changed = copy.deepcopy(current)
            changed[field] = previous[field] if field in ("url", "service_instance") else "crossed"
            with self.subTest(field=field), self.assertRaises(AssertionError):
                CAMPAIGN.verify_relocated(previous, changed)

    def test_relocation_requires_monotonic_endpoint_generation(self):
        previous = self.endpoint(generation=2)
        current = self.endpoint(url="after", service_instance="after", generation=2)
        with self.assertRaisesRegex(AssertionError, "generation"):
            CAMPAIGN.verify_relocated(previous, current)
        current["generation"] = 1
        with self.assertRaisesRegex(AssertionError, "generation"):
            CAMPAIGN.verify_relocated(previous, current)

    def test_complete_mocked_campaign_executes_all_clients_before_and_after_relocation(self):
        refused = {"error": {"code": "scope_required", "message": "Call list_scopes to see indexes."}}
        def result(tool, arguments):
            if "workspace" in arguments:
                return {"content": [{"type": "text", "text": json.dumps(refused)}], "structuredContent": refused, "isError": True}
            if arguments.get("index_ids") != ["idx_fixture"]:
                raise AssertionError("read without the fixture scope: " + json.dumps(arguments))
            arguments = {key: value for key, value in arguments.items() if key != "index_ids"}
            negative = arguments.get("path", "").startswith("../")
            if negative:
                body = {"error":{"code":"scoped_read_unavailable","retryable":False}}
            elif tool == "query_scope":
                body = {"schema":"airk.query-scope/1","projects":[]}
            elif tool == "list_files":
                body = {"files":[{"path":"fixture.py"}]}
            else:
                body = {"hits":[{"citation":{"path":"fixture.py"},"text":"def rawexamplemarker():\n    return 42\n"}]}
            body.update({"tool":tool,"arguments":arguments,"future":{"retained":True}})
            return {"content":[{"type":"text","text":json.dumps(body)}],"structuredContent":body,"isError":negative}

        def schema(tool):
            props = {"index_ids":{}}
            required = []
            if tool == "query_scope":
                props["view"] = {}; required = ["view"]
            else:
                props["response_mode"] = {}
                if tool == "read_file":
                    props.update({key:{} for key in ("path","start_line","end_line")}); required = ["path","start_line","end_line"]
                elif tool == "search":
                    props.update({"query":{},"mode":{}}); required = ["query"]
            return {"type":"object","properties":props,"required":required,"additionalProperties":False}
        catalog = {"schema": "airk.local-api/1", "contract_version": 1,
                   "tools": [{"name": tool, "inputSchema": schema(tool)} for tool in sorted(CAMPAIGN.READ_NAMES)]}

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
                self.scopes[name] = {"namespace": "namespace", "index_id": "idx_fixture"}

            def job(self, *_):
                pass

            def rpc(self, *_):
                return {"observation": {}}

            def arguments(self, *_):
                raise AssertionError("CLI parity must take the open read path")

            def open_call(self, name, tool, arguments, *, workspace=None):
                assert name == "raw-example"
                scoped = {**arguments, "workspace": workspace} if workspace is not None else {**arguments, "index_ids": ["idx_fixture"]}
                body = result(tool, scoped)
                return int(body["isError"]), body

            def run(self, *arguments, **_):
                raise AssertionError("unexpected airkctl command: " + repr(arguments))

            def service_run(self, *arguments, **_):
                if arguments[:2] == ("runtime", "start"):
                    if self.managed_stop is not None or self.managed_health is not None:
                        raise AssertionError("Old Stop/health evidence survived restart dispatch")
                    self.restarted = True
                    if self.fail_start:
                        raise TimeoutError("uncertain start")
                    return subprocess.CompletedProcess([], 0, '{}', '')
                raise AssertionError("unexpected native command: " + repr(arguments))

            def close(self):
                self.close_count += 1
                self.managed_stop = {"complete": True, "ordinal": self.close_count}

            def enroll_http(self, *_):
                raise AssertionError("U110: no credential enrollment")

            def http_endpoint(self):
                return {"schema": 1, "url": "https://127.0.0.1:" + ("4321" if self.restarted else "1234") + "/mcp",
                        "service_instance": "after" if self.restarted else "before",
                        "generation": 2 if self.restarted else 1,
                        "installation": "install", "user": "owner", "namespace": "namespace",
                        "ca_sha256": "ca", "executable_sha256": "exe"}

        clients_seen = []

        class Network:
            bad_baseline = False
            mutate_source = False
            def __init__(self, endpoint, control, *, scope):
                assert "credential_file" not in endpoint and scope == {"index_ids": ["idx_fixture"]}
                clients_seen.append(endpoint["url"])
                self.pending, self.scope = {}, scope

            def scoped(self, tool, arguments):
                # The real client attaches its scope unless the call names one (http_fixture tests).
                return arguments if "workspace" in arguments or tool in ("index", "missing_fixture_tool") else {**arguments, **self.scope}

            def send(self, request):
                if "id" in request:
                    value = {"tools": catalog["tools"]} if request["method"] == "tools/list" else (
                        result(request["params"]["name"], self.scoped(request["params"]["name"], request["params"]["arguments"]))
                        if request["method"] == "tools/call" else {})
                    self.pending[request["id"]] = {"result": value}

            def receive(self, identifier):
                return self.pending.pop(identifier)

            def api_catalog(self):
                return catalog

            def api_read(self, tool, arguments):
                if self.bad_baseline and tool == "query_scope":
                    if self.mutate_source:
                        (Fixture.instances[-1].owner / "source/fixture.py").write_text("changed source")
                    body = {"error":{"code":"invalid_query_scope_arguments","retryable":False},"future":{"original":"retained"}}
                    return {"isError":True,"content":[{"type":"text","text":json.dumps(body)}],"structuredContent":body}
                if tool in ("index", "missing_fixture_tool"):
                    raise CAMPAIGN.transport.HttpRefused(404, {"error": {"code": "unknown_tool"}})
                return result(tool, self.scoped(tool, arguments))

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

        def invoke(_command, _flags, _control, tool, arguments, _environment, *, index=None, workspace=None):
            assert (index is None) == (tool is None or workspace is not None)
            scoped = {**arguments, "workspace": workspace} if workspace is not None else {**(arguments or {}), "index_ids": [index]}
            body = catalog if tool is None else ({"error": {"code": "unknown_tool"}}
                    if tool in ("index", "missing_fixture_tool") else result(tool, scoped))
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
                    patch.object(CAMPAIGN.transport, "private_bytes", side_effect=AssertionError("no private credential read")), \
                    patch.object(CAMPAIGN.socket, "socket", Occupant), \
                    patch.object(CAMPAIGN, "invoke", invoke):
                receipt = CAMPAIGN.qualify(runtime, base / "report.json")
                owner = base / "failed-owner"
                owner.mkdir()
                Fixture.fail_start = True
                with self.assertRaisesRegex(TimeoutError, "uncertain start"):
                    CAMPAIGN.qualify(runtime, base / "failed-report.json")
                Fixture.fail_start = False
                Network.bad_baseline = True
                Network.mutate_source = True
                owner = base / "bad-baseline-owner"
                owner.mkdir()
                with self.assertRaisesRegex(AssertionError, "positive/negative"):
                    CAMPAIGN.qualify(runtime, base / "bad-baseline-report.json")
                bad = json.loads((base / "bad-baseline-report.json").read_text())
                attempt = bad["attempts"][0]
                self.assertEqual(attempt["tool"], "query_scope")
                self.assertEqual(attempt["requests"]["raw"]["body"], {"view":"projects","index_ids":["idx_fixture"]})
                self.assertEqual(attempt["expected"], {"intent":"positive","index_id":"idx_fixture"})
                self.assertEqual(attempt["raw"]["structuredContent"]["future"], {"original":"retained"})
                self.assertFalse(bad["passed"])
                self.assertTrue(bad["native_stop_complete"])
                self.assertIsNotNone(bad["native_stop"])
                self.assertFalse(bad["source_preserved"])
                self.assertFalse(bad["source_observations"]["indexed"]["unchanged"])
                self.assertTrue(bad["source_observations"]["outside"]["unchanged"])
            self.assertTrue(receipt["passed"])
            self.assertEqual(receipt["selected_observation_count"], 70)
            self.assertEqual(Fixture.instances[0].close_count, 2)
            self.assertEqual(len(receipt["endpoints"]), 2)
            self.assertEqual(receipt["read_scope"], {"index_ids": ["idx_fixture"]})
            self.assertEqual([sorted(row) for row in receipt["endpoints"]], [["endpoint", "phase"]] * 2)
            self.assertEqual(clients_seen[:2], ["https://127.0.0.1:1234/mcp", "https://127.0.0.1:4321/mcp"])  # then the failed-start run
            self.assertEqual(sum(row.get("kind") == "scope_required" for row in receipt["observations"]), 2)
            self.assertFalse(receipt["occupant_contacted"])
            failed = json.loads((base / "failed-report.json").read_text())
            self.assertFalse(failed["passed"])
            self.assertEqual(failed["failure_type"], "TimeoutError")
            self.assertEqual(Fixture.instances[1].close_count, 2)



class RawCaseIntentControls(unittest.TestCase):
    @staticmethod
    def envelope(body, failed=False):
        return {"content":[{"type":"text","text":json.dumps(body)}],"structuredContent":copy.deepcopy(body),"isError":failed}

    def test_native_schema_blocks_the_observed_query_scope_and_escape_input_mistakes(self):
        scope = {"properties":{"view":{},"index_ids":{}},"required":["view"],"additionalProperties":False}
        CAMPAIGN.verify_case_arguments(scope, {"view":"projects","index_ids":["idx"]})
        with self.assertRaisesRegex(AssertionError, "unsupported"):
            CAMPAIGN.verify_case_arguments(scope, {"view":"projects","response_mode":"full","index_ids":["idx"]})
        read = {"properties":{"path":{},"start_line":{},"end_line":{},"index_ids":{}},
                "required":["path","start_line","end_line"],"additionalProperties":False}
        with self.assertRaisesRegex(AssertionError, "required"):
            CAMPAIGN.verify_case_arguments(read, {"path":"../other/secret.py","index_ids":["idx"]})

    def test_scope_escape_requires_the_actual_path_fence_not_arbitrary_is_error(self):
        good = self.envelope({"error":{"code":"scoped_read_unavailable","retryable":False}}, True)
        self.assertEqual(CAMPAIGN.verify_case_reply("read_file", {"path":"../other/secret.py"}, True, good, "idx", "outside-sentinel"), 1)
        for error in [{"code":"invalid_native_arguments","retryable":False},
                      {"code":"read_failed","message":"worker unavailable","retryable":False},
                      {"code":"scoped_read_unavailable","retryable":True}]:
            with self.subTest(error=error), self.assertRaises(AssertionError):
                CAMPAIGN.verify_case_reply("read_file", {"path":"../other/secret.py"}, True, self.envelope({"error":error}, True), "idx", "outside-sentinel")

    def test_combined_report_failure_keeps_documented_outer_exit_but_strict_member_refusal(self):
        refusal = {"error":{"code":"scoped_read_unavailable","retryable":False}}
        body = {"schema":"airk.combined-read/1","partial":True,"truncated":False,"ranking":[],
                "members":[{"index_id":"idx","is_error":True,"outcome":"failed","result":refusal}],"future":{"kept":True}}
        reply = self.envelope(body)
        self.assertEqual(CAMPAIGN.verify_case_reply("read_file", {"path":"../other/secret.py"}, True, reply, "idx", "sentinel"), 0)
        CAMPAIGN.verify_result({"exit_code":0,"body":copy.deepcopy(reply)}, reply, True, expected_exit=0)
        for pointer, value in [("index_id","another-index"),("is_error",False),("outcome","unqueried")]:
            changed = copy.deepcopy(body); changed["members"][0][pointer] = value
            with self.subTest(pointer=pointer), self.assertRaises(AssertionError):
                CAMPAIGN.verify_case_reply("read_file", {"path":"../other/secret.py"}, True, self.envelope(changed), "idx", "sentinel")

    def test_combined_canonical_conditional_body_is_not_mistaken_for_a_text_envelope(self):
        native = {"evidence":{"status":"content"},"content":{"hits":[{
            "citation":{"path":"fixture.py"},"text":"def rawexamplemarker():\n    return 42\n"}]},"future":{"kept":True}}
        body = {"schema":"airk.combined-read/1","partial":False,"truncated":False,
                "members":[{"index_id":"idx","is_error":False,"outcome":"complete","result":native}],"ranking":[]}
        reply = self.envelope(body)
        self.assertEqual(CAMPAIGN.verify_case_reply("read_file", {}, False, reply, "idx", "outside-sentinel"), 0)
        with self.assertRaisesRegex(AssertionError, "envelope"):
            CAMPAIGN.verify_case_reply("read_file", {}, False, native, "idx", "outside-sentinel")

    def test_positive_outer_success_cannot_hide_unready_or_failed_members(self):
        body = {"schema":"airk.combined-read/1","partial":True,"truncated":False,"ranking":[],
                "members":[{"index_id":"idx","is_error":True,"outcome":"failed","result":{"error":{"code":"worker_unavailable"}}}]}
        with self.assertRaisesRegex(AssertionError, "readiness"):
            CAMPAIGN.verify_case_reply("search", {}, False, self.envelope(body), "idx", "sentinel")
        with self.assertRaisesRegex(AssertionError, "not ready"):
            CAMPAIGN.verify_case_reply("search", {}, False, self.envelope({"hits":[]}), "idx", "sentinel")

    def test_unknown_route_refusal_cannot_be_substituted_with_service_or_auth_failure(self):
        CAMPAIGN.verify_http_refusal(404, {"error":{"code":"unknown_tool"},"future":"retained"})
        for status, body in [(503,{"error":{"code":"service_unavailable"}}),(401,{"error":{"code":"auth_required"}}),(404,{"error":{"code":"another_error"}})]:
            with self.subTest(status=status), self.assertRaises(AssertionError):
                CAMPAIGN.verify_http_refusal(status, body)

    def test_source_bytes_and_inconsistent_full_result_copies_are_not_accepted(self):
        body = {"error":{"code":"scoped_read_unavailable","retryable":False},"future":"outside-sentinel"}
        with self.assertRaisesRegex(AssertionError, "Outside-source"):
            CAMPAIGN.verify_case_reply("read_file", {"path":"../other/secret.py"}, True, self.envelope(body,True), "idx", "outside-sentinel")
        changed = self.envelope({"schema":"airk.query-scope/1"})
        changed["structuredContent"]["future"] = "dropped from text copy"
        with self.assertRaisesRegex(AssertionError, "copies differ"):
            CAMPAIGN.verify_case_reply("query_scope", {}, False, changed, "idx", "outside-sentinel")


if __name__ == "__main__":
    unittest.main()
