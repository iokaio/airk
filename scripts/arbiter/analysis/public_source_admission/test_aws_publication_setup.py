# SPDX-License-Identifier: Apache-2.0
"""Check source phase clocks, settled restart evidence, and source publication semantics."""
from datetime import datetime, timedelta, timezone
from pathlib import Path
import runpy
import unittest

HERE = Path(__file__).resolve().parent
runner = runpy.run_path(str(HERE / "run_aws_publication.py"))
stage = runpy.run_path(str(HERE / "aws_publication_setup.py"))
menu = runpy.run_path(str(HERE / "verify_aws_natural_menu.py"))
w = {"require": menu["require"], "tool_body": lambda env: env}


class OfflinePublication(unittest.TestCase):
    def test_deadlines_are_phase_specific_and_cut_off_by_campaign(self):
        now = datetime(2026, 11, 20, tzinfo=timezone.utc)
        self.assertEqual(runner["window"]("setup", now + timedelta(hours=5), now), 14400)
        self.assertEqual(runner["window"]("census", now + timedelta(hours=5), now), 1200)
        self.assertEqual(runner["window"]("setup", now + timedelta(minutes=10), now), 600)
        with self.assertRaises(ValueError):
            runner["window"]("setup", now + timedelta(seconds=100), now)

    def test_source_discovery_requires_completed_native_publication_with_vectors_off(self):
        status = {"discovery_readiness": {"state": "ready"}, "semantic_readiness": {"state": "disabled"},
                  "index_state": "available", "files": 34100}
        def capture(replacement):
            return stage["source_status"](w, lambda *args: status | replacement, "index", "label")
        self.assertEqual(capture({})["files"], 34100)
        for wrong in [{"discovery_readiness": {"state": "pending"}}, {"semantic_readiness": {"state": "ready"}},
                      {"files": 35516}, {"index_state": "missing"}]:
            with self.subTest(wrong=wrong), self.assertRaises(ValueError):
                capture(wrong)

    def test_unsettled_checkpoint_cannot_reopen(self):
        observed = {"exit_code": 0, "timed_out": False, "log_oversized": False,
                    "child_assigned_before_resume": True, "owned_descendants_settled": True,
                    "owned_job_forced_termination": False, "owned_job_final_active_processes": 0,
                    "owned_job_final_member_pids": []}
        stage["require_settlement"](w, observed)
        for wrong in [{"timed_out": True}, {"owned_job_forced_termination": True},
                      {"owned_job_final_member_pids": [123]}, {"owned_descendants_settled": False}]:
            with self.subTest(wrong=wrong), self.assertRaises(ValueError):
                stage["require_settlement"](w, observed | wrong)


if __name__ == "__main__":
    unittest.main()
