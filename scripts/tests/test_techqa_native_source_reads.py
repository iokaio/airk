# SPDX-License-Identifier: Apache-2.0
"""Checks the publisher offset projection and both native reader entry points."""
import hashlib
from pathlib import Path
import runpy
import unittest
from unittest.mock import MagicMock, Mock, patch

FAMILY = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
project = runpy.run_path(str(FAMILY / "stage_techqa_exact_gold_corpus.py"))["source_span"]


class TechqaNativeSourceReadTests(unittest.TestCase):
    def test_multiline_nonascii_offsets_exclude_the_following_line(self):
        body = "α\r\nβa\nq".encode()
        case = {"publisher_start": 1, "publisher_end": 6, "byte_start": 2, "byte_end_exclusive": 8,
                "span_sha256": hashlib.sha256("\r\nβa\n".encode()).hexdigest()}
        observed = project(body, case)
        self.assertEqual((1, 2), (observed["line_start"], observed["line_end_inclusive"]))
        self.assertEqual(case["span_sha256"], observed["span_sha256"])

    def test_repeated_literal_incorrect_unicode_position_refused(self):
        body = "αx x".encode()
        case = {"publisher_start": 1, "publisher_end": 2, "byte_start": 4, "byte_end_exclusive": 5,
                "span_sha256": hashlib.sha256(b"x").hexdigest()}
        with self.assertRaisesRegex(ValueError, "scalar"):
            project(body, case)

    def test_explicit_native_retry_keeps_requests_identical_and_logs_attempts(self):
        native = runpy.run_path(str(FAMILY / "native_cohort_source_reads.py"))
        query = ["tools", "call", "airk_read_file"]
        reply = {"isError": True, "structuredContent": {
            "code": "publication_busy", "action": "repeat_same_request", "status": "retry",
            "retryable": True, "retry_after_ms": 25,
        }}
        cli = Mock(side_effect=[ValueError("CLI refused"), {"isError": False}])
        runs = MagicMock()
        result = runs.__truediv__.return_value
        result.is_file.return_value = True
        result.stat.return_value.st_size = 1600
        with patch.object(native["time"], "sleep") as wait:
            actual = native["publication_read"](cli, lambda _: reply, runs, "read-1", query, float("inf"))
        self.assertEqual(actual, {"isError": False})
        self.assertEqual([call.args for call in cli.call_args_list],
                         [("read-1", *query), ("read-1-retry-1", *query)])
        wait.assert_called_once_with(0.025)
        # Expiration, untyped errors and non-repeatable decisions never replay.
        for change, cutoff in (({}, 0), ({"retryable": False}, float("inf")),
                               ({"code": "stale_revision"}, float("inf"))):
            response = {"isError": True, "structuredContent": reply["structuredContent"] | change}
            refused = Mock(side_effect=ValueError("CLI refused"))
            with self.assertRaisesRegex(ValueError, "refused"):
                native["publication_read"](refused, lambda _: response, runs, "read-1", query, cutoff)
            self.assertEqual(refused.call_count, 1)

    def test_cohorts_have_separate_manifests_and_selector_types(self):
        aws = runpy.run_path(str(FAMILY / "verify_aws_native_source_reads.py"))["COHORT"]
        techqa = runpy.run_path(str(FAMILY / "verify_techqa_native_source_reads.py"))["COHORT"]
        self.assertEqual(("aws", 41, 27, "row_index"), (aws.name, aws.case_count, aws.document_count, aws.selector_field))
        self.assertEqual(("techqa", 76, 71, "question_id"),
                         (techqa.name, techqa.case_count, techqa.document_count, techqa.selector_field))
        self.assertNotEqual(aws.manifest_sha256, techqa.manifest_sha256)


if __name__ == "__main__":
    unittest.main()
