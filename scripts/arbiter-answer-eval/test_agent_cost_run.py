# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the cost runner and the one-tool MCP server."""

import json
import tempfile
import unittest
from pathlib import Path

import agent_cost_run as runner
import gather_context_mcp as server


def assistant(message_id, *blocks):
    return {"type": "assistant", "message": {"id": message_id, "content": list(blocks)}}


class SummaryTest(unittest.TestCase):
    def test_round_trips_are_distinct_model_requests_and_cost_is_the_agents_own_report(self):
        events = [
            {"type": "system", "subtype": "init", "tools": ["Read", "mcp__airk__gather_context"],
             "mcp_servers": [{"name": "airk"}]},
            assistant("m1", {"type": "tool_use", "name": "mcp__airk__gather_context", "id": "t1", "input": {}}),
            assistant("m2", {"type": "tool_use", "name": "Read", "id": "t2", "input": {}}),
            assistant("m2", {"type": "tool_use", "name": "Read", "id": "t3", "input": {}}),  # same request, second block
            {"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "map" * 10}]},
                {"type": "tool_result", "tool_use_id": "t2", "content": "line one\nline two\n"},
                {"type": "tool_result", "tool_use_id": "t3", "content": "abc"}]}},
            assistant("m3", {"type": "text", "text": "Done: a.rs:4"}),
            {"type": "result", "subtype": "success", "is_error": False, "num_turns": 4, "total_cost_usd": 0.0421,
             "duration_ms": 9000, "duration_api_ms": 7000, "permission_denials": [],
             "usage": {"input_tokens": 12, "cache_read_input_tokens": 3000, "cache_creation_input_tokens": 900,
                       "output_tokens": 210}},
        ]
        got = runner.summarize(events)
        self.assertEqual((got["round_trips"], got["tool_use_total"], got["cost_usd"]), (3, 3, 0.0421))
        self.assertEqual(got["tool_uses"], {"mcp__airk__gather_context": 1, "Read": 2})
        self.assertEqual(got["tokens"], {"input": 12, "cache_read": 3000, "cache_write": 900, "output": 210})
        self.assertEqual(got["tool_result_bytes"], {"mcp__airk__gather_context": 30, "Read": 21})
        self.assertEqual(got["own_read_bytes"], 21)
        self.assertEqual((got["completed"], got["final_message"], got["mcp_servers"]), (True, "Done: a.rs:4", ["airk"]))

    def test_a_session_without_a_result_is_not_completed(self):
        got = runner.summarize([assistant("m1", {"type": "text", "text": "partial"})])
        self.assertEqual((got["completed"], got["round_trips"], got["cost_usd"]), (False, 1, None))

    def test_the_plain_arm_is_offered_no_gather_context_tool(self):
        plain = runner.command("claude", "m", "plain", Path("c.json"), 5)
        tool = runner.command("claude", "m", "tool", Path("c.json"), 5)
        self.assertEqual(plain[plain.index("--tools") + 1], "Read,Grep,Glob,Edit,Write")
        self.assertEqual(tool[tool.index("--tools") + 1], "Read,Grep,Glob,Edit,Write,mcp__airk__gather_context")
        self.assertIn("Bash", plain[plain.index("--disallowedTools") + 1].split(","))


class ChangesTest(unittest.TestCase):
    def test_changed_added_and_removed_files_are_reported_with_snapshot_lines(self):
        with tempfile.TemporaryDirectory() as folder:
            snapshot, work = Path(folder) / "s", Path(folder) / "w"
            for root in (snapshot, work):
                (root / "src").mkdir(parents=True)
                (root / "src" / "a.rs").write_text("one\ntwo\nthree\nfour\n", encoding="utf-8", newline="\n")
                (root / "gone.md").write_text("old\n", encoding="utf-8", newline="\n")
            (work / "src" / "a.rs").write_text("one\nTWO\nthree\nfour\n", encoding="utf-8", newline="\n")
            (work / "gone.md").unlink()
            (work / "new.rs").write_text("fn new() {}\n", encoding="utf-8", newline="\n")
            patch, rows = runner.changes(snapshot, work)
        self.assertEqual([(row["path"], row["state"]) for row in rows],
                         [("gone.md", "removed"), ("new.rs", "added"), ("src/a.rs", "modified")])
        self.assertEqual(rows[2]["snapshot_lines"], [[2, 2]])
        self.assertIn("-two\n+TWO\n", patch)


class ServerTest(unittest.TestCase):
    def test_the_tool_is_served_as_advertised_and_each_call_is_logged(self):
        seen = []

        def backend(native, repo, arguments):
            seen.append((native, repo, arguments))
            return "reply text", False, b'{"raw": true}'

        tool = {"name": "gather_context", "description": "as the build says", "inputSchema": {"type": "object"},
                "listed_name": "gather_context"}
        with tempfile.TemporaryDirectory() as folder:
            mcp = server.Server(tool=tool, repo="munarium", log_dir=Path(folder), backend=backend,
                                instructions="guidance", max_calls=1)
            start = mcp.handle_message({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
            listed = mcp.handle_message({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
            first = mcp.handle_message({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                                        "params": {"name": "gather_context", "arguments": {"task": "t"}}})
            second = mcp.handle_message({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                                         "params": {"name": "gather_context", "arguments": {"task": "t"}}})
            other = mcp.handle_message({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                                        "params": {"name": "search", "arguments": {}}})
            logged = [json.loads(line) for line in (Path(folder) / "calls.jsonl").read_text(encoding="utf-8").splitlines()]
            raw = (Path(folder) / "call-001.reply.json").read_bytes()
        self.assertEqual(start["result"]["instructions"], "guidance")
        self.assertEqual(listed["result"]["tools"], [{"name": "gather_context", "description": "as the build says",
                                                     "inputSchema": {"type": "object"}}])
        self.assertEqual(seen, [("airk_answer", "munarium", {"task": "t"})])
        self.assertEqual(first["result"], {"content": [{"type": "text", "text": "reply text"}], "isError": False})
        self.assertTrue(second["result"]["isError"] and other["result"]["isError"])
        self.assertEqual((len(logged), logged[0]["reply_bytes"], logged[0]["arguments"], raw),
                         (1, 10, {"task": "t"}, b'{"raw": true}'))


if __name__ == "__main__":
    unittest.main()
