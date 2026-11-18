# SPDX-License-Identifier: Apache-2.0
"""Inert verification controls for the retained fresh CUDA aggregate lineage."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
from types import SimpleNamespace
import stat
import unittest
from unittest.mock import Mock, patch

SCRIPTS = Path(__file__).resolve().parents[1]
BUILD = SCRIPTS / "cuda-static-runtime/build"


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


verifier = load(BUILD / "fresh_verify.py", "fresh_verify_controls")
compiler = load(BUILD / "fresh_compile.py", "fresh_verify_compile_controls")
aggregate = load(BUILD / "fresh_aggregate.py", "fresh_verify_aggregate_controls")


def pin(path, raw=b"literal evidence"):
    return {"path": str(path), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def timestamp(minute, second):
    return f"2026-11-17T00:{minute:02d}:{second:02d}+00:00"


class FreshVerify(unittest.TestCase):
    def test_compile_replay_audits_every_path_under_one_deadline(self):
        paths = [Path(f"output-{index}") for index in range(769)]
        audit = Mock()
        output_guard = Mock(return_value=audit)
        owner = SimpleNamespace(output_guard=output_guard)
        compile_pin = {"path": "compile.json"}

        def verify(pin, deadline):
            self.assertEqual((pin, deadline), (compile_pin, 600))
            owner.output_guard("component", "recipe")(paths, True, deadline)
            return "verified"

        owner.verify_receipt = verify
        self.assertEqual(verifier.verify_compilation(owner, compile_pin, 600), "verified")
        output_guard.assert_any_call("component", "recipe", batch_size=32, workers=1)
        self.assertEqual(audit.call_count, 25)
        seen = []
        for call in audit.call_args_list:
            group, created, deadline = call.args
            self.assertTrue(0 < len(group) <= 32)
            seen.extend(group)
            self.assertEqual((created, deadline), (True, 600))
        self.assertCountEqual(seen, paths)
        self.assertIs(owner.output_guard, output_guard)

        def deny_later_batch(group, created, deadline):
            if paths[-1] in group:
                raise ValueError("output denied")

        audit.side_effect = deny_later_batch
        with self.assertRaisesRegex(ValueError, "output denied"):
            verifier.verify_compilation(owner, compile_pin, 600)
        self.assertIs(owner.output_guard, output_guard)

    def setUp(self):
        anchor = Path(__file__).resolve().parent / "inert-anchor"
        self.root = anchor / "inert-fresh-build"
        build = self.root / "b/Release"
        output = self.root / "gpu-aggregate/onnxruntime.lib"
        response = self.root / "gpu-aggregate/inputs.rsp"
        archiver = self.root / "toolchain/lib.exe"
        stage = self.root / "logs/fresh-aggregate"
        self.raw_pin = pin(self.root / "fresh-aggregate.json")
        self.compile_pin = pin(self.root / "fresh-compile.json")
        self.configure_pin = pin(self.root / "fresh-configure.json")
        recipe_pin = pin(self.root / "aggregate-recipe.json")
        configure_recipe_pin = pin(self.root / "configure-recipe.json")
        self.stage_pin = pin(stage.with_suffix(".receipt.json"))
        self.archive_pin = {**pin(output), "bytes": 100000}
        self.plan = {
            "producer_proof_sha256": "d" * 64,
            "inputs": [pin(build / f"input-{index:03d}.lib") for index in range(96)],
            "excluded_provider_imports": [{"module": "onnxruntime_providers_shared.dll",
                                           "pin": pin(build / "onnxruntime_providers_shared.lib")}],
        }
        self.response_bytes = "".join(f'"{entry["path"]}"\n' for entry in self.plan["inputs"]).encode()
        response_pin = pin(response, self.response_bytes)
        self.argv = [str(archiver), "/NOLOGO", "/OUT:" + str(output), "@" + str(response)]
        self.coff = {"kind": "static", "export_directives": [
            "airk_ort_collect_final_graph_metadata_v1", "airk_ort_collect_final_placement_v1",
        ]}
        self.owned = {
            "exit_code": 0, "timed_out": False, "log_oversized": False,
            "owned_descendants_settled": True, "owned_job_forced_termination": False,
            "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
            "parent_assigned_to_job": False, "child_assigned_before_resume": True,
        }
        self.command = {
            "actual_tree_result": {"start_utc": timestamp(3, 0), "finished_utc": timestamp(3, 5)},
            "owned": self.owned,
            "files": [pin(stage.with_suffix(f".{name}.log")) for name in ("stdout", "stderr", "outer")],
        }
        owners = {
            "driver": BUILD / "fresh_aggregate.py", "archive": SCRIPTS / "cuda-static-archive.py",
            "compile": BUILD / "fresh_compile.py", "pin_reader": SCRIPTS / "cuda-static-incremental.py",
            "projection": BUILD / "fresh_projection.py",
        }
        self.recipe = {
            "schema": "airk.private.cuda-fresh-aggregate-recipe/1", "repository_root": str(SCRIPTS.parent),
            "compile_receipt": self.compile_pin, "owners": {name: pin(path) for name, path in owners.items()},
            "archiver": pin(archiver), "wall_seconds": 100, "execution_authorized": True, "qualification": False,
        }
        self.receipt = {
            "schema": "airk.private.cuda-fresh-aggregate/1", "status": "AGGREGATED_UNQUALIFIED",
            "recipe": recipe_pin, "compile_receipt": self.compile_pin, "configure_receipt": self.configure_pin,
            "plan": self.plan, "plan_sha256": "d" * 64, "tool": self.recipe["archiver"], "argv": self.argv,
            "compile_verification": {
                "started_utc": timestamp(2, 10), "finished_utc": timestamp(2, 50),
                "started_monotonic": 130, "finished_monotonic": 170, "elapsed_seconds": 40,
                "original_deadline_monotonic": 300, "effective_deadline_monotonic": 290,
            },
            "started_utc": timestamp(2, 50), "started_monotonic": 170,
            "original_deadline_monotonic": 290, "deadline_monotonic": 270,
            "qualification": False, "error": None, "response": response_pin,
            "response_sha256": response_pin["sha256"], "command": self.command, "stage_receipt": self.stage_pin,
            "coff": self.coff, "output": {**self.archive_pin, "path": "gpu-aggregate/onnxruntime.lib"},
            "finished_utc": timestamp(3, 30), "finished_monotonic": 210, "elapsed_seconds": 40,
        }
        self.compiled = {
            "build_root": str(build), "configured": self.configure_pin, "recipe": configure_recipe_pin,
            "finished_utc": timestamp(2, 0), "original_deadline_monotonic": 300,
            "ninja": [pin(build / "build.ninja")],
            "providers": [pin(build / "onnxruntime_providers_cuda.dll"),
                          pin(build / "onnxruntime_providers_shared.dll")],
        }
        self.configured = {
            "started_utc": timestamp(0, 0), "finished_utc": timestamp(0, 10),
            "elapsed_seconds": 10, "configure": {"original_deadline_monotonic": 300},
        }
        cpu_path = SCRIPTS / "provision-static-onnx-runtime.py"
        guard_path = SCRIPTS / "cuda-component-execution.py"
        self.configure_recipe = {
            "fresh_output": str(self.root), "wall_seconds": 300,
            "tools": {"compiler": pin(archiver.with_name("cl.exe"))},
            "compiler_tools": [self.recipe["archiver"]],
            "owners": {"cpu": pin(cpu_path), "guards": pin(guard_path)},
        }
        self.guard_paths = Mock()
        self.cpu = SimpleNamespace(plain=Mock(), MAX_LOG=64 << 20)
        self.pins = SimpleNamespace(
            check_pin=Mock(side_effect=lambda row, _: Path(row["path"])), deadline_check=Mock(),
            canonical=Mock(return_value="d" * 64), bound_module=lambda module, _: module, parse_json=json.loads,
        )
        self.compile_owner = SimpleNamespace(
            ANCHOR=anchor, STAGE_SUFFIXES=compiler.STAGE_SUFFIXES, utc=compiler.utc,
            valid_seconds=compiler.valid_seconds, configure_window=compiler.configure_window,
            verify_receipt=Mock(return_value=(self.compiled, self.configured, self.configure_recipe)),
            output_guard=Mock(return_value=self.guard_paths), verify_stage=Mock(return_value=self.stage_pin),
            build_deadline=Mock(side_effect=AssertionError("verification attempted to reopen the launch clock")),
        )
        self.aggregate_owner = SimpleNamespace(
            RECEIPT_SCHEMA=aggregate.RECEIPT_SCHEMA, RECIPE_SCHEMA=aggregate.RECIPE_SCHEMA,
            select_plan=Mock(return_value=self.plan), response_bytes=aggregate.response_bytes,
            verify_command_logs=aggregate.verify_command_logs, inspect_inputs=Mock(return_value=[{"kind": "static"}]),
            inspect_output=Mock(return_value=self.coff),
        )
        self.modules = {
            SCRIPTS / "cuda-static-incremental.py": self.pins,
            BUILD / "fresh_compile.py": self.compile_owner,
            BUILD / "fresh_aggregate.py": self.aggregate_owner,
            BUILD / "fresh_projection.py": SimpleNamespace(project_build=Mock(return_value={"kind": "build"})),
            SCRIPTS / "cuda-static-archive.py": SimpleNamespace(MAX_ARCHIVE=2 << 30),
            cpu_path: self.cpu, guard_path: SimpleNamespace(guard=Mock()),
        }

    def verify(self, raw_response=None, *, wrong_receipt_sha=False):
        recipe_path = Path(self.receipt["recipe"]["path"])
        recipe = json.dumps(self.recipe).encode()
        self.receipt["recipe"] = pin(recipe_path, recipe)
        receipt_path = Path(self.raw_pin["path"])
        receipt = json.dumps(self.receipt).encode()
        self.raw_pin = pin(receipt_path, receipt)
        reviewed = {**self.raw_pin, "sha256": "0" * 64} if wrong_receipt_sha else self.raw_pin
        bootstrap = {receipt_path: receipt, recipe_path: recipe}
        raw = self.response_bytes if raw_response is None else raw_response
        self.loaded = Mock(side_effect=lambda path, _: self.modules[path])

        def open_bootstrap(path, mode):
            self.assertEqual(mode, "rb")
            return io.BytesIO(bootstrap[path])

        def bootstrap_stat(path):
            if path in bootstrap:
                return SimpleNamespace(st_mode=stat.S_IFREG, st_size=len(bootstrap[path]))
            return SimpleNamespace(st_mode=stat.S_IFDIR)

        with patch.object(verifier.time, "monotonic", return_value=500), \
                patch.object(verifier, "ANCHOR", self.compile_owner.ANCHOR), \
                patch.object(verifier, "load", self.loaded), \
                patch.object(Path, "lstat", autospec=True, side_effect=bootstrap_stat), \
                patch.object(Path, "open", autospec=True, side_effect=open_bootstrap), \
                patch.object(Path, "read_bytes", return_value=raw):
            return verifier.verify_receipt(reviewed, 600)

    def test_applies_only_the_explicit_read_window_after_execution_expiration(self):
        result = self.verify()
        self.assertEqual(result["archive"], self.archive_pin)
        self.assertEqual(result["aggregate_receipt"], self.raw_pin)
        self.assertEqual(result["aggregate"], self.receipt)
        self.assertEqual(result["configure_recipe"], self.configure_recipe)
        self.compile_owner.build_deadline.assert_not_called()
        self.compile_owner.verify_receipt.assert_called_once_with(self.compile_pin, 600)
        self.assertEqual(self.aggregate_owner.select_plan.call_args.kwargs, {"producer_proof_sha256": "d" * 64})
        self.compile_owner.verify_stage.assert_called_once_with(
            self.pins, self.cpu, self.root, self.command, "fresh-aggregate", self.argv, 270, 600,
        )
        for call in self.guard_paths.call_args_list:
            self.assertEqual(call.args[1:], (True, 600))
        self.assertEqual([call.args for call in self.pins.deadline_check.call_args_list], [(600,)] * 5)

    def test_requires_explicit_finite_unexpired_verification_window_before_loading_owners(self):
        with patch.object(verifier.time, "monotonic", return_value=500), patch.object(verifier, "load") as loaded:
            for invalid in (None, False, 500, float("inf"), float("nan")):
                with self.subTest(deadline=invalid), self.assertRaisesRegex(ValueError, "verification deadline"):
                    verifier.verify_receipt(self.raw_pin, invalid)
        loaded.assert_not_called()

    def test_refuses_bad_bootstrap_pin_without_loading_compile_or_aggregate(self):
        with self.assertRaisesRegex(ValueError, "consumed fresh aggregate bootstrap bytes differ"):
            self.verify(wrong_receipt_sha=True)
        self.loaded.assert_called_once_with(verifier.PIN_READER, "fresh_verify_pins")
        self.pins.check_pin.assert_not_called()

    def test_refuses_parent_extension_archive_extension_and_relabelled_chronology(self):
        for change in (
            {"original_deadline_monotonic": 301},
            {"deadline_monotonic": 271},
            {"finished_monotonic": 270, "elapsed_seconds": 100},
            {"finished_utc": timestamp(5, 0)},
        ):
            with self.subTest(change=change), patch.dict(self.receipt, change), self.assertRaises(ValueError):
                self.verify()
        self.compile_owner.verify_stage.assert_not_called()

    def test_refuses_changed_recipe_ownership_and_response_bytes(self):
        with patch.dict(self.recipe, {"execution_authorized": False}), \
                self.assertRaisesRegex(ValueError, "recipe scope"):
            self.verify()
        with patch.dict(self.recipe["owners"], {"archive": pin(self.root / "other.py")}), \
                self.assertRaisesRegex(ValueError, "owner path"):
            self.verify()
        self.loaded.assert_called_once_with(verifier.PIN_READER, "fresh_verify_pins")
        with self.assertRaisesRegex(ValueError, "observed response bytes"):
            self.verify(b"other response")
        self.compile_owner.verify_stage.assert_not_called()

    def test_refuses_forced_or_nonempty_job_with_a_zero_command_exit(self):
        for fields, message in (
            ({"owned_job_forced_termination": True}, "natural owned settlement"),
            ({"owned_job_final_active_processes": 1}, "Job is not empty"),
            ({"owned_job_final_member_pids": [123]}, "Job is not empty"),
        ):
            with patch.dict(self.owned, fields), self.assertRaisesRegex(ValueError, message):
                self.verify()
        self.compile_owner.verify_stage.assert_not_called()

    def test_refuses_different_argv_stage_receipt_or_reconstructed_coff(self):
        with patch.dict(self.receipt, {"argv": self.argv[:-1]}), self.assertRaisesRegex(ValueError, "argument vector"):
            self.verify()
        with patch.dict(self.receipt, {"stage_receipt": pin(self.root / "other.json")}), \
                self.assertRaisesRegex(ValueError, "stage receipt pin"):
            self.verify()
        with patch.dict(self.receipt, {"coff": {"kind": "static"}}), \
                self.assertRaisesRegex(ValueError, "observed COFF proof"):
            self.verify()

    def test_refuses_cross_generation_recipe_or_compile_pin_before_archive_inspection(self):
        with patch.dict(self.receipt, {"status": "PASS_GPU_AGGREGATE_ONLY_UNQUALIFIED"}), \
                self.assertRaisesRegex(ValueError, "completed unqualified fresh"):
            self.verify()
        with patch.dict(self.receipt, {"compile_receipt": pin(self.root / "other.json")}), \
                self.assertRaisesRegex(ValueError, "recipe scope"):
            self.verify()
        self.aggregate_owner.inspect_inputs.assert_not_called()


if __name__ == "__main__":
    unittest.main()
