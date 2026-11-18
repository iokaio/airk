# SPDX-License-Identifier: Apache-2.0
"""Cheap exact prior evidence and direct Job controls."""

import importlib.util
import os
from pathlib import Path
import sys
import time
import unittest

source = Path(__file__).with_name("fresh_architecture_warm.py")
spec = importlib.util.spec_from_file_location("cuda_warm_controls", source)
warm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(warm)


class Controls(unittest.TestCase):
    def test_direct_commands_keep_exact_targets(self):
        self.assertEqual(warm.ninja_command("ninja.exe", "build", 8, ("all", "re2")),
                         ["ninja.exe", "-C", "build", "-j", "8", "all", "re2"])
        with self.assertRaisesRegex(ValueError, "warm Ninja"):
            warm.ninja_command("ninja.exe", "build", 8, ("clean",))

    def test_current_job_members_are_required(self):
        actual = {"exit_code": 0, "owned_descendants_settled": True,
                  "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
                  "parent_assigned_to_job": False, "child_assigned_before_resume": True}
        self.assertTrue(warm.empty_owned_job(actual))
        self.assertFalse(warm.empty_owned_job(dict(actual, owned_job_final_member_pids=[42])))
        self.assertFalse(warm.empty_owned_job(dict(actual, cleanup_refusal="observed")))

    def original(self):
        arch = warm.load_architecture()
        _, compiler, _ = arch.modules()
        pins = compiler.load(compiler.OWNERS["pin_reader"], "warm_control_pins")
        root = Path(os.environ["AIRK_TEST_WARM_CU17"])
        self.assertEqual(root, compiler.ANCHOR / "cu17")
        configured_pin = pins.pin(root / "fresh-configure.json")
        self.assertEqual(configured_pin["sha256"], warm.CONFIGURED_SHA)
        configured = compiler.read_pinned(pins, configured_pin, None)
        recipe = compiler.read_pinned(pins, configured["recipe"], None)
        component = compiler.load(compiler.OWNERS["guards"], "warm_control_guard")
        return root, compiler, pins, configured, recipe, compiler.output_guard(component, recipe)

    @unittest.skipUnless(os.environ.get("AIRK_TEST_WARM_CU17"), "explicit existing cu17 path required")
    def test_exact_retained_failed_stage(self):
        root, compiler, pins, configured, recipe, guard = self.original()
        _, original = compiler.configure_window(configured, recipe)
        deadline = time.monotonic() + 25
        prior_path = root / "logs/core-session-build.receipt.json"
        guard([prior_path], True, deadline)
        prior_pin = pins.pin(prior_path, deadline)
        self.assertEqual(prior_pin["sha256"], warm.FAILED_STAGE_SHA)
        prior = compiler.read_pinned(pins, prior_pin, deadline)
        self.assertEqual(len(warm.failed_session(prior, root, recipe, compiler, original, guard, pins, deadline)), 5)

    @unittest.skipUnless(os.environ.get("AIRK_TEST_WARM_DIRECT_LOG") and os.environ.get("AIRK_TEST_WARM_CU17"),
                         "explicit guarded fresh control output required")
    def test_real_direct_job(self):
        _, compiler, pins, _, _, guard = self.original()
        cpu = compiler.load(compiler.OWNERS["cpu"], "warm_direct_control_cpu")
        log = Path(os.environ["AIRK_TEST_WARM_DIRECT_LOG"])
        self.assertTrue(log.is_absolute() and log.is_relative_to(compiler.REPO / "artifacts"))
        proof = {"stages": []}
        observed = warm.run_direct(cpu, pins, guard, compiler.REPO, log.parent, log.stem,
                                   [sys.executable, "-B", "-S", "-c", "print('owned direct control')"],
                                   dict(os.environ), time.monotonic() + 25, proof)
        self.assertEqual(Path(observed["path"]), log)
        self.assertIn("owned direct control", log.read_text(encoding="utf-8"))
        self.assertTrue(warm.empty_owned_job(proof["stages"][0]["actual_owned"]))


if __name__ == "__main__":
    unittest.main()
