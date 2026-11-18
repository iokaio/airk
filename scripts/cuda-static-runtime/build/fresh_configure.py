# SPDX-License-Identifier: Apache-2.0
"""Materialize and configure a fresh Windows CUDA static build after recipe review."""

import argparse
import datetime as dt
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
ANCHOR = Path("C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z")
CAMPAIGN_END = dt.datetime(2026, 11, 19, 5, 29, 42, tzinfo=dt.timezone.utc)
TARGET = "x86_64-pc-windows-msvc"
SCHEMA = "airk.private.cuda-fresh-configure-recipe/1"


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


def configure_command(cpu, r8, destination, tools, frontend):
    command = r8.gpu_command(cpu, destination, tools, frontend)
    # CMake's CUDA compiler-ID flags use UNIX_COMMAND argument splitting.
    compiler = Path(tools["compiler"]["path"]).as_posix()
    return [
        *command,
        "-DCMAKE_C_COMPILER=" + compiler,
        "-DCMAKE_CXX_COMPILER=" + compiler,
        "-DCMAKE_CUDA_HOST_COMPILER=" + compiler,
        "-DCMAKE_CXX_STANDARD=20",
    ]


def verify_msvc_cuda_compat(ninja):
    definition = "-D_SILENCE_CXX20_CISO646_REMOVED_WARNING"
    provider_rule = re.compile(r": (CXX|CUDA)_COMPILER__onnxruntime_providers_cuda(?:[_A-Za-z0-9]*) ")
    languages = set()
    for edge in ninja.split("\nbuild "):
        heading, _, variables = edge.partition("\n")
        match = provider_rule.search(heading)
        if match is None:
            continue
        languages.add(match[1])
        defines = re.search(r"^  DEFINES = (.+)$", variables, re.MULTILINE)
        if defines is None or definition not in defines[1].split():
            raise ValueError("CUDA provider compile edge lacks MSVC compatibility definition")
        if "activations.cc.obj:" in heading:
            flags = re.search(r"^  FLAGS = (.+)$", variables, re.MULTILINE)
            if flags is None or not {"-WX", "/WX"}.intersection(flags[1].split()):
                raise ValueError("CUDA provider C++ compile edge lacks warnings-as-errors")
    if languages != {"CXX", "CUDA"}:
        raise ValueError("generated Ninja graph lacks C++ or CUDA provider compile edges")


def configured_inputs(cpu, r8, destination, command, lock, collector, *, msvc_ciso646_compat=False):
    configure_lock = dict(lock, final_placement={
        **lock["final_placement"], "after_sha256": collector["sha256"],
    })
    observed = r8.configured_build_preflight(cpu, destination, command, configure_lock)
    build = destination / "b/Release"
    ninja = (build / "build.ninja").read_text(encoding="utf-8")
    if "-std:c++20" not in ninja and "/std:c++20" not in ninja:
        raise ValueError("generated Ninja graph lacks the selected C++20 build mode")
    if msvc_ciso646_compat:
        verify_msvc_cuda_compat(ninja)
    return observed


def execute(recipe_path, expected_sha):
    started = time.monotonic()
    started_utc = dt.datetime.now(dt.timezone.utc)
    component = load(REPO / "scripts/cuda-component-execution.py", "fresh_cuda_component")
    pins = load(REPO / "scripts/cuda-static-incremental.py", "fresh_cuda_pins")
    require = pins.require
    recipe_pin = pins.pin(recipe_path)
    require(recipe_pin["sha256"] == expected_sha, "reviewed fresh recipe hash differs")
    recipe = pins.read(recipe_path)
    require(os.name == "nt" and recipe["schema"] == SCHEMA
            and recipe["repository_root"] == str(REPO)
            and recipe["execution_authorized"] is True
            and recipe["qualification"] is False, "fresh configure recipe scope differs")
    require(recipe.get("msvc_ciso646_compat") is True, "reviewed CUDA provider MSVC compatibility patch required")
    require(type(recipe["wall_seconds"]) is int and 0 < recipe["wall_seconds"] <= 10800,
            "fresh configure exceeds parent build wall bound")
    deadline = started + min(recipe["wall_seconds"], (CAMPAIGN_END-started_utc).total_seconds())
    pins.deadline_check(deadline)
    root = Path(recipe["fresh_output"])
    require(root.parent == ANCHOR and not root.exists(), "fresh approved build root required")

    owners = {
        "driver": Path(__file__),
        "guards": REPO / "scripts/cuda-component-execution.py",
        "pin_reader": REPO / "scripts/cuda-static-incremental.py",
        "source_stage": HERE / "fresh_source.py",
        "process_stage": HERE / "fresh_process.py",
        "cpu": REPO / "scripts/provision-static-onnx-runtime.py",
        "cpu_lock": REPO / "scripts/static-onnx-runtime-lock.json",
        "r8": HERE / "r8/provision_cuda_runtime.py",
        "lock": HERE / "r8/cuda-runtime-producer-lock.json",
        "tree_waiter": HERE / "r12/wait_process_tree.ps1",
    }
    require(set(owners) == set(recipe["owners"]), "fresh owner roster differs")
    for name, expected_path in owners.items():
        require(pins.check_pin(recipe["owners"][name], deadline) == expected_path,
                "fresh build owner path differs: " + name)
    for row in pins.all_pins(recipe):
        pins.check_pin(row, deadline)
    audit_owner = pins.check_pin(recipe["output_audit_owner"], deadline)
    require(audit_owner == HERE / "fresh_compile.py", "fresh output audit owner path differs")
    audit = load(audit_owner, "fresh_configure_output_audit")
    sources = load(owners["source_stage"], "fresh_cuda_source")
    processes = load(owners["process_stage"], "fresh_cuda_process")
    lock = pins.read(owners["lock"])
    cpu = load(owners["cpu"], "fresh_cuda_cpu")
    r8 = load(owners["r8"], "fresh_cuda_r8")
    require(lock["cpu_owner_sha256"] == recipe["owners"]["cpu"]["sha256"]
            and lock["cpu_lock_sha256"] == recipe["owners"]["cpu_lock"]["sha256"],
            "reviewed source acquisition closure differs")
    cpu.validate_lock()
    tools = recipe["tools"]
    require(set(tools) == {"compiler", "cmake", "ninja", "patch", "python",
                           "windows_sdk", "msvc_toolset", "vcvars"},
            "fresh build tool roster differs")
    require([row for row in recipe["compiler_tools"] if row["path"] == tools["compiler"]["path"]]
            == [tools["compiler"]], "compiler absent from exact installed tool census")
    env = dict(recipe["environment"])
    require(".EXE" in env.get("PATHEXT", "").upper().split(";"),
            "fresh Windows compiler environment must resolve extensionless executables")
    require(env["VCTOOLSVERSION"] == tools["msvc_toolset"]
            and env["WINDOWSSDKVERSION"].rstrip("\\/") == tools["windows_sdk"]
            and env.get("CL", "") == env.get("_CL_", "") == "",
            "fresh compiler environment differs from pinned toolchain")

    guard_paths = audit.output_guard(component, recipe)

    component.guard(recipe, deadline=deadline)
    component.mkdir(recipe, root, deadline=deadline)
    for directory in ("logs", "tmp", "cuda-cache", "b"):
        component.mkdir(recipe, root / directory, deadline=deadline)
    component.mkdir(recipe, root / "b/Release", deadline=deadline)
    env.update(TEMP=str(root / "tmp"), TMP=str(root / "tmp"), TMPDIR=str(root / "tmp"),
               CUDA_CACHE_PATH=str(root / "cuda-cache"), PYTHONNOUSERSITE="1")

    def run(argv, cwd, stage):
        return processes.run_stage(
            argv=argv, cwd=Path(cwd), root=root, stage=stage, environment=env,
            cpu=cpu, guard_paths=guard_paths, power_shell=recipe["powershell"],
            tree_waiter=recipe["owners"]["tree_waiter"], deadline=deadline)

    source = sources.materialize(
        destination=root, source_archive=Path(recipe["source_archive"]["path"]),
        dependency_archives={name: Path(row["path"]) for name, row in
                             recipe["dependency_archives"].items()},
        lock=lock, cpu=cpu, r8=r8, patch_tool=Path(tools["patch"]["path"]),
        run_patch=run, guard_paths=guard_paths, deadline=deadline, msvc_ciso646_compat=True)
    command = configure_command(cpu, r8, root, tools, source["frontend"])
    configured = run(command, root, "configure")
    guard_paths(sorted((root / "b").rglob("*")), True, deadline)
    facts = configured_inputs(cpu, r8, root, command, lock, source["collector_source"], msvc_ciso646_compat=True)
    require(cpu.source_inventory(source["source"], exclude_mirror=True) == source["source_inventory"]
            and cpu.source_inventory(source["frontend"]) == source["frontend_inventory"],
            "fresh source or cuDNN frontend changed during configure")
    for row in pins.all_pins(recipe):
        pins.check_pin(row, deadline)
    receipt = {
        "schema": "airk.private.cuda-fresh-configured/1", "status": "CONFIGURED_UNQUALIFIED",
        "recipe": recipe_pin, "toolchain": tools, "configure": configured,
        "configured_graph": facts, "collector_source": source["collector_source"],
        "source_inventory": source["source_inventory"], "frontend_inventory": source["frontend_inventory"],
        "mirror_inventory": source["mirror_inventory"], "patch_inputs": source["patch_inputs"],
        "started_utc": started_utc.isoformat(),
        "finished_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "elapsed_seconds": time.monotonic() - started, "qualification": False,
    }
    pins.deadline_check(deadline)
    component.write_new(recipe, root / "fresh-configure.json", receipt)
    return pins.pin(root / "fresh-configure.json")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    parser.add_argument("--recipe-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(execute(args.recipe, args.recipe_sha256)))


if __name__ == "__main__":
    main()
