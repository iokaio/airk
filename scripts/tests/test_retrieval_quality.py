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
    def test_combined_publication_refs_preserve_raw_and_reject_wrong_member(self):
        value, sources = self.combined()
        for number, rank in enumerate(value["ranking"]):
            rank["origin"].pop("publication")
            rank["origin"]["publication_ref"] = {"member": number}
        raw = json.dumps(value, sort_keys=True)
        self.assertEqual(QUALITY.judge_combined(value, sources, {key: 3 for key in sources})["recall_at_10"], 1)
        self.assertEqual(json.dumps(value, sort_keys=True), raw)
        for invalid in (1, -1, True, 32):
            value["ranking"][0]["origin"]["publication_ref"] = {"member": invalid}
            with self.assertRaises(AssertionError):
                QUALITY.judge_combined(value, sources, {key: 3 for key in sources})

    def combined(self):
        projects = ["prj_" + "03" * 16]
        indexes = ["idx_" + "01" * 16, "idx_" + "02" * 16]
        root = "src_" + "04" * 16
        sources = {(index, root, "same.rs"): {"sha256": "a" * 64, "project_ids": projects}
                   for index in indexes}
        value = {"schema": "airk.combined-read/1", "members": [{"index_id": index,
            "result": {"structuredContent": {"hits": [self.hit("same.rs")]}}} for index in indexes],
            "publication_vector": {"members": [{"index_id": index, "publication": {"token": f"fixture-{index}"}} for index in indexes]},
            "ranking": [{"index_id": index,
                "result_pointer": f"/members/{number}/result/structuredContent/hits/0",
                "origin": {"index_id": index, "source_root_id": root, "source_relative_path": "same.rs",
                           "project_ids": projects, "publication": {"token": f"fixture-{index}"}}} for number, index in enumerate(indexes)]}
        return value, sources

    def test_combined_relevance_keeps_identical_paths_in_independent_indexes(self):
        value, sources = self.combined()
        grades = {key: 3 for key in sources}
        result = QUALITY.judge_combined(value, sources, grades)
        self.assertEqual(result["relevant_files"], 2)
        self.assertEqual(result["recall_at_10"], 1)
        self.assertEqual(result["ndcg_at_10"], 1)
        value["ranking"].pop()
        self.assertEqual(QUALITY.judge_combined(value, sources, grades)["recall_at_10"], .5)

    def test_combined_relevance_rejects_false_project_or_member_origin(self):
        value, sources = self.combined()
        grades = {key: 3 for key in sources}
        value["ranking"][0]["origin"]["project_ids"] = []
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            QUALITY.judge_combined(value, sources, grades)
        value, sources = self.combined()
        value["ranking"][0]["result_pointer"] = "/members/1/result/structuredContent/hits/0"
        with self.assertRaisesRegex(AssertionError, "member differs"):
            QUALITY.judge_combined(value, sources, grades)

    def test_combined_relevance_duplicate_chunks_do_not_boost_shared_index_gain(self):
        value, sources = self.combined()
        grades = {key: 3 for key in sources}
        value["ranking"].insert(1, value["ranking"][0].copy())
        result = QUALITY.judge_combined(value, sources, grades)
        self.assertEqual(result["recall_at_10"], 1)
        self.assertLess(result["ndcg_at_10"], 1)
        self.assertEqual(result["returned_chunks"], 3)

    def test_combined_relevance_rejects_cross_member_publication_substitution(self):
        value, sources = self.combined()
        value["ranking"][0]["origin"]["publication"] = value["ranking"][1]["origin"]["publication"]
        with self.assertRaisesRegex(AssertionError, "source or scope differs"):
            QUALITY.judge_combined(value, sources, {key: 3 for key in sources})

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
                    patch.object(QUALITY.platform, "platform", return_value="fixture platform"), \
                    patch.object(QUALITY, "fixture", side_effect=RuntimeError("private path and source")), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(SystemExit, "qualification failed"):
                    QUALITY.main()
            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertFalse(receipt["passed"])
            self.assertEqual(receipt["failure_class"], "RuntimeError")
            self.assertEqual(receipt["platform"], "fixture platform")
            self.assertNotIn("private path and source", output.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
