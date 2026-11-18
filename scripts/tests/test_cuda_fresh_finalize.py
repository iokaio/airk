# SPDX-License-Identifier: Apache-2.0
"""Inert controls for resuming a settled configure's timed-out metadata audit."""

import datetime as dt
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "cuda-static-runtime/build/fresh_finalize.py"
SPEC = importlib.util.spec_from_file_location("test_cuda_finalize", SCRIPT)
OWNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNER)


class FreshFinalize(unittest.TestCase):
    def test_requires_the_same_launch_and_exact_post_command_audit_failure(self):
        recipe = {"path": "reviewed.json", "sha256": "a" * 64}
        original = {"tools": {"python": {"path": "pinned-python.exe"}}, "wall_seconds": 10800}
        started = dt.datetime(2026, 11, 18, 4, tzinfo=dt.timezone.utc)
        now = started + dt.timedelta(seconds=300)
        outer = {
            "exit_code": 1, "raw_receipt": None, "input_hash": recipe["sha256"],
            "arguments": ["-B", "-S", "scripts/cuda-static-runtime/build/fresh_configure.py",
                          "--recipe", "reviewed.json", "--recipe-sha256", "a" * 64],
            "command": "pinned-python.exe", "qualification": False, "human_sid": "verified",
            "started_utc": started.isoformat(), "elapsed_seconds": 240,
        }
        log = (b'    guard_paths(sorted((root / "b").rglob("*")), True, deadline)\n'
               b"subprocess.TimeoutExpired: command timed out after 10.0 seconds\n")
        args = (recipe, original, "verified", dt.datetime.fromisoformat, now)
        self.assertEqual(OWNER.check_failed_audit(outer, log, *args),
                         (started, started + dt.timedelta(seconds=240)))
        for field, value in (
            ("exit_code", 0), ("input_hash", "b" * 64), ("command", "other.exe"),
            ("elapsed_seconds", 301), ("raw_receipt", {}),
        ):
            with self.subTest(field=field), self.assertRaises(ValueError):
                OWNER.check_failed_audit({**outer, field: value}, log, *args)
        with self.assertRaisesRegex(ValueError, "post-command"):
            OWNER.check_failed_audit(outer, b"compiler error before output audit", *args)

    def test_recovery_never_extends_the_original_utc_or_monotonic_limit(self):
        start = dt.datetime(2026, 11, 18, 4, tzinfo=dt.timezone.utc)
        now = start + dt.timedelta(seconds=200)
        later = now + dt.timedelta(seconds=300)
        bounds = OWNER.recovery_deadline
        self.assertEqual(bounds(100, now, start, 260, later, 180), 160)
        self.assertEqual(bounds(100, now, start, 260, later, 140), 140)
        self.assertEqual(bounds(100, now, start, 260, now + dt.timedelta(seconds=30), 180), 130)
        for cutoff in (100, 90):
            with self.assertRaisesRegex(ValueError, "exhausted"):
                bounds(100, now, start, 260, later, cutoff)
        with self.assertRaisesRegex(ValueError, "exhausted"):
            bounds(100, now, start, 200, later, 180)
        with self.assertRaisesRegex(ValueError, "wall bound"):
            bounds(100, now, start, 10801, later, 180)


if __name__ == "__main__":
    unittest.main()
