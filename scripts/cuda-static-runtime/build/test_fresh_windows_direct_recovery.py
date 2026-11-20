# SPDX-License-Identifier: Apache-2.0
"""Cheap checks for the read-only CU18 direct recovery plan."""

import datetime as dt
from pathlib import Path
import runpy
import types
import unittest

P = runpy.run_path(str(Path(__file__).with_name("fresh_windows_direct_recovery.py")))


class RecoveryPlan(unittest.TestCase):
    def test_original_monotonic_deadline_beats_retry_or_wall_clock(self):
        first = {"start": "2026-11-20T13:44:00+00:00", "deadline": 7300}
        retry = {"start": "2026-11-20T14:00:00+00:00", "deadline": 7200}
        now = dt.datetime(2026, 11, 20, 14, 40, tzinfo=dt.timezone.utc)
        campaign_end = dt.datetime(2026, 11, 20, 17, 10, tzinfo=dt.timezone.utc)
        expires, end = P["deadline"](first, retry, campaign_end, now, 5000)
        self.assertEqual(expires.isoformat(), "2026-11-20T16:44:00+00:00")
        self.assertEqual(end, 7200)
        with self.assertRaisesRegex(ValueError, "window exhausted"):
            P["deadline"](first, retry, campaign_end, now, 7200)
        with self.assertRaisesRegex(ValueError, "clock or admission differs"):
            P["deadline"](first, retry | {"deadline": 7301}, campaign_end, now, 5000)

    def test_failed_original_is_distinct_from_settlement_or_success(self):
        job = {"exit_code": 1, "owned_descendants_settled": True,
               "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
               "parent_assigned_to_job": False, "child_assigned_before_resume": True}
        P["require_failed_settled"](job)
        for change in ({"exit_code": 0}, {"owned_job_final_member_pids": [123]},
                       {"owned_job_final_active_processes": 1}, {"owned_descendants_settled": False}):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "not proved settled"):
                P["require_failed_settled"](job | change)

    def test_direct_commands_keep_the_build_root_and_separate_evidence(self):
        root = Path.cwd() / "inert-cu18-root"
        tools = {"ninja": {"path": str(root / "ninja.exe")},
                 "compiler": {"path": str(root / "cl.exe")}}
        recipe = {"tools": tools, "parallel_workers": 8}
        compiler = types.SimpleNamespace(parallel_workers=lambda r: r["parallel_workers"])
        configure = [str(root / "cmake.exe"), "-B", str(root / "b/Release")]
        before = root.exists()
        stages = P["commands"](root, recipe, configure, compiler)
        self.assertEqual([row["name"] for row in stages], ["configure", "core-session", "provider", "archive"])
        self.assertEqual(stages[0]["argv"], configure)
        self.assertEqual(stages[1]["argv"], [str(root / "ninja.exe"), "-C", str(root / "b/Release"),
                                             "-j", "8", "onnxruntime_session"])
        self.assertEqual(stages[2]["argv"][-2:], ["all", "re2"])
        self.assertEqual(stages[3]["argv"], [str(root / "lib.exe"), "/NOLOGO",
                                             f"/OUT:{root / 'direct-r3/onnxruntime.lib'}",
                                             f"@{root / 'direct-r3/inputs.rsp'}"])
        self.assertTrue(all(Path(row["log"]).parent == root / "direct-r3" for row in stages))
        self.assertEqual(root.exists(), before)


if __name__ == "__main__":
    unittest.main()
