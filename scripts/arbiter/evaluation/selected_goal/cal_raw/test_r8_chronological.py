# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the completed original R8 chronological projection."""

from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r8_chronological_inputs as owner


class ProjectionTests(unittest.TestCase):
    def setUp(self):
        self.ids = [[f"question-{index:03}" for index in range(start, min(start + 8, 100))]
                    for start in range(0, 100, 8)]
        self.rows = [{"id": key, "captured": [], "capture_refs": [], "probes": [],
                      "pipeline_searches": 0, "answer": {"refused": True}}
                     for keys in self.ids for key in keys]
        self.plan_ref = {"sha256": owner.PLAN}
        self.plan = {"phase_ids": self.ids, "input_audit": {"sha256": owner.AUDIT}}
        self.lifetimes = [{"sha256": sha} for sha in owner.LIFETIMES]
        self.readers = {}
        for group, phases in enumerate((range(7), range(7, 13))):
            directory = owner.R8_ROOT / f"lifetime-{group:02}"
            self.readers[directory / "lifetime-closure.json"] = {
                "plan": self.plan_ref, "phases": list(phases),
                "phase_closures": [{"sha256": f"{phase:064x}"} for phase in phases],
            }
            for phase in phases:
                self.readers[directory / f"phase-{phase:02}.owned.json"] = {
                    "phase": phase, "transcript": {"path": str(directory / f"phase-{phase:02}.jsonl"),
                                                    "sha256": f"{phase + 90:064x}"},
                }

    def reports(self):
        with patch.object(owner, "read", side_effect=lambda ref, path: self.readers[path]) as reads:
            result = owner.reports_for(self.plan_ref, self.plan, self.lifetimes, self.rows)
            self.assertEqual(reads.call_count, 15)
            return result

    def test_both_models_retain_100_original_ordinals_and_refusals(self):
        reports = self.reports()
        self.assertEqual([r["closed_questions"] for r in reports], [8] * 12 + [4])
        phases = [{"path": f"phase-{phase}.json"} for phase in range(13)]
        result = owner.original.batches(self.plan_ref, self.plan, reports, self.rows, phases)
        self.assertEqual(set(result), {"compact", "large"})
        for model in result.values():
            self.assertEqual(model["original_requests"], 100)
            self.assertEqual(model["statuses"], {"no_native_assessment_request": 100})
            self.assertEqual([row["ordinal"] for row in model["rows"][-4:]], [96, 97, 98, 99])
            self.assertEqual(model["phase_plan_sha256"], owner.PLAN)

    def test_prefix_reordered_or_historical_terminal_refused_before_reader(self):
        reader = Mock()
        for lifetimes, terminal in ((owner.LIFETIMES[:1], owner.TERMINAL),
                                    (owner.LIFETIMES[::-1], owner.TERMINAL),
                                    (owner.LIFETIMES, owner.TERMINAL[::-1])):
            with self.subTest(lifetimes=lifetimes, terminal=terminal), self.assertRaisesRegex(ValueError, "completed R8"):
                owner.collect(owner.PLAN, lifetimes, terminal, reader=reader)
        reader.assert_not_called()

    def test_single_certificate_read_feeds_both_models(self):
        audit = {"inputs": {"service": {"sha256": owner.SERVICE}}}
        pairs = iter((row, object()) for row in self.rows)
        reader = Mock(return_value=(self.plan, audit, pairs))
        with patch.object(owner, "reference", side_effect=lambda path, sha: {"sha256": sha}), \
                patch.object(owner.original, "check_scoped_evidence") as scope, \
                patch.object(owner, "read", side_effect=lambda ref, path: self.readers[path]):
            _, _, reports, observed = owner.collect(owner.PLAN, owner.LIFETIMES, owner.TERMINAL, reader=reader)
        self.assertEqual(scope.call_count, 100)
        reader.assert_called_once_with(self.plan_ref, owner.LIFETIMES, owner.TERMINAL)
        self.assertEqual(observed, self.rows)
        self.assertEqual(len(reports), 13)

    def test_missing_or_swapped_original_rows_refused(self):
        self.rows[55], self.rows[56] = self.rows[56], self.rows[55]
        with self.assertRaisesRegex(ValueError, "reordered completed R8"):
            owner.reports_for(self.plan_ref, self.plan, self.lifetimes, self.rows)

    def test_original_capture_pin_keeps_native_lifetime_and_digest(self):
        directory = owner.R8_ROOT / "lifetime-01/fixture/context-cal-admissions"
        pin = {"path": str(directory / "record-0001.json"), "bytes": 123, "sha256": "c" * 64}
        self.rows[56]["capture_refs"] = [pin]
        self.rows[56]["captured"] = [{"ordinal_in_private_run": 1}]
        self.assertEqual(self.reports()[7]["rows"][0]["captures"], [pin])
        pin["path"] = str(owner.R8_ROOT / "lifetime-00/fixture/context-cal-admissions/record-0001.json")
        with patch.object(owner, "read", side_effect=lambda ref, path: self.readers[path]), \
                self.assertRaisesRegex(ValueError, "different native lifetime"):
            owner.reports_for(self.plan_ref, self.plan, self.lifetimes, self.rows)


if __name__ == "__main__":
    unittest.main()
