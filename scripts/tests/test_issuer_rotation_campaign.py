# SPDX-License-Identifier: Apache-2.0
"""Acceptance-driver boundaries; these mocks do not qualify native rotation."""
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("issuer_campaign", Path(__file__).parents[1] / "check-issuer-rotation.py")
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)


class IssuerRotationCampaignTests(unittest.TestCase):
    def test_os_trust_mutation_is_never_a_driver_action(self):
        fixture = SimpleNamespace(install=Path("owned"), run=Mock())
        with self.assertRaisesRegex(AssertionError, "OS trust mutation"):
            driver.rotation(fixture, "trust-new")
        fixture.run.assert_not_called()

    def test_mutation_uses_exact_reviewed_operation_and_receipt(self):
        receipt = {"schema": 1, "rotation": {"operation_id": "a" * 32}, "receipt_sha256": "b" * 64}
        fixture = SimpleNamespace(install=Path("owned"), run=Mock(return_value=SimpleNamespace(stdout=json.dumps(receipt))))
        self.assertEqual(driver.rotation(fixture, "commit", receipt), receipt)
        self.assertEqual(fixture.run.call_args.args, ("runtime", "http-issuer-rotation", "--install-dir", Path("owned"),
            "--action", "commit", "--operation-id", "a" * 32, "--expected-receipt-sha256", "b" * 64))

    def test_scope_can_advance_clock_but_not_replace_grant_or_projects(self):
        before = {"kind": "scoped", "selection": {"expected_catalog_revision": 3, "grant_id": "original", "project_ids": ["one"]}}
        after = json.loads(json.dumps(before))
        after["selection"]["expected_catalog_revision"] = 4
        driver.same_scope(before, after)
        self.assertEqual(before["selection"]["expected_catalog_revision"], 3)
        for field, value in (("grant_id", "other"), ("project_ids", ["one", "two"]), ("expected_catalog_revision", 2)):
            crossed = json.loads(json.dumps(after))
            crossed["selection"][field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                driver.same_scope(before, crossed)

    def test_overlap_context_preserves_existing_roots_and_validation(self):
        roots = [b"existing enterprise root"]
        context = SimpleNamespace(check_hostname=True, verify_mode=driver.ssl.CERT_REQUIRED, verify_flags=0,
            get_ca_certs=lambda **_: list(roots), load_verify_locations=lambda *, cadata: roots.append(cadata.encode()))
        with patch.object(driver.ssl, "create_default_context", return_value=context):
            self.assertIs(driver.additive_context(b"old public CA", b"new public CA"), context)
        self.assertEqual(roots, [b"existing enterprise root", b"old public CA", b"new public CA"])
        self.assertTrue(context.verify_flags & driver.ssl.VERIFY_X509_STRICT)
        context.check_hostname = False
        with patch.object(driver.ssl, "create_default_context", return_value=context), self.assertRaises(AssertionError):
            driver.additive_context(b"new public CA")

    def test_old_issuer_rejection_requires_certificate_validation_error(self):
        endpoint = {"url": "https://127.0.0.1:57421/mcp"}
        with patch.object(driver.socket, "create_connection", side_effect=ConnectionRefusedError()):
            with self.assertRaises(ConnectionRefusedError):
                driver.stale_issuer_refused(endpoint, b"public certificate")

    def test_overlap_delete_is_acknowledged_before_waiting_and_failures_propagate(self):
        events = []
        network = SimpleNamespace(close=lambda: events.append("delete"))
        with patch.object(driver.time, "time", side_effect=[1000, 1000, 1002]), \
             patch.object(driver.time, "monotonic", return_value=0), \
             patch.object(driver.time, "sleep", side_effect=lambda delay: events.append(("wait", delay))):
            driver.close_overlap_before_wait(network, 1001000)
        self.assertEqual(events, ["delete", ("wait", 1)])
        failed = SimpleNamespace(close=Mock(side_effect=ConnectionAbortedError("unknown DELETE")))
        with patch.object(driver.time, "sleep") as sleep, self.assertRaises(ConnectionAbortedError):
            driver.close_overlap_before_wait(failed, 1001000)
        failed.close.assert_called_once_with()
        sleep.assert_not_called()

    def test_overlap_wait_keeps_original_deadline_and_requires_native_timestamp(self):
        network = SimpleNamespace(close=Mock())
        for expiry in (None, True, -1, "1001000"):
            with self.subTest(expiry=expiry), self.assertRaises(AssertionError):
                driver.close_overlap_before_wait(network, expiry)
        network.close.assert_not_called()
        with patch.object(driver.time, "time", return_value=1000), \
             patch.object(driver.time, "monotonic", side_effect=[0, 71]), \
             patch.object(driver.time, "sleep") as sleep, \
             self.assertRaisesRegex(AssertionError, "bounded deadline"):
            driver.close_overlap_before_wait(network, 1001000)
        network.close.assert_called_once_with()
        sleep.assert_not_called()
