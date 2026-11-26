# SPDX-License-Identifier: Apache-2.0
"""Actor dispatch, budget, transcript, and receipt controls without starting a child."""

import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock

import codex_isolation_probe as codex_driver
import common
import gather_context_actor as actor


TASK = "Which branch returns the selected value?"
TOOL = actor.ARCHIVE_TOOL
REPLY = {"content": [{"type": "text", "text": '{"range":"branch.py:5-8","text":"résultat"}'}]}
REPLY_SHA = common.sha256_bytes(common.canonical_json(REPLY))
ROOT = Path(__file__).resolve().parent


def actor_event(method, params, rpc_id=None):
    event = {"method": method, "params": params}
    if rpc_id is not None:
        event["id"] = rpc_id
    return event


def archive_dispatch(**options):
    return actor.ActorDispatch("archived", TASK, options.pop("limit", 200), 10.0, options.pop("clock", lambda: 0.0),
                               archived_reply=options.pop("reply", REPLY),
                               archived_reply_sha256=options.pop("digest", REPLY_SHA), **options)


def observed_with_reply(dispatch):
    arguments = {"task": TASK}
    request = {"threadId": "fresh-thread", "turnId": "fresh-turn", "namespace": codex_driver.DYNAMIC_NAMESPACE,
               "callId": "call-one", "tool": TOOL, "arguments": arguments}
    calls = []
    event = actor_event("item/tool/call", request, 8)
    codex_driver.dispatch_tool(event, "fresh-thread", "fresh-turn", dispatch, {TOOL}, calls)
    final = "The selected branch is in branch.py:5-8."
    events = [
        event,
        actor_event("item/completed", {
            "threadId": "fresh-thread", "turnId": "fresh-turn",
            "item": {"type": "agentMessage", "phase": "final_answer", "text": final},
        }),
        actor_event("turn/completed", {
            "threadId": "fresh-thread", "turn": {"id": "fresh-turn", "status": "completed"},
        }),
    ]
    stream = b"".join(common.canonical_json(row) + b"\n" for row in events)
    status = {"configured_mcp": {}, "instructions_count": 0, "model_observed": actor.MODEL,
              "effort_observed": actor.EFFORT, "provider_observed": "openai", "auth_route": "chatgpt",
              "thread_id": "fresh-thread", "turn_id": "fresh-turn", "turn_status": "completed", "final": final,
              "attempts": calls}
    return stream, status


class ActorDispatchTests(unittest.TestCase):
    def test_archive_is_bound_to_exact_first_task_and_frozen_reply(self):
        reply = {"content": [dict(REPLY["content"][0])]}
        dispatch = archive_dispatch(reply=reply)
        reply["content"][0]["text"] = "altered after admission"
        self.assertEqual(dispatch(TOOL, {"task": TASK}), REPLY)
        self.assertTrue(dispatch(TOOL, {"task": TASK})["isError"])
        self.assertEqual(dispatch.replies[0], common.canonical_json(REPLY))
        self.assertEqual(dispatch.rows[0]["evidence_text_bytes"], len(REPLY["content"][0]["text"].encode("utf-8")))
        self.assertEqual(dispatch.rows[1]["evidence_text_bytes"], 0)

    def test_invalid_first_call_cannot_be_replayed_as_a_valid_archive_dispatch(self):
        dispatch = archive_dispatch()
        self.assertTrue(dispatch(TOOL, {"task": TASK + " "})["isError"])
        self.assertTrue(dispatch(TOOL, {"task": TASK})["isError"])
        self.assertEqual(dispatch.evidence_bytes, 0)

    def test_archive_refuses_changed_pin_and_budget_overrun(self):
        with self.assertRaisesRegex(common.HarnessError, "digest differs"):
            archive_dispatch(digest="0" * 64)
        with self.assertRaisesRegex(common.HarnessError, "shared evidence byte limit"):
            archive_dispatch(limit=8)

    def test_ordinary_tool_retains_evidence_and_refuses_result_past_shared_budget(self):
        raw = {"content": [{"type": "text", "text": "abcé"}]}
        with mock.patch.object(actor.sources, "dispatch", return_value=raw) as source:
            dispatch = actor.ActorDispatch("ordinary", TASK, 7, 10.0, lambda: 0.0, source_root=ROOT)
            self.assertEqual(dispatch("source_list", {}), raw)
            self.assertTrue(dispatch("source_list", {})["isError"])
        source.assert_called_with(ROOT, "source_list", {})
        self.assertEqual(dispatch.evidence_bytes, 5)
        self.assertEqual([row["success"] for row in dispatch.rows], [True, False])

    def test_deadline_reached_during_source_read_delivers_only_refusal(self):
        clock = iter([9.0, 10.0]).__next__
        dispatch = archive_dispatch(clock=clock)
        self.assertTrue(dispatch(TOOL, {"task": TASK})["isError"])
        self.assertEqual(dispatch.evidence_bytes, 0)

    def test_actor_transcript_authenticates_delivery_and_final(self):
        dispatch = archive_dispatch()
        stream, status = observed_with_reply(dispatch)
        self.assertEqual(actor._audit_actor(stream, status, dispatch), [])
        status["attempts"][0]["output_sha256"] = "0" * 64
        self.assertIn("Actor tool dispatch or reply digest differs", actor._audit_actor(stream, status, dispatch))
        status["final"] = "altered after completion"
        self.assertIn("Retained actor brief differs from the transcript", actor._audit_actor(stream, status, dispatch))


class ActorReceiptTests(unittest.TestCase):
    def run_in_memory(self, runner, after_error=None):
        actions = []
        receipts = {}
        directory = ROOT / "fake-actor-directory"
        config = SimpleNamespace(isolated_home=ROOT / "fake-codex-home")

        def audit(phase, observed_directory, observed_home):
            self.assertEqual((observed_directory, observed_home), (directory, config.isolated_home))
            actions.append(phase)
            if phase == "after" and after_error:
                raise common.HarnessError(after_error)

        def write(path, raw):
            self.assertEqual(path.parent, directory)
            actions.append(path.name)
            self.assertNotIn(path.name, receipts)
            receipts[path.name] = raw

        with (
            mock.patch.object(actor, "_workspace"),
            mock.patch.object(actor.run_session, "codex_provenance", return_value={"auth_route": "isolated_chatgpt"}),
            mock.patch.object(actor.run_session, "codex_command", return_value=["pinned-codex", "app-server"]),
            mock.patch.object(actor.run_session, "codex_child_env", return_value={}),
        ):
            kwargs = dict(config=config, instruction=TASK, directory=directory, timeout_s=30, arm="archived",
                          reply_byte_limit=200, audit_workspace=audit, write_receipt=write, archived_reply=REPLY,
                          archived_reply_sha256=REPLY_SHA, runner=runner, clock=lambda: 0.0)
            if not after_error:
                return actor.run_actor(**kwargs), actions, receipts
            with self.assertRaisesRegex(common.HarnessError, after_error):
                actor.run_actor(**kwargs)
            return None, actions, receipts

    def test_terminal_receipt_is_written_after_workspace_audit_and_all_payloads(self):
        def runner(command, env, directory, **kwargs):
            self.assertEqual(kwargs["instruction"], TASK)
            self.assertEqual(kwargs["tool_catalog"], actor.ARCHIVE_TOOLS)
            self.assertEqual(kwargs["timeout"], 30)
            self.assertEqual((kwargs["model"], kwargs["effort"]), (actor.MODEL, actor.EFFORT))
            self.assertTrue(kwargs["clean_home"])
            self.assertTrue(kwargs["require_chatgpt"])
            self.assertIn("gather_context with the task text", kwargs["base_instructions"])
            stream, status = observed_with_reply(kwargs["dispatch"])
            return 0, stream, b"", status

        record, actions, receipt = self.run_in_memory(runner)
        self.assertEqual(record["status"], "complete")
        self.assertEqual(actions[:2], ["before", "after"])
        self.assertEqual(actions[-1], "session.json")
        self.assertEqual(json.loads(receipt["session.json"]), record)
        self.assertEqual(json.loads(receipt["reply-001.json"]), REPLY)
        self.assertEqual(json.loads(receipt["tool-offer.json"]), codex_driver.dynamic_specs(actor.ARCHIVE_TOOLS))
        self.assertEqual({name: common.sha256_bytes(receipt[name]) for name in record["files"]}, record["files"])

    def test_child_startup_failure_is_audited_and_recorded(self):
        def runner(*args, **kwargs):
            raise FileNotFoundError("synthetic startup error")

        record, actions, receipt = self.run_in_memory(runner)
        self.assertEqual(record["status"], "failed")
        self.assertIn("Actor process failed to start", record["problems"])
        self.assertEqual(actions[:2], ["before", "after"])
        self.assertFalse(json.loads(receipt["session.json"])["tool_calls"])

    def test_failed_post_run_owner_audit_prevents_any_receipt_write(self):
        def runner(*args, **kwargs):
            raise FileNotFoundError("synthetic startup error")

        _, actions, receipt = self.run_in_memory(runner, after_error="foreign child output")
        self.assertEqual(actions, ["before", "after"])
        self.assertEqual(receipt, {})


if __name__ == "__main__":
    unittest.main()
