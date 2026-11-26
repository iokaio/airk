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

    def test_ticket_and_crossed_publication_are_ineligible(self):
        treatment = self.load()
        for extra in ({"ticket": {"id": "queued"}}, {"scope": {"publication": {"token": "different"}}}):
            with patch.dict(self.body, extra), self.assertRaises(common.HarnessError):
                treatment.augment(self.reply())
