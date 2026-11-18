# SPDX-License-Identifier: Apache-2.0
"""Project private build and aggregate proofs from the verified fresh CUDA chain."""

from copy import deepcopy
from pathlib import Path

BUILD_SCHEMA = "airk.private.cuda-fresh-static-build-proof/1"
AGGREGATE_SCHEMA = "airk.cuda-static-aggregate-proof/1"
TARGET = "x86_64-pc-windows-msvc"
VERSION = "1.30.0"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def project_build(compile_pin, compiled, configured, configure_recipe, *, canonical):
    """Project compile-owner facts before aggregation, using the owner's canonical hash."""
    root = Path(configure_recipe["fresh_output"])
    require(compiled["schema"] == "airk.private.cuda-fresh-compiled/1"
            and compiled["status"] == "COMPILED_UNQUALIFIED" and compiled["qualification"] is False
            and configured["schema"] == "airk.private.cuda-fresh-configured/1"
            and configured["status"] == "CONFIGURED_UNQUALIFIED" and configured["qualification"] is False
            and configure_recipe["schema"] == "airk.private.cuda-fresh-configure-recipe/1"
            and configure_recipe["qualification"] is False,
            "verified unqualified fresh compile lineage required")
    require(Path(compile_pin["path"]) == root / "fresh-compile.json"
            and Path(compiled["configured"]["path"]) == root / "fresh-configure.json"
            and Path(compiled["build_root"]) == root / "b/Release"
            and compiled["recipe"] == configured["recipe"]
            and compiled["toolchain"] == configured["toolchain"] == configure_recipe["tools"]
            and compiled["configured_graph"] == configured["configured_graph"]
            and compiled["collector_source"] == configured["collector_source"],
            "verified fresh compile/configure identity differs")
    return deepcopy({
        "schema": BUILD_SCHEMA,
        "target": TARGET,
        "ort_version": VERSION,
        "build_receipt": compile_pin,
        "producer_source": compiled["driver"],
        "configure_receipt": compiled["configured"],
        "configure_recipe": compiled["recipe"],
        "toolchain": compiled["toolchain"],
        "compiler_tools": configure_recipe["compiler_tools"],
        "native_input_closure_sha256": canonical(configure_recipe["native_inputs"]),
        "provider_dlls": compiled["providers"],
        "collector_source": compiled["collector_source"],
        "configured_graph": compiled["configured_graph"],
        "ninja": compiled["ninja"],
        "source_lock": configure_recipe["owners"]["lock"],
        "cpu_lock": configure_recipe["owners"]["cpu_lock"],
        "source_archive": configure_recipe["source_archive"],
        "dependency_archives": configure_recipe["dependency_archives"],
        "source_inventory_sha256": canonical(configured["source_inventory"]),
        "frontend_inventory_sha256": canonical(configured["frontend_inventory"]),
        "mirror_inventory_sha256": canonical(configured["mirror_inventory"]),
        "patch_inputs": configured["patch_inputs"],
        "qualification": False,
        "activation_pins_available": False,
    })


def project_bundle(verified_facts, *, canonical):
    """Project the verified raw command's link to its pre-aggregate build proof."""
    raw = verified_facts["aggregate"]
    raw_pin = verified_facts["aggregate_receipt"]
    recipe = verified_facts["configure_recipe"]
    compile_pin = raw["compile_receipt"]
    build = project_build(compile_pin, verified_facts["compile"], verified_facts["configure"],
                          recipe, canonical=canonical)
    plan = raw["plan"]
    build_sha = canonical(build)
    plan_sha = canonical(plan)
    require(raw["schema"] == "airk.private.cuda-fresh-aggregate/1"
            and raw["status"] == "AGGREGATED_UNQUALIFIED" and raw["qualification"] is False
            and raw["configure_receipt"] == build["configure_receipt"]
            and verified_facts["aggregate_recipe"]["compile_receipt"] == compile_pin
            and Path(raw_pin["path"]) == Path(recipe["fresh_output"]) / "fresh-aggregate.json"
            and raw_pin["sha256"] != compile_pin["sha256"],
            "verified distinct fresh aggregate lineage required")
    require(plan["schema"] == "airk.private.cuda-fresh-aggregate-plan/1" and plan["target"] == TARGET
            and plan["qualification"] is False and plan["compile_receipt"] == compile_pin
            and plan.get("producer_proof_sha256") == build_sha and raw["plan_sha256"] == plan_sha,
            "fresh command plan does not bind the exact current producer proof")
    output = raw["output"]
    require(output["path"] == "gpu-aggregate/onnxruntime.lib"
            and verified_facts["archive"] == {**output, "path": str(Path(recipe["fresh_output"]) / output["path"])},
            "fresh aggregate archive pin differs")
    return {
        "build": build,
        "aggregate": deepcopy({
            "schema": AGGREGATE_SCHEMA,
            "target": TARGET,
            "producer_proof_sha256": build_sha,
            "receipt_sha256": raw_pin["sha256"],
            "output": output,
            "plan_sha256": plan_sha,
            "coff": raw["coff"],
            "qualification": False,
        }),
    }
