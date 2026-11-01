# SPDX-License-Identifier: Apache-2.0
"""Exercise the separate stdio adapter without a Docker daemon or native worker."""
import importlib.util
import io
import json
from pathlib import Path
import queue
import unittest
from unittest.mock import MagicMock, patch

spec = importlib.util.spec_from_file_location("pool", Path(__file__).parents[1] / "check-service-pool.py")
pool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pool)


class PoolTransportTests(unittest.TestCase):
    def test_constructor_keeps_binding_collection_and_protocol_handshake(self):
        process = MagicMock()
        process.stdin = io.BytesIO()
        process.stdout = io.BytesIO(b'{"id":1,"result":{}}\n')
        with patch.object(pool.subprocess, "Popen", return_value=process) as spawn:
            bridge = pool.Bridge(Path("airk"), Path("runtime"), "project", {}, "collection")
            self.assertEqual((bridge.binding, bridge.collection), ("project", "collection"))
            self.assertEqual(spawn.call_args.args[0][-2:], ["--project", "project"])
            self.assertEqual(len(process.stdin.getvalue().splitlines()), 2)
            bridge.close()

    def test_read_passes_remaining_budget_and_keeps_exact_request(self):
        bridge = pool.Bridge.__new__(pool.Bridge)
        bridge.sequence = 0
        bridge.process = MagicMock(stdin=io.BytesIO())
        bridge.responses = MagicMock()
        hint = {"status": "retry", "code": "publication_busy", "retryable": True,
                "action": "repeat_same_request", "retry_after_ms": 25}
        envelope = lambda value, error=False: {"isError": error, "content": [{"type": "text", "text": json.dumps(value)}]}
        bridge.responses.get.side_effect = [
            {"id": 1, "result": envelope(hint, True)}, {"id": 2, "result": envelope({"token": "kept"})}]
        now = [10.0]
        def sleep(delay):
            now[0] += delay
        with patch.object(pool.time, "monotonic", side_effect=lambda: now[0]), patch.object(pool.time, "sleep", side_effect=sleep):
            self.assertEqual(bridge.read("airk_changes", {"revision": "exact", "cursor": "same"}, 11), {"token": "kept"})
        self.assertEqual(bridge.responses.get.call_args_list[0].kwargs["timeout"], 1)
        self.assertAlmostEqual(bridge.responses.get.call_args_list[1].kwargs["timeout"], .975)
        sent = [json.loads(line) for line in bridge.process.stdin.getvalue().splitlines()]
        self.assertEqual(sent[0]["params"], sent[1]["params"])
        bridge.responses.get.side_effect = queue.Empty
        with self.assertRaises(queue.Empty):
            bridge.read("airk_changes", {})
        self.assertEqual(bridge.responses.get.call_count, 3, "transport failure must not replay")

    def test_failed_semantics_stops_without_another_observation(self):
        bridge = pool.Bridge.__new__(pool.Bridge)
        bridge.readiness = MagicMock(return_value={"state": "failed", "completed_artifacts": 0})
        with self.assertRaisesRegex(RuntimeError, "state=failed"):
            bridge.wait_semantics()
        bridge.readiness.assert_called_once()


class SemanticRecoveryTests(unittest.TestCase):
    def job(self, state="pending", attempts=1, outcome=None):
        return {"id": "exact-job", "kind": "vectors", "state": state,
                "attempts": attempts, "outcome": outcome}

    def waiting(self, **changes):
        return dict({"job": "exact-job", "reason": "helper_unavailable", "state": "waiting",
                     "attempts": 1, "limit": 3}, **changes)

    def test_exact_deferred_receipt_requires_matching_helper_wait(self):
        receipt = self.job()
        self.assertEqual(pool.semantic_recovery_mode(receipt, [self.waiting()]), "waiting")
        self.assertEqual(receipt, self.job(), "classification must preserve the accepted receipt")
        for observation in [self.waiting(job=None), self.waiting(job="foreign"),
                            self.waiting(reason="source_unavailable"), self.waiting(state="stopped"),
                            self.waiting(attempts=2), self.waiting(limit=0)]:
            with self.subTest(observation=observation):
                self.assertIsNone(pool.semantic_recovery_mode(receipt, [observation]))

    def test_queued_or_running_work_is_not_inferred_as_deferred(self):
        self.assertIsNone(pool.semantic_recovery_mode(self.job(attempts=0), [self.waiting(attempts=0)]))
        self.assertIsNone(pool.semantic_recovery_mode(self.job(state="running"), [self.waiting()]))
        self.assertIsNone(pool.semantic_recovery_mode(self.job(outcome={"state": "failed"}), [self.waiting()]))

    def test_terminal_failure_keeps_real_outcome_for_explicit_retry(self):
        receipt = self.job(state="failed", attempts=3, outcome={"state": "failed", "code": "helper_unavailable"})
        self.assertEqual(pool.semantic_recovery_mode(receipt, []), "failed")
        self.assertEqual(receipt["outcome"], {"state": "failed", "code": "helper_unavailable"})
        with self.assertRaisesRegex(RuntimeError, "original failure outcome"):
            pool.semantic_recovery_mode(self.job(state="failed"), [])
        unrelated = self.job(state="failed", outcome={"state": "failed", "code": "retained_object_missing"})
        with self.assertRaisesRegex(RuntimeError, "not caused by helper"):
            pool.semantic_recovery_mode(unrelated, [])
        self.assertEqual(unrelated["outcome"]["code"], "retained_object_missing")

    def test_foreign_kind_never_authorizes_semantic_recovery(self):
        receipt = dict(self.job(), kind="index")
        with self.assertRaisesRegex(RuntimeError, "foreign job"):
            pool.semantic_recovery_mode(receipt, [self.waiting()])


if __name__ == "__main__":
    unittest.main()
