# SPDX-License-Identifier: Apache-2.0
import json
import unittest

from adaptive_passage_budget import Passage
from passage_reply_budget import render


class ReplyBudgetTests(unittest.TestCase):
    def setUp(self):
        self.passages = [Passage('src/語:1-1', 'src/語', 1, 1, '日本語🙂"\\\n', 1, 'src/語:1-1'),
                         Passage('src/語:2-2', 'src/語', 2, 2, 'again\n', None, None)]

    def frame(self, limit, framing="line", passages=None):
        return render('réviser "語"', ['必要🙂'], 'a' * 40, self.passages if passages is None else passages,
                      3, limit, request_id='ID/語', framing=framing)

    def test_unicode_escaped_text_exact_wire_boundary(self):
        wire, chosen = self.frame(20000)
        self.assertEqual(chosen, self.passages)
        self.assertEqual(self.frame(len(wire))[0], wire)
        short, fewer = self.frame(len(wire)-1)
        self.assertLessEqual(len(short), len(wire)-1)
        self.assertLess(len(fewer), 2)
        result = json.loads(wire)["result"]
        body = json.loads(result["content"][0]["text"])
        self.assertEqual(body["passages"][0]["text"], self.passages[0].text)
        self.assertEqual(body["omitted_count"], 1)
        self.assertEqual(body["needs"][0]["where"], [{"at": self.passages[0].at}])

    def test_empty_and_small_caller_limit(self):
        empty, _ = self.frame(20000, passages=[])
        frame, chosen = self.frame(len(empty)-1)
        self.assertTrue(json.loads(frame)["result"]["isError"])
        self.assertFalse(chosen)
        self.assertEqual(self.frame(1), (None, []))
        self.assertEqual(json.loads(json.loads(empty)["result"]["content"][0]["text"])["needs"][0]["status"], "no_candidate_evidence")

    def test_header_framing_counts_all_wire_bytes(self):
        header, chosen = self.frame(20000, "header")
        line, _ = self.frame(20000)
        self.assertGreater(len(header), len(line))
        metadata, payload = header.split(b"\r\n\r\n", 1)
        self.assertEqual(int(metadata.split(b":")[1]), len(payload))
        limited, fewer = self.frame(len(header)-1, "header")
        self.assertLess(len(fewer), len(chosen))
        self.assertLess(len(limited), len(header))

    def test_caller_and_native_limits_and_publication_reservation(self):
        from passage_reply_budget import effective_wire_limit
        self.assertEqual(effective_wire_limit({"budget": {"max_bytes": 9000}}, 20000), 9000)
        self.assertEqual(effective_wire_limit({}, 1_000_000), 131072)
        def decorate(body):
            body.pop("source_revision")
            body["publication"] = "π" * 120
        before, _ = self.frame(20000)
        wire, fewer = render('réviser "語"', ['必要🙂'], 'a' * 40, self.passages, 3, len(before),
                             request_id='ID/語', decorate=decorate)
        self.assertLessEqual(len(wire), len(before))
        self.assertLess(len(fewer), 2)
        body = json.loads(json.loads(wire)["result"]["content"][0]["text"])
        if "error" not in body:
            self.assertEqual(body["publication"], "π" * 120)