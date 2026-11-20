# SPDX-License-Identifier: Apache-2.0
"""Check compiler inputs without opening a native image or launching a Job."""
import unittest
from unittest.mock import patch

import run_r7_compiler_batch as batch


class R7CompilerInputs(unittest.TestCase):
    def setUp(self):
        self.path = batch.CAMPAIGN / "first56-test/projection-receipt.json"
        self.pin = {"path": str(self.path), "sha256": "0" * 64}
        self.audit = {"path": "separate-audit.json", "sha256": batch.AUDIT}
        roster = [{"path": f"phase-{ordinal}.json", "sha256": str(ordinal) * 64} for ordinal in range(7)]
        self.models = {}
        for model, (model_id, manifest) in batch.original.MODELS.items():
            self.models[model] = {
                "model": model_id, "original_manifest": manifest, "launch_audit_sha256": batch.AUDIT,
                "phase_plan_sha256": "plan", "NN_invocations": 0, "original_requests": 56, "phases": list(roster),
                "statuses": {"eligible": 50, "no_native_assessment_request": 6},
                "rows": [{"ordinal": ordinal} for ordinal in range(56)],
            }
        self.projection = {
            "schema": batch.SCHEMA, "service_sha256": batch.SERVICE, "launch_audit": self.audit,
            "source_sha256": batch.digest((batch.HERE / "r7_chronological_inputs.py").read_bytes()),
            "NN_invocations": 0, "gain_certificates": 0, "original_requests": 56, "terminal": None,
            "plan_sha256": "plan", "models": {
                model: {"path": str(self.path.with_name(model + ".json")), "sha256": "a" * 64}
                for model in self.models
            },
        }

    def read(self, pin, path=None):
        if pin == self.pin:
            return self.projection
        if pin == self.audit:
            return {"inputs": {"service": {"sha256": batch.SERVICE}}}
        if path is None or str(path) != pin["path"]:
            raise AssertionError("reader did not compare the expected path")
        return self.models[path.stem]

    def test_original_prefix_sets_the_exact_audit_and_reports_each_model(self):
        with patch.object(batch, "read", side_effect=self.read):
            env, expected = batch.projection_inputs(self.pin)
        self.assertEqual(env["AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256"], batch.AUDIT)
        self.assertEqual(expected["compact"], "ContextQwenCompact original_requests=56 native_recompiled=50 retained_refusals=6")
        self.assertEqual(expected["large"], "ContextQwenLarge original_requests=56 native_recompiled=50 retained_refusals=6")

    def test_refuses_a_historical_audit_or_advanced_phase(self):
        with patch.object(batch, "read", side_effect=self.read):
            self.audit["sha256"] = "1" * 64
            with self.assertRaisesRegex(ValueError, "foreign or incomplete"):
                batch.projection_inputs(self.pin)
            self.audit["sha256"] = batch.AUDIT
            self.models["large"]["phases"][0] = {"path": "phase-1.json", "sha256": "1" * 64}
            with self.assertRaisesRegex(ValueError, "batch differs"):
                batch.projection_inputs(self.pin)

    def test_refuses_an_ordinal_change_and_a_partial_terminal(self):
        with patch.object(batch, "read", side_effect=self.read):
            self.models["compact"]["rows"][-1]["ordinal"] -= 1
            with self.assertRaisesRegex(ValueError, "batch differs"):
                batch.projection_inputs(self.pin)
            self.projection["terminal"] = ["outer", "fixture", "release"]
            with self.assertRaisesRegex(ValueError, "foreign or incomplete"):
                batch.projection_inputs(self.pin)

    def test_rejects_cargo_image_without_a_settled_outer_job(self):
        build = {
            "passed": True, "inputs_unchanged": True, "exit_code": 0,
            "target_directory": str(batch.ROOT / "target"), "log": "large-image-r1.log",
            "groups": {"native": {"files": {batch.CAL: batch.digest((batch.ROOT / batch.CAL).read_bytes())}}},
        }
        owner = {
            "exit_code": 0, "child_assigned_before_resume": True, "owned_descendants_settled": False,
            "owned_job_final_active_processes": 1, "owned_job_final_member_pids": [101],
        }
        with patch.object(batch, "read", side_effect=[build, owner]):
            with self.assertRaisesRegex(ValueError, "settled build Job"):
                batch.emitted_image({}, {}, {}, batch.ROOT / "target/debug/deps/airk_inference-test.exe")
