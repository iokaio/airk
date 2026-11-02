# SPDX-License-Identifier: Apache-2.0
"""Retrieval qualification rejects false-positive rankings and identities."""
import importlib.util
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "retrieval_quality", Path(__file__).resolve().parents[1] / "check-retrieval-quality.py")
QUALITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QUALITY)


class RetrievalQualityTests(unittest.TestCase):
    def test_only_unsubmitted_busy_rejections_are_retried(self):
        for submitted in (False, True):
            session = self.session([self.hit("wanted.txt")])
            accepted = session.request()
            replies = [{"isError":True,"content":[{"type":"text","text":json.dumps({"error":{
                "code":"direct_query_admission","reason":"busy","query_submitted":submitted,"retryable":True,"fresh_brokerage_required":False,"retry_after_ms":25}})}]}, accepted]
            def request(*_):
                return replies.pop(0)
            session.request = request
            with patch.object(QUALITY.time,"sleep") as sleep:
                if submitted:
                    with self.assertRaises(AssertionError):
                        session.search(self.case())
                    self.assertEqual(len(replies),1)
                    sleep.assert_not_called()
                else:
                    self.assertEqual(session.search(self.case())["admission_retries"],1)
                    self.assertEqual(replies,[])
                    sleep.assert_called_once_with(0.025)

    def session(self, hits, publication="fixture-revision"):
        session = QUALITY.Session.__new__(QUALITY.Session)
        session.publication = None
        session.request = lambda *_: {"content": [{"text": json.dumps({
            "publication": {"token": publication}, "hits": hits})}]}
        return session

    def hit(self, path, text="evidence", source_hash="a" * 64):
        return {"citation": {"path": path, "content_hash": source_hash}, "text": text}

    def case(self, **extra):
        return {"name": "fixture", "query": "needle", "mode": "hybrid",
                "expected": "wanted.txt", "source_sha256": "a" * 64, **extra}

    def test_repeated_paths_keep_their_original_chunk_rank(self):
        session = self.session([self.hit("other.txt"), self.hit("other.txt"), self.hit("wanted.txt")])
        self.assertEqual(session.search(self.case())["reciprocal_rank_at_5"], 1 / 3)

    def test_long_line_requires_the_expected_chunk_text(self):
        session = self.session([self.hit("wanted.txt", "earlier chunk"),
                                self.hit("wanted.txt", "tailmarker")])
        self.assertEqual(session.search(self.case(expected_text="tailmarker"))["reciprocal_rank_at_5"], 1 / 2)
        with self.assertRaises(AssertionError):
            session.search(self.case(expected_text="absent marker"))

    def test_citation_hash_must_match_the_fixture_source_bytes(self):
        session = self.session([self.hit("wanted.txt", source_hash="b" * 64)])
        with self.assertRaises(AssertionError):
            session.search(self.case())

    def test_session_cannot_change_publication(self):
        session = self.session([self.hit("wanted.txt")])
        session.publication = "previous-revision"
        with self.assertRaises(AssertionError):
            session.search(self.case())

    def test_graded_search_keeps_relevance_misses_and_checks_every_hit_identity(self):
        session = self.session([self.hit("other.txt")])
        case = {"query": "needle", "mode": "lexical", "grades": {"wanted.txt": 3}}
        result = session.judged_search(case, {"other.txt": "a" * 64, "wanted.txt": "a" * 64})
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["recall_at_10"], 0)
        self.assertEqual(result["ndcg_at_10"], 0)
        session = self.session([self.hit("other.txt", source_hash="b" * 64)])
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            session.judged_search(case, {"other.txt": "a" * 64})

    def test_graded_search_rejects_scope_leak_and_requests_ten_chunks(self):
        session = self.session([self.hit("outside.txt")])
        original = session.request
        calls = []
        def request(*arguments):
            calls.append(arguments)
            return original()
        session.request = request
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            session.judged_search({"query": "needle", "mode": "lexical", "grades": {},
                                   "path_prefix": "docs/"}, {"outside.txt": "a" * 64})
        self.assertEqual(calls[0][1]["arguments"]["limit"], 10)

    def test_setup_failure_writes_a_failed_receipt_without_serializing_error_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "airk"
            binary.write_bytes(b"fixture identity only")
            output = Path(temporary) / "receipt.json"
            with patch("sys.argv", ["check-retrieval-quality", "--binary", str(binary),
                                    "--output", str(output), "--trials", "2"]), \
                    patch.object(QUALITY, "source_identity", return_value={"fixture": "fixed"}), \
                    patch.object(QUALITY.subprocess, "check_output", return_value="airk fixture"), \
                    patch.object(QUALITY, "fixture", side_effect=RuntimeError("private path and source")), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(SystemExit, "qualification failed"):
                    QUALITY.main()
            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertFalse(receipt["passed"])
            self.assertEqual(receipt["failure_class"], "RuntimeError")
            self.assertNotIn("private path and source", output.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
