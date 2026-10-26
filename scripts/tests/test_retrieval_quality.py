# SPDX-License-Identifier: Apache-2.0
"""Retrieval qualification rejects false-positive rankings and identities."""
import importlib.util
import json
from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
