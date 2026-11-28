# SPDX-License-Identifier: Apache-2.0
"""Contract probes for the selected Gather prototype, with no model or external service."""

import hashlib
import json
import unittest
from dataclasses import replace
from types import SimpleNamespace

import finalist_gather_contract as contract
from adaptive_passage_budget import Passage


class FinalistContractTest(unittest.TestCase):
    def setUp(self):
        self.args = {"task": "Investigate", "needs": ["first", "second"]}
        self.budget = contract.native_limits()["default_budget"]
        self.lines = ["á雪" * 500, "x", "😀" * 700]
        source = "\r\n".join(self.lines).encode()
        self.repo = SimpleNamespace(texts={"src": self.lines}, digests={"src": hashlib.sha256(source).digest()})
        self.passages = [Passage(f"src:{n}-{n}", "src", n, n, self.lines[n-1] + "\n", n if n < 3 else None,
                                 f"src:{n}-{n}" if n < 3 else None) for n in (1, 2, 3)]
        self.scope = {"publication": "native-token", "scope_fingerprint": "grant"}

    def test_native_default_and_nested_cap(self):
        self.assertEqual(contract.reply_cap(self.args), 131072)
        self.assertEqual(contract.reply_cap({**self.args, "budget": {**self.budget, "max_bytes": 6000}}), 6000)
        with self.assertRaises(ValueError):
            contract.reply_cap({**self.args, "budget": {"max_bytes": 6000}})
        with self.assertRaises(ValueError):
            contract.reply_cap({**self.args, "budget": {**self.budget, "max_bytes": 2000}})

    def test_unsupported_semantics_fail_before_scoring(self):
        for control in ({"max_bytes": 6000}, {"continuation": "x"}, {"verification": "require"},
                        {"response_mode": "full"}, {"known": [{"path": "lib"}]},
                        {"budget": {**self.budget, "deadline_ms": 5}}):
            with self.subTest(control=control), self.assertRaises(contract.UnsupportedRequest):
                contract.reply_cap({**self.args, **control})
        for data in ({"task": ""}, {"needs": ["❄" * 134]}, {"needs": ["a", "a"]}, {"anchors": ["a\nb"]}):
            with self.subTest(data=data), self.assertRaises(ValueError):
                contract.reply_cap({**self.args, **data})

    def test_framed_reply_retains_real_workspace_hash_and_partial_needs(self):
        args = {**self.args, "budget": {**self.budget, "max_bytes": 6500}, "deliverable": "code_change"}
        for framing in ("line", "header"):
            wire, kept = contract.render(args, self.repo, "idx", self.scope, self.passages, 4, "測試", framing)
            self.assertLessEqual(len(wire), 6500)
            actual = wire.split(b"\r\n\r\n",1)[-1]
            frame = json.loads(actual)
            self.assertEqual(frame["id"], "測試")
            body = json.loads(frame["result"]["content"][0]["text"])
            self.assertEqual(body["eligibility"]["publication"], "native-token")
            self.assertNotIn("source_revision", body)
            self.assertEqual(body["omitted_count"], 4 - len(kept))
            for row in body["passages"]:
                self.assertEqual(row["workspace_sha256"], self.repo.digests["src"].hex())
                self.assertEqual(row["text_sha256"], hashlib.sha256(row["text"].encode()).hexdigest())
            for need in body["needs"]:
                self.assertNotIn(need["status"], ("complete", "covered"))
            self.assertIn("standing_guidance", body)

    def test_bundled_file_proposals_do_not_claim_individual_need_support(self):
        first = self.passages[0]
        shared = SimpleNamespace(at=first.at, text=first.text, supports=[], file_scored_at=first.scored_at)
        wire, _ = contract.render(self.args, self.repo, "idx", self.scope, [shared], 1, 1, "line",
                                  attribution="bundled_file")
        body = json.loads(json.loads(wire)["result"]["content"][0]["text"])
        self.assertEqual(body["evidence_basis"], "task_level_file_ranking")
        self.assertEqual(body["passages"][0]["file_scored_at"], first.scored_at)
        self.assertEqual(body["passages"][0]["needs"], [])
        self.assertTrue(all(n["status"] == "not_individually_scored" and n["where"] == [] for n in body["needs"]))
        with self.assertRaisesRegex(ValueError, "cannot be attributed"):
            contract.render(self.args, self.repo, "idx", self.scope, [first], 1, 1, "line", attribution="bundled_file")

    def test_refuses_missing_publication_wrong_content_and_invalid_scored_range(self):
        for scope, ps in (({}, self.passages), (self.scope, [replace(self.passages[0], text="other")]),
                          (self.scope, [replace(self.passages[0], scored_at="src:2-2")]),
                          (self.scope, [replace(self.passages[0], owner=3)])):
            with self.subTest(scope=scope, ps=ps), self.assertRaises(ValueError):
                contract.render(self.args, self.repo, "idx", scope, ps, len(ps), 1, "line")
