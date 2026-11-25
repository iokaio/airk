# SPDX-License-Identifier: Apache-2.0

import contextlib
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch

import baseline_summary


class BaselineL7MetricsTests(unittest.TestCase):
    def test_sample_one_read_only_public_metrics(self):
        root = Path("private-root")
        session = root / "private-session-id"
        missing = root / "missing-metadata"
        coverage = session / "coverage.json"
        turns = session / "wrapper" / "turns.jsonl"
        result = session / "raw-private-reply.json"
        ticket = session / "queued-private-reply.json"
        body = {
            "schema": baseline_summary.loop_reply.RESPONSE_SCHEMA,
            "partial": {"reason": "deadline", "continuation": "private-token"},
            "records": [{"text": "private evidence"}],
            "decision_log": {"counts": [
                {"stage": "plan", "source": "model", "count": 3},
                {"stage": "coverage", "source": "unavailable", "count": 2},
                {"stage": "follow_up", "source": "not_dispatched", "count": 1},
                {"stage": "split", "source": "deterministic", "count": 4},
                {"stage": "plan", "source": "do-not-print-this-source", "count": 1},
            ]},
        }
        tool_log = [
            {"turn": 1, "tool": "airk_answer", "ok": True, "result_path": str(result),
             "typed_retries": [{"code": "private-code"}, {"code": "ticket", "ticket": "private-token"}]},
            {"turn": 2, "tool": "airk_answer", "ok": True, "result_path": str(ticket)},
            {"turn": 3, "tool": "airk_search", "ok": False, "error": "private-error"},
            {"tool": "airk_search", "ok": False, "typed_retries": [{"code": "private-uncounted"}]},
            {"tool": "airk_search", "rejected": True, "reason": "private-refusal"},
        ]
        index = {"series": "private-series", "results": [
            {"key": "first", "repo": "bookbuilder", "slot": "C1", "scenario_id": "bookbuilder-C1", "kind": "compound",
             "configuration": "compact", "device": "gpu", "session_dir": str(session), "coverage": str(coverage)},
            {"key": "second", "repo": "bookbuilder", "slot": "C1", "scenario_id": "bookbuilder-C1", "kind": "compound", "sample": 2,
             "configuration": "compact", "device": "gpu", "session_dir": "unexpected-sample-two"},
            {"key": "missing", "repo": "munarium", "slot": "C1", "scenario_id": "munarium-C1", "kind": "compound",
             "configuration": "parse", "device": "none", "session_dir": str(missing)},
        ]}
        documents = {
            root / "pass-index.json": index,
            session / "session.json": {"status": "complete", "session_id": "private-session-id", "elapsed_ms": 8000},
            coverage: {"essential_covered": 1, "essential_total": 2, "helpful": ["private-note-a", "private-note-b"],
                       "covered_any": ["private-note-a", "private-note-c"], "all_essential_turn": None,
                       "first_call_yield": 0.5, "counted_turns": 3, "total_bytes": 8192, "total_elapsed_ms": 6500,
                       "later_reads": {"to_truncated_row": 2, "to_whole_row": 1}},
            missing / "session.json": {"status": "complete"},
        }
        bodies = {result: body, ticket: {"schema": body["schema"], "ticket": {"id": "private-token"}}}
        with (
            patch.object(Path, "is_file", autospec=True,
                         side_effect=lambda p: p in documents or p in bodies or p == turns),
            patch.object(Path, "read_bytes", autospec=True,
                         side_effect=lambda p: json.dumps({"structuredContent": bodies[p]}).encode()),
            patch.object(baseline_summary.common, "read_json", side_effect=lambda p: documents[Path(p)]),
            patch.object(baseline_summary.common, "read_jsonl", return_value=tool_log) as read_log,
            patch.object(baseline_summary.common, "write_json") as write_json,
            contextlib.redirect_stdout(io.StringIO()) as stdout,
        ):
            self.assertEqual(baseline_summary.main(["--pass-dir", str(root), "--l7-metrics"]), 0)
        read_log.assert_called_once_with(turns)
        write_json.assert_not_called()
        output = stdout.getvalue()
        self.assertIn("| bookbuilder | compact-gpu | 1 | 1/2 | 1/2 | 8192 | 2/1 | 1/3 | 1 | 1/1 | "
                      "1/1 | 3/2/1/4/1 (1 replies) | 8.00 | 6.50 | 1/1/1 |", output)
        self.assertIn("| munarium | parse | 1 | - | - | - | - | - | - | - | - | - | - | - | 0/0/0 |", output)
        self.assertNotIn("private", output)
        self.assertNotIn("do-not-print-this-source", output)

    def test_handback_joins_each_sample_by_exact_result_key(self):
        base = {"repo": "bookbuilder", "slot": "C1", "first_call_yield": 0.5,
                "essential_covered": 1, "essential_total": 2, "judge1_grade": 0, "judge2_grade": 0,
                "bytes": 2000, "cost_usd": 0.05, "reused_from": ""}
        key = "bookbuilder-C1/compact-gpu"
        first = dict(base, key=key, turns_to_finish=None, adjudicated_grade=0, elapsed_ms=1000)
        second = dict(base, key=key + ".s2", turns_to_finish=4, adjudicated_grade=3, elapsed_ms=7000)
        report = {"results": [{"key": key + ".s2", "turns": 5, "wall_ms": 13000},
                              {"key": key, "turns": 8, "wall_ms": 21000}]}

        rows = baseline_summary.result_table([first, second], report)

        self.assertEqual(
            [(row["result"], row["adj"], row["turns_to_finish"], row["turns"], row["wall_s"], row["tool_s"])
             for row in rows],
            [("compact-gpu", "0", "-", 8, 21, 1), ("compact-gpu.s2", "3", "4", 5, 13, 7)],
        )

    def test_duplicate_sample_one_refuses_instead_of_overwriting(self):
        row = {"repo": "munarium", "slot": "C1", "scenario_id": "munarium-C1", "configuration": "search", "device": "none",
               "key": "sample-one", "essential_total": None}
        index = {"results": [{"key": row["key"], "session_dir": "unused"}]}
        with (
            patch.object(baseline_summary.grade, "build_rows", return_value=[row, row]),
            patch.object(baseline_summary.common, "read_json", return_value={}),
            patch.object(Path, "is_file", return_value=False),
        ):
            with self.assertRaisesRegex(ValueError, "Duplicate L7 sample-one result"):
                baseline_summary.l7_cells(index, {})
