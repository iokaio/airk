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
        selected = {"expected_catalog_revision": 4, "grant": "original"}
        connection = {"selection": {"kind": "scoped", "selection": {**selected, "expected_catalog_revision": 5}}}
        if changed:
            connection["selection"]["selection"]["grant"] = "other"
        fixture = SimpleNamespace(single_image=True, scopes={"p": selected}, control=Path("owned"),
            managed_health={"instance": "exact"}, assert_alive=Mock(),
            enroll_http=Mock(return_value=connection),
            rpc=Mock(return_value={"observation": {"instance": "exact"}}))
        network = Mock()
        transport = SimpleNamespace(validate_native_identity=Mock(), McpClient=Mock(return_value=network))
        with patch.object(driver.importlib.util, "module_from_spec", return_value=transport), \
             patch.object(driver.importlib.util, "spec_from_file_location", return_value=SimpleNamespace(loader=SimpleNamespace(exec_module=lambda _: None))), \
             patch.object(driver.subprocess, "Popen", side_effect=AssertionError("Modern stdio launch forbidden")):
            if changed:
                with self.assertRaisesRegex(AssertionError, "changed reviewed scope"):
                    with driver.usage_mcp(fixture, "p", Path("owned")):
                        self.fail("Changed grant was accepted")
                transport.McpClient.assert_not_called()
            else:
                with driver.usage_mcp(fixture, "p", Path("owned")) as (send, receive):
                    send({"id": 1})
                    receive(1)
                network.close.assert_called_once()
                enrollment = fixture.enroll_http.call_args.args
                self.assertEqual(enrollment[2], "codex")
                self.assertEqual(enrollment[0], {"kind": "scoped", "selection": selected})
                fixture.rpc.assert_called_once_with("health")
                transport.validate_native_identity.assert_called_once()

    def test_modern_uses_native_scoped_codex_enrollment_and_https(self):
        self.exercise()

    def test_enrollment_cannot_substitute_authority(self):
        self.exercise(changed=True)
