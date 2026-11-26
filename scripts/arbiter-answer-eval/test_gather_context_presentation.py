# SPDX-License-Identifier: Apache-2.0
"""Synthetic checks for the presentation-only Gather Context comparison."""

import json
import unittest

import common
from complete_evidence import CompleteEvidence, Passage
from gather_context_presentation import render
import loop_reply


def hit(path, first, last, text):
    return {
        "citation": {"path": path, "start_line": first, "end_line": last},
        "text": text,
    }


def envelope(text):
    return {"content": [{"type": "text", "text": text}]}


def dispatch_digest(text):
    return common.sha256_bytes(common.canonical_json([{"type": "inputText", "text": text}]))


class PresentationTests(unittest.TestCase):
    def setUp(self):
        self.passages = (
            Passage("a/readme.md", 1, 2, "unmatched Δ\r\n"),
            Passage("a/readme.md", 21, 22, "later evidence\n"),
            Passage("z/branch.rs", 7, 9, "first Ö\r\ncondition"),
            Passage("z/branch.rs", 30, 31, "first exception\n"),
        )
        self.treatment = CompleteEvidence(
            "bookbuilder", "revision", "manifest-fixture", self.passages, ("evaluator-only",),
        )
        self.ordinary = {
            "schema": loop_reply.RESPONSE_SCHEMA,
            "publication": {"token": "revision"},
            "parts": [
                {"id": 1, "sub_needs": [{"evidence": ["first", "omitted"]}]},
                {"id": 2, "sub_needs": [{"evidence": []}]},
            ],
            "records": [
                {"id": "later", "canonical_result": {"hits": [hit("a/readme.md", 20, 22, "yes")]}},
                {"id": "first", "canonical_result": {
                    "projection": "airk.arbiter.loop-evidence/1",
                    "spans": [
                        {"path": "z/branch.rs", "start_line": 5, "end_line": 7, "text": "branch"},
                        {"path": "z/branch.rs", "start_line": 30, "end_line": 30, "text": "except"},
                    ],
                }},
            ],
            "record_uses": [{"part": 2, "record": "later"}],
        }
        self.native = envelope(json.dumps(self.ordinary))
        self.archived = self.treatment.augment(self.native)["content"][0]["text"]

    def render(self, *, native=None, text=None, digest=None):
        if text is None:
            text = self.archived
        return render(self.native if native is None else native, text.encode("utf-8"),
                      treatment=self.treatment,
                      recorded_output_sha256=dispatch_digest(text) if digest is None else digest)

    def test_moves_only_passages_and_preserves_exact_actor_bytes(self):
        pair = self.render()
        self.assertEqual(pair.original, self.archived.encode())
        self.assertEqual(pair.original_output_sha256, dispatch_digest(self.archived))
        self.assertEqual(pair.grouped_output_sha256, dispatch_digest(pair.grouped.decode()))
        self.assertNotEqual(pair.grouped_output_sha256, pair.original_output_sha256)
        self.assertEqual(len(pair.original), len(pair.grouped))
        grouped = json.loads(pair.grouped)
        self.assertEqual(list(grouped), [
            "schema", "publication", "parts", "source_passages", "records", "record_uses",
        ])
        self.assertEqual(grouped.pop("source_passages"), [
            hit("z/branch.rs", 7, 9, "first Ö\r\ncondition"),
            hit("z/branch.rs", 30, 31, "first exception\n"),
            hit("a/readme.md", 21, 22, "later evidence\n"),
            hit("a/readme.md", 1, 2, "unmatched Δ\r\n"),
        ])
        self.assertEqual(grouped, self.ordinary)
        self.assertNotIn(b"evaluator-only", pair.grouped)

    def test_rejects_crossed_native_answer_or_dispatch(self):
        with self.assertRaisesRegex(common.HarnessError, "digest differs"):
            self.render(digest="0" * 64)
        native = self.ordinary | {"parts": []}
        with self.assertRaisesRegex(common.HarnessError, "differs from ordinary Answer"):
            self.render(native=envelope(json.dumps(native)))
        native = self.ordinary | {"publication": {"token": "other"}}
        with self.assertRaisesRegex(common.HarnessError, "publication differs"):
            self.render(native=envelope(json.dumps(native)))

    def test_rejects_lost_altered_or_reordered_passages_even_with_fresh_digest(self):
        for action in ("remove", "alter", "reorder"):
            with self.subTest(action=action):
                changed = json.loads(self.archived)
                passages = changed["source_passages"]
                if action == "remove":
                    passages.pop()
                elif action == "alter":
                    passages[0]["text"] = passages[0]["text"].replace("\r\n", "\n")
                else:
                    passages.reverse()
                with self.assertRaisesRegex(common.HarnessError, "citation or excerpt differs"):
                    self.render(text=json.dumps(changed))
