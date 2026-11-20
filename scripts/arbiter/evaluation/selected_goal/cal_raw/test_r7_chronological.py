# SPDX-License-Identifier: Apache-2.0
"""Model-free refusal controls for the new R7 chronology and per-question scopes."""
import copy
from pathlib import Path
from types import SimpleNamespace
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_chronological_inputs as owner


class R7Projection(unittest.TestCase):
    def setUp(self):
        self.plan_ref = {"sha256": "7"*64}
        self.ids = [[f"cal-{8*n+i}" for i in range(8)] for n in range(7)]
        self.reports = [
            {"phase": n, "plan_sha256": self.plan_ref["sha256"], "input_audit_sha256": owner.AUDIT,
             "certificate_count": 0, "rows": [{"ordinal": 8*n+i, "question_id": key, "captures": []}
                                            for i, key in enumerate(ids)]}
            for n, ids in enumerate(self.ids)
        ]
        self.observations = [{"id": key, "captured": [], "answer": {"refused": True}}
                             for ids in self.ids for key in ids]
        self.phases = [{"path": f"phase-{n:02}.json", "sha256": f"{n}"*64} for n in range(7)]

    def project(self):
        return owner.batches(self.plan_ref, {"phase_ids": self.ids}, self.reports, self.observations, self.phases)

    def test_exact_r7_census_preserves_all_refusals_and_compiler_inputs(self):
        result = self.project()
        self.assertEqual(set(result), {"compact", "large"})
        for batch in result.values():
            self.assertEqual(batch["original_requests"], 56)
            self.assertEqual(batch["statuses"], {"no_native_assessment_request": 56})
            self.assertEqual(batch["launch_audit_sha256"], owner.AUDIT)
            self.assertEqual(batch["phase_plan_sha256"], self.plan_ref["sha256"])
            self.assertEqual(batch["phases"], self.phases)
            self.assertEqual([row["id"] for row in batch["rows"]], [key for ids in self.ids for key in ids])
            self.assertTrue(all("compiled" not in row and row["captures"] == [] for row in batch["rows"]))

    def test_full100_keeps_all_four_final_phase_ordinals(self):
        self.ids = [[f"cal-{i}" for i in range(start, min(start+8,100))]
                    for start in range(0, 100, 8)]
        for phase in range(7, 13):
            ids = self.ids[phase]
            report = {"phase": phase, "plan_sha256": self.plan_ref["sha256"], "input_audit_sha256": owner.AUDIT,
                      "certificate_count": 0, "rows": [{"ordinal": 8*phase+i, "question_id": key, "captures": []}
                                                     for i, key in enumerate(ids)]}
            self.reports.append(report)
            self.observations.extend({"id": key, "captured": [], "answer": {"refused": True}} for key in ids)
            self.phases.append({"path": f"phase-{phase:02}.json", "sha256": "8"*64})
        result = self.project()
        self.assertEqual(result["compact"]["original_requests"], 100)
        self.assertEqual([row["ordinal"] for row in result["large"]["rows"][-4:]], [96,97,98,99])
        self.observations.pop()
        with self.assertRaises(ValueError):
            self.project()

    def test_prefix_refuses_second_lifetime_or_historical_attempt(self):
        from r6_phased_closure import ROOT, R7_ROOT, prefix_events
        for root, group in ((R7_ROOT, "lifetime-01"), (ROOT, "lifetime-00")):
            pin = {"path": str(root/group/"lifetime-closure.json")}
            with self.subTest(root=root, group=group), self.assertRaisesRegex(ValueError, "first R7 native lifetime"):
                list(prefix_events(root, pin, None, None, None, None, None, None, r6={"resume_r6": True}))

    def test_historical_audit_and_plan_refuse(self):
        for field, old in (("launch", owner.original.AUDIT), ("plan", owner.original.PLAN)):
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "historical or substituted"):
                self.setUp()
                self.reports[3]["input_audit_sha256" if field == "launch" else "plan_sha256"] = old
                self.project()

    def test_duplicate_swapped_and_missing_phase_refuse(self):
        for order in ([0, 1, 2, 4, 3, 5, 6], [0, 1, 2, 3, 4, 5, 5], [0, 1, 2, 3, 4, 5]):
            with self.subTest(order=order), self.assertRaisesRegex(ValueError, "reordered R7 phases"):
                self.setUp()
                self.reports = [self.reports[index] for index in order]
                self.project()

    def test_wrong_phase_owner_layout_refuses_before_reading_transcript(self):
        plan = {"phase_ids": self.ids}
        lifetime = {"plan": self.plan_ref, "phases": [0, 1, 3, 2, 4, 5, 6], "phase_closures": self.phases}
        with patch.object(owner, "read", return_value=lifetime), patch.object(owner, "phase_report") as transcript:
            with self.assertRaisesRegex(ValueError, "phase custody"):
                owner.reports_for(self.plan_ref, plan, [{"sha256": "8"*64}], self.observations)
            transcript.assert_not_called()

    def test_observation_from_different_ordinal_refuses(self):
        self.observations[16], self.observations[17] = self.observations[17], self.observations[16]
        with self.assertRaisesRegex(ValueError, "changed question ID"):
            self.project()


class QuestionScopes(unittest.TestCase):
    def setUp(self):
        self.scope = {"selection": {"owner": "window", "catalog_revision": 8, "scope_identity": "8"*64},
                      "fingerprint": "1"*64}
        self.publication = {"token": "original"}
        self.binding = SimpleNamespace(observation=self.scope, publication=self.publication)
        self.result = {"scope_observation": self.scope, "publication": self.publication}
        self.evidence = {"scope": self.result, "answer": {"refused": False, "body": self.result},
                         "captured": [], "probes": []}

    def test_historical_catalog_revision_or_branch_scope_rejected(self):
        owner.check_scoped_evidence(self.evidence, self.binding, {})
        for changed in ({"catalog_revision": 4}, {"scope_identity": "4"*64}):
            evidence = copy.deepcopy(self.evidence)
            wrong_scope = self.scope | {"selection": self.scope["selection"] | changed}
            evidence["answer"]["body"] = self.result | {"scope_observation": wrong_scope}
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, "selected source revision differs"):
                owner.check_scoped_evidence(evidence, self.binding, {})

    def test_delivered_probe_cannot_borrow_another_window_or_publication(self):
        for changed in ({"scope_observation": self.scope | {"fingerprint": "2"*64}},
                        {"publication": {"token": "foreign"}}):
            evidence = copy.deepcopy(self.evidence)
            evidence["probes"] = [{"result": {"refused": False, "body": self.result | changed}}]
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                owner.check_scoped_evidence(evidence, self.binding, {})


if __name__ == "__main__":
    unittest.main()
