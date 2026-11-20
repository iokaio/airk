# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the retained R8 certificate reader."""

import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r8_capture as capture


def ref(path, sha):
    return {"path": str(path), "bytes": 17, "sha256": sha}


class R8CaptureTest(unittest.TestCase):
    def setUp(self):
        self.plan = ref(capture.ROOT / "lifetime-plan.json", capture.PLAN_SHA)

    def test_foreign_partial_and_reordered_receipt_pins_fail_before_read(self):
        outer, fixture, release = capture.TERMINAL_SHAS
        cases = (
            (self.plan | {"path": "other.json"}, capture.LIFETIME_SHAS, capture.TERMINAL_SHAS),
            (self.plan | {"sha256": "0" * 64}, capture.LIFETIME_SHAS, capture.TERMINAL_SHAS),
            (self.plan, capture.LIFETIME_SHAS[:1], capture.TERMINAL_SHAS),
            (self.plan, capture.LIFETIME_SHAS[::-1], capture.TERMINAL_SHAS),
            (self.plan, capture.LIFETIME_SHAS, (outer, release, fixture)),
            (self.plan, capture.LIFETIME_SHAS, (outer, fixture, "0" * 64)),
        )
        for plan, lifetimes, terminal in cases:
            with self.subTest(plan=plan, lifetimes=lifetimes, terminal=terminal), patch.object(capture, "read") as read:
                with self.assertRaisesRegex(ValueError, "exact complete R8 receipt chain"):
                    capture.collect(plan, lifetimes, terminal)
                read.assert_not_called()

    def test_terminal_requires_native_job_settlement_and_original_population(self):
        outer = {
            "argv": [
                "python.exe", "-I", "-B", "-S", str(capture.CAPTURE / "grouped/r8_full_capture.py"),
                "--output", str(capture.ROOT), "--node", "C:\\Program Files\\nodejs\\node.exe",
                "--owned-plan-sha256", capture.PLAN_SHA,
            ],
            "exit_code": 0, "timed_out": False, "log_oversized": False, "child_assigned_before_resume": True,
            "owned_descendants_settled": True, "owned_job_forced_termination": False,
            "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
        }
        lifetimes = [ref(capture.ROOT / f"lifetime-{i:02}/lifetime-closure.json", sha)
                     for i, sha in enumerate(capture.LIFETIME_SHAS)]
        fixture = {"schema": "airk.private.cal100-r8-two-lifetime-fixture/1", "plan": self.plan,
                   "lifetimes": lifetimes, "ordered_questions": 100}
        with patch.object(capture, "reference", side_effect=ref):
            with patch.object(capture, "read", side_effect=(outer, fixture)):
                self.assertEqual(capture.verify_terminal(self.plan, capture.LIFETIME_SHAS, capture.TERMINAL_SHAS), fixture)
            cases = (
                (outer | {"owned_job_final_member_pids": [1]}, fixture),
                (outer | {"argv": outer["argv"][:-1] + ["unbound"]}, fixture),
                (outer, fixture | {"lifetimes": lifetimes[::-1]}),
                (outer, fixture | {"ordered_questions": 99}),
            )
            for bad_outer, bad_fixture in cases:
                with self.subTest(outer=bad_outer, fixture=bad_fixture), patch.object(
                        capture, "read", side_effect=(bad_outer, bad_fixture)):
                    with self.assertRaisesRegex(ValueError, "incomplete, reordered or foreign"):
                        capture.verify_terminal(self.plan, capture.LIFETIME_SHAS, capture.TERMINAL_SHAS)

    def test_native_event_sha_follows_correct_question(self):
        directory = capture.ROOT / "lifetime-01"
        owner = {"transcript": ref(directory / "phase-08.jsonl", "1" * 64)}
        events = [
            {"kind": "window_grant", "question_id": "first"},
            {"kind": "cal_question", "evidence": {"id": "first"}},
            {"kind": "window_release", "question_id": "first"},
            {"kind": "window_grant", "question_id": "second"},
            {"kind": "native_observation", "name": "record-0007.json", "sha256": "2" * 64},
            {"kind": "cal_question", "evidence": {"id": "second"}},
            {"kind": "window_release", "question_id": "second"},
        ]
        raw = "\n".join(json.dumps(event) for event in events).encode()
        with patch.object(capture, "checked", return_value=raw) as checked, patch.object(capture, "reference", side_effect=ref):
            self.assertEqual(capture.observation_refs(directory, owner), {
                "second": [ref(directory / "fixture/context-cal-admissions/record-0007.json", "2" * 64)],
            })
            checked.assert_called_once_with(owner["transcript"])

    def test_complete_roster_counters_and_common_clock(self):
        ids = [f"original-{i}" for i in range(100)]
        observations = [({"id": key, "pipeline_searches": 2, "probes": [None]}, None) for key in ids]
        instances, fingerprints = set(range(7)), set(range(104))
        plan = {"start_ms": 700000, "setup_elapsed_ms": 545806, "deadline_ms": 10954194, "common_ms": 10800000}
        fixture = {"pipeline_searches": 200, "probes": 100, "elapsed_ms": 645806}
        capture.check_complete(plan, fixture, observations, ids, instances, fingerprints, 800000)
        cases = (
            (fixture, observations[::-1], fingerprints, 800000),
            (fixture, observations[:-1], fingerprints, 800000),
            (fixture, observations, fingerprints - {103}, 800000),
            (fixture | {"probes": 99}, observations, fingerprints, 800000),
            (fixture | {"pipeline_searches": 0}, observations, fingerprints, 800000),
            (fixture | {"elapsed_ms": 10800001}, observations, fingerprints, 800000),
            (fixture, observations, fingerprints, 10954195),
        )
        for bad_fixture, bad_observations, bad_fingerprints, last in cases:
            with self.subTest(fixture=bad_fixture, fingerprints=len(bad_fingerprints), last=last):
                with self.assertRaisesRegex(ValueError, "incomplete R8 population"):
                    capture.check_complete(plan, bad_fixture, bad_observations, ids, instances, bad_fingerprints, last)


if __name__ == "__main__":
    unittest.main()
