# SPDX-License-Identifier: Apache-2.0
"""Only a native unsubmitted busy receipt permits qualification query retry."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("service_clients", Path(__file__).parents[1] / "check-service-clients.py")
CLIENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENTS)


class ServiceClientTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
