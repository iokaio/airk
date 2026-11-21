# SPDX-License-Identifier: Apache-2.0
"""Check the shipping and packaging build guard with and without the successor volume policy.

The owned process, the guest filesystem and the watcher are replaced by recorded values.
"""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest import mock

import gnu_volume_policy as policy

ROW = (b'{"utc":"2026-11-20T13:58:30.6559742+00:00","windows_free":1128990367744,"backing_free":2643336200192,'
       b'"vhd_bytes":79086747648,"windows_start":1233246265344,"backing_start":2669960904704,'
       b'"vhd_start":60649635840,"windows_budget":195421011968,"backing_budget":120000000000,'
       b'"free_floor":107374182400,"within_budget":true,"has_headroom":true,"backing_writes_held":false,'
       b'"original_alert_log_sha256":"9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda",'
       b'"previous_backing_budget":25769803776}')
# First row of the retained guest baseline log, and the guest's reading at the retained 14:04 UTC boot.
BASELINE = b'{"guest_free":967378755584,"backing_free":2669979959296,"vhd_length":60649635840}\n'
BOOT = {"time": 1795183496.0, "guest_free": 953439313920, "backing_free": 2643244216320, "vhd_length": 79086747648}
RESERVE = 536870912


class ShippingGuardPolicy(unittest.TestCase):
    def setUp(self):
        modules = mock.patch.dict(sys.modules)
        modules.start()
        self.addCleanup(modules.stop)
        for name in ("pwd", "resource"):
            try:
                __import__(name)
            except ImportError:
                sys.modules[name] = types.ModuleType(name)
        sys.modules.pop("shipping_build", None)
        import shipping_build
        self.build = shipping_build
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.campaign = Path(directory.name).resolve()
        (self.campaign / policy.GUEST_BASELINE_LOG).write_bytes(BASELINE)
        self.log = self.campaign / "host-volume-watch-r8.jsonl"
        self.log.write_bytes(ROW + b"\r\n" + ROW.replace(b"13:58:30.6559742", b"14:05:00.0000000") + b"\r\n")
        self.result = self.campaign / "result"
        self.result.mkdir()
        self.now = datetime(2026, 11, 20, 14, 5, 30, tzinfo=timezone.utc).timestamp()
        self.samples = [dict(BOOT), dict(BOOT)]
        self.owned = []
        self.binding = policy.Binding(self.log, hashlib.sha256(ROW).hexdigest(),
                                      datetime(2026, 11, 20, 14, 30, tzinfo=timezone.utc).timestamp())
        guard, bounded = policy.Binding.guard, policy.Binding.bounded

        def run_owned(command, root, log, timeout, env):
            self.owned.append((command, root, log, timeout, env))
            return {"exit_code": 0}

        for target, name, value in (
                (shipping_build, "CAMPAIGN", self.campaign), (shipping_build, "space", lambda: self.samples.pop(0)),
                (shipping_build, "audit", lambda root, uid: None),
                (shipping_build.runpy, "run_path", lambda path: {"run_owned": run_owned, "successful": lambda done: True}),
                (policy, "guest_baseline", mock.Mock()),
                (policy.Binding, "guard", lambda binding, reserve, uid: guard(
                    binding, reserve, uid, clock=lambda: self.now, campaign=self.campaign, d_free=lambda: 1128377892864)),
                (policy.Binding, "bounded", lambda binding, timeout: bounded(binding, timeout, clock=lambda: self.now))):
            patch = mock.patch.object(target, name, value)
            patch.start()
            self.addCleanup(patch.stop)

    def monitored(self, **options):
        return self.build.monitored(["/usr/bin/true"], self.campaign, self.result, {"LC_ALL": "C"}, RESERVE,
                                    self.log.lstat().st_uid, **options)

    def activate(self):
        patch = mock.patch.object(policy, "_ACTIVE", self.binding)
        patch.start()
        self.addCleanup(patch.stop)

    def test_original_guard_refuses_the_retained_state(self):
        # C: has grown 26735742976 bytes from this guard's own baseline; its limit is 25769803776.
        self.assertFalse(self.build.capacity(json.loads(BASELINE), BOOT, RESERVE))
        with self.assertRaisesRegex(ValueError, "insufficient guest or backing reservation"):
            self.monitored()
        self.assertEqual(self.owned, [])

    def test_active_policy_admits_bounds_the_run_and_records_the_watcher(self):
        self.activate()
        observed, samples, stopped, success = self.monitored(timeout=3600, log_name="build.log")
        self.assertEqual((observed, stopped, success), ({"exit_code": 0}, False, True))
        command, root, log, timeout, env = self.owned[0]
        self.assertEqual(command, ["/usr/bin/nice", "-n", "10", "/usr/bin/prlimit", "--as=8589934592", "--",
                                   "/usr/bin/true"])
        self.assertEqual((root, log, env), (self.campaign, self.result / "build.log", {"LC_ALL": "C"}))
        # The run ends at 14:30 UTC, 1470 seconds after this clock, not after the entry's own hour.
        self.assertEqual(timeout, 1470)
        written = json.loads((self.result / "volumes.json").read_text())
        self.assertEqual(written, samples)
        self.assertEqual([row["volume_refusals"] for row in written], [[], []])
        self.assertEqual([row["windows_free"] for row in written], [1128377892864, 1128377892864])
        self.assertEqual(written[0]["host_monitor"]["utc"], "2026-11-20T14:05:00.0000000+00:00")

    def test_active_policy_refuses_a_stale_watcher_before_the_process_starts(self):
        self.activate()
        self.now += 16
        with self.assertRaisesRegex(ValueError, "volume monitor observation is stale"):
            self.monitored()
        self.assertEqual(self.owned, [])
        self.assertFalse((self.result / "volumes.json").exists())

    def test_active_policy_refuses_a_reservation_beyond_the_vhd_cap(self):
        self.activate()
        with self.assertRaisesRegex(ValueError, "refuses admission: VHD growth cap"):
            self.build.monitored(["/usr/bin/true"], self.campaign, self.result, {}, 7332691969, self.log.lstat().st_uid)
        self.assertEqual(self.owned, [])

    def test_growth_beyond_the_reservation_fails_the_run(self):
        self.activate()
        self.samples[1]["guest_free"] -= RESERVE + 1
        _, samples, stopped, success = self.monitored()
        self.assertEqual((stopped, success), (False, False))
        self.assertEqual(samples[-1]["volume_refusals"], ["storage reservation of this run"])

    def test_an_explicit_guard_takes_precedence_over_the_active_policy(self):
        self.activate()
        seen = []
        _, _, _, success = self.monitored(volume_guard=lambda sample, initial, admitting: seen.append(admitting) or True)
        self.assertIs(success, True)
        self.assertEqual(seen, [True, False])
        self.assertEqual(self.owned[0][3], 3600)


if __name__ == "__main__":
    unittest.main()
