# SPDX-License-Identifier: Apache-2.0
"""External call budgets preserve the original diagnostic and product prompts."""

import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock

import codex_isolation_probe as driver
import common
import gather_context_actor as actor
import gather_context_judge as diagnostic_judge
import gather_context_call_limit as limits
import run_session


class GatherContextToolCallLimitTests(unittest.TestCase):
    def test_twelve_preserves_existing_prompt_bytes(self):
        base = run_session.SYSTEM_PROMPT
        self.assertEqual(limits.set_prompt_tool_call_limit(base, 12).encode("utf-8"), base.encode("utf-8"))
        self.assertEqual(common.TURN_CAP, 12)

    def test_twenty_four_changes_only_the_external_cap(self):
        base = run_session.SYSTEM_PROMPT.format(primary="gather_context", field="task")
        amended = limits.set_prompt_tool_call_limit(base, 24)
        self.assertEqual(amended.replace("24 tool calls", "12 tool calls", 1), base)
        self.assertIn('as its "task" argument', amended)
        self.assertEqual(limits.DEFAULT_TOOL_CALL_LIMIT, 24)

    def test_unknown_budget_and_changed_prompt_are_refused(self):
        for cap in (True, 11, 13, 25, "24", 24.0):
            with self.subTest(cap=cap), self.assertRaisesRegex(common.HarnessError, "limit of 12 or 24"):
                limits.validate_tool_call_limit(cap)
        with self.assertRaisesRegex(common.HarnessError, "prompt tool-call instruction changed"):
            limits.set_prompt_tool_call_limit("You may make at most 12 tool calls.", 24)

    def test_reply_artifacts_have_a_finite_roster_for_each_cap(self):
        self.assertEqual(limits.reply_artifact(12, 12), "reply-012.json")
        self.assertIsNone(limits.reply_artifact(13, 12))
        self.assertEqual(limits.reply_artifact(13, 24), "work/reply-013.json")
        self.assertEqual(limits.reply_artifact(24, 24), "work/reply-024.json")
        self.assertIsNone(limits.reply_artifact(25, 24))


    def test_attempt_25_is_audited_and_discarded_after_24_admitted_replies(self):
        root = Path(__file__).resolve().parent
        config = SimpleNamespace(isolated_home=root / "unused-codex-home")
        evidence = {"content": [{"type": "text", "text": "{}"}]}
        raw_receipts = {}
        audits = []

        def runner(*args, **kwargs):
            self.assertIn("at most 24 tool calls", kwargs["base_instructions"])
            dispatcher = kwargs["dispatch"]
            events, attempts = [], []
            for ordinal in range(1, 26):
                params = {
                    "threadId": "fresh-thread", "turnId": "fresh-turn", "namespace": driver.DYNAMIC_NAMESPACE,
                    "callId": f"call-{ordinal}", "tool": "source_list", "arguments": {},
                }
                request = {"method": "item/tool/call", "id": ordinal, "params": params}
                reply = driver.dispatch_tool(request, "fresh-thread", "fresh-turn", dispatcher,
                                             {"source_list"}, attempts)
                self.assertEqual(reply["result"]["success"], ordinal <= 24)
                events.append(request)
            final = "The scope was inspected."
            events.extend([
                {"method": "item/completed", "params": {
                    "threadId": "fresh-thread", "turnId": "fresh-turn",
                    "item": {"type": "agentMessage", "phase": "final_answer", "text": final},
                }},
                {"method": "turn/completed", "params": {
                    "threadId": "fresh-thread", "turn": {"id": "fresh-turn", "status": "completed"},
                }},
            ])
            stream = b"".join(common.canonical_json(event) + b"\n" for event in events)
            observed = {"configured_mcp": {}, "instructions_count": 0, "model_observed": actor.MODEL,
                        "effort_observed": actor.EFFORT, "provider_observed": "openai", "auth_route": "chatgpt",
                        "thread_id": "fresh-thread", "turn_id": "fresh-turn", "turn_status": "completed",
                        "final": final, "attempts": attempts}
            return 0, stream, b"", observed

        with (
            mock.patch.object(actor, "_workspace"),
            mock.patch.object(actor.run_session, "codex_provenance", return_value={"auth_route": "isolated_chatgpt"}),
            mock.patch.object(actor.run_session, "codex_command", return_value=["codex"]),
            mock.patch.object(actor.run_session, "codex_child_env", return_value={}),
            mock.patch.object(actor.sources, "dispatch", return_value=evidence) as dispatch,
        ):
            record = actor.run_actor(
                config=config, instruction="Locate the owner.", directory=root, timeout_s=20, arm="ordinary",
                reply_byte_limit=100, tool_call_limit=24, source_root=root, runner=runner, clock=lambda: 0.0,
                audit_workspace=lambda phase, directory, home: audits.append(phase),
                write_receipt=lambda path, raw: raw_receipts.update({path.relative_to(root).as_posix(): raw}),
            )
        self.assertEqual(audits, ["before", "after"])
        self.assertEqual(dispatch.call_count, 24)
        self.assertEqual(record["status"], "discarded")
        self.assertEqual(record["problems"], ["Actor exceeded the tool call cap"])
        self.assertEqual((record["max_tool_calls"], record["tool_calls"], record["evidence_text_bytes"]), (24, 25, 48))
        turns = [json.loads(row) for row in raw_receipts["turns.jsonl"].splitlines()]
        self.assertFalse(turns[-1]["success"])
        self.assertIsNone(turns[-1]["reply_file"])
        self.assertTrue(all(f"reply-{i:03d}.json" in raw_receipts for i in range(1, 13)))
        self.assertTrue(all(f"work/reply-{i:03d}.json" in raw_receipts for i in range(13, 25)))
        self.assertNotIn("work/reply-025.json", raw_receipts)


    def test_extended_reader_rejects_names_outside_the_exact_work_roster(self):
        for alias in ("work/reply-025.json", "work/../work/reply-013.json", r"work\reply-013.json",
                      "work/REPLY-013.json", "work/reply-013.json:other", "reply-013.json:other"):
            with self.subTest(alias=alias):
                self.check_extended_artifact(alias)

    def test_extended_reader_rejects_linked_reply_even_with_valid_content_identity(self):
        self.check_extended_artifact("work/reply-013.json", aliased=True)

    def check_extended_artifact(self, name, *, aliased=False):
        root = Path(__file__).resolve().parent / "unused-actor-root"
        expected = "frozen-prompt-sha"
        record = {"status": "complete", "actor": {"freeze_sha256": "frozen-run"},
                  "max_tool_calls": 24, "tool_calls": 13,
                  "files": {"base-instructions.txt": expected, name: "frozen-result-sha"}}

        def resolve(path, **kwargs):
            if aliased and path.name == "reply-013.json":
                return root / "other" / path.name
            return path

        with (
            mock.patch.object(Path, "read_bytes", return_value=common.canonical_json(record)),
            mock.patch.object(Path, "resolve", resolve),
            mock.patch.object(Path, "is_file", return_value=True),
            mock.patch.object(Path, "stat", return_value=SimpleNamespace(st_nlink=1)),
            mock.patch.object(common, "sha256_file", return_value=expected) as digest,
            self.assertRaisesRegex(common.HarnessError, "aliased" if aliased else "outside the frozen layout"),
        ):
            diagnostic_judge.read_actor(root, "frozen-run", {}, tool_call_limit=24, base_instruction_sha=expected)
        digest.assert_called_once_with(root / "base-instructions.txt")


if __name__ == "__main__":
    unittest.main()
