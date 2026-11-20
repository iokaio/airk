# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the stopped R7 predecessor verifier."""

from contextlib import nullcontext
from copy import deepcopy
import json
import unittest
from unittest.mock import patch

import r8_predecessor as predecessor


def failed_job():
    return {
        "exit_code": 1, "child_assigned_before_resume": True, "owned_descendants_settled": True,
        "timed_out": False, "log_oversized": False, "owned_job_forced_termination": False,
        "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
    }


def custody():
    refs = {key: {"path": key, "sha256": "f" * 64}
            for key in ("plan", "outer", "acquired", "final_request", "transcript")}
    worker = {"path": "snapshot/worker.cjs"}
    plan = {
        "schema": "airk.private.cal100-r7-two-lifetime-plan/1", "common_ms": 10_800_000,
        "clock_decision": None, "setup_elapsed_ms": 545_806, "start_ms": 10_000,
        "deadline_ms": 10_264_194, "installation_mutex": refs["acquired"],
        "candidate_service": {"sources": {"worker.cjs": worker}},
        "phase_ids": [list(range(n, min(n + 8, 100))) for n in range(0, 100, 8)],
        "phase_groups": [list(range(7)), list(range(7, 13))],
    }
    outer = failed_job() | {
        "argv": ["python", "-I", "-B", "-S", str(predecessor.HERE / "r6_capture.py"),
                 "--output", str(predecessor.R7), "--owned-plan-sha256", refs["plan"]["sha256"], "--resume-r6"],
    }
    acquired = {"name": r"Local\AIRK-20261116-Focus1024-Reuse", "owner_pid": 70, "at_ms": 10_000}
    release = acquired | {"acquired": refs["acquired"], "outer": refs["outer"], "at_ms": 10_200_000}
    phase = failed_job() | {
        "phase": 12, "start_ms": 9_800_000, "end_ms": 10_190_000, "deadline_ms": 10_264_194,
        "request_sha256": refs["final_request"]["sha256"],
        "transcript": refs["transcript"], "sources": [worker],
        "argv": ["node", worker["path"], "final_request", refs["final_request"]["sha256"], "12", "464.0"],
    }
    start = {"start_ms": 5_510_000, "deadline_ms": 16_310_000}
    first = {"end_ms": 5_500_000}
    return refs, plan, outer, acquired, release, phase, start, first


class PredecessorTests(unittest.TestCase):
    def test_accepts_naturally_failed_original_custody(self):
        predecessor.verify_custody(*custody())

    def test_refuses_success_forced_or_surviving_jobs(self):
        for index in (2, 5):
            for field, value in (("exit_code", 0), ("timed_out", True), ("owned_job_forced_termination", True),
                                 ("owned_job_final_member_pids", [100])):
                args = list(custody())
                args[index][field] = value
                with self.subTest(index=index, field=field), self.assertRaises(ValueError):
                    predecessor.verify_custody(*args)

    def test_refuses_new_clock_foreign_mutex_and_substituted_phase(self):
        for index, field, value in ((1, "common_ms", 11_700_000), (1, "clock_decision", {}), (4, "owner_pid", 71),
                                    (5, "phase", 11), (5, "transcript", {}), (6, "deadline_ms", 17_210_000),
                                    (7, "end_ms", 5_520_000)):
            args = list(custody())
            args[index][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                predecessor.verify_custody(*args)

    def test_refuses_missing_native_lease_and_foreign_image(self):
        anchor = {"control_root": "control", "inputs_sha256": "inputs", "profiles": []}
        ready = anchor | {"schema": "airk.private-arbiter-fixture/1", "evaluation": True, "release_qualified": False,
                          "health": {"executable_sha256": "image", "instance": "native"}}
        closed = {"schema": "airk.private-arbiter-fixture-closed/1", "evaluation": True, "release_qualified": False,
                  "control_root": "control", "instance": "native", "shutdown_completed": True,
                  "service_lease_reacquired": True}
        predecessor.verify_service_lease(ready, closed, anchor, "image")
        for change in ({"service_lease_reacquired": False}, {"shutdown_completed": False}, {"instance": "foreign"}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                predecessor.verify_service_lease(ready, closed | change, anchor, "image")
        with self.assertRaises(ValueError):
            predecessor.verify_service_lease(ready, closed, anchor, "foreign")

    def test_phase_12_does_not_turn_partial_rows_or_disposal_into_success(self):
        events = ([{"kind": kind} for _ in range(3) for kind in ("window_grant", "cal_question", "window_release")]
                  + [{"kind": "window_grant"}, {"kind": "session_event",
                                               "event": {"kind": "session_lost", "code": "session_disposed"}}])

        def transcript(rows):
            return ("\n".join(map(json.dumps, rows)) + "\noriginal phase or fixture no longer available\n").encode()

        predecessor.verify_transcript(transcript(events))
        closed = events[:-1] + [{"kind": "phase_complete"}, events[-1]]
        expired = deepcopy(events)
        expired[-1]["event"]["code"] = "session_closed"
        for rows in (closed, expired, events[1:]):
            with self.subTest(rows=len(rows)), self.assertRaises(ValueError):
                predecessor.verify_transcript(transcript(rows))

    def test_control_recheck_is_guarded_and_preserves_anchor(self):
        control = predecessor.ANCHOR / "control"
        before = {"control_root": str(control)}
        verified = {"files": {}, "directories": [], "control_root": str(control)}
        with (patch.object(predecessor.Path, "resolve", return_value=control),
              patch.object(predecessor, "InstallationMutex", return_value=nullcontext()) as mutex,
              patch.object(predecessor, "inventory", return_value=verified) as inventory):
            self.assertEqual(predecessor.verify_control(before), verified)
            mutex.assert_called_once_with()
            self.assertEqual(inventory.call_args.args[:2], (control, before))
            with self.assertRaises(ValueError):
                predecessor.verify_control({"control_root": "different"})


if __name__ == "__main__":
    unittest.main()
