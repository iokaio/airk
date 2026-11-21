# SPDX-License-Identifier: Apache-2.0
"""Check how the maintained GNU gate selects the successor volume policy. Cargo and the guest are not used."""
import argparse
from pathlib import Path
import sys
import types
import unittest
from unittest import mock

import gnu_volume_policy

LOG = "/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/host-volume-watch-r8.jsonl"
BINDING = ["--volume-log", LOG, "--volume-log-first-row-sha256", "ab" * 32, "--not-after", "2026-11-21T12:00:00Z"]
SAMPLE = {"guest_free": 953439313920, "backing_free": 2643244216320, "vhd_length": 79086747648}
INVENTORY = {"sha256": "source", "files": {"Cargo.toml": "ef" * 32}}


class MaintainedGatePolicy(unittest.TestCase):
    def setUp(self):
        # The gate's identity and volume owners import Linux-only modules. Stand-ins exist only inside
        # this test: patch.dict restores sys.modules afterwards.
        modules = mock.patch.dict(sys.modules)
        modules.start()
        self.addCleanup(modules.stop)
        try:
            import pwd  # noqa: F401
        except ImportError:
            sys.modules["pwd"] = types.ModuleType("pwd")
        sys.modules["shipping_build"] = types.SimpleNamespace(space=lambda: dict(SAMPLE))
        sys.modules.pop("maintained_gate", None)
        import maintained_gate
        self.gate = maintained_gate
        self.events = mock.Mock()
        self.events.census.return_value = dict(INVENTORY)
        for target, name, value in ((maintained_gate, "identity", lambda: 1000),
                                    (maintained_gate, "source_inventory", self.events.census),
                                    (gnu_volume_policy.Binding, "admit", self.events.admit)):
            patch = mock.patch.object(target, name, value)
            patch.start()
            self.addCleanup(patch.stop)

    def options(self, *argv):
        parser = argparse.ArgumentParser()
        parser.add_argument("--gate", required=True)
        source = parser.add_mutually_exclusive_group()
        source.add_argument("--signed-r7", action="store_true")
        source.add_argument("--signed-joint", action="store_true")
        parser.add_argument("--attempt", type=int, default=1)
        parser.add_argument("--freeze", type=Path)
        parser.add_argument("--freeze-sha256")
        parser.add_argument("--revised-host-volume-log", type=Path)
        gnu_volume_policy.arguments(parser)
        return parser.parse_args(argv)

    def test_inspect_admits_before_the_census_and_keeps_its_document(self):
        self.assertEqual(self.gate.run(self.options("--gate", "inspect", *BINDING)), INVENTORY)
        self.assertEqual(self.events.mock_calls, [mock.call.admit(SAMPLE, 0, 1000), mock.call.census(self.gate.REPO)])
        # Without the policy arguments the gate behaves as before and consults no watcher.
        self.events.reset_mock()
        self.assertEqual(self.gate.run(self.options("--gate", "inspect")), INVENTORY)
        self.assertEqual(self.events.mock_calls, [mock.call.census(self.gate.REPO)])

    def test_a_refused_admission_stops_every_gate_before_its_census(self):
        self.events.admit.side_effect = ValueError("volume policy refuses admission: VHD growth cap")
        for gate in ("inspect", "clippy", "product-clippy", "nextest"):
            with self.subTest(gate=gate), self.assertRaisesRegex(ValueError, "VHD growth cap"):
                self.gate.run(self.options("--gate", gate, *BINDING))
        self.events.census.assert_not_called()

    def test_clippy_and_nextest_accept_the_policy_and_reach_their_freeze_check(self):
        for gate in ("clippy", "product-clippy", "nextest"):
            self.events.reset_mock()
            with self.subTest(gate=gate), self.assertRaisesRegex(ValueError, "maintained source changed before"):
                self.gate.run(self.options("--gate", gate, *BINDING))
            self.assertEqual(self.events.mock_calls, [mock.call.admit(SAMPLE, 0, 1000),
                                                      mock.call.census(self.gate.REPO)])

    def test_one_policy_and_a_complete_binding_are_required(self):
        with self.assertRaisesRegex(ValueError, "select one volume policy"):
            self.gate.run(self.options("--gate", "nextest", "--signed-r7", "--revised-host-volume-log", LOG, *BINDING))
        with self.assertRaisesRegex(ValueError, "together"):
            self.gate.run(self.options("--gate", "inspect", *BINDING[:2]))
        # The expired R7 revision keeps its original scope.
        with self.assertRaisesRegex(ValueError, "scoped to signed R7 Nextest"):
            self.gate.run(self.options("--gate", "clippy", "--revised-host-volume-log", LOG))
        self.events.admit.assert_not_called()
        self.events.census.assert_not_called()


if __name__ == "__main__":
    unittest.main()
