# SPDX-License-Identifier: Apache-2.0
"""Exercise the R6 plan, common deadline and stopped-state admission without starting a fixture."""
import copy
import unittest
from unittest.mock import patch

import r6_capture as r6


class NativeOwner(unittest.TestCase):
    def test_real_source_and_retirement_plan_uses_complete_original_population(self):
        plan = r6.make_plan(1_000_000, None, None)
        self.assertEqual(plan["schema"], "airk.private.cal100-r6-two-lifetime-plan/1")
        self.assertEqual(plan["deadline_ms"], 11_254_194)
        self.assertIsNone(plan["clock_decision"])
        self.assertEqual(len(plan["sources"]), 23)
        self.assertEqual([sum(len(plan["phase_ids"][i]) for i in group) for group in plan["phase_groups"]], [56, 44])
        self.assertEqual(plan["physical_handoff"]["retired"]["sha256"], r6.r6_handoff.RETIREMENT)
        self.assertEqual(plan["installation"]["status"]["sha256"], r6.r6_handoff.IDLE)
        self.assertEqual(plan["input_audit"]["sha256"], r6.r5.AUDIT_SHA)
        self.assertEqual(plan["prior_attempt"]["sha256"], r6.policy.OUTER)

    def test_projected_worker_and_service_keep_original_clocks_and_select_fresh_epoch(self):
        output = r6.OUTPUT / "candidate-sources"
        digest = "e" * 64
        signed = r6.r6_candidate.projected({"audit": {"sha256": digest}}, output)
        worker = signed["worker.cjs"].decode()
        group = signed["group_service.py"].decode()
        self.assertEqual(worker.count(digest), 1)
        self.assertIn("ticket.deadline_ms-ticket.start_ms===10800000", worker)
        self.assertIn("check(125000)", worker)
        self.assertIn("start_ms+10800000", group)
        self.assertIn("timeout > 125", group)
        self.assertIn("time.monotonic()+120", group)
        self.assertIn(repr(str(output / "worker.cjs")), group)
        self.assertNotIn(r6.r6_candidate.OLD_AUDIT, worker)
        compile(group, "group_service.py", "exec")

    def test_candidate_audit_allows_only_the_original_shape_with_new_service(self):
        original = {"inputs": {"service": {"path": "old", "sha256": "a"}, "scopes": "original"},
                    "selector_check": ["old", "--list", "--exact", "test"], "census": ["original"]}
        image = {"path": "new", "sha256": "b"}
        candidate = copy.deepcopy(original)
        candidate["inputs"]["service"] = image
        candidate["selector_check"][0] = "new"
        r6.r6_candidate.equivalent(original, candidate, image)
        for bad in (dict(candidate, census=[]), dict(candidate, selector_check=["new", "--list"]),
                    dict(candidate, inputs={"service": image, "scopes": "other"})):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                r6.r6_candidate.equivalent(original, bad, image)

    def test_clipped_admission_requires_strictly_more_than_125_seconds(self):
        with patch.object(r6.time, "monotonic", return_value=500):
            for deadline in (499, 500, 624.999, 625):
                with self.subTest(deadline=deadline), self.assertRaises(ValueError):
                    r6.require_admission(deadline)
            r6.require_admission(625.001)

    def test_idle_profile_or_semantic_configuration_change_refuses(self):
        state = {"availability": "ready", "selected_profile": "model", "active_profile": "model",
                 "queued": 0, "running": 0}
        original = {"configuration": {"revision": 3, "sources": ["CAL"]}, "focus": state, "context": state}
        current = copy.deepcopy(original)
        current["configuration"]["revision"] = 13
        r6.validate_configuration(original, current)
        for k, value in (("availability", "warming"), ("selected_profile", "other"), ("queued", 1)):
            changed = copy.deepcopy(current)
            changed["focus"][k] = value
            with self.subTest(k=k), self.assertRaises(ValueError):
                r6.validate_configuration(original, changed)
        current["configuration"]["sources"] = ["TRAIN"]
        with self.assertRaises(ValueError):
            r6.validate_configuration(original, current)

    def test_short_or_refused_handoff_never_reaches_model_service(self):
        plan = {"installation_mutex": {}, "start_ms": 1, "deadline_ms": 126_000,
                "physical_handoff": {}, "installation": {}}
        with (patch.object(r6.r5, "linked", side_effect=lambda row: row),
              patch.object(r6.r5, "require_parent_owns"), patch.object(r6, "make_plan", return_value=plan),
              patch.object(r6.r5, "now_ms", return_value=1000), patch.object(r6.time, "monotonic", return_value=1),
              patch.object(r6.r6_handoff, "verify", side_effect=ValueError("physical retirement differs")) as check,
              patch.object(r6.r5, "run_group") as model):
            with self.assertRaisesRegex(ValueError, "insufficient"):
                r6.execute(plan, None, None)
            check.assert_not_called()
            plan["deadline_ms"] += 1
            with self.assertRaisesRegex(ValueError, "physical retirement"):
                r6.execute(plan, None, None)
            check.assert_called_once()
            model.assert_not_called()
