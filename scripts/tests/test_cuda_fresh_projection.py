# SPDX-License-Identifier: Apache-2.0
"""Inert controls for the fresh CUDA build and aggregate proof projection."""

from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


projection = load(SCRIPTS / "cuda-static-runtime/build/fresh_projection.py", "cuda_projection_controls")
pins = load(SCRIPTS / "cuda-static-incremental.py", "cuda_projection_pin_controls")


def pin(path, body=b"inert exact bytes"):
    return {"path": str(path), "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}


def identity(value):
    encoded = json.dumps(value, separators=(",", ":"), sort_keys=True, ensure_ascii=False).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


class FreshProjection(unittest.TestCase):
    def setUp(self):
        self.root = SCRIPTS / "inert-projection-root"
        self.build_root = self.root / "b/Release"
        self.compile_pin = pin(self.root / "fresh-compile.json", b"original compile receipt")
        self.configure_pin = pin(self.root / "fresh-configure.json")
        self.configure_recipe_pin = pin(self.root / "configure-recipe.json")
        self.aggregate_pin = pin(self.root / "fresh-aggregate.json", b"separate actual aggregate receipt")
        self.source = pin(self.root / "source/onnxruntime/core/session/onnxruntime_c_api.cc")
        self.tools = {
            "compiler": pin(self.root / "compiler/cl.exe"),
            "ninja": pin(self.root / "ninja/ninja.exe"),
            "cmake": pin(self.root / "cmake/cmake.exe"),
            "python": pin(self.root / "python/python.exe"),
            "patch": pin(self.root / "patch/patch.exe"),
            "vcvars": pin(self.root / "compiler/vcvars.bat"),
            "msvc_toolset": "14.44.35207",
            "windows_sdk": "10.0.26100.0",
        }
        self.graph = {
            "ninja_sha256": pin(self.build_root / "build.ninja")["sha256"],
            "final_capi_source_sha256": self.source["sha256"],
            "objects": 123,
        }
        self.configure_recipe = {
            "schema": "airk.private.cuda-fresh-configure-recipe/1", "qualification": False,
            "fresh_output": str(self.root), "tools": self.tools,
            "compiler_tools": [self.tools["compiler"], pin(self.root / "compiler/lib.exe")],
            "native_inputs": [pin(self.root / "cuda/bin/nvcc.exe"), pin(self.root / "cudnn/include/cudnn.h")],
            "owners": {"lock": pin(self.root / "cuda-source-lock.json"),
                       "cpu_lock": pin(self.root / "cpu-lock.json")},
            "source_archive": pin(self.root / "ort-source.tar.gz"),
            "dependency_archives": {"cudnn_frontend": pin(self.root / "cudnn-frontend.zip")},
        }
        self.configured = {
            "schema": "airk.private.cuda-fresh-configured/1",
            "status": "CONFIGURED_UNQUALIFIED", "qualification": False,
            "recipe": self.configure_recipe_pin, "toolchain": self.tools,
            "configured_graph": self.graph, "collector_source": self.source,
            "source_inventory": [pin("source-\u03bb.cc")], "frontend_inventory": [pin("frontend.h")],
            "mirror_inventory": [pin("dep/source.zip")],
            "patch_inputs": [{"stage": "final-graph", "original": pin(self.root / "collector.patch"),
                              "snapshot": pin(self.root / "copies/collector.patch")}],
        }
        self.compiled = {
            "schema": "airk.private.cuda-fresh-compiled/1", "status": "COMPILED_UNQUALIFIED",
            "qualification": False, "driver": pin(SCRIPTS / "cuda-static-runtime/build/fresh_compile.py"),
            "configured": self.configure_pin, "recipe": self.configure_recipe_pin,
            "build_root": str(self.build_root), "toolchain": self.tools, "configured_graph": self.graph,
            "collector_source": self.source,
            "ninja": [pin(self.build_root / name) for name in ("CMakeCache.txt", "build.ninja", "CMakeFiles/rules.ninja")],
            "providers": [pin(self.build_root / f"onnxruntime_providers_{name}.dll") for name in ("cuda", "shared")],
        }
        self.expected_build = {
            "schema": "airk.private.cuda-fresh-static-build-proof/1",
            "target": "x86_64-pc-windows-msvc", "ort_version": "1.30.0",
            "build_receipt": self.compile_pin, "producer_source": self.compiled["driver"],
            "configure_receipt": self.configure_pin, "configure_recipe": self.configure_recipe_pin,
            "toolchain": self.tools, "compiler_tools": self.configure_recipe["compiler_tools"],
            "native_input_closure_sha256": identity(self.configure_recipe["native_inputs"]),
            "provider_dlls": self.compiled["providers"], "collector_source": self.source,
            "configured_graph": self.graph, "ninja": self.compiled["ninja"],
            "source_lock": self.configure_recipe["owners"]["lock"],
            "cpu_lock": self.configure_recipe["owners"]["cpu_lock"],
            "source_archive": self.configure_recipe["source_archive"],
            "dependency_archives": self.configure_recipe["dependency_archives"],
            "source_inventory_sha256": identity(self.configured["source_inventory"]),
            "frontend_inventory_sha256": identity(self.configured["frontend_inventory"]),
            "mirror_inventory_sha256": identity(self.configured["mirror_inventory"]),
            "patch_inputs": self.configured["patch_inputs"],
            "qualification": False, "activation_pins_available": False,
        }
        output = pin("gpu-aggregate/onnxruntime.lib", b"actual aggregate archive placeholder")
        self.plan = {
            "schema": "airk.private.cuda-fresh-aggregate-plan/1",
            "target": "x86_64-pc-windows-msvc", "qualification": False,
            "compile_receipt": self.compile_pin, "producer_proof_sha256": identity(self.expected_build),
            "inputs": [pin(self.build_root / f"input-{index:03}.lib") for index in range(96)],
        }
        self.raw = {
            "schema": "airk.private.cuda-fresh-aggregate/1",
            "status": "AGGREGATED_UNQUALIFIED", "qualification": False,
            "compile_receipt": self.compile_pin, "configure_receipt": self.configure_pin,
            "plan": self.plan, "plan_sha256": identity(self.plan), "output": output,
            "coff": {"kind": "static", "export_directives": [
                "airk_ort_collect_final_graph_metadata_v1", "airk_ort_collect_final_placement_v1",
            ]},
        }
        self.facts = {
            "aggregate_receipt": self.aggregate_pin, "aggregate": self.raw,
            "archive": {**output, "path": str(self.root / output["path"])},
            "compile": self.compiled, "configure": self.configured,
            "aggregate_recipe": {"compile_receipt": self.compile_pin},
            "configure_recipe": self.configure_recipe,
        }

    def build(self):
        return projection.project_build(
            self.compile_pin, self.compiled, self.configured, self.configure_recipe, canonical=pins.canonical,
        )

    def test_build_projects_exact_compile_only_identities_without_file_access(self):
        with patch.object(Path, "open", side_effect=AssertionError("projection opened a file")):
            result = self.build()
        self.assertEqual(result, self.expected_build)
        self.assertEqual(identity(result), pins.canonical(result))
        result["provider_dlls"].clear()
        self.assertEqual(len(self.compiled["providers"]), 2)

    def test_projects_existing_aggregate_shape_from_distinct_receipt_and_actual_plan(self):
        with patch.object(Path, "open", side_effect=AssertionError("projection opened a file")):
            bundle = projection.project_bundle(self.facts, canonical=pins.canonical)
        self.assertEqual(bundle["build"], self.expected_build)
        self.assertEqual(bundle["aggregate"], {
            "schema": "airk.cuda-static-aggregate-proof/1", "target": "x86_64-pc-windows-msvc",
            "producer_proof_sha256": identity(self.expected_build), "receipt_sha256": self.aggregate_pin["sha256"],
            "output": self.raw["output"], "plan_sha256": identity(self.plan), "coff": self.raw["coff"],
            "qualification": False,
        })
        self.assertNotEqual(bundle["aggregate"]["receipt_sha256"], bundle["build"]["build_receipt"]["sha256"])

    def test_refuses_cross_build_plan_even_after_its_hash_is_resealed(self):
        other = deepcopy(self.expected_build)
        other["collector_source"] = pin(self.source["path"], b"different collector")
        self.plan["producer_proof_sha256"] = identity(other)
        self.raw["plan_sha256"] = identity(self.plan)
        with self.assertRaisesRegex(ValueError, "command plan does not bind"):
            projection.project_bundle(self.facts, canonical=pins.canonical)

    def test_refuses_missing_current_build_binding_and_unsealed_command_plan(self):
        producer_sha = self.plan.pop("producer_proof_sha256")
        self.raw["plan_sha256"] = identity(self.plan)
        with self.assertRaisesRegex(ValueError, "command plan does not bind"):
            projection.project_bundle(self.facts, canonical=pins.canonical)
        self.plan["producer_proof_sha256"] = producer_sha
        with self.assertRaisesRegex(ValueError, "command plan does not bind"):
            projection.project_bundle(self.facts, canonical=pins.canonical)

    def test_refuses_changed_compile_ancestry_and_cross_role_receipt(self):
        with patch.dict(self.compiled, {"recipe": pin(self.root / "other-recipe.json")}), \
                self.assertRaisesRegex(ValueError, "compile/configure identity"):
            self.build()
        with patch.dict(self.facts["aggregate_recipe"], {"compile_receipt": self.configure_pin}), \
                self.assertRaisesRegex(ValueError, "distinct fresh aggregate lineage"):
            projection.project_bundle(self.facts, canonical=pins.canonical)
        with patch.dict(self.facts, {"aggregate_receipt": {**self.aggregate_pin, "sha256": self.compile_pin["sha256"]}}), \
                self.assertRaisesRegex(ValueError, "distinct fresh aggregate lineage"):
            projection.project_bundle(self.facts, canonical=pins.canonical)

    def test_refuses_different_archive_pin(self):
        self.facts["archive"] = pin(self.root / "gpu-aggregate/onnxruntime.lib", b"cross-generation archive")
        with self.assertRaisesRegex(ValueError, "archive pin differs"):
            projection.project_bundle(self.facts, canonical=pins.canonical)


if __name__ == "__main__":
    unittest.main()
