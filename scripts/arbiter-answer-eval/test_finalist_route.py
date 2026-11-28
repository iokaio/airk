# SPDX-License-Identifier: Apache-2.0
"""End-to-end framing, source validation and inference accounting for finalist calls."""
import hashlib
import io
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import finalist_route
import mcp_wrapper
import product_skill_http


class RouteTest(unittest.TestCase):
    def test_actual_frame_tracks_shared_need_ranges_and_scores(self):
        with TemporaryDirectory() as folder:
            root = Path(folder)
            def inventory(handle, repo, metrics, cache):
                metrics.update(publication="published", scope_fingerprint="grant")
                return {"a.py"}

            (root / "a.py").write_text("first\nsecond\n", encoding="utf-8")
            def policy(repository, args, score):
                self.assertEqual(score("direct", [("need", "evidence")]), [0.8])
                p = SimpleNamespace(at="a.py:1-2", text="first\nsecond\n", supports=[
                    {"need": 1, "scored_at": "a.py:1-1"}, {"need": 2, "scored_at": "a.py:2-2"}])
                return [p], 3, {"identity": "injected-test", "attribution": "need_ranges"}

            handle = SimpleNamespace(index_id=lambda r: "idx")
            route = finalist_route.Gather(handle, "repo", root, root, lambda prompt, pairs: [0.8], policy, 4)
            bridge = product_skill_http.Bridge(None, route, {"instructions": "", "tools": [
                {"name": "gather_context", "inputSchema": {"properties": {"task": {}, "needs": {}}}}]})
            incoming = {"jsonrpc": "2.0", "id": "unicode-測", "method": "tools/call",
                        "params": {"name": "gather_context", "arguments": {"task": "t", "needs": ["n1", "n2"]}}}
            with patch.object(finalist_route.workspace_scope, "eligible_paths", side_effect=inventory):
                for i, framing in enumerate(("header", "line"), 1):
                    request, output = io.BytesIO(), io.BytesIO()
                    mcp_wrapper.write_message(request, incoming, framing)
                    request.seek(0)
                    mcp_wrapper.serve(bridge, request, output, framed_calls=True)
                    self.assertEqual((root / f"call-{i:03}.wire").read_bytes(), output.getvalue())
                    output.seek(0)
                    wire, _ = mcp_wrapper.read_message(output)
                    self.assertFalse(wire["result"]["isError"])
                    body = json.loads(wire["result"]["content"][0]["text"])
                    self.assertEqual(body["passages"][0]["needs"], [1, 2])
                    self.assertEqual(body["passages"][0]["workspace_sha256"], hashlib.sha256((root / "a.py").read_bytes()).hexdigest())
                    self.assertEqual([row["where"] for row in body["needs"]], [[{"at": "a.py:1-2"}]] * 2)
            receipts = [json.loads(row) for row in (root / "calls.jsonl").read_text().splitlines()]
            self.assertEqual([r["judgments"] for r in receipts], [1, 1])

    def test_large_native_error_reduces_to_bounded_typed_error_without_replay(self):
        with TemporaryDirectory() as folder:
            route = finalist_route.Gather(None, "repo", Path(folder), Path(folder), None, None, 1)
            budget = dict(finalist_route.contract.native_limits()["default_budget"], max_bytes=8192)
            message = {"id": "example", "params": {"arguments": {"task": "lookup", "budget": budget}}}
            native = {"code": "direct_observation_unavailable", "message": "x" * 20000,
                      "fresh_brokerage_required": True, "query_replay_safe": False, "query_submitted": None}
            error = finalist_route.workspace_scope.InventoryUnavailable(2, ("pub", "scope"), {
                "result": {"isError": True, "structuredContent": {"error": native}}})
            output = io.BytesIO()
            with patch.object(finalist_route.workspace_scope, "eligible_paths", side_effect=error) as inventory:
                reply = route.handle_message(message, "header")
                inventory.assert_called_once()
            mcp_wrapper.write_message(output, reply, "header")
            self.assertEqual((Path(folder) / "call-001.wire").read_bytes(), output.getvalue())
            self.assertLessEqual(len(output.getvalue()), 8192)
            detail = json.loads(reply["result"]["content"][0]["text"])["error"]
            self.assertEqual(detail["code"], "workspace_evidence_unavailable")
            self.assertEqual(detail["native_error"]["code"], native["code"])
            self.assertFalse(detail["native_error"]["query_replay_safe"])
            saved = json.loads((Path(folder) / "call-001.scores.json").read_text())["policy"]
            self.assertEqual(saved["native_inventory"]["reply"]["result"]["structuredContent"]["error"], native)

    def test_unframeable_request_terminates_without_emitting_over_budget_error(self):
        with TemporaryDirectory() as folder:
            route = finalist_route.Gather(None, "repo", Path(folder), Path(folder), None, None, 1)
            limit = finalist_route.contract.native_limits()["reply_frame_bytes"]
            request = {"id": "a" * limit, "params": {"arguments": {"task": "t"}}}
            input_stream, output = io.BytesIO(), io.BytesIO()
            mcp_wrapper.write_message(input_stream, request, "header")
            input_stream.seek(0)
            with patch.object(finalist_route.workspace_scope, "eligible_paths") as inventory:
                mcp_wrapper.serve(route, input_stream, output, framed_calls=True)
                inventory.assert_not_called()
            self.assertEqual(output.getvalue(), b"")
            self.assertEqual((Path(folder) / "call-001.wire").read_bytes(), b"")
            audit = json.loads((Path(folder) / "call-001.scores.json").read_text())["policy"]
            self.assertIn("unframeable_error", audit)

    def test_unsupported_request_never_scores_or_opens_inventory(self):
        with TemporaryDirectory() as folder:
            route = finalist_route.Gather(None, "repo", Path(folder), Path(folder), None, None, 1)
            message = {"id": 5, "params": {"arguments": {"task": "t", "fresh": True}}}
            with patch.object(finalist_route.workspace_scope, "eligible_paths") as inventory:
                reply = route.handle_message(message)
                inventory.assert_not_called()
            self.assertTrue(reply["result"]["isError"])
            self.assertEqual(json.loads(reply["result"]["content"][0]["text"])["error"]["code"], "unsupported_argument")