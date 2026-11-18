# SPDX-License-Identifier: Apache-2.0
"""Cheap direct stage and stock archive helper controls for cu17."""

import importlib.util
from pathlib import Path
import unittest

source = Path(__file__).with_name("fresh_architecture_warm_archive.py")
spec = importlib.util.spec_from_file_location("warm_archive_control", source)
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


class Controls(unittest.TestCase):
    def test_stock_archive_helpers_are_loaded_by_exact_source_pins(self):
        stock = archive.exact_module(archive.HERE / "fresh_aggregate.py", archive.STOCK_SHA)
        coff = archive.exact_module(archive.REPO / "scripts/cuda-static-archive.py", archive.COFF_SHA)
        self.assertTrue(callable(stock.select_plan))
        self.assertTrue(callable(stock.inspect_inputs))
        self.assertTrue(callable(coff.verify_member_closure))

    def test_retained_direct_stage_rejects_wrong_command_or_job_members(self):
        warm = archive.exact_module(archive.HERE / "fresh_architecture_warm.py", archive.WARM_SHA)
        _, compiler, _ = warm.load_architecture().modules()
        cpu = compiler.load(compiler.OWNERS["cpu"], "retained_direct_stage_cpu")
        root = Path("cu17")
        output = root / "warm-continuation-r1"
        path = str(output / "warm-session.log")
        argv = ["ninja.exe", "-C", "build", "-j", "8", "onnxruntime_session"]
        owned = {"exit_code": 0, "timed_out": False, "log_oversized": False,
                 "owned_descendants_settled": True, "owned_job_forced_termination": False,
                 "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
                 "parent_assigned_to_job": False, "child_assigned_before_resume": True,
                 "argv": argv, "log": path}
        row = {"stage": "warm-session", "argv": argv, "cwd": str(root),
               "deadline_monotonic": 123, "log": {"path": path}, "actual_owned": owned}
        arguments = {"stage": "warm-session", "argv": argv, "root": root, "output": output, "deadline": 123}
        self.assertEqual(archive.check_stage(warm, cpu, row, **arguments), {"path": path})
        for change in ({"owned_job_final_member_pids": [1]}, {"argv": ["different.exe"]},
                       {"owned_job_forced_termination": True}):
            altered = dict(row, actual_owned=dict(owned, **change))
            with self.assertRaisesRegex(ValueError, "did not naturally settle"):
                archive.check_stage(warm, cpu, altered, **arguments)


if __name__ == "__main__":
    unittest.main()
