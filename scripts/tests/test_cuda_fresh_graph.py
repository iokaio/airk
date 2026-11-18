# SPDX-License-Identifier: Apache-2.0
"""Inert fresh graph boundary and dispatch controls."""

import argparse
import datetime as dt
import hashlib
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
OWNER = ROOT / "scripts/cuda-component-execution"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fresh = load("inert_fresh_graph_owner", OWNER / "fresh_graph.py")
legacy = load("inert_legacy_graph_owner", OWNER / "graph_provenance.py")
host_policy = load("inert_compact_graph_host_policy", OWNER / "compact_graph_host.py")


def pin(label):
    path = str(ROOT / label)
    return {"path": path, "bytes": 1, "sha256": hashlib.sha256(path.encode()).hexdigest()}


def fixture():
    fields = ("wrapper", "custody_owner", "contract", "parent_python", "powershell", "checked_guard",
              "metadata_receipt", "image_packet", "native_build_receipt", "native_image", "request",
              "native_input", "profile", "descriptor", "graph_preserver", "graph_provenance_owner",
              "graph_provenance_contract", "graph_output_guard", "output_guard_lock",
              "fresh_bootstrap", "fresh_lease_owner")
    plan = {field: pin("fixture/" + field) for field in fields}
    plan["output_guard"] = plan["graph_output_guard"]
    plan["limits"] = {"wall_seconds": 120, "process_bytes": 17179869184, "job_bytes": 25769803776}
    binding = {field: pin("fixture/binding-" + field) for field in (
        "environment", "source_roster", "archive", "producer", "manifest", "runtime_producer",
        "cargo_json", "preserver_source", "aggregate_receipt", "aggregate_recipe",
        "configure_receipt", "configure_recipe", "build_receipt")}
    binding["schema"] = "airk.private.cuda-fresh-finalgraph-image-binding/1"
    binding["preserver_source"] = plan["graph_preserver"]
    binding["source_pins"] = [pin("scripts/fresh-example.py")]
    proof = {field: pin("fixture/proof-" + field) for field in
             ("producer_source", "collector_source", "source_lock", "cpu_lock")}
    proof.update(schema="airk.private.cuda-fresh-static-build-proof/1",
                 ninja=[pin("fixture/build.ninja")], provider_dlls=[pin("fixture/original-provider.dll")],
                 patch_inputs=[{"original": pin("fixture/provider.patch"), "snapshot": pin("fixture/applied.patch")}])
    binding["proof"] = proof
    helper_root = ROOT / "scripts/cuda-native-execution/helpers"
    helper = pin("scripts/cuda-native-execution/helpers/reference_worker.py")
    source = pin("fixture/model-source.json")
    model = pin("fixture/weights")
    numeric = pin("fixture/numeric.json")
    reference = pin("fixture/reference.json")
    original_plan = pin("fixture/metadata-plan")
    compile_log = pin("fixture/compile.log")
    configure_log = pin("fixture/configure.log")
    aggregate_log = pin("fixture/aggregate.log")
    older_native = pin("fixture/metadata-native-build")
    docs = {
        binding["producer"]["path"]: {
            "libraries": [pin("fixture/onnxruntime_session.lib"), pin("fixture/unused.lib")],
            "stages": [{"files": [compile_log]}], "stage_receipts": [pin("fixture/compile-stage.json")],
        },
        binding["configure_receipt"]["path"]: {"configure": {"files": [configure_log]}},
        pin("fixture/logs/configure.receipt.json")["path"]: {"files": [configure_log]},
        binding["aggregate_receipt"]["path"]: {
            "command": {"files": [aggregate_log]}, "stage_receipt": pin("fixture/aggregate-stage.json"),
            "response": pin("fixture/archive.rsp"),
        },
        plan["metadata_receipt"]["path"]: {"plan": original_plan, "outputs": [pin("fixture/profile.json")]},
        original_plan["path"]: {"native_build_receipt": older_native},
        plan["native_input"]["path"]: {
            "files": {"weights": model["path"]}, "source_manifest": source, "numeric_fixture": numeric,
        },
        numeric["path"]: {"reference_producer": reference},
    }
    plan.update(runtime_files=[pin("fixture/provider.dll")], ownership_helper_root=str(helper_root),
                required_native_sources={"crates/literal.rs": pin("crates/literal.rs")["sha256"]},
                input_seals={model["path"]: model["sha256"]})
    base = SimpleNamespace(pin=pin, MAINTAINED_HELPER_ROOT=helper_root,
                           HELPERS={"reference_worker.py": helper["sha256"]})
    return plan, binding, base, lambda row, _: docs[row["path"]]


def adapter(plan):
    component = SimpleNamespace(execution_mode=lambda _: "graph", run_loaded=Mock())
    approved = {"component": plan["wrapper"], "native": plan["custody_owner"],
                "preserver": plan["graph_preserver"], "roster": plan["fresh_bootstrap"],
                "lease": plan["fresh_lease_owner"]}
    clock = (10.0, dt.datetime(2026, 11, 18, tzinfo=dt.timezone.utc),
             dt.datetime(2026, 11, 18, 0, 2, tzinfo=dt.timezone.utc), 130.0)
    component.LIMITS = plan["limits"]
    return fresh.FreshGraph(component, legacy, Mock(), approved,
                            plan["graph_provenance_owner"], plan["graph_provenance_contract"], clock)


class FreshGraphTests(unittest.TestCase):
    def test_reviewed_32g_runner_requires_explicit_policy_and_actual_job(self):
        path = ROOT / "scripts/cuda-component-execution.py"
        raw = path.read_bytes()
        runner = host_policy.load_graph_runner(path, raw)
        self.assertEqual(runner.LIMITS, {"wall_seconds": 120, "process_bytes": 34359738368,
                                        "job_bytes": 34359738368, "log_bytes": 67108864,
                                        "cleanup_seconds": 5})
        self.assertEqual(runner.BOUNDS["host_limit_bytes"], 34359738368)
        self.assertEqual(runner.BOUNDS["device_limit_bytes"], 25769803776)

        limits = [34359738368, 34359738368]

        def query(_job, _kind, pointer, _size, _ret):
            current = pointer._obj
            current.process, current.job = limits
            current.basic.flags = 8968
            current.basic.active = 2
            return 1

        api = SimpleNamespace(QueryInformationJobObject=query,
                              SetInformationJobObject=Mock(return_value=1))
        self.assertEqual(runner.configure_job(api, 42), {"process_bytes": 34359738368,
                         "job_bytes": 34359738368, "flags": 8968, "active_limit": 2})
        limits[1] = 25769803776
        with self.assertRaisesRegex(ValueError, "suspended Job"):
            runner.configure_job(api, 42)
        with self.assertRaisesRegex(ValueError, "sealed producer"):
            host_policy.load_graph_runner(path, raw + b"\n")

        plan, binding, base, read = fixture()
        owner = adapter(plan)
        policy = pin("fixture/compact-host-policy.py")
        owner.approved["memory_policy"] = policy
        owner.component.LIMITS = runner.LIMITS
        with self.assertRaisesRegex(ValueError, "host policy"):
            owner.identify(plan)
        plan["compact_host_policy"] = policy
        plan["input_seals"][policy["path"]] = policy["sha256"]
        with self.assertRaisesRegex(ValueError, "host policy"):
            owner.identify(plan)
        plan["limits"] = runner.LIMITS
        owner.identify(plan)
        self.assertIn(policy, fresh.derive_boundary(legacy, plan, binding, base, read))

    def test_held_boundary_uses_current_sources_and_process_evidence(self):
        plan, binding, base, read = fixture()
        rows = fresh.derive_boundary(legacy, plan, binding, base, read)
        paths = {row["path"] for row in rows}
        for name in ("fixture/binding-aggregate_receipt", "fixture/binding-configure_recipe",
                     "fixture/provider.patch", "fixture/weights", "fixture/reference.json",
                     "fixture/compile.log", "fixture/configure.log", "fixture/aggregate.log",
                     "fixture/logs/configure.receipt.json",
                     "fixture/onnxruntime_session.lib", "fixture/metadata-native-build",
                     "scripts/cuda-native-execution/helpers/reference_worker.py"):
            self.assertIn(str(ROOT / name), paths)
        self.assertNotIn(str(ROOT / "fixture/unused.lib"), paths)
        known = {legacy.key(row["path"]): {"pin": row} for row in rows}
        self.assertEqual(fresh.derive_boundary(legacy, plan, binding, base, read, known=known), rows)
        del known[legacy.key(ROOT / "fixture/weights")]
        with self.assertRaisesRegex(ValueError, "omitted"):
            fresh.derive_boundary(legacy, plan, binding, base, read, known=known)

    def test_configure_stage_receipt_is_bounded_and_matches_on_preparation_and_reopen(self):
        plan, binding, base, read = fixture()
        stage = pin("fixture/logs/configure.receipt.json")
        observed = Mock(side_effect=read)
        rows = fresh.derive_boundary(legacy, plan, binding, base, observed)
        observed.assert_any_call(stage, fresh.MAX_CHAIN_JSON)
        known = {legacy.key(row["path"]): {"pin": row} for row in rows}
        base.pin = Mock(side_effect=AssertionError("held preparation must not reopen a path"))
        self.assertEqual(fresh.derive_boundary(legacy, plan, binding, base, read, known=known), rows)
        base.pin.assert_not_called()

        def changed(row, maximum):
            if row == stage:
                return {"files": [], "stage": "configure"}
            return read(row, maximum)

        with self.assertRaisesRegex(ValueError, "retained configure stage receipt differs"):
            fresh.derive_boundary(legacy, plan, binding, base, changed, known=known)
        del known[legacy.key(stage["path"])]
        with self.assertRaisesRegex(ValueError, "omitted a derived file"):
            fresh.derive_boundary(legacy, plan, binding, base, read, known=known)
        base.pin.assert_not_called()

    def test_cross_generation_and_conflicting_pins_refuse(self):
        plan, binding, base, read = fixture()
        prior = {**binding, "schema": "airk.private.cuda-finalgraph-image-binding/1"}
        with self.assertRaisesRegex(ValueError, "only fresh producer"):
            fresh.derive_boundary(legacy, plan, prior, base, read)
        binding["source_pins"].append({**plan["wrapper"], "sha256": "0" * 64})
        with self.assertRaisesRegex(ValueError, "conflicting"):
            fresh.derive_boundary(legacy, plan, binding, base, read)

    def test_replay_uses_held_current_verification_under_original_deadline(self):
        plan, binding, _, _ = fixture()
        owner = adapter(plan)
        lease = SimpleNamespace(deadline=130.0, pin=Mock(return_value=plan["output_guard"]))
        preserver = SimpleNamespace(verify_fresh_current=Mock(return_value={"observed": True}),
                                    verify_graph_binding=Mock(side_effect=AssertionError("full verification")))
        self.assertEqual(owner.replay(plan, binding, lease, preserver), {"observed": True})
        preserver.verify_fresh_current.assert_called_once_with(
            binding, plan["descriptor"], plan["native_image"], plan["native_build_receipt"], deadline=130.0)
        preserver.verify_graph_binding.assert_not_called()
        plan["output_guard"] = pin("fixture/unreviewed-policy")
        with self.assertRaisesRegex(ValueError, "output policy"):
            owner.replay(plan, binding, lease, preserver)

    def test_fresh_preserver_rebinds_pin_and_bytes_to_the_same_handles(self):
        plan, binding, _, _ = fixture()
        owner = adapter(plan)
        target = binding["configure_receipt"]
        lease = SimpleNamespace(deadline=130.0, pin=Mock(return_value=target), bytes=Mock(return_value=b"x"))
        preserver = SimpleNamespace()
        with patch.object(legacy, "held_preserver", return_value=preserver):
            self.assertIs(owner.held_preserver(lease, plan), preserver)
        self.assertEqual(preserver.fresh_reviewed_pin(target["path"], target["sha256"], 130.0), target)
        self.assertEqual(preserver.fresh_bytes(target, 2, 130.0), b"x")
        for changed, maximum, deadline in (
            ({**target, "sha256": "0" * 64}, 2, 130.0), (target, 0, 130.0), (target, 2, 131.0),
        ):
            with self.assertRaisesRegex(ValueError, "byte pin"):
                preserver.fresh_bytes(changed, maximum, deadline)
        lease.bytes.assert_called_once_with(target["path"], 2)

    def test_dispatch_passes_the_original_clock_and_exact_lease_to_the_unchanged_owner(self):
        plan, _, _, _ = fixture()
        plan["graph_preparation"] = pin("fixture/preparation.json")
        owner = adapter(plan)
        plan_pin = pin("fixture/execution-plan.json")
        lease = unittest.mock.MagicMock()
        lease.__enter__.return_value = lease
        prepared = {"fixture": True}
        args = argparse.Namespace(plan=Path(plan_pin["path"]), plan_sha256=plan_pin["sha256"])
        with patch.object(fresh, "captured", return_value=(plan_pin, b'{"inert":true}')), \
                patch.object(legacy, "strict_json", return_value=plan), \
                patch.object(fresh, "read_checked", return_value=prepared), \
                patch.object(owner, "open_prepared", return_value=lease) as opened:
            owner.run(args)
        opened.assert_called_once_with(plan, plan_pin, prepared, plan["graph_preparation"], 130.0)
        owner.component.run_loaded.assert_called_once_with(
            args, graph=True, graph_context=(*owner.clock, lease, owner, owner.output_base))
        lease.__exit__.assert_called_once()

    def test_exact_modes_refuse_a_mixed_cli(self):
        flags = ["--approved-script-sha256", "driver", "--approved-sources", "sources.json",
                 "--approved-sources-sha256", "sources", "--fresh-contract-sha256", "contract"]
        argv = ["--execute-reviewed-graph-diagnostic", "--plan", "plan.json", "--plan-sha256", "plan", *flags]
        self.assertTrue(fresh.parse(argv).execute_reviewed_graph_diagnostic)
        with patch("sys.stderr"), self.assertRaises(SystemExit):
            fresh.parse([*argv, "--graph-preparation", "preparation.json"])


if __name__ == "__main__":
    unittest.main()
