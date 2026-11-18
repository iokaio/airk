# SPDX-License-Identifier: Apache-2.0
"""Inert controls for the exact original Compact component adapter."""

import copy
from datetime import datetime, timezone
import math
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import Mock


ROOT = Path(__file__).resolve().parents[2]
M = SimpleNamespace(**runpy.run_path(str(ROOT / "scripts/cuda-component-execution/fresh_context.py")))
FINGERPRINTS = {
    128: ("all-live-1", "791067affc1eeba53660a0044369f22f1f463535656419c5da9a486517222c7a"),
    8192: ("native-all-live-8192", "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0"),
}
FIXTURE_SHA = "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd"
REFERENCE_SHA = "c48e88b5d6bd8dff20b052bedebf0a4a48354d57caa1d2944b2dda16281805b2"


def pin(name, sha=None):
    return {"path": str(ROOT / name), "bytes": 5, "sha256": sha or "1" * 64}


def exact(value, expected, message):
    if type(value) is not type(expected) or value != expected:
        raise ValueError(message)


def finite(value, lower=0, upper=math.inf):
    return type(value) in (int, float) and math.isfinite(value) and lower <= value <= upper


def inputs(tokens):
    name, digest = FINGERPRINTS[tokens]
    ids = [17] * tokens
    row = {"name": name, "input": {"input_ids": ids, "attention_mask": [1] * tokens,
          "position_ids": list(range(tokens)), "decision_position": tokens - 1,
          "candidate_positions": list(range(32, 40)), "option_mask": [True] * 8, "input_digest": digest},
          "reference_same_artifact": [float(i) for i in range(8)],
          "reference_original_fp32": [float(i) for i in range(8)]}
    native = {"numeric_fixture": pin("fixture", FIXTURE_SHA), "source_manifest": pin("source")}
    fixture = {"schema": "airk.arbiter.native-numeric/2", "model": "arbiter-context-qwen3-17b",
               "reference_producer": pin("reference", REFERENCE_SHA), "rows": [{}, {}, {}]}
    fixture["rows"][1 if tokens == 128 else 2] = row
    return native, fixture, row


def result(tokens):
    native, fixture, row = inputs(tokens)
    component = SimpleNamespace(BOUNDS={"reviewed": True}, SID="verified", finite=finite, exact=exact)
    component.base = lambda: SimpleNamespace(core_pin_matches=lambda actual, expected: actual == expected)
    plan = {"request": pin("request"), "native_input": pin("input"), "descriptor": pin("descriptor"),
            "native_image": pin("image")}
    profile = {"manifest": "profile", "runtime": "private",
               "device_execution": {"host_fit": {"minimum_physical_cores": 8}}}
    node = {"name": "node", "domain": "", "op": "Add", "ep": "CUDAExecutionProvider", "subgraph_index": 0}
    adapter = {"uuid": [1] * 16, "ordinal": 0, "compute_major": 8, "compute_minor": 6,
               "driver_api_version": 12080, "driver_image_sha256": "1" * 64, "epoch": 1,
               "physical_total_bytes": 24 << 30, "total_bytes": 24 << 30, "free_bytes": 16 << 30}
    value = {"schema": "airk.private.cuda-component-result/1", "complete": True, "qualified": False,
             "product_admission": False, "measured_peak_device_bytes": None, "request": plan["request"],
             "native_input": plan["native_input"], "runtime_manifest": plan["descriptor"],
             "fixture": native["numeric_fixture"], "reference_producer": fixture["reference_producer"],
             "source_manifest": native["source_manifest"], "profile": "profile", "model": "arbiter-context-qwen3-17b",
             "pid": 42, "execution_user": "verified", "native_binary_sha256": "1" * 64, "bounds": component.BOUNDS,
             "native_host_reservation_bytes": 16 << 30, "actual_job": {"process_memory_limit_bytes": 16 << 30,
             "job_memory_limit_bytes": 24 << 30, "flags": 0x2308}, "native_control_retained_for_unknown_cuda": False,
             "native_control_cleanup_observed": True, "native_host_charge_release_observed": True,
             "process_cuda_poisoned": False, "native_control_cleanup_error": None, "final_host_peak_error": None,
             "peak_process_rss_bytes": 4096, "elapsed_us_to_final_report_start": 50, "load_us": 24,
             "final_placement": {"scope": "onnxruntime-final-session-state-ep-placement", "complete": True,
             "shim_status": 0, "ort_error_code": 0, "nodes": [node], "reported_nodes": 1,
             "reported_string_bytes": sum(len(node[k]) for k in ("name", "domain", "op", "ep"))},
             "execution_identity": ["private/device-execution/" + "1" * 64, 8],
             "adapter_before": adapter, "adapter_after": dict(adapter),
             "original_expires_monotonic_ns": 1200,
             "rows": [{"name": row["name"], "tokens": tokens, "input_digest": row["input"]["input_digest"],
                        "raw_scores": list(row["reference_same_artifact"]), "argmax_matches": True,
                        "same_artifact_raw_error": 0, "same_artifact_mass_error": 0, "original_fp32_raw_error": 0,
                        "original_fp32_mass_error": 0, "elapsed_us": 8, "native_call_elapsed_us": 7}]}
    return value, plan, native, profile, fixture, component


def journal(value, ordinal):
    identity = {"input_sha256": value["request"]["sha256"], "profile": value["profile"],
                "process_id": 42, "native_binary_sha256": "1" * 64}
    return [
        {"schema": "airk.arbiter.native-operation/1", "sequence": sequence, "identity": identity,
         "kind": "load" if sequence < 3 else "numeric", "ordinal": 0 if sequence < 3 else ordinal,
         "phase": "returned" if sequence % 2 == 0 else "started", "clock": "windows_performance_counter",
         "operation_input_digest": None if sequence < 3 else value["rows"][0]["input_digest"],
         "started_monotonic_ns": 100 if sequence < 3 else 200, "observed_monotonic_ns": observed,
         "expires_monotonic_ns": 1200, "native_success": True if sequence % 2 == 0 else None,
         "native_call_elapsed_us": 1 if sequence % 2 == 0 else None}
        for sequence, observed in enumerate((100, 101, 200, 201), 1)
    ]


class ContextControls(unittest.TestCase):
    def test_exact_original_rows_and_all_live_tensors(self):
        for tokens in (128, 8192):
            with self.subTest(tokens=tokens):
                native, fixture, row = inputs(tokens)
                self.assertIs(M.selected_row(native, fixture, tokens), row)
                row["input"]["attention_mask"][-1] = 0
                with self.assertRaisesRegex(ValueError, "all-live tensor"):
                    M.selected_row(native, fixture, tokens)
                row["input"]["attention_mask"][-1] = 1
                fixture["reference_producer"]["sha256"] = "2" * 64
                with self.assertRaisesRegex(ValueError, "independent reference producer"):
                    M.selected_row(native, fixture, tokens)

    def test_score_and_placement_gates(self):
        for tokens in (128, 8192):
            value, plan, native, profile, fixture, component = result(tokens)
            call = lambda item: M.validate_result(item, plan, 42, native, profile, fixture, component, tokens)
            call(value)
            changed = copy.deepcopy(value)
            changed["rows"][0]["raw_scores"][0] += .01
            with self.assertRaisesRegex(ValueError, "independent score replay"):
                call(changed)
            changed = copy.deepcopy(value)
            changed["rows"][0]["tokens"] = 17
            with self.assertRaisesRegex(ValueError, "selected Compact row identity"):
                call(changed)
            changed = copy.deepcopy(value)
            changed["final_placement"]["nodes"][0]["ep"] = "CPUExecutionProvider"
            with self.assertRaisesRegex(ValueError, "unexpected native graph placement"):
                call(changed)

    def test_journal_uses_actual_selected_ordinal_without_projection(self):
        for tokens, actual in ((128, 2), (8192, 3)):
            value, _, _, _, _, component = result(tokens)
            M.validate_operations(journal(value, actual), value, component, tokens)
            with self.assertRaisesRegex(ValueError, "native operation identity/order"):
                M.validate_operations(journal(value, 1), value, component, tokens)
            changed = journal(value, actual)
            changed[-1]["expires_monotonic_ns"] += 1
            with self.assertRaisesRegex(ValueError, "original operation clock"):
                M.validate_operations(changed, value, component, tokens)

    def test_fresh_graph_receipt_is_bound_to_current_request(self):
        graph_root = ROOT / "fixture-graph"
        base = SimpleNamespace(CAMPAIGN=datetime(2026, 11, 19, 5, 29, 42, tzinfo=timezone.utc))
        component = SimpleNamespace(finite=finite, LIMITS={"fixed": True}, SID="verified", ANCHOR=ROOT,
                                    GRAPH_SELECTOR="native-graph", DIAGNOSTIC_PROOFS={"native_result": 1 << 20,
                                    "stdout": 64 << 20}, execution_mode=lambda plan: "graph")
        plan = dict.fromkeys(M.GRAPH_LINKS, "current")
        plan.update(output_root=str(ROOT / "fixture-component"), native_image=pin("same-image"))
        graph = dict(plan, output_root=str(graph_root), input_seals={"checked": "sha"}, human_sid="verified",
                     limits=component.LIMITS, graph_provenance_owner=pin(M.GRAPH), fresh_lease_owner=pin("lease"),
                     fresh_bootstrap=pin("roster"), graph_preparation=pin("prepared"))
        graph_pin = pin("fixture-graph-plan")
        receipt_pin = pin(graph_root / "execution.json")
        receipt = {"schema": "airk.private.owned-cuda-graph-metadata-diagnostic/1", "complete": True,
                   "graph_metadata_observed": True, "diagnostic_only": True, "whole_phase_deadline_not_refreshed": True,
                   "qualification": False, "product_admission": False, "component_pass": False, "model_pass": False,
                   "whole_phase_expired": False, "failure": None, "post_input_drift": [], "final_input_errors": [],
                   "final_proof_errors": [], "final_proof_success": ["native_result", "stdout"], "elapsed_seconds": 13,
                   "execution_user_sid": "verified", "started_utc": "2026-11-18T12:00:00+00:00",
                   "finished_utc": "2026-11-18T12:00:13+00:00", "original_deadline": "2026-11-18T12:02:00+00:00",
                   "plan": graph_pin, "input_seals": {"checked": "sha", graph_pin["path"]: graph_pin["sha256"]},
                   "command": [plan["native_image"]["path"], "--exact", "native-graph", "--ignored", "--nocapture",
                               "--test-threads=1"], "graph_custody": {"preparation": graph["graph_preparation"],
                               "close_errors": [], "opened_handles": 1, "closed_handles": 1, "boundary": [None]},
                   "native_result": pin(graph_root / "native-result.json"),
                   "stdout": pin(graph_root / "stdout-stderr.log")}
        base.pinned = lambda row: row["path"]
        base.read = {receipt_pin["path"]: receipt, graph_pin["path"]: graph}.__getitem__
        self.assertEqual(M.graph_inputs(plan, base, receipt_pin, component)[2][0], receipt_pin)
        wrong = dict(plan, request=pin("other-request"))
        with self.assertRaisesRegex(ValueError, "another Compact source, model or request"):
            M.graph_inputs(wrong, base, receipt_pin, component)
        receipt["graph_custody"]["closed_handles"] = True
        with self.assertRaisesRegex(ValueError, "settled graph input custody"):
            M.graph_inputs(plan, base, receipt_pin, component)
        receipt["graph_custody"]["closed_handles"] = 1
        receipt["whole_phase_expired"] = True
        with self.assertRaisesRegex(ValueError, "successful original owned fresh graph"):
            M.graph_inputs(plan, base, receipt_pin, component)

    def test_preservation_seals_are_additive_and_execution_refuses_missing_pins(self):
        checked = pin("already-reviewed")
        candidate = {"input_seals": {}}
        with self.assertRaisesRegex(ValueError, "evidence source seal differs"):
            M.bind_plan(candidate, [checked], False)
        M.bind_plan(candidate, [checked], True)
        M.bind_plan(candidate, [checked], False)
        with self.assertRaisesRegex(ValueError, "evidence source seal differs"):
            M.bind_plan(candidate, [dict(checked, sha256="2" * 64)], True)

    def test_private_install_keeps_frozen_owner_dispatch(self):
        frozen = Mock()
        component = SimpleNamespace(input_facts=Mock(), preflight=Mock(), run_loaded=frozen)
        M.install(component, SimpleNamespace(tokens=8192, prepare=False), pin("self"), pin("contract"), 1)
        self.assertIs(component.run_loaded, frozen)
        self.assertEqual(component.SCHEMA, "airk.private.cuda-fresh-context-component-plan/1")
        self.assertTrue(component.SELECTOR.endswith("cuda_original_context_fp32_t8192_component"))

    def test_live_frozen_binding_retains_real_dispatch_and_is_private(self):
        raw = M.source(M.COMPONENT, M.COMPONENT_SHA)
        M.source(M.NATIVE, M.NATIVE_SHA)
        component = M.private_module(M.COMPONENT, raw, "compact_control")
        frozen = M.private_module(M.COMPONENT, raw, "original_compact_control")
        dispatch = component.run_loaded
        M.install(component, SimpleNamespace(tokens=128, prepare=False), pin("self"), pin("contract"), 1)
        plan = {"schema": "airk.private.cuda-fresh-context-component-plan/1",
                "selector": "inference_engine::pool::arbiter::qualification::software_v2::"
                            "cuda_qualification::cuda_original_context_fp32_t128_component"}
        self.assertIs(component.run_loaded, dispatch)
        self.assertIs(component.execution_mode(plan), False)
        with self.assertRaisesRegex(ValueError, "exact Context17"):
            frozen.execution_mode(plan)
        self.assertEqual(component.LIMITS, frozen.LIMITS)


if __name__ == "__main__":
    unittest.main()
