# SPDX-License-Identifier: Apache-2.0
"""Model-free tests for the isolated R7 phase 11 timing owner."""

from copy import deepcopy
import json
import unittest

import r8_phase_timing as timing


def job():
    return {"exit_code": 0, "child_assigned_before_resume": True, "owned_descendants_settled": True,
            "timed_out": False, "log_oversized": False, "owned_job_forced_termination": False,
            "owned_job_final_active_processes": 0, "owned_job_final_member_pids": []}


def plans():
    ids = [list(range(i, min(i + 8, 100))) for i in range(0, 100, 8)]
    original = {"phase_ids": ids, "recorder": {"admission_limit": 1024},
                "input_audit": {"sha256": "old-audit"},
                "installation": {"ready": "R3 ready", "closed": "R3 closed", "status": "older idle"}}
    proof = {"native_restart": {"status": "R7 idle"}, "scope_baseline": "R7 last binding",
             "receipts": {"plan": "R7 plan"}, "capture_complete": False}
    plan = {
        "schema": "airk.private.cal100-phase11-timing-plan/1", "phase_groups": [[11]], "phase_ids": ids,
        "start_ms": 1000, "deadline_ms": 1_801_000, "requested_call_ms": 120_000, "delivery_grace_ms": 5000,
        "recorder": {"admission_limit": 1024}, "input_audit": {"sha256": "old-audit"},
        "installation": {"ready": "R3 ready", "closed": "R3 closed", "status": "R7 idle"},
        "scope_baseline": "R7 last binding", "original_plan": "R7 plan", "calibration_fitted": False,
    }
    return plan, original, proof


class TimingTests(unittest.TestCase):
    def test_one_explicit_closure_edit(self):
        base = (timing.HERE / "group_service.py").read_bytes()
        result = timing.diagnostic_group(base)
        self.assertEqual(result.count(b"phase-11-timing-closure.json"), 1)
        self.assertNotIn(b"lifetime-closure.json", result)
        self.assertNotIn(b"airk.private.cal100-group-lifetime/1", result)
        self.assertEqual(result.replace(b"phase-11-timing-closure.json", b"lifetime-closure.json").replace(
            b"airk.private.cal100-phase11-timing-closure/1", b"airk.private.cal100-group-lifetime/1"), base)
        for changed in (b"", base + base):
            with self.assertRaises(ValueError):
                timing.diagnostic_group(changed)

    def test_original_phase_order_and_clock_are_retained(self):
        timing.validate_plan(*plans())
        for name, value in (
            ("phase_groups", [[0]]), ("phase_groups", [[11, 12]]), ("deadline_ms", 1_801_001),
            ("requested_call_ms", 119_999), ("delivery_grace_ms", 4999), ("scope_baseline", "older binding"),
            ("input_audit", {"sha256": "different"}), ("installation", {"ready": "restart Ready instead of R3 anchor"}),
        ):
            plan, original, proof = plans()
            plan[name] = value
            with self.subTest(name=name, value=value), self.assertRaises(ValueError):
                timing.validate_plan(plan, original, proof)

    def test_retains_ordered_native_arguments_without_exposing_response(self):
        sequence = [("status", {"response_mode": "full"}), ("answer", {"budget": {"deadline_ms": 120000}})]
        transcript = []
        for tool, args in sequence:
            transcript.append({"kind": "native_exchange",
                               "argv": ["tools", "call", tool, "--args", json.dumps(args), "--output", "json"],
                               "result": {"stdout": "not needed"}})
        raw = "\n".join(json.dumps(row) for row in transcript).encode()
        self.assertEqual(timing.exchange_roster(raw), sequence)
        calls = [(tool, {"i": i}) for tool, count in (("status", 8), ("answer", 8), ("context", 11), ("search", 29))
                 for i in range(count)]
        self.assertNotEqual(timing.roster_digest(calls), timing.roster_digest(calls[::-1]))
        changed = deepcopy(calls)
        changed[-1][1]["i"] = -1
        self.assertNotEqual(timing.roster_digest(calls), timing.roster_digest(changed))
        with self.assertRaises(ValueError):
            timing.roster_digest(calls[:-1])

    def test_timing_closure_cannot_be_an_earlier_or_scientific_lifetime(self):
        plan, _, _ = plans()
        ref = {"path": str(timing.CLOSURE)}
        result = {"schema": "airk.private.cal100-phase11-timing-closure/1", "group": 0, "phases": [11], "probes": 40,
                  "phase_closures": [{}], "instance": "new", "start_ms": 1010, "end_ms": 1250,
                  "dispatch": {"context_dispatches": 0}}
        phase = job() | {"phase": 11, "start_ms": 1010, "end_ms": 1200, "deadline_ms": 1801000}
        fingerprints, prior, current = set(range(8)), {"R3", "R7"}, {"R3", "R7", "new"}
        timing.validate_result(plan, ref, result, phase, fingerprints, prior, current)
        for changed in ({"schema": "airk.private.cal100-group-lifetime/1"}, {"phases": [0]}, {"probes": 39},
                        {"instance": "R7"}, {"dispatch": {"context_dispatches": 1}}):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                timing.validate_result(plan, ref, result | changed, phase, fingerprints, prior, current)
        with self.assertRaises(ValueError):
            timing.validate_result(plan, ref, result, phase | {"timed_out": True}, fingerprints, prior, current)
        with self.assertRaises(ValueError):
            timing.validate_result(plan, {"path": "fixture-closure.json"}, result, phase, fingerprints, prior, current)


if __name__ == "__main__":
    unittest.main()
