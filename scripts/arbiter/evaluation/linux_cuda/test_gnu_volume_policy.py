# SPDX-License-Identifier: Apache-2.0
"""Exercise the successor volume policy for the GNU gates without WSL, Cargo or a watcher."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import gnu_volume_policy as policy

RETAINED = Path(__file__).resolve().parents[4] / "artifacts/v190/final-plan/exec-20261116T052942Z"
# First row of the retained host-volume-watch-r4.jsonl, copied by hand.
R4_FIRST = (b'{"utc":"2026-11-20T13:58:30.6559742+00:00","windows_free":1128990367744,"backing_free":2643336200192,'
            b'"vhd_bytes":79086747648,"windows_start":1233246265344,"backing_start":2669960904704,'
            b'"vhd_start":60649635840,"windows_budget":195421011968,"backing_budget":120000000000,'
            b'"free_floor":107374182400,"within_budget":true,"has_headroom":true,"backing_writes_held":false,'
            b'"original_alert_log_sha256":"9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda",'
            b'"previous_backing_budget":25769803776}')
# First row of the retained host-volume-watch-r1.jsonl: the original 24 GiB C: budget.
R1_FIRST = (b'{"utc":"2026-11-20T06:09:04.0168694+00:00","windows_free":1203506462720,"backing_free":2669960904704,'
            b'"vhd_bytes":60649635840,"windows_start":1233246265344,"backing_start":2669960904704,'
            b'"vhd_start":60649635840,"windows_budget":195421011968,"backing_budget":25769803776,'
            b'"free_floor":107374182400,"within_budget":true,"has_headroom":true}')


def at(hour, minute, second):
    return datetime(2026, 11, 20, hour, minute, second, tzinfo=timezone.utc).timestamp()


class Limits(unittest.TestCase):
    def test_constants_are_the_recorded_authorization(self):
        self.assertEqual(policy.C_BASELINE, 2669960904704)
        self.assertEqual(policy.C_GROWTH_ALLOWANCE, 120000000000)
        self.assertEqual(policy.D_BASELINE, 1233246265344)
        self.assertEqual(policy.D_GROWTH_BUDGET, 195421011968)
        self.assertEqual(policy.FREE_FLOOR, 107374182400)
        self.assertEqual(policy.GUEST_FREE_BASELINE, 967378755584)
        self.assertEqual(policy.VHD_BASELINE, 60649635840)
        self.assertEqual(policy.GUEST_GROWTH_CAP, 25769803776)
        self.assertEqual(policy.VHD_GROWTH_CAP, 25769803776)
        self.assertEqual(policy.SWAP_RESERVE, 17179869184)
        self.assertEqual(policy.MAX_ROW_AGE_SECONDS, 45)

    def test_each_cap_has_one_definition(self):
        source = Path(policy.__file__).read_text(encoding="utf-8")
        self.assertEqual(source.count("GUEST_GROWTH_CAP = "), 1)
        self.assertEqual(source.count("VHD_GROWTH_CAP = "), 1)
        # Only the two caps and the watcher's record of the original C: budget carry this number.
        self.assertEqual(source.count("25_769_803_776"), 3)
        self.assertNotIn("24 << 30", source)

    def test_guest_baseline_requires_the_retained_log(self):
        with self.assertRaisesRegex(ValueError, "baseline changed"):
            policy.guest_baseline(b'{"guest_free":967378755584,"vhd_length":60649635840}\n')

    @unittest.skipUnless((RETAINED / policy.GUEST_BASELINE_LOG).is_file(), "private campaign receipts are absent")
    def test_retained_guest_baseline_matches_the_constants(self):
        first = policy.guest_baseline((RETAINED / policy.GUEST_BASELINE_LOG).read_bytes())
        self.assertEqual((first["guest_free"], first["vhd_length"]), (967378755584, 60649635840))


class Capacity(unittest.TestCase):
    def setUp(self):
        # Guest and host readings of the retained 14:04 UTC boot on 20 November.
        self.sample = {"guest_free": 953439313920, "backing_free": 2643244216320, "vhd_length": 79086747648}
        self.monitor = {"backing_free": 2643244216320, "windows_free": 1128377892864, "vhd_bytes": 79086747648}

    def found(self, sample=None, *, monitor=None, initial=None, reserve=536870912, admitting=True):
        sample = self.sample | (sample or {})
        return policy.refusals(sample, self.monitor | (monitor or {}), initial or sample, reserve, admitting=admitting)

    def test_retained_state_fits_the_c_allowance_and_shows_its_headroom(self):
        self.assertEqual(self.found(), [])
        self.assertEqual(policy.headroom(self.sample, self.monitor), {
            "guest_growth": 13939441664, "guest_remaining": 11830362112,
            "vhd_growth": 18437111808, "vhd_remaining": 7332691968,
            "c_growth": 26716688384, "c_remaining": 76103442432,
            "d_growth": 104868372480, "d_remaining": 90552639488})
        # The same state is beyond the original 24 GiB C: budget that the default guard still applies.
        self.assertGreater(26716688384, 25769803776)

    def test_vhd_cap_is_exact_and_separate(self):
        self.assertEqual(self.found(reserve=7332691968), [])
        self.assertEqual(self.found(reserve=7332691969), ["VHD growth cap"])
        self.assertEqual(self.found(monitor={"vhd_bytes": 86419439617}, admitting=False), ["VHD growth cap"])

    def test_guest_cap_is_exact_and_separate(self):
        self.assertEqual(self.found({"guest_free": 941608951808}, admitting=False), [])
        self.assertEqual(self.found({"guest_free": 941608951807}, admitting=False), ["guest free-space growth cap"])
        # With a 512 MiB reservation the last admissible reading is 536870912 bytes higher.
        self.assertEqual(self.found({"guest_free": 942145822720}), [])
        self.assertEqual(self.found({"guest_free": 942145822719}), ["guest free-space growth cap"])

    def test_c_allowance_keeps_the_swap_reserve_and_takes_the_lower_reading(self):
        name = "C: growth allowance with the swap reserve"
        self.assertEqual(self.found({"backing_free": 2567140773888}, admitting=False), [])
        self.assertEqual(self.found({"backing_free": 2567140773887}, admitting=False), [name])
        self.assertEqual(self.found(monitor={"backing_free": 2567140773887}, admitting=False), [name])
        self.assertEqual(self.found({"backing_free": 2567140773888}, reserve=1), [name])

    def test_d_budget_and_floor(self):
        self.assertEqual(self.found(monitor={"windows_free": 1037825253376}), [])
        self.assertEqual(self.found(monitor={"windows_free": 1037825253375}), ["D: growth budget"])
        self.assertEqual(self.found({"windows_free": 1037825253375}), ["D: growth budget"])
        self.assertEqual(self.found(monitor={"windows_free": 107374182399}),
                         ["D: growth budget", "D: free-space floor"])

    def test_free_space_floors(self):
        self.assertEqual(self.found({"backing_free": 107374182399}, admitting=False),
                         ["C: growth allowance with the swap reserve", "C: free-space floor"])
        self.assertEqual(self.found({"guest_free": 107374182399}, admitting=False),
                         ["guest free-space growth cap", "guest free-space floor"])

    def test_a_run_cannot_exceed_its_reservation(self):
        name = "storage reservation of this run"
        grown = {"guest_free": self.sample["guest_free"] - 536870913}
        self.assertEqual(self.found(grown, initial=self.sample, admitting=False), [name])
        self.assertEqual(self.found({"guest_free": self.sample["guest_free"] - 536870912},
                                    initial=self.sample, admitting=False), [])
        self.assertEqual(self.found({"vhd_length": 79086747648 + 536870913}, initial=self.sample, admitting=False),
                         [name])
        # An observer without a reservation is still held to every total.
        self.assertEqual(self.found(grown, initial=self.sample, reserve=None), [])
        self.assertEqual(self.found({"guest_free": 941608951807}, initial=self.sample, reserve=None),
                         ["guest free-space growth cap"])


class Monitor(unittest.TestCase):
    def setUp(self):
        self.first = json.loads(R4_FIRST)
        self.last = self.first | {"utc": "2026-11-20T14:05:00.1234567+00:00"}
        self.now = at(14, 5, 30)
        self.not_after = at(18, 0, 0)

    def refused(self, pattern, *, first=None, last=None, now=None, not_after=None):
        with self.assertRaisesRegex(ValueError, pattern):
            policy.check_monitor(self.first | (first or {}), self.last | (last or {}),
                                 self.now if now is None else now, self.not_after if not_after is None else not_after)

    def test_fresh_healthy_rows_are_accepted(self):
        policy.check_monitor(self.first, self.last, self.now, self.not_after)
        policy.check_monitor(self.first, self.last, at(14, 5, 45), self.not_after)

    def test_last_row_older_than_45_seconds_or_from_the_future_refuses(self):
        self.refused("stale", now=at(14, 5, 46))
        self.refused("stale", now=at(14, 4, 59))
        self.refused("stale", last={"utc": "2026-11-20T13:58:00+00:00"}, now=at(13, 58, 10))

    def test_breach_or_hold_refuses(self):
        self.refused("breach", last={"within_budget": False})
        self.refused("breach", last={"has_headroom": False})
        self.refused("breach", last={"within_budget": 1})
        self.refused("holds", last={"backing_writes_held": True})
        self.refused("breach", first={"has_headroom": False})

    def test_rows_must_carry_the_recorded_baselines_and_allowances(self):
        for changed in ({"backing_budget": 25769803776}, {"backing_budget": 120000000001},
                        {"backing_start": 2669979959296}, {"windows_budget": 195421011969},
                        {"windows_start": 1233246265345}, {"vhd_start": 79086747648},
                        {"free_floor": 107374182399}, {"previous_backing_budget": 120000000000},
                        {"original_alert_log_sha256": "0" * 64}, {"backing_budget": 120000000000.0}):
            with self.subTest(changed=changed):
                self.refused("recorded baselines", last=changed)
                self.refused("recorded baselines", first=changed)
        with self.assertRaisesRegex(ValueError, "recorded baselines"):
            policy.check_row(json.loads(R1_FIRST))

    def test_malformed_rows_refuse_with_one_error_type(self):
        self.refused("three observations", last={"backing_free": None})
        self.refused("three observations", last={"vhd_bytes": "79086747648"})
        self.refused("explicit UTC offset", last={"utc": None})
        for value in ([], "row", 1, None):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "not an object"):
                policy.check_row(value)
        missing = dict(self.last)
        del missing["windows_free"]
        with self.assertRaisesRegex(ValueError, "three observations"):
            policy.check_monitor(self.first, missing, self.now, self.not_after)

    def test_run_window(self):
        self.refused("window is closed", not_after=self.now)
        self.refused("window is closed", now=at(14, 5, 30), not_after=at(14, 5, 29))
        self.refused("within one day", not_after=at(13, 58, 30) + 86401)
        policy.check_monitor(self.first, self.last, self.now, at(13, 58, 30) + 86400)

    def test_times_need_an_explicit_offset(self):
        self.assertEqual(policy.instant("2026-11-20T18:00:00Z"), at(18, 0, 0))
        self.assertEqual(policy.instant("2026-11-20T11:00:00-07:00"), at(18, 0, 0))
        with self.assertRaisesRegex(ValueError, "explicit UTC offset"):
            policy.instant("2026-11-20T18:00:00")
        self.refused("explicit UTC offset", last={"utc": "2026-11-20T14:05:00"})

    def test_rows_ignore_a_line_still_being_written(self):
        tail = R4_FIRST.replace(b"13:58:30.6559742", b"13:59:00.0000000")
        first, last = policy.rows(R4_FIRST + b"\r\n" + tail + b"\r\n" + b'{"utc":"2026-11-20T13:59:3')
        self.assertEqual((first, last), (R4_FIRST + b"\r", tail + b"\r"))
        self.assertEqual(policy.rows(R4_FIRST + b"\n"), (R4_FIRST, R4_FIRST))
        self.assertEqual(policy.row_digest(first), hashlib.sha256(R4_FIRST).hexdigest())
        for empty in (b"", b"\n", R4_FIRST):
            with self.subTest(empty=empty), self.assertRaisesRegex(ValueError, "no complete row"):
                policy.rows(empty)


class Custody(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.campaign = Path(directory.name).resolve()
        self.log = self.campaign / "host-volume-watch-r8.jsonl"
        self.last = R4_FIRST.replace(b"13:58:30.6559742", b"14:05:00.0000000")
        self.log.write_bytes(R4_FIRST + b"\r\n" + self.last + b"\r\n")
        self.digest = hashlib.sha256(R4_FIRST).hexdigest()
        self.uid = self.log.lstat().st_uid
        self.now = at(14, 5, 30)
        self.sample = {"guest_free": 953439313920, "backing_free": 2643244216320, "vhd_length": 79086747648}

    def read(self, path=None, *, campaign=None, uid=None, digest=None):
        return policy.read_monitor(path or self.log, campaign or self.campaign, self.uid if uid is None else uid,
                                   digest or self.digest, self.now, at(18, 0, 0))

    def test_bound_log_returns_first_and_last_row(self):
        first, last = self.read()
        self.assertEqual(first["utc"], "2026-11-20T13:58:30.6559742+00:00")
        self.assertEqual(last["utc"], "2026-11-20T14:05:00.0000000+00:00")

    def test_other_log_owner_name_place_or_first_row_refuses(self):
        with self.assertRaisesRegex(ValueError, "first row differs"):
            self.read(digest="0" * 64)
        with self.assertRaisesRegex(ValueError, "human-owned campaign volume monitor"):
            self.read(uid=self.uid + 1)
        for name in ("host-volume-watch-r0.jsonl", "host-volume-watch-r8.jsonl.bak", "volume.jsonl"):
            other = self.campaign / name
            other.write_bytes(self.log.read_bytes())
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "human-owned campaign volume monitor"):
                self.read(other)
        nested = self.campaign / "nested"
        nested.mkdir()
        (nested / self.log.name).write_bytes(self.log.read_bytes())
        with self.assertRaisesRegex(ValueError, "human-owned campaign volume monitor"):
            self.read(nested / self.log.name)
        with self.assertRaisesRegex(ValueError, "human-owned campaign volume monitor"):
            self.read(self.campaign / "nested" / ".." / self.log.name)

    def binding(self):
        return policy.Binding(self.log, self.digest, at(18, 0, 0))

    def test_guard_requires_the_guest_baseline_log(self):
        with self.assertRaises(FileNotFoundError):
            self.binding().guard(0, self.uid, campaign=self.campaign)
        (self.campaign / policy.GUEST_BASELINE_LOG).write_bytes(b"{}\n")
        with self.assertRaisesRegex(ValueError, "baseline changed"):
            self.binding().guard(0, self.uid, campaign=self.campaign)

    def test_guard_admits_records_and_refuses(self):
        (self.campaign / policy.GUEST_BASELINE_LOG).write_bytes(b"{}\n")
        with mock.patch.object(policy, "guest_baseline") as baseline:
            fits = self.binding().guard(536870912, self.uid, clock=lambda: self.now, campaign=self.campaign,
                                        d_free=lambda: 1128377892864)
            baseline.assert_called_once_with(b"{}\n")
            sample = dict(self.sample)
            self.assertTrue(fits(sample, sample, True))
            self.assertEqual(sample["volume_refusals"], [])
            self.assertEqual(sample["windows_free"], 1128377892864)
            self.assertEqual(sample["host_monitor"]["utc"], "2026-11-20T14:05:00.0000000+00:00")
            grown = dict(self.sample, vhd_length=79086747648 + 536870913)
            self.assertFalse(fits(grown, self.sample, False))
            self.assertEqual(grown["volume_refusals"], ["storage reservation of this run"])
            low = self.binding().guard(536870912, self.uid, clock=lambda: self.now, campaign=self.campaign,
                                       d_free=lambda: 1037825253375)
            with self.assertRaisesRegex(ValueError, "refuses admission: D: growth budget"):
                self.binding().admit(dict(self.sample), 536870912, self.uid, clock=lambda: self.now,
                                     campaign=self.campaign, d_free=lambda: 1037825253375)
            with self.assertRaisesRegex(ValueError, "refuses admission: D: growth budget"):
                low(dict(self.sample), self.sample, True)
            running = dict(self.sample)
            self.assertFalse(low(running, self.sample, False))
            self.assertEqual(running["volume_refusals"], ["D: growth budget"])
            stale = self.binding().guard(0, self.uid, clock=lambda: at(14, 5, 46), campaign=self.campaign,
                                         d_free=lambda: 1128377892864)
            with self.assertRaisesRegex(ValueError, "stale"):
                stale(dict(self.sample), self.sample, False)

    def test_binding_arguments_and_window(self):
        parser = argparse.ArgumentParser()
        policy.arguments(parser)
        self.assertIsNone(policy.selected(parser.parse_args([])))
        with self.assertRaisesRegex(ValueError, "together"):
            policy.selected(parser.parse_args(["--volume-log", str(self.log)]))
        chosen = policy.selected(parser.parse_args(["--volume-log", str(self.log), "--volume-log-first-row-sha256",
                                                    self.digest, "--not-after", "2026-11-20T18:00:00Z"]))
        self.assertEqual((chosen.log, chosen.first_row_sha256, chosen.not_after), (self.log, self.digest, at(18, 0, 0)))
        self.assertEqual(chosen.bounded(1800, clock=lambda: at(17, 0, 0)), 1800)
        self.assertEqual(chosen.bounded(1800, clock=lambda: at(17, 50, 0)), 600)
        with self.assertRaisesRegex(ValueError, "window is closed"):
            chosen.bounded(1800, clock=lambda: at(18, 0, 0))
        with self.assertRaisesRegex(ValueError, "64 lowercase"):
            policy.Binding(self.log, self.digest.upper(), at(18, 0, 0))
        self.assertEqual(chosen.receipt(), {
            "volume_log": str(self.log), "first_row_sha256": self.digest, "not_after": "2026-11-20T18:00:00+00:00",
            "c_growth_allowance": 120000000000, "d_growth_budget": 195421011968, "free_floor": 107374182400,
            "guest_growth_cap": 25769803776, "vhd_growth_cap": 25769803776, "swap_reserve": 17179869184})

    def test_one_binding_per_process(self):
        with mock.patch.object(policy, "_ACTIVE", None):
            self.assertIsNone(policy.active())
            chosen = self.binding()
            policy.activate(chosen)
            self.assertIs(policy.active(), chosen)
            with self.assertRaisesRegex(ValueError, "one volume policy binding"):
                policy.activate(self.binding())
        with mock.patch.object(policy, "_ACTIVE", None), self.assertRaisesRegex(ValueError, "one volume policy binding"):
            policy.activate(None)


if __name__ == "__main__":
    unittest.main()
