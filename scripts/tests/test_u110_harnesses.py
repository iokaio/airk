# SPDX-License-Identifier: Apache-2.0
"""Current tokenless harness contracts, independent of retained pre-U110 tests."""
from copy import deepcopy
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / file)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


LOAD = load("u110_load_harness", "benchmark-service.py")
USAGE = load("u110_usage_harness", "check-usage-inputs.py")
CONTAINER = load("u110_container_harness", "check-container.py")


class TokenlessHarnessTests(unittest.TestCase):
    def fixture(self):
        endpoint = {"schema":1, "url":"https://127.0.0.1:57429/mcp", "service_instance":"owned",
                    "user":"human", "executable_sha256":"a"*64, "namespace":"b"*32}
        health = {"instance":"owned", "user":"human", "executable_sha256":"a"*64,
                  "catalog_clock":{"namespace":"b"*32}}
        return SimpleNamespace(single_image=True, http_endpoint=Mock(return_value=endpoint),
            scopes={"one":{"index_id":"idx_"+"1"*32}, "two":{"index_id":"idx_"+"2"*32}},
            control=Path("owned-control"), rpc=Mock(return_value={"observation":health}),
            managed_health=deepcopy(health), assert_alive=Mock(),
            enroll_http=Mock(side_effect=AssertionError("No enrollment or credential artifacts")))

    def test_sixty_four_load_contexts_keep_named_index_without_credentials(self):
        fixture = self.fixture()
        contexts = [LOAD.endpoint_context(fixture, "one" if i % 2 == 0 else "two") for i in range(64)]
        self.assertEqual(len(contexts), 64)
        self.assertTrue(all(context["endpoint"] == fixture.http_endpoint.return_value for context in contexts))
        self.assertEqual([value["scope"] for value in contexts],
            [{"index_ids":["idx_" + str(1 + i % 2)*32]} for i in range(64)])
        fixture.enroll_http.assert_not_called()
        contexts[0]["scope"]["index_ids"].append("foreign")
        self.assertEqual(contexts[2]["scope"], {"index_ids":["idx_"+"1"*32]})

    def test_load_session_passes_exact_scope_and_retains_mcp_lifecycle(self):
        fixture = self.fixture()
        context = LOAD.endpoint_context(fixture, "two")
        network = Mock(session="new-native-session")
        network.receive.return_value = {"result": {"serverInfo":{"name":"airk"}}}
        with patch.object(LOAD.http_fixture, "McpClient", return_value=network) as create:
            session = LOAD.HttpSession(context, fixture.control)
            session.close()
        create.assert_called_once_with(context["endpoint"], fixture.control, scope={"index_ids":["idx_"+"2"*32]})
        network.close.assert_called_once_with(delete=True)
        self.assertEqual(network.send.call_args_list[0].args[0]["method"], "initialize")
        self.assertEqual(network.send.call_args_list[1].args[0]["method"], "notifications/initialized")

    def test_usage_session_names_exact_index_without_enrollment(self):
        fixture = self.fixture()
        network = Mock()
        transport = SimpleNamespace(McpClient=Mock(return_value=network), validate_native_identity=Mock())
        with patch.object(USAGE.importlib.util, "module_from_spec", return_value=transport), patch.object(USAGE.importlib.util, "spec_from_file_location", return_value=SimpleNamespace(loader=SimpleNamespace(exec_module=lambda _:None))), patch.object(USAGE.subprocess, "Popen", side_effect=AssertionError("No stdio frontend")):
            with USAGE.usage_mcp(fixture, "one", fixture.control) as (send, receive):
                send({"id":1}); receive(1)
        fixture.enroll_http.assert_not_called()
        transport.McpClient.assert_called_once_with(fixture.http_endpoint.return_value, fixture.control, scope={"index_ids":["idx_"+"1"*32]})
        network.close.assert_called_once()

    def test_crossed_usage_service_refuses_before_network(self):
        fixture = self.fixture()
        fixture.rpc.return_value["observation"]["instance"] = "other"
        transport = SimpleNamespace(McpClient=Mock(), validate_native_identity=Mock())
        with patch.object(USAGE.importlib.util, "module_from_spec", return_value=transport), patch.object(USAGE.importlib.util, "spec_from_file_location", return_value=SimpleNamespace(loader=SimpleNamespace(exec_module=lambda _:None))):
            with self.assertRaises(AssertionError):
                with USAGE.usage_mcp(fixture, "one", fixture.control):
                    self.fail("Crossed endpoint admitted")
        transport.McpClient.assert_not_called()
        fixture.enroll_http.assert_not_called()

    def test_container_open_delivery_keeps_full_body_and_rejects_wider_scope(self):
        scope = {"open_read":True, "collection":"fixture", "selection":{"index_ids":["idx_"+"1"*32]}}
        value = {"collection":"fixture", "publication":{"token":"airk-r1:"+"a"*32+":1:"+"b"*64},
                 "read_scope":{"source":"named", "index_ids":["idx_"+"1"*32]}, "hits":[],
                 "future_provenance":{"retained":True}}
        self.assertEqual(CONTAINER.logical_result(value, scope), value)
        changed = deepcopy(value); changed["future_provenance"] = {"retained":False}
        self.assertNotEqual(CONTAINER.logical_result(changed, scope), CONTAINER.logical_result(value, scope))
        changed["read_scope"]["index_ids"].append("idx_"+"2"*32)
        with self.assertRaises(AssertionError):
            CONTAINER.logical_result(changed, scope)

    def test_container_endpoint_discovery_creates_no_selection_or_credential(self):
        endpoint = self.fixture().http_endpoint.return_value
        record = SimpleNamespace(stdout=__import__("json").dumps(endpoint))
        records = []
        with patch.object(CONTAINER, "docker", return_value=record) as docker:
            self.assertEqual(CONTAINER.open_container_endpoint("owned", "/owned/control", records), endpoint)
        docker.assert_called_once_with("exec", "owned", "cat", "/owned/control/http-1/endpoint.json")
        self.assertEqual(records[0]["endpoint"], endpoint)

    def test_cli_endpoint_projection_retains_exact_descriptor_and_validates_every_manual_field(self):
        import json
        endpoint = self.fixture().http_endpoint.return_value
        manual = [
            {"client":"claude","scope":"project","file":".mcp.json","format":"json","text":'{"mcpServers":{"airk":{"type":"http","url":"https://127.0.0.1:57429/mcp","headers":{"X-AIRK-Response-Mode":"terse"}}}}'},
            {"client":"codex","scope":"project","file":".codex/config.toml","format":"toml","text":'[mcp_servers.airk]\nurl = "https://127.0.0.1:57429/mcp"\n[mcp_servers.airk.http_headers]\n"X-AIRK-Response-Mode" = "terse"\n'},
            {"client":"vscode","scope":"project","file":".vscode/mcp.json","format":"json","text":'{"servers":{"airk":{"type":"http","url":"https://127.0.0.1:57429/mcp","headers":{"X-AIRK-Response-Mode":"terse"}}}}'},
        ]
        projected = {**endpoint,"manual":manual}
        fixture = SimpleNamespace(cli=Path("verified-airkctl"), open_service=lambda:(),control=Path("owned"),
            run=Mock(return_value=SimpleNamespace(stdout=json.dumps(projected))))
        transport = SimpleNamespace(read_endpoint=Mock(return_value=endpoint))
        with patch.object(LOAD.clients,"http_transport",return_value=transport):
            self.assertEqual(LOAD.clients.Fixture.http_endpoint(fixture), projected)
            changed = deepcopy(projected)
            changed["manual"][1]["text"] += 'bearer_token_env_var = "secret"\n'
            fixture.run.return_value.stdout = json.dumps(changed)
            with self.assertRaises(AssertionError): LOAD.clients.Fixture.http_endpoint(fixture)
            changed = deepcopy(projected); changed["service_instance"]="other"
            fixture.run.return_value.stdout = json.dumps(changed)
            with self.assertRaises(AssertionError): LOAD.clients.Fixture.http_endpoint(fixture)
