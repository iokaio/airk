# SPDX-License-Identifier: Apache-2.0
"""Pure tests for the frozen Codex actor and shared wrapper deadline."""

import json
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import mcp_wrapper
import run_session


class CodexActor(unittest.TestCase):
    def test_openai_actor_never_falls_back_to_claude(self):
        with self.assertRaisesRegex(common.HarnessError, "explicit frozen Codex"):
            run_session.run(scenario={}, path="search", handle=Path("unused"), series="unused",
                            sessions_root=Path("unused"), model="gpt-6.2-sol")

    def test_codex_audit_reconciles_declined_rpc_without_mcp_evidence(self):
        status = {"thread_id": "t", "turn_id": "u", "configured_mcp": {}, "attempts": [
            {"call_id": "c", "rpc_id": 11, "tool": "airk_search", "arguments": {"query": "wrong"}, "success": False}]}
        events = [{"id": 11, "method": "item/tool/call", "params": {"threadId": "t", "turnId": "u",
                   "callId": "c", "namespace": "airk_eval", "tool": "airk_search", "arguments": {"query": "wrong"}}}]
        turns = [{"call": 1, "turn": None, "tool": "airk_search", "arguments": {"query": "wrong"}, "ok": False}]
        with patch.object(common, "read_jsonl", return_value=events):
            audit = run_session.audit_codex(Path("unused"), status, "search", turns, "")
            self.assertFalse(audit["discard"])
            self.assertEqual(audit["tool_use_count"], {
                "stream": 1, "wrapper_logged": 1, "counted": 0, "refused": 1, "agree": True})
            self.assertTrue(run_session.audit_codex(Path("unused"), status, "search", [], "")["discard"])
            events.append(events[0])
            self.assertTrue(run_session.audit_codex(Path("unused"), status, "search", turns, "")["discard"])

    def test_wrapper_retains_retry_reply_if_wait_cannot_fit_deadline(self):
        body = {"status": "retry", "retryable": True, "action": "repeat_same_request",
                "retry_after_ms": 300, "code": "publication_busy"}
        receipt = {"structuredContent": body}
        outcome = common.CallOutcome(True, 0, json.dumps(receipt).encode(), b"", 0, receipt, None)
        wrapper = object.__new__(mcp_wrapper.Wrapper)
        wrapper.deadline = 100.1
        wrapper.handle = None
        wrapper.backend = Mock(return_value=outcome)
        with patch.object(mcp_wrapper.time, "monotonic", return_value=100), patch.object(mcp_wrapper.time, "sleep") as wait:
            self.assertEqual(wrapper._with_typed_retries("airk_search", {}, "turn-01"), (outcome, []))
            wait.assert_not_called()
        wrapper.backend.assert_called_once_with("airk_search", {})


if __name__ == "__main__":
    unittest.main()
