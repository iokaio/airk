# SPDX-License-Identifier: Apache-2.0
"""Check which entries the volume policy launcher starts, without starting one."""
from datetime import datetime, timezone
from pathlib import Path
import unittest

from volume_policy_entry import split

HERE = Path(__file__).resolve().parent
LOG = "/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/host-volume-watch-r8.jsonl"
BINDING = ["--volume-log", LOG, "--volume-log-first-row-sha256", "ab" * 32, "--not-after", "2026-11-21T12:00:00Z"]


class VolumePolicyEntry(unittest.TestCase):
    def test_binding_and_entry_arguments_are_separated(self):
        binding, command = split([*BINDING, "--", "shipping_archive.py", "native", "--build", "/b.json",
                                  "--build-sha256", "cd" * 32, "--attempt", "2"])
        self.assertEqual(binding.log, Path(LOG))
        self.assertEqual(binding.first_row_sha256, "ab" * 32)
        self.assertEqual(binding.not_after, datetime(2026, 11, 21, 12, tzinfo=timezone.utc).timestamp())
        self.assertEqual(command, [str(HERE / "shipping_archive.py"), "native", "--build", "/b.json",
                                   "--build-sha256", "cd" * 32, "--attempt", "2"])
        for entry in ("shipping_build.py", "shipping_service.py"):
            self.assertEqual(split([*BINDING, "--", entry])[1], [str(HERE / entry)])

    def test_other_entries_and_incomplete_bindings_refuse(self):
        for argv in ([*BINDING, "shipping_build.py"], [*BINDING, "--"], [*BINDING, "--", "maintained_gate.py"],
                     [*BINDING, "--", "../shipping_build.py"], [*BINDING, "--", str(HERE / "shipping_build.py")],
                     [*BINDING, "--", "cargo_gate.py", "--gate", "clippy"], ["--", "shipping_build.py"]):
            with self.subTest(argv=argv), self.assertRaises(ValueError):
                split(argv)
        with self.assertRaisesRegex(ValueError, "together"):
            split([*BINDING[:4], "--", "shipping_build.py"])
        with self.assertRaisesRegex(ValueError, "explicit UTC offset"):
            split([*BINDING[:5], "2026-11-21T12:00:00", "--", "shipping_build.py"])


if __name__ == "__main__":
    unittest.main()
