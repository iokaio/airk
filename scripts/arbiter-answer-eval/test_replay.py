# SPDX-License-Identifier: Apache-2.0
"""Recorded-session filters require the requested Answer configuration."""

import contextlib
import io
from pathlib import Path
import unittest
from unittest import mock

import replay


class SessionPathTests(unittest.TestCase):
    def test_recorded_replay_uses_only_the_requested_path(self):
        root = Path("recorded")
        first = root / "munarium-A1" / "compact-gpu" / "session.json"
        other = root / "munarium-A1" / "arbiter-gpu" / "session.json"
        incomplete = root / "munarium-A1" / "compact-cpu" / "session.json"
        records = {
            first: {"scenario_id": "munarium-A1", "path": "compact", "status": "complete"},
            other: {"scenario_id": "munarium-A1", "path": "arbiter", "status": "complete"},
            incomplete: {"scenario_id": "munarium-A1", "path": "compact", "status": "failed"},
        }
        scenario = {"id": "munarium-A1"}
        handle = mock.Mock(sha256="handle")
        result = {"scenario_id": scenario["id"], "first_digest": "digest",
                  "coverage": {"first_call_yield": 0.5, "essential_covered": 1, "essential_total": 2}}
        with (
            mock.patch.object(replay.common, "load_handle", return_value=handle),
            mock.patch.object(replay.common, "scenario_files", return_value=[Path("scenario")]),
            mock.patch.object(replay.common, "load_scenario", return_value=scenario),
            mock.patch.object(replay.common, "read_json", side_effect=records.get),
            mock.patch.object(replay.common, "write_json") as write_report,
            mock.patch.object(replay, "replay_session", return_value=result) as issue,
            mock.patch.object(Path, "glob", return_value=list(records)),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(replay.main(["--handle", "handle.json", "--out", "results",
                                         "--sessions", str(root), "--session-path", "compact"]), 0)
        self.assertEqual(issue.call_count, 1)
        self.assertEqual(issue.call_args.args[0], first.parent)
        self.assertEqual(write_report.call_args.args[1]["session_path"], "compact")

    def test_canonical_replay_refuses_the_recorded_path_filter(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            replay.main(["--handle", "handle.json", "--out", "results", "--canonical", "arbiter",
                         "--session-path", "compact"])
        self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
