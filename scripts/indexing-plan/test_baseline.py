# SPDX-License-Identifier: Apache-2.0
"""Unit tests for the measurement script's own logic. No harness is run."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

_SPEC = importlib.util.spec_from_file_location(
    "indexing_plan_baseline", Path(__file__).with_name("baseline.py"))
baseline = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(baseline)


class StorageSummary(unittest.TestCase):
    def test_counts_files_and_bytes_by_the_first_three_levels(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            (root / "a" / "b" / "c" / "d").mkdir(parents=True)
            (root / "top.bin").write_bytes(b"12345")
            (root / "a" / "one").write_bytes(b"1")
            (root / "a" / "b" / "c" / "two").write_bytes(b"22")
            (root / "a" / "b" / "c" / "d" / "three").write_bytes(b"333")
            summary = baseline.storage_summary(root)
        self.assertEqual((summary["files"], summary["bytes"]), (4, 11))
        self.assertEqual(summary["groups"], {
            ".": {"files": 1, "bytes": 5},
            "a": {"files": 1, "bytes": 1},
            "a/b/c": {"files": 2, "bytes": 5},
        })


class Stages(unittest.TestCase):
    def test_rows_are_seconds_sorted_by_time_then_name(self):
        rows = baseline.stage_rows([
            {"stage": "b", "calls": 2, "nanoseconds": 500_000_000},
            {"stage": "a", "calls": 1, "nanoseconds": 500_000_000},
            {"stage": "c", "calls": 3, "nanoseconds": 2_000_000_000},
        ])
        self.assertEqual([row["stage"] for row in rows], ["c", "a", "b"])
        self.assertEqual(rows[0], {"stage": "c", "calls": 3, "seconds": 2.0})


class Receipts(unittest.TestCase):
    def test_path_lists_become_counts_and_other_fields_stay(self):
        report = {"indexed": 3, "skipped": [{"path": "x"}, {"path": "y"}],
                  "selection_diagnostics": [], "skipped_truncated": 0}
        self.assertEqual(baseline.without_paths(report), {
            "indexed": 3, "skipped": 2, "selection_diagnostics": 0, "skipped_truncated": 0})
        self.assertEqual(report["skipped"][0], {"path": "x"}, "the input is not changed")

    def test_search_results_keep_counts_only(self):
        value = {"hits": [{"text": "secret", "citation": {"path": "p"}}], "truncated": False}
        self.assertEqual(baseline.search_counts(value), {"hits": 1, "truncated": False})
        self.assertEqual(baseline.first_hit_path(value), "p")
        self.assertIsNone(baseline.first_hit_path({"hits": []}))

    def test_summary_row_derives_the_rate_from_indexed_and_reused(self):
        row = baseline.summary_row({
            "label": "m", "operation": "index", "wall_seconds": 4.0,
            "report": {"indexed": 6, "reused": 2, "indexed_vectors": 9},
            "storage": {"files": 5, "bytes": 50}})
        self.assertEqual((row["files"], row["files_per_second"]), (8, 2.0))
        self.assertEqual((row["vectors"], row["storage_files"], row["storage_bytes"]), (9, 5, 50))
        empty = baseline.summary_row({"label": "s", "operation": "search"})
        self.assertIsNone(empty["files_per_second"])


class Environment(unittest.TestCase):
    def test_request_and_result_paths_reach_the_harness(self):
        environment = baseline.child_environment(
            {"KEEP": "1", "TEMP": "old"}, "root", "data", ["index", "--full"],
            "result.json", "stages.json", temp="scratch", workers=4)
        self.assertEqual(json.loads(environment["AIRK_PERF_REQUEST"]),
                         ["root", "data", ["index", "--full"]])
        self.assertEqual(environment["AIRK_PERF_RESULT"], "result.json")
        self.assertEqual(environment["AIRK_STAGE_TIMING_RESULT"], "stages.json")
        self.assertEqual((environment["AIRK_USAGE_MODE"], environment["AIRK_INDEX_WORKERS"]),
                         ("disabled", "4"))
        self.assertEqual((environment["TEMP"], environment["TMP"], environment["KEEP"]),
                         ("scratch", "scratch", "1"))

    def test_unset_options_leave_the_inherited_values(self):
        environment = baseline.child_environment(
            {"TEMP": "old"}, "r", "d", ["status"], "a", "b")
        self.assertEqual(environment["TEMP"], "old")
        self.assertNotIn("AIRK_INDEX_WORKERS", environment)

    def test_search_arguments_carry_an_optional_prefix(self):
        self.assertEqual(baseline.search_arguments("q", "vector"),
                         ["search", "q", "--mode", "vector"])
        self.assertEqual(baseline.search_arguments("q", "lexical", "src/"),
                         ["search", "q", "--mode", "lexical", "--path-prefix", "src/"])


if __name__ == "__main__":
    unittest.main()
