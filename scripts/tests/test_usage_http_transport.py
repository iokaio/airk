# SPDX-License-Identifier: Apache-2.0
"""Transport routing/authority guard; installed accounting is a separate native gate."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("usage_http_driver", Path(__file__).parents[1] / "check-usage-inputs.py")
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)


class UsageHttpTransportTests(unittest.TestCase):
    def test_usage_accounting_and_policy_use_owned_management_without_scoped_or_foreign_fallback(self):
        fixture = SimpleNamespace(management_arguments=Mock(return_value=["exact-owned-management"]),
                                  arguments=Mock(side_effect=AssertionError("Accounting must not use evidence grant")))
        for command in (("index",), ("usage","report"), ("usage","report","--price-scenario","owned.json"),
                        ("usage","policy"), ("usage","configure"), ("usage","clear"),
                        ("usage","import","owned.json"), ("usage","calibrate")):
            self.assertEqual(driver.usage_arguments(fixture,"owned-project",command), ["exact-owned-management"])
        self.assertEqual(fixture.management_arguments.call_count, 8)
        fixture.management_arguments.assert_called_with("owned-project")
        with self.assertRaises(AssertionError):
            driver.usage_arguments(fixture,"owned-project",("search","fictional"))
        self.assertEqual(fixture.management_arguments.call_count, 8)
        fixture.arguments.assert_not_called()

    def exercise(self, changed=False):
        selected = {"index_id": "idx_" + "a" * 32}
        endpoint = {"url": "https://127.0.0.1:57421/mcp"}
        fixture = SimpleNamespace(single_image=True, scopes={"p": selected}, control=Path("owned"),
            managed_health={"instance": "exact"}, assert_alive=Mock(), http_endpoint=Mock(return_value=endpoint),
            enroll_http=Mock(side_effect=AssertionError("No credential enrollment")),
            rpc=Mock(return_value={"observation": {"instance": "other" if changed else "exact"}}))
        network = Mock()
        transport = SimpleNamespace(validate_native_identity=Mock(), McpClient=Mock(return_value=network))
        with patch.object(driver.importlib.util, "module_from_spec", return_value=transport), \
             patch.object(driver.importlib.util, "spec_from_file_location", return_value=SimpleNamespace(loader=SimpleNamespace(exec_module=lambda _: None))), \
             patch.object(driver.subprocess, "Popen", side_effect=AssertionError("Modern stdio launch forbidden")):
            if changed:
                with self.assertRaisesRegex(AssertionError, ""):
                    with driver.usage_mcp(fixture, "p", Path("owned")):
                        self.fail("Changed grant was accepted")
                transport.McpClient.assert_not_called()
            else:
                with driver.usage_mcp(fixture, "p", Path("owned")) as (send, receive):
                    send({"id": 1})
                    receive(1)
                network.close.assert_called_once()
                fixture.enroll_http.assert_not_called()
                transport.McpClient.assert_called_once_with(endpoint, fixture.control, scope={"index_ids": ["idx_" + "a" * 32]})
                fixture.rpc.assert_called_once_with("health")
                transport.validate_native_identity.assert_called_once()

    def test_modern_uses_tokenless_endpoint_and_explicit_index(self):
        self.exercise()

    def test_crossed_service_cannot_replace_owned_endpoint(self):
        self.exercise(changed=True)
