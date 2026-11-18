# SPDX-License-Identifier: Apache-2.0
"""Inert fresh static archive controls with no compiler, archiver, or file writes."""

import datetime as dt
import hashlib
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SCRIPTS = Path(__file__).resolve().parents[1]


def load(relative, name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


aggregate = load("cuda-static-runtime/build/fresh_aggregate.py", "fresh_aggregate_controls")
archive = load("cuda-static-archive.py", "fresh_aggregate_archive")
GRAPH_EXPORT = "airk_ort_collect_final_graph_metadata_v1"
PLACEMENT_EXPORT = "airk_ort_collect_final_placement_v1"


def pin(path, raw=b"literal test bytes"):
    return {"path": str(path), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


class FreshAggregate(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parent / "inert-fresh-root"
        self.build = self.root / "b/Release"
        self.dependencies = [f"_deps/test/number-{index:03d}.lib" for index in range(93)]
        self.roots = (
            "onnxruntime_common.lib", "_deps/re2-build/re2.lib", "_deps/pytorch_cpuinfo-build/cpuinfo.lib",
        )
        self.selected = sorted([*self.roots, *self.dependencies])
        self.import_relative = "onnxruntime_providers_shared.lib"
        lines = [
            "build onnxruntime_common.lib: CXX_STATIC_LIBRARY_LINKER common.obj "
            + " ".join(self.dependencies) + " || " + self.import_relative,
            *[f"build {name}: CXX_STATIC_LIBRARY_LINKER part.obj" for name in [*self.roots[1:], *self.dependencies]],
            "build onnxruntime_providers_shared.dll | onnxruntime_providers_shared.lib: CXX_SHARED_LIBRARY_LINKER object.obj",
            "build onnxruntime_providers_cuda.dll: CUDA_MODULE_LIBRARY_LINKER provider.obj",
        ]
        self.ninja = ("\n".join(lines) + "\n").encode()
        ninja_pin = pin(self.build / "build.ninja", self.ninja)
        self.compile_pin = pin(self.root / "fresh-compile.json")
        self.compiled = {
            "ninja": [ninja_pin],
            "configured_graph": {"ninja_sha256": ninja_pin["sha256"]},
            "libraries": [pin(self.build / name) for name in reversed([*self.selected, self.import_relative])],
        }
        self.cpu = SimpleNamespace(STATIC_COMPONENTS=("common",), plain=Mock(side_effect=lambda path, **_: Path(path)),
                                   child=lambda root, relative: root / relative)
        self.pins = SimpleNamespace(check_pin=Mock(), deadline_check=Mock())

    def plan(self):
        with patch.object(Path, "read_bytes", return_value=self.ninja):
            return aggregate.select_plan(self.build, self.compiled, self.compile_pin,
                                         self.cpu, archive, self.pins, 120,
                                         producer_proof_sha256="5" * 64)

    def test_uses_actual_sorted_graph_inputs_and_excludes_provider_interface(self):
        plan = self.plan()
        self.assertEqual(plan["compile_receipt"], self.compile_pin)
        self.assertEqual(plan["producer_proof_sha256"], "5" * 64)
        self.assertEqual(plan["graph"]["static_inputs"], self.selected)
        self.assertEqual(len(plan["inputs"]), 96)
        self.assertEqual(plan["excluded_provider_imports"], [{
            "module": "onnxruntime_providers_shared.dll",
            "pin": pin(self.build / self.import_relative),
        }])
        self.assertFalse(plan["qualification"])
        expected_response = "".join(f'"{self.build / path}"\n' for path in self.selected).encode()
        self.assertEqual(aggregate.response_bytes(plan), expected_response)

    def test_refuses_missing_graph_selected_archive_or_changed_graph(self):
        self.compiled["libraries"] = self.compiled["libraries"][:-1]
        with self.assertRaisesRegex(ValueError, "missing from the fresh compile census"):
            self.plan()
        self.compiled["configured_graph"]["ninja_sha256"] = "0" * 64
        with patch.object(Path, "read_bytes") as reader, self.assertRaisesRegex(ValueError, "graph identity"):
            aggregate.select_plan(self.build, self.compiled, self.compile_pin, self.cpu, archive, self.pins, 120,
                                  producer_proof_sha256="5" * 64)
        reader.assert_not_called()

    def test_refuses_different_graph_bytes_and_unsafe_response_paths(self):
        self.ninja += b"# new graph bytes\n"
        with self.assertRaisesRegex(ValueError, "consumed fresh build graph"):
            self.plan()
        for path in ("relative.lib", str(self.build / 'bad".lib'), str(self.build / "bad\n.lib")):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "unsafe archive response path"):
                aggregate.response_bytes({"inputs": [{"path": path}]})

    def test_refuses_import_in_core_wrong_provider_and_missing_input_exports(self):
        plan = {
            "inputs": [pin(self.build / "core.lib")],
            "excluded_provider_imports": [{
                "pin": pin(self.build / self.import_relative), "module": "onnxruntime_providers_shared.dll",
            }],
        }
        static = {"kind": "static", "export_directives": [GRAPH_EXPORT, PLACEMENT_EXPORT]}
        shared = {"kind": "import_or_mixed", "imported_dlls": ["onnxruntime_providers_shared.dll"]}
        for inputs, message in (
            ([shared], "nonstatic"),
            ([{**static, "export_directives": [PLACEMENT_EXPORT]}], "both collector"),
            ([static, {**shared, "imported_dlls": ["foreign.dll"]}], "declared DLL interface"),
        ):
            with patch.object(archive, "inspect_archive", side_effect=inputs), self.assertRaisesRegex(ValueError, message):
                aggregate.inspect_inputs(plan, self.cpu, archive, self.pins, 120)
        with patch.object(archive, "inspect_archive", side_effect=[static, shared]):
            self.assertEqual(aggregate.inspect_inputs(plan, self.cpu, archive, self.pins, 120), [static])

    def test_output_requires_exact_object_bodies_permitted_dedup_and_both_exports(self):
        first = {"sha256": "1" * 64, "bytes": 32, "count": 2}
        second = {"sha256": "2" * 64, "bytes": 40, "count": 1}
        inputs = [{"kind": "static", "object_bodies": [first, second]}]
        output = {"kind": "static", "object_bodies": [{**first, "count": 1}, second],
                  "export_directives": [GRAPH_EXPORT, PLACEMENT_EXPORT]}
        disposition = self.root / "gpu-aggregate/onnxruntime.lib"
        with patch.object(archive, "inspect_archive", return_value=output):
            self.assertEqual(aggregate.inspect_output(disposition, inputs, archive), output)
        for value, message in (
            ({**output, "object_bodies": [first]}, "object byte closure"),
            ({**output, "object_bodies": [{**first, "count": 3}, second]}, "object byte closure"),
            ({**output, "export_directives": [PLACEMENT_EXPORT]}, "both collector exports"),
        ):
            with patch.object(archive, "inspect_archive", return_value=value), self.assertRaisesRegex(ValueError, message):
                aggregate.inspect_output(disposition, inputs, archive)


    def test_reopens_compiled_lineage_under_parent_before_starting_archive_clock(self):
        path = self.root / "reviewed-archive.json"
        body = b"opaque literal recipe"
        archiver = self.root / "lib.exe"
        owner_paths = {
            "driver": SCRIPTS / "cuda-static-runtime/build/fresh_aggregate.py",
            "archive": SCRIPTS / "cuda-static-archive.py",
            "compile": SCRIPTS / "cuda-static-runtime/build/fresh_compile.py",
            "pin_reader": SCRIPTS / "cuda-static-incremental.py",
            "projection": SCRIPTS / "cuda-static-runtime/build/fresh_projection.py",
        }
        archive_recipe = {
            "schema": "airk.private.cuda-fresh-aggregate-recipe/1", "repository_root": str(SCRIPTS.parent),
            "compile_receipt": self.compile_pin, "owners": {name: pin(path) for name, path in owner_paths.items()},
            "archiver": pin(archiver), "wall_seconds": 60, "execution_authorized": True, "qualification": False,
        }
        configured_pin = pin(self.root / "fresh-configure.json")
        configured = {"recipe": pin(self.root / "configure-recipe.json")}
        compiled = {"configured": configured_pin, "build_root": str(self.build)}
        configured_recipe = {
            "fresh_output": str(self.root), "tools": {"compiler": pin(self.root / "cl.exe")},
            "compiler_tools": [],
        }
        compile_owner = SimpleNamespace(
            CAMPAIGN_END=dt.datetime.max.replace(tzinfo=dt.timezone.utc),
            read_pinned=Mock(side_effect=[compiled, configured, configured_recipe]),
            build_deadline=Mock(return_value=600),
            verify_receipt=Mock(return_value=(compiled, configured, configured_recipe)),
        )
        pin_reader = SimpleNamespace(
            pin=Mock(return_value=pin(path, body)), parse_json=Mock(return_value=archive_recipe),
            check_pin=Mock(side_effect=lambda row, _: Path(row["path"])), deadline_check=Mock(),
        )
        with patch.object(aggregate, "os", SimpleNamespace(name="nt")), \
                patch.object(aggregate.time, "monotonic", side_effect=[10, 150]), \
                patch.object(aggregate, "load", side_effect=[pin_reader, compile_owner]), \
                patch.object(Path, "stat", return_value=SimpleNamespace(st_size=len(body))), \
                patch.object(Path, "read_bytes", return_value=body), \
                self.assertRaisesRegex(ValueError, "fresh archiver absent"):
            aggregate.execute(path, hashlib.sha256(body).hexdigest())
        compile_owner.verify_receipt.assert_called_once_with(self.compile_pin, 600)
        pin_reader.deadline_check.assert_called_with(210)
        pin_reader.check_pin.assert_called_with(archive_recipe["archiver"], 210)

    def test_requires_complete_bounded_archive_logs_and_an_empty_owned_job(self):
        paths = [self.root / "logs" / f"fresh-aggregate.{name}.log" for name in ("stdout", "stderr", "outer")]
        logs = [pin(path, b"1234") for path in paths]
        owned = {"owned_job_final_active_processes": 0, "owned_job_final_member_pids": []}
        aggregate.verify_command_logs({"files": logs, "owned": owned}, self.root, 12)
        for files in (logs[:-1], [logs[0], logs[1], logs[1]], [*logs[:-1], {**logs[-1], "bytes": 5}]):
            with self.assertRaisesRegex(ValueError, "command log bound"):
                aggregate.verify_command_logs({"files": files, "owned": owned}, self.root, 12)
        for active in (True, 1):
            with self.assertRaisesRegex(ValueError, "Job is not empty"):
                aggregate.verify_command_logs({"files": logs, "owned": {
                    **owned, "owned_job_final_active_processes": active,
                }}, self.root, 12)


if __name__ == "__main__":
    unittest.main()
