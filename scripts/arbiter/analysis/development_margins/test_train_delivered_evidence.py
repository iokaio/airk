# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import runpy
import unittest


grader = runpy.run_path(str(Path(__file__).with_name("train_delivered_evidence.py")))
delivered = grader["delivered"]


class ReturnedTrainSupport(unittest.TestCase):
    body = b"foo\nbart\n"
    document = {"sha256": "pinned"}
    goal = {"file": "original.txt", "byte_start": 2, "byte_end": 6}

    def result(self, snippets, publication="original", extra=False):
        hits = [{"text": snippet, "citation": {"path": "original.txt", "content_hash": "pinned",
                 "start_line": 1, "end_line": 2, "location": {"Lines": {"start": 1, "end": 2}}}}
                for snippet in snippets]
        records = [{"operation": "search", "canonical_result": {"publication": publication, "hits": hits}}]
        if extra:
            records.append({"operation": "other", "canonical_result": {}})
        return delivered({"records": records, "paragraphs": []}, "original", self.body, self.document, self.goal)[0]

    def test_adjacent_returns_cover_required_publisher_bytes(self):
        self.assertEqual(self.result(["foo\n", "ba"]), "covered")

    def test_positive_document_alone_cannot_count_as_complete(self):
        self.assertEqual(self.result(["foo"]), "incomplete")

    def test_unhandled_evidence_stays_unresolved(self):
        self.assertEqual(self.result(["foo"], extra=True), "unresolved")

    def test_ambiguous_literal_does_not_cover_either_occurrence(self):
        self.body = b"foo\nfoo\n"
        self.goal = {"file": "original.txt", "byte_start": 0, "byte_end": 3}
        self.assertEqual(self.result(["foo"]), "unresolved")

    def test_swapped_scope_or_attempt_command_refuses(self):
        run = Path("fixture-attempt")
        request = Path("question/request.json")
        publication = {"token": "p", "revision": "r"}
        scope = {"source": "named", "index_ids": ["i"]}
        status = {"isError": False, "structuredContent": {"read_scope": scope, "publication": {"token": "p"},
                                                        "scope_observation": {"fingerprint": "a"}}}
        command = {"exit_code": 0, "argv": ["cli", "--output", "json", "--service-runtime-dir", str(run / "fixture/control"),
                                           "tools", "call", "airk_answer", "--index", "i", "--args-file", str(request)]}
        answer = {"read_scope": scope, "scope_observation": {"fingerprint": "a"},
                  "assessment": {"scope": {"publication": publication}}}
        verify = grader["verify_delivery_scope"]
        self.assertEqual(verify(run, request, status, command, publication, answer), "i")
        with self.assertRaises(ValueError):
            verify(run, request, status, command, publication, answer | {"scope_observation": {"fingerprint": "b"}})
        with self.assertRaises(ValueError):
            verify(Path("other-attempt"), request, status, command, publication, answer)

    def test_wrong_publication_refuses(self):
        with self.assertRaisesRegex(ValueError, "publication differs"):
            self.result(["foo\nbart"], publication="other")


if __name__ == "__main__":
    unittest.main()
