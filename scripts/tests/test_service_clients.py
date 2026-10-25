# SPDX-License-Identifier: Apache-2.0
"""Only explicit native read-rejection receipts permit qualification retry."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("service_clients", Path(__file__).parents[1] / "check-service-clients.py")
CLIENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENTS)


class ServiceClientTests(unittest.TestCase):
    def test_runtime_receipts_stay_outside_the_mac_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            nested = root / "AIRK.app/Contents/MacOS"
            nested.mkdir(parents=True)
            (nested / "airk").write_bytes(b"fictional native payload")
            for selected in (root, nested):
                fixture = CLIENTS.Fixture(selected, root)
                try:
                    self.assertEqual(fixture.runtime, root)
                    self.assertEqual(fixture.binary, nested / "airk")
                finally:
                    fixture.log.close()
            unknown = root / "Other.app/Contents/MacOS"
            unknown.mkdir(parents=True)
            self.assertEqual(CLIENTS.runtime_root(unknown), unknown)

    def envelope(self, value, failed=False):
        return {"isError": failed, "content": [{"type": "text", "text": json.dumps(value)}]}

    def test_unknown_or_submitted_results_never_replay(self):
        for code, reason, submitted in (("direct_query_admission", "busy", True),
                                         ("worker_lost", "busy", False),
                                         ("direct_query_admission", "unknown", False)):
            fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
            fixture.arguments = lambda _: []
            failure = self.envelope({"error": {"code": code, "reason": reason, "query_submitted": submitted}}, True)
            with patch.object(fixture, "run", return_value=subprocess.CompletedProcess([], 1, json.dumps(failure), "failed")) as run:
                with self.assertRaises(AssertionError):
                    fixture.query("fixture", "search", "marker")
                run.assert_called_once()

    def test_only_an_explicit_unsubmitted_receipt_retries(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.arguments = lambda _: []
        busy = self.envelope({"error": {"code": "direct_query_admission", "reason": "busy", "query_submitted": False}}, True)
        accepted = self.envelope({"request_id": "accepted", "hits": []})
        replies = [subprocess.CompletedProcess([], 1, json.dumps(busy), "busy"),
                   subprocess.CompletedProcess([], 0, json.dumps(accepted), "")]
        with patch.object(fixture, "run", side_effect=replies) as run, patch.object(CLIENTS.time, "sleep") as sleep:
            self.assertEqual(fixture.query("fixture", "search", "marker")["request_id"], "accepted")
            self.assertEqual(run.call_count, 2)
            sleep.assert_called_once_with(0.025)

    def test_publication_retry_preserves_exact_revision_and_cursor(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.arguments = lambda _: []
        args = ("tools", "call", "airk_outline", "--args", '{"revision":"saved","cursor":"exact"}')
        hint = {"status": "retry", "retryable": True, "action": "repeat_same_request",
                "code": "publication_busy", "retry_after_ms": 25}
        replies = [subprocess.CompletedProcess([], 1, json.dumps(self.envelope(hint, True)), ""),
                   subprocess.CompletedProcess([], 0, json.dumps(self.envelope({"items": []})), "")]
        with patch.object(fixture, "run", side_effect=replies) as run, patch.object(CLIENTS.time, "sleep"):
            self.assertEqual(fixture.query("fixture", *args), {"items": []})
            self.assertEqual(run.call_args_list[0], run.call_args_list[1])
        for key, value in (("code", "unknown"), ("retryable", False), ("action", "refresh_revision"),
                           ("retry_after_ms", 0), ("retry_after_ms", True), ("retry_after_ms", 1001)):
            self.assertIsNone(CLIENTS.read_retry_delay(self.envelope({**hint, key: value}, True)))

    def test_setup_rejection_is_unsubmitted_and_honors_native_backoff(self):
        error = {"code": "direct_query_setup", "reason": "busy", "query_submitted": False,
                 "retryable": True, "fresh_brokerage_required": True, "retry_after_ms": 100}
        self.assertEqual(CLIENTS.read_retry_delay(self.envelope({"error": error}, True)), 0.1)
        self.assertIsNone(CLIENTS.read_retry_delay(self.envelope({"error": {**error, "query_submitted": True}}, True)))


if __name__ == "__main__":
    unittest.main()
