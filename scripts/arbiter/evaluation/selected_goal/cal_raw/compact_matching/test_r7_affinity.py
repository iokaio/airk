# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for original R7 CAL affinity custody."""
from contextlib import contextmanager
from pathlib import Path
import runpy
import sys
import time
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_affinity as affinity


PID = 1234
OWNER_PID = 4321
SID = "same-human"
TERMINAL = dict(outer="a"*64, fixture="b"*64, mutex="c"*64)
FAILED = dict(outer="a"*64, mutex="c"*64, first_lifetime="d"*64, phase="e"*64, request="f"*64, closed="1"*64)
DEADLINE = time.monotonic() + 1000


class ConfigurationTests(unittest.TestCase):
    def test_two_explicit_operating_modes(self):
        affinity.validate_args(PID, 2, DEADLINE, None)
        affinity.validate_args(None, 2, DEADLINE, TERMINAL)
        affinity.validate_args(None, 2, DEADLINE, FAILED)
        with self.assertRaises(ValueError):
            affinity.validate_args(None, 2, DEADLINE, FAILED | {"fixture": "b"*64})
        invalid = [(None, 2, DEADLINE, None), (0, 2, DEADLINE, None), (True, 2, DEADLINE, None),
                   (PID, 2, DEADLINE, TERMINAL), (PID, 1, DEADLINE, None), (PID, 2, float("nan"), None),
                   (None, 2, DEADLINE, dict(outer="a"*64, fixture="b"*64))]
        for arguments in invalid:
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                affinity.validate_args(*arguments)

    def test_quiet_cores_exclude_both_siblings_of_active_cal_thread(self):
        picker = runpy.run_path(str(affinity.PREVIOUS))["pick_cpus"]
        sample = {
            "threads": [{"cpu_seconds": 1.0, "ideal": [0, 0]}, {"cpu_seconds": 0.0, "ideal": [0, 2]}],
            "system_mask": "0xff", "physical_siblings": [[0, 1], [2, 3], [4, 5], [6, 7]],
            "machine_cpu_busy_percent": [0.0, 0.0, 3.0, 2.0, 4.0, 3.0, 9.0, 9.0],
        }
        self.assertEqual(picker(sample, 2), [3, 5])
        sample["system_mask"] = "0xf7"
        self.assertEqual(picker(sample, 2), [5, 6])

    def test_cpu_counters_fail_on_inconsistent_delta(self):
        self.assertEqual(affinity.activity([(0, 0), (0, 0)], [(3, 4), (0, 4)]), [25.0, 100.0])
        with self.assertRaisesRegex(ValueError, "activity counters"):
            affinity.activity([(0, 1)], [(2, 2)])

    def test_other_process_cannot_call_either_setter(self):
        other = object.__new__(affinity.Process)
        other.current = False
        other.api = Mock()
        with self.assertRaisesRegex(ValueError, "current reference worker"):
            other.place(4)
        other.api.assert_not_called()


class R7ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.anchor = {"control_root": "shared", "inputs_sha256": "input", "profiles": ["profiles"],
                       "health": {"instance": "0"*32}}
        self.plan = {"start_ms": 100, "deadline_ms": 900, "installation_mutex": {"sha256": "acquired"},
                     "candidate_service": {"image": {"path": "candidate.exe", "sha256": "service"}},
                     "installation": {"ready": {"path": "anchor"}, "closed": {"path": "original"}},
                     "input_audit": {"sha256": "audit"}}
        self.pin = {"sha256": affinity.PLAN_SHA}
        self.files = {}
        self.support = SimpleNamespace(receipt=self.read, sid=SID, settled=Mock(return_value=True),
                                       check_group=Mock(), check_mutex=Mock())
        for group in range(2):
            directory = affinity.R7 / f"lifetime-{group:02}"
            fixture = directory / "fixture"
            instance = str(group+1)*32
            started = 110 + group*100
            self.files.update({
                directory / "live-request.json": ({"schema": "airk.private.cal100-live-fixture/1",
                    "ready": {"path": "ready"}, "bindings": {"path": "binding"}, "answer": {"path": "answer"},
                    "start_receipt": {"path": "start"}, "fixture_root": str(fixture)}, {"path": "request"}, 130),
                directory / "fixture-start.json": ({"schema": "airk.private.cal100-fixture-start/1",
                    "command": ["candidate.exe", "--ignored", "--exact", affinity.SELECTOR, "--test-threads", "1"],
                    "start_ms": started, "deadline_ms": started+10800000, "fixture_root": str(fixture),
                    "pid": PID+group, "input_audit_sha256": "audit"}, {}, started+1),
                fixture / "ready.json": (dict(self.anchor, schema="airk.private-arbiter-fixture/1", evaluation=True,
                    release_qualified=False, health={"instance": instance, "process_id": PID+group,
                    "executable_sha256": "service", "user": SID, "policy": {"accepting": True, "service": instance}}),
                    {}, started+10),
                fixture / "enable-context.bindings.json": ({"instance": instance}, {}, started+10),
                fixture / "context-request.json": ({}, {}, started+15),
                fixture / "closed.json": ({"schema": "airk.private-arbiter-fixture-closed/1",
                    "instance": instance, "shutdown_completed": True, "service_lease_reacquired": True,
                    "control_root": "shared"}, {}, started+60),
                directory / "lifetime-closure.json": ({"plan": self.pin, "publication": "index", "instance": instance,
                    "start_ms": started, "end_ms": started+60,
                    "prior": self.plan["installation"]["closed"] if not group else {"path": "group0"},
                    "live_request": {"path": "request"}, "closed": {"path": "native closed"}}, {}, started+60),
            })
        self.files.update({
            affinity.R7 / "outer-owned.json": ({"argv": ["python", "-I", "-B", "-S", str(affinity.OWNER),
                "--output", str(affinity.R7), "--node", "node.exe", "--owned-plan-sha256", affinity.PLAN_SHA,
                "--resume-r6"]}, {}, 320),
            affinity.R7 / "fixture-closure.json": ({"schema": affinity.FIXTURE_SCHEMA, "plan": self.pin,
                "ordered_questions": 100, "lifetimes": [{"path": "group0"}, {"path": "group1"}],
                "publication": "index"}, {}, 320),
            affinity.R7 / "control-mutex-released.json": ({}, {}, 321),
            affinity.R7 / "control-mutex-acquired.json": ({"name": "native-mutex", "owner_pid": OWNER_PID,
                "at_ms": 95}, {}, 95),
            Path("anchor"): (self.anchor, {}, 1),
        })

    def read(self, path, *, pin=None, sha=None):
        if sha is not None:
            expected = {"outer-owned.json": TERMINAL["outer"], "fixture-closure.json": TERMINAL["fixture"],
                        "control-mutex-released.json": TERMINAL["mutex"], "lifetime-closure.json": FAILED["first_lifetime"],
                        "phase-12.owned.json": FAILED["phase"], "live-request.json": FAILED["request"],
                        "closed.json": FAILED["closed"]}
            if expected[path.name] != sha:
                raise ValueError("invalid pinned terminal hash")
        return self.files[path]

    def terminal(self, pins=TERMINAL):
        return affinity.terminal_receipts(self.support, self.plan, self.pin, self.anchor, pins)

    def test_original_terminal_proves_both_native_releases_and_held_mutex(self):
        starts = self.terminal()
        self.assertEqual([(start["pid"], stamp) for start, stamp in starts], [(PID, 120), (PID+1, 220)])
        self.support.check_mutex.assert_called_once_with(
            affinity.R7, self.plan, TERMINAL["outer"], TERMINAL["mutex"], 270)
        self.assertEqual(self.support.check_group.call_count, 2)

    def test_unpinned_or_unsuccessful_terminal_refused(self):
        with self.assertRaisesRegex(ValueError, "invalid pinned terminal hash"):
            self.terminal(dict(TERMINAL, mutex="d"*64))
        self.support.settled.return_value = False
        with self.assertRaisesRegex(ValueError, "successful original R7 outer Job"):
            self.terminal()

    def test_terminal_refuses_missing_native_lease_and_swapped_lifetime(self):
        closed, _, _ = self.files[affinity.R7 / "lifetime-01/fixture/closed.json"]
        closed["service_lease_reacquired"] = False
        with self.assertRaisesRegex(ValueError, "exclusive lease"):
            self.terminal()
        closed["service_lease_reacquired"] = True
        record, _, _ = self.files[affinity.R7 / "lifetime-01/lifetime-closure.json"]
        record["prior"] = {"path": "different lifetime"}
        with self.assertRaisesRegex(ValueError, "terminal lifetime custody"):
            self.terminal()

    def failed_fixture(self):
        natural_failure = {
            "exit_code": 1, "timed_out": False, "log_oversized": False, "child_assigned_before_resume": True,
            "owned_descendants_settled": True, "owned_job_forced_termination": False,
            "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
        }
        outer = self.files[affinity.R7 / "outer-owned.json"][0]
        outer.update(natural_failure)
        del self.files[affinity.R7 / "fixture-closure.json"]
        last = affinity.R7 / "lifetime-01"
        del self.files[last / "lifetime-closure.json"]
        native, pin, _ = self.files[last / "fixture/closed.json"]
        self.files[last / "fixture/closed.json"] = native, pin, 300
        request, pin, stamp = self.files[last / "live-request.json"]
        self.files[last / "live-request.json"] = request, pin | {"sha256": FAILED["request"]}, stamp
        failed = natural_failure | {"phase": 12, "request_sha256": FAILED["request"],
                                    "start_ms": 280, "end_ms": 290, "deadline_ms": self.plan["deadline_ms"]}
        self.files[last / "phase-12.owned.json"] = failed, {}, 291
        return failed, native

    def test_failed_terminal_proves_retirement_without_a_full_capture(self):
        self.failed_fixture()
        starts = affinity.failed_terminal_receipts(self.support, self.plan, self.pin, self.anchor, FAILED)
        self.assertEqual([(start["pid"], stamp) for start, stamp in starts], [(PID, 120), (PID+1, 220)])
        self.support.check_group.assert_called_once()
        self.support.check_mutex.assert_called_once_with(
            affinity.R7, self.plan, FAILED["outer"], FAILED["mutex"], 320)

    def test_failed_terminal_refuses_changed_request_or_unclean_teardown(self):
        failed, native = self.failed_fixture()
        for change in ({"request_sha256": "wrong"}, {"owned_job_forced_termination": True},
                       {"end_ms": 301}, {"exit_code": 0}):
            with self.subTest(change=change):
                saved = failed.copy()
                failed.update(change)
                with self.assertRaisesRegex(ValueError, "failed R7 phase or its native cleanup"):
                    affinity.failed_terminal_receipts(self.support, self.plan, self.pin, self.anchor, FAILED)
                failed.update(saved)
        native["service_lease_reacquired"] = False
        with self.assertRaisesRegex(ValueError, "failed R7 phase or its native cleanup"):
            affinity.failed_terminal_receipts(self.support, self.plan, self.pin, self.anchor, FAILED)

    def test_live_retained_ready_must_match_token_and_pid(self):
        start, ready, ready_ms, _ = affinity.lifetime_receipts(self.support, self.plan, self.anchor, 1)
        self.assertEqual((start["pid"], ready["health"]["process_id"], ready_ms), (PID+1, PID+1, 220))
        ready["health"]["user"] = "another-human"
        with self.assertRaisesRegex(ValueError, "readiness"):
            affinity.lifetime_receipts(self.support, self.plan, self.anchor, 1)
        ready["health"]["user"] = SID
        ready["health"]["process_id"] = PID
        with self.assertRaisesRegex(ValueError, "readiness"):
            affinity.lifetime_receipts(self.support, self.plan, self.anchor, 1)

    def test_terminal_holds_mutex_across_work_and_rechecks_retirement(self):
        events = []

        @contextmanager
        def mutex():
            events.append("acquire")
            try:
                yield
            finally:
                events.append("release")

        self.support.mutex_type = Mock(return_value=(mutex, "native-mutex"))
        with patch.object(affinity, "require_retired", side_effect=lambda *_: events.append("retirement")):
            with affinity.cal_custody(self.support, self.plan, self.pin, None, TERMINAL, DEADLINE) as (cal, info, check):
                self.assertIsNone(cal)
                self.assertEqual(info["mode"], "settled")
                events.append("inside")
                check()
        self.assertEqual(events, ["acquire", "retirement", "inside", "retirement", "release"])

    def test_live_wrong_pid_fails_before_opening_process(self):
        self.support.mutex_type = Mock(return_value=(Mock(), "native-mutex"))
        self.support.api = Mock()
        with patch.object(affinity, "unclosed", side_effect=lambda group: group == 1), \
             patch.object(Path, "is_file", return_value=True), patch.object(affinity, "Process") as process:
            with self.assertRaisesRegex(ValueError, "requested CAL PID differs"):
                with affinity.cal_custody(self.support, self.plan, self.pin, PID, None, DEADLINE):
                    self.fail("unrelated CAL process accepted")
            process.assert_not_called()

    def test_terminal_refuses_original_live_pid_but_allows_newer_reuse(self):
        self.support.api = Mock()
        retained = Mock()
        retained.wait.return_value = affinity.STILL_RUNNING
        retained.born.return_value = 119
        retained.__enter__ = Mock(return_value=retained)
        retained.__exit__ = Mock(return_value=False)
        with patch.object(affinity, "Process", return_value=retained):
            with self.assertRaisesRegex(ValueError, "still has a live native process"):
                affinity.require_retired(self.support, [({"pid": PID}, 120)])
            retained.born.return_value = 121
            affinity.require_retired(self.support, [({"pid": PID}, 120)])


if __name__ == "__main__":
    unittest.main()
