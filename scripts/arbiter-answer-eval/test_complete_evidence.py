# SPDX-License-Identifier: Apache-2.0
"""Synthetic exact ceiling delivery tests. No fixture, provider or service."""

import json
import unittest
from pathlib import Path
from unittest.mock import patch

import codex_isolation_probe as driver
import common
import complete_evidence as ceiling
import evidence
import loop_reply
import mcp_wrapper
import replay
import run_session
import synthetic


class CompleteEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.data = "before\r\nΔ sentinel\r\nno final newline".encode()
        self.note = "secret-note-key"
        self.source = Path(__file__).resolve()
        span = {"essential_ids": [self.note], "path": self.source.name, "start_line": 2,
                "end_line": 3, "source_sha256": common.sha256_bytes(self.data),
                "excerpt_sha256": common.sha256_bytes(self.data.split(b"\n", 1)[1])}
        self.manifest = {"schema": "airk.private.complete-evidence-input/1", "repo": "airk",
                         "slot": "C1", "scenario_sha256": "a" * 64, "publication_token": "revision",
                         "spans": [span]}
        self.body = {"schema": loop_reply.RESPONSE_SCHEMA, "publication": {"token": "revision"}}

    def load(self, **changes):
        raw = common.canonical_json(self.manifest)
        args = dict(expected_sha256=common.sha256_bytes(raw), source_root=self.source.parent,
                    repo="airk", slot="C1", scenario_sha256="a" * 64, publication_token="revision",
                    essential_ids=[self.note])
        with patch.object(Path, "read_bytes", return_value=self.data):
            return ceiling.CompleteEvidence.from_manifest(raw, **(args | changes))

    def reply(self):
        return {"content": [{"type": "text", "text": json.dumps(self.body)}]}

    @staticmethod
    def digest(reply):
        return common.sha256_bytes(common.canonical_json(
            [{"type": "inputText", "text": block["text"]} for block in reply["content"]]))

    def test_real_wrapper_dispatch_and_ordinary_followup(self):
        treatment = self.load()
        native = self.reply()
        raw = common.canonical_json(native)
        outcome = common.CallOutcome(True, 0, raw, b"", 1, native, None)
        records, files = [], {}
        with (patch.object(Path, "mkdir"), patch.object(common, "write_bytes", side_effect=files.__setitem__),
              patch.object(common, "append_jsonl", side_effect=lambda _, row: records.append(row))):
            wrapper = mcp_wrapper.Wrapper(handle=None, repo="airk", path="answer", instruction="task",
                                          catalog={t["name"]: t for t in synthetic.catalog()["tools"]},
                                          log_dir=Path("log"),
                                          backend=lambda *_: outcome, complete_evidence=treatment)
            attempts = []
            params = {"callId": "1", "threadId": "actor", "turnId": "t", "namespace": driver.DYNAMIC_NAMESPACE,
                      "tool": "airk_answer", "arguments": {"question": "task"}}
            sent = driver.dispatch_tool({"id": 1, "params": params}, "actor", "t",
                                        wrapper.call, wrapper.tools, attempts)
            self.assertEqual(wrapper.call("airk_answer", {"question": "followup"}), native)
        first, second = records
        self.assertEqual(first["first_call"]["rule"], "canonical")
        self.assertEqual(files[Path(first["native_result_path"])], raw)
        actual = json.loads(files[Path(first["result_path"])])
        self.assertNotIn("native_result_path", second)
        self.assertEqual(files[Path(second["result_path"])], raw)
        receipt = treatment.verify_dispatch(actual, attempts[0]["output_sha256"])
        self.assertEqual(receipt["essential_ids"], [self.note])
        text = sent["result"]["contentItems"][0]["text"]
        self.assertNotIn(self.note, text)
        self.assertNotIn("excerpt_sha256", text)
        self.assertEqual(actual["content"][0]["text"], text)
        hits = evidence.extract(json.loads(text))
        self.assertEqual([(s.start, s.end, s.text) for s in hits], [(2, 3, "Δ sentinel\r\nno final newline")])

    def test_delivery_truncation_or_reordering_with_fresh_digest(self):
        span = dict(self.manifest["spans"][0], start_line=1, end_line=1,
                    excerpt_sha256=common.sha256_bytes(b"before\r\n"))
        self.manifest["spans"].append(span)
        treatment = self.load()
        reply = treatment.augment(self.reply())
        for change in ("trim", "reorder"):
            body = json.loads(reply["content"][0]["text"])
            if change == "trim":
                body["source_passages"][0]["text"] = "before"
            else:
                body["source_passages"].reverse()
            altered = {"content": [{"type": "text", "text": json.dumps(body)}]}
            with self.assertRaisesRegex(common.HarnessError, "excerpt differs"):
                treatment.verify_dispatch(altered, self.digest(altered))
        with self.assertRaisesRegex(common.HarnessError, "digest differs"):
            treatment.verify_dispatch(reply, "0" * 64)

    def test_reject_source_mismatch_and_incomplete_roster(self):
        with self.assertRaises(common.HarnessError):
            self.load(essential_ids=[self.note, "missing"])
        with self.assertRaises(common.HarnessError):
            self.load(slot="A1")
        for field, bad in (("path", "../secret"), ("source_sha256", "0" * 64),
                           ("excerpt_sha256", "0" * 64), ("end_line", 4)):
            with self.subTest(field=field), patch.dict(self.manifest["spans"][0], {field: bad}):
                with self.assertRaises(common.HarnessError):
                    self.load()

    def test_actor_delivery_receipt_and_refusals(self):
        treatment = self.load()
        native = self.reply()
        directory = Path("session").resolve()
        results = directory / "wrapper" / "results"
        native_path = results / "turn-01.native.stdout.json"
        delivered_path = results / "turn-01.stdout.json"
        delivered = treatment.augment(native)
        output_sha = self.digest(delivered)
        files = {native_path: common.canonical_json(native), delivered_path: common.canonical_json(delivered)}
        interface = {"question": "task"}
        _, sent = replay.canonical_first_call("answer", "task")
        first = {"turn": 1, "call": 1, "arguments": interface, "sent_arguments": sent, "ok": True,
                 "tool": "airk_answer", "rejected": False, "first_call": {"rule": "canonical"},
                 "native_result_path": str(native_path), "result_path": str(delivered_path),
                 "native_result_sha256": common.sha256_bytes(files[native_path]),
                 "result_sha256": common.sha256_bytes(files[delivered_path]),
                 "complete_evidence_manifest_sha256": treatment.manifest_sha256,
                 "bytes": len(common.envelope_text(delivered).encode())}
        rows = [first]
        meta = {"threadId": "thread", "turnId": "turn"}
        call = {**meta, "callId": "call", "tool": "airk_answer", "arguments": interface,
                "namespace": driver.DYNAMIC_NAMESPACE}
        transcript = [{"method": "item/tool/call", "id": 9, "params": call},
                      {"method": "item/completed", "params": {**meta, "item": {"type": "agentMessage",
                                                                            "text": "brief"}}},
                      {"method": "turn/completed", "params": {"threadId": "thread", "turn": {
                          "id": "turn", "status": "completed"}}}]
        attempt = {"tool": "airk_answer", "call_id": "call", "rpc_id": 9, "arguments": interface,
                   "success": True, "output_sha256": output_sha}
        observed = {"thread_id": "thread", "turn_id": "turn", "turn_status": "completed", "final": "brief",
                    "attempts": [attempt], "model_observed": "gpt-6.2-sol", "effort_observed": "medium",
                    "auth_route": "chatgpt", "provider_observed": "openai"}
        record = {"status": "complete", "is_error": False, "exit_code": 0, "session_id": "thread",
                  "model_requested": "gpt-6.2-sol", "effort_requested": "medium", "path": "answer",
                  "stream_sha256": "stream.jsonl", "brief_sha256": "brief.md", "codex_observed": observed,
                  "complete_evidence_manifest_sha256": treatment.manifest_sha256, "answer_defaults": None}
        def verify():
            return run_session.verify_complete_evidence(directory, record, treatment)

        with (patch.object(common, "sha256_file", side_effect=lambda p: p.name),
              patch.object(common, "read_jsonl", side_effect=lambda p: transcript if p.name == "stream.jsonl" else rows),
              patch.object(Path, "read_bytes", autospec=True, side_effect=lambda p: files[p]),
              patch.object(Path, "read_text", autospec=True, side_effect=lambda p, **_: "brief" if p.name == "brief.md" else "task")):
            self.assertEqual(verify()["output_sha256"], output_sha)
            for obj, delta in ((observed, {"auth_route": "other"}), (attempt, {"output_sha256": "other"}),
                               (record, {"status": "timeout"}), (first, {"complete_evidence_manifest_sha256": "other"}),
                               (call, {"threadId": "other"}), (first, {"bytes": first["bytes"] - 1})):
                with self.subTest(delta=delta), patch.dict(obj, delta), self.assertRaises(common.HarnessError):
                    verify()
            with patch("judge.FIRST_REPLY_CAP", 1), self.assertRaisesRegex(common.HarnessError, "clipped"):
                verify()

    def test_ticket_and_crossed_publication_are_ineligible(self):
        treatment = self.load()
        for extra in ({"ticket": {"id": "queued"}}, {"scope": {"publication": {"token": "different"}}}):
            with patch.dict(self.body, extra), self.assertRaises(common.HarnessError):
                treatment.augment(self.reply())
