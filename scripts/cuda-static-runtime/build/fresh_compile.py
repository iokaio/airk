# SPDX-License-Identifier: Apache-2.0
"""Compile an exact, separately reviewed, fresh Windows CUDA configuration."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import datetime as dt
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time


sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
ANCHOR = Path("C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z")
CAMPAIGN_END = dt.datetime(2026, 11, 19, 5, 29, 42, tzinfo=dt.timezone.utc)
COMPILED_SCHEMA = "airk.private.cuda-fresh-compiled/1"
PROVIDERS = ("onnxruntime_providers_shared.dll", "onnxruntime_providers_cuda.dll")
GRAPH_FILES = ("CMakeCache.txt", "build.ninja", "CMakeFiles/rules.ninja")
GRAPH_KEYS = ("cache_sha256", "ninja_sha256", "rules_sha256")
STAGE_TARGETS = (("core-session-build", ("onnxruntime_session",)), ("build", ("all", "re2")))
STAGE_SUFFIXES = {
    "request": ".request.json", "stdout": ".stdout.log", "stderr": ".stderr.log",
    "result": ".tree.json", "started": ".started.json", "outer": ".outer.log",
}
OWNERS = {
    "driver": HERE / "fresh_configure.py",
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


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def utc(value):
    clock = dt.datetime.fromisoformat(value)
    require(clock.utcoffset() == dt.timedelta(), "fresh build timestamp must be UTC")
    return clock


def valid_seconds(value):
    return type(value) in (int, float) and math.isfinite(value) and value > 0


def configure_window(configured, recipe):
    wall = recipe["wall_seconds"]
    require(type(wall) is int and 0 < wall <= 10800, "fresh build wall bound differs")
    started = utc(configured["started_utc"])
    finished = utc(configured["finished_utc"])
    cutoff = min(started + dt.timedelta(seconds=wall), CAMPAIGN_END)
    original = configured["configure"]["original_deadline_monotonic"]
    elapsed = configured["elapsed_seconds"]
    require(started <= finished < cutoff and valid_seconds(original)
            and valid_seconds(elapsed) and elapsed < (cutoff - started).total_seconds()
            and elapsed < original, "invalid original configure window")
    return cutoff, original


def build_deadline(configured, recipe, *, now_utc=None, now_monotonic=None):
    cutoff, original = configure_window(configured, recipe)
    now_utc = now_utc if now_utc is not None else dt.datetime.now(dt.timezone.utc)
    now_monotonic = now_monotonic if now_monotonic is not None else time.monotonic()
    require(utc(configured["finished_utc"]) <= now_utc, "configure proof is in the future")
    deadline = min(original, now_monotonic + (cutoff - now_utc).total_seconds())
    require(now_monotonic < deadline, "original fresh-build deadline exhausted")
    return deadline


def build_commands(root, cmake):
    return [
        (stage, [cmake, "--build", str(root / "b/Release"), "--config", "Release",
                 "--target", *targets, "--parallel", "2"])
        for stage, targets in STAGE_TARGETS
    ]


def read_pinned(pins, row, deadline):
    path = pins.check_pin(row, deadline)
    require(row["bytes"] <= 32 << 20, "fresh receipt exceeds JSON size bound")
    raw = path.read_bytes()
    require(len(raw) == row["bytes"] and hashlib.sha256(raw).hexdigest() == row["sha256"],
            "fresh receipt changed during read: " + str(path))
    pins.deadline_check(deadline)
    return pins.parse_json(raw)


def load_chain(pins, configured_pin, deadline):
    configured = read_pinned(pins, configured_pin, deadline)
    recipe = read_pinned(pins, configured["recipe"], deadline)
    require(os.name == "nt" and recipe["schema"] == "airk.private.cuda-fresh-configure-recipe/1"
            and recipe["repository_root"] == str(REPO)
            and recipe["execution_authorized"] is True and recipe["qualification"] is False,
            "fresh build recipe scope differs")
    root = Path(recipe["fresh_output"])
    require(root.parent == ANCHOR and Path(configured_pin["path"]) == root / "fresh-configure.json",
            "fresh configure receipt outside approved build root")
    require(configured["schema"] == "airk.private.cuda-fresh-configured/1"
            and configured["status"] == "CONFIGURED_UNQUALIFIED"
            and configured["qualification"] is False and configured["toolchain"] == recipe["tools"],
            "successful unqualified fresh configure receipt required")
    configure_window(configured, recipe)
    require(set(recipe["owners"]) == set(OWNERS), "fresh owner roster differs")
    for name, path in OWNERS.items():
        require(Path(recipe["owners"][name]["path"]) == path, "fresh owner path differs: " + name)
    pins.frozen_inputs(recipe, deadline)
    modules = {name: load(OWNERS[name], "cuda_compile_" + name)
               for name in ("driver", "guards", "source_stage", "process_stage", "cpu", "r8")}
    return configured, recipe, root, modules


def output_guard(component, recipe):
    def guard_paths(paths, created, deadline):
        batch_size = 48
        if len(paths) <= batch_size:
            if paths:
                component.guard_many(recipe, paths, created, deadline=deadline)
            return

        workers = 8
        wave_size = workers * batch_size
        with ThreadPoolExecutor(max_workers=workers) as executor:
            for start in range(0, len(paths), wave_size):
                # Each guard independently observes every path in its own batch.
                end = min(start + wave_size, len(paths))
                checks = [
                    executor.submit(component.guard_many, recipe, paths[offset:min(offset + batch_size, end)],
                                    created, deadline=deadline)
                    for offset in range(start, end, batch_size)
                ]
                for check in checks:
                    check.result()
    return guard_paths


def tree_paths(root, deadline):
    for directory, folders, files in os.walk(root, followlinks=False, onerror=_raise_walk_error):
        parent = Path(directory)
        for path in (parent, *(parent / name for name in sorted(folders + files))):
            info = path.lstat()
            require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400,
                    "fresh build output reparse point refused: " + str(path))
            require(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode),
                    "fresh build special output refused: " + str(path))
            require(deadline is None or time.monotonic() < deadline, "fresh output inventory deadline expired")
        yield parent
        yield from (parent / name for name in sorted(files))


def _raise_walk_error(error):
    raise error


def verify_stage(pins, cpu, root, proof, stage, argv, stage_deadline, deadline):
    prefix = root / "logs" / stage
    retained = pins.pin(prefix.with_suffix(".receipt.json"), deadline)
    require(read_pinned(pins, retained, deadline) == proof, "retained fresh stage proof differs: " + stage)
    paths = {name: str(prefix.with_suffix(suffix)) for name, suffix in STAGE_SUFFIXES.items()}
    require(proof["stage"] == stage and proof["argv"] == argv and proof["cwd"] == str(root)
            and proof["original_deadline_monotonic"] == stage_deadline
            and cpu.successful(proof["owned"]), "fresh stage command, cutoff or settlement differs: " + stage)
    raw = {row["path"]: row for row in proof["files"]}
    require(len(raw) == len(proof["files"]) == len(paths) and raw.keys() == set(paths.values()),
            "fresh stage output roster differs: " + stage)
    for row in raw.values():
        pins.check_pin(row, deadline)
    request = read_pinned(pins, raw[paths["request"]], deadline)
    require(request == {
        "schema": "airk.private.process-tree-command/1",
        "executable": argv[0], "arguments": subprocess.list2cmdline(argv[1:]), "cwd": str(root),
        **{key: paths[key] for key in ("stdout", "stderr", "result", "started")},
        "started_event": None, "done_event": None,
    }, "retained fresh stage request differs: " + stage)
    tree = read_pinned(pins, raw[paths["result"]], deadline)
    started = read_pinned(pins, raw[paths["started"]], deadline)
    require(proof["actual_tree_result"] == tree
            and tree["schema"] == "airk.private.process-tree-result/1"
            and type(tree["exit_code"]) is int and tree["exit_code"] == 0
            and tree["wait"] == "Start-Process-Wait-process-and-descendants"
            and all(type(tree[key]) is int and tree[key] > 0 for key in ("command_pid", "wrapper_pid"))
            and tree["command_pid"] == started["pid"] and tree["wrapper_pid"] == started["wrapper_pid"]
            and utc(tree["start_utc"]) == utc(started["started_utc"])
            and utc(tree["start_utc"]) <= utc(tree["finished_utc"]),
            "actual fresh stage process tree differs: " + stage)
    return retained


def verify_inputs(pins, chain, deadline):
    configured, recipe, root, modules = chain
    cpu, r8 = modules["cpu"], modules["r8"]
    lock = read_pinned(pins, recipe["owners"]["lock"], deadline)
    require(lock["cpu_owner_sha256"] == recipe["owners"]["cpu"]["sha256"]
            and lock["cpu_lock_sha256"] == recipe["owners"]["cpu_lock"]["sha256"],
            "fresh source acquisition owners differ")
    cpu.validate_lock()
    tools = recipe["tools"]
    require(set(tools) == {"compiler", "cmake", "ninja", "patch", "python",
                           "windows_sdk", "msvc_toolset", "vcvars"}
            and [row for row in recipe["compiler_tools"] if row["path"] == tools["compiler"]["path"]]
            == [tools["compiler"]], "fresh toolchain census differs")
    source, frontend = root / "source", root / "frontend"
    roster = modules["source_stage"].patch_roster(source, frontend, lock)
    patches = configured["patch_inputs"]
    require(len(patches) == len(roster), "fresh patch inventory length differs")
    for row, (stage, _, path, expected, _) in zip(patches, roster):
        original = row["original"]
        snapshot = row["snapshot"]
        require(row["stage"] == stage and Path(original["path"]) == path
                and original["sha256"] == expected
                and Path(snapshot["path"]) == root / "patch-inputs" / (stage + ".patch"),
                "fresh patch input identity differs: " + stage)
        original_path = pins.check_pin(original, deadline)
        snapshot_path = pins.check_pin(snapshot, deadline)
        require(original["bytes"] <= modules["source_stage"].MAX_PATCH_BYTES
                and snapshot["bytes"] <= modules["source_stage"].MAX_PATCH_BYTES
                and modules["source_stage"].patch_transport(stage, original_path.read_bytes())
                == snapshot_path.read_bytes(), "fresh patch transport differs: " + stage)

    for name, path, options in (
        ("source_inventory", source, {"exclude_mirror": True}),
        ("frontend_inventory", frontend, {}),
        ("mirror_inventory", source / "mirror", {}),
    ):
        require(cpu.source_inventory(path, **options) == configured[name],
                "fresh configure inventory differs: " + name)
        pins.deadline_check(deadline)
    collector = configured["collector_source"]
    require(pins.check_pin(collector, deadline) == source / modules["source_stage"].CAPI
            and collector["sha256"] == modules["source_stage"].COLLECTOR_SOURCE,
            "fresh collector source identity differs")
    driver = modules["driver"]
    command = driver.configure_command(cpu, r8, root, tools, frontend)
    graph = driver.configured_inputs(cpu, r8, root, command, lock, collector)
    require(graph == configured["configured_graph"], "fresh configure CMake/Ninja graph changed")
    ninja = [pins.pin(root / "b/Release" / name, deadline) for name in GRAPH_FILES]
    require([row["sha256"] for row in ninja] == [graph[name] for name in GRAPH_KEYS],
            "fresh configure CMake/Ninja pin differs")
    verify_stage(pins, cpu, root, configured["configure"], "configure", command,
                 configured["configure"]["original_deadline_monotonic"], deadline)
    return ninja


def compilation_outputs(root, files):
    build_root = root / "b/Release"
    libraries = sorted(path for path in files if path.suffix.lower() == ".lib")
    require(not any(path.name.lower() in {"onnxruntime.dll", "onnxruntime.lib"} for path in files),
            "second ORT core in fresh build output")
    require(build_root / "onnxruntime_session.lib" in libraries, "fresh session archive missing")
    providers = []
    for name in PROVIDERS:
        found = [path for path in files if path.name.lower() == name]
        require(found == [build_root / name], "same-build provider output missing, misplaced or ambiguous: " + name)
        providers.extend(found)
    return libraries, providers


def pin_outputs(pins, root, files, deadline):
    libraries, providers = compilation_outputs(root, files)
    for path in libraries:
        with path.open("rb") as stream:
            require(stream.read(8) == b"!<arch>\n", "invalid fresh library archive: " + str(path))
    for path in providers:
        with path.open("rb") as stream:
            require(stream.read(2) == b"MZ", "invalid fresh provider image: " + str(path))
    return ([pins.pin(path, deadline) for path in libraries],
            [pins.pin(path, deadline) for path in providers])


def require_clean_build(root, files):
    build = root / "b/Release"
    for path in files:
        relative = path.relative_to(build).as_posix() if path.is_relative_to(build) else ""
        compiler_probe = re.fullmatch(
            r"CMakeFiles/(?:[0-9.]+/CompilerId(?:C|CXX|CUDA)/(?:tmp/)?[^/]+|ShowIncludes/main\.obj)",
            relative,
        )
        populate_log = re.fullmatch(r"_deps/[A-Za-z0-9_.+-]+-subbuild/\.ninja_log", relative)
        require((path.name not in (".ninja_deps", ".ninja_log") or populate_log is not None)
                and (path.suffix.lower() not in {".obj", ".lib", ".dll"} or compiler_probe is not None),
                "fresh build contains previous compilation output: " + str(path))


def build_files(root, guard_paths, deadline, *, include_caches=False):
    directories = [root / "b"]
    if include_caches:
        directories.extend([root / "tmp", root / "cuda-cache"])
    for directory in directories:
        require(directory.is_dir(), "fresh build output directory missing: " + str(directory))
        paths = list(tree_paths(directory, deadline))
        guard_paths(paths, True, deadline)
        if directory == root / "b":
            files = [path for path in paths if path.is_file()]
    return files


def verify_compile_stages(pins, chain, compiled, deadline):
    configured, recipe, root, modules = chain
    cutoff, original = configure_window(configured, recipe)
    effective = compiled["deadline_monotonic"]
    started = utc(compiled["started_utc"])
    finished = utc(compiled["finished_utc"])
    elapsed = compiled["elapsed_seconds"]
    require(compiled["original_deadline_monotonic"] == original
            and valid_seconds(effective) and effective <= original
            and utc(configured["finished_utc"]) <= started <= finished < cutoff
            and valid_seconds(elapsed) and elapsed < effective
            and elapsed < (cutoff - started).total_seconds(),
            "fresh compile elapsed or original cutoff differs")
    commands = build_commands(root, recipe["tools"]["cmake"]["path"])
    require(len(compiled["stages"]) == len(commands), "fresh compile stage count differs")
    previous = started
    retained = []
    for proof, (stage, argv) in zip(compiled["stages"], commands):
        retained.append(verify_stage(pins, modules["cpu"], root, proof, stage, argv, effective, deadline))
        tree = proof["actual_tree_result"]
        require(previous <= utc(tree["start_utc"]) <= utc(tree["finished_utc"]) <= finished,
                "fresh compile stages are out of order")
        previous = utc(tree["finished_utc"])
    require(retained == compiled["stage_receipts"], "fresh compile stage receipt pins differ")


def verify_receipt(compile_pin, deadline):
    """Reopen real compile evidence for a separately authorized downstream owner."""
    pins = load(OWNERS["pin_reader"], "cuda_compile_pins")
    compiled = read_pinned(pins, compile_pin, deadline)
    require(compiled["schema"] == COMPILED_SCHEMA and compiled["status"] == "COMPILED_UNQUALIFIED"
            and compiled["qualification"] is False, "unqualified fresh compile receipt required")
    chain = load_chain(pins, compiled["configured"], deadline)
    configured, recipe, root, modules = chain
    component = modules["guards"]
    component.guard(recipe, deadline=deadline)
    guard_paths = output_guard(component, recipe)
    require(Path(compile_pin["path"]) == root / "fresh-compile.json"
            and Path(compiled["build_root"]) == root / "b/Release"
            and configured["recipe"] == compiled["recipe"]
            and configured["toolchain"] == compiled["toolchain"]
            and configured["configured_graph"] == compiled["configured_graph"]
            and configured["collector_source"] == compiled["collector_source"],
            "fresh compile ancestry differs")
    guard_paths([Path(compile_pin["path"]), Path(compiled["configured"]["path"])], True, deadline)
    stages = ("configure", *(stage for stage, _ in STAGE_TARGETS))
    stage_paths = [
        root / "logs" / (stage + suffix)
        for stage in stages for suffix in (*STAGE_SUFFIXES.values(), ".receipt.json")
    ]
    guard_paths(stage_paths, True, deadline)
    require(pins.check_pin(compiled["driver"], deadline) == Path(__file__).resolve(),
            "fresh compile driver changed")
    require(verify_inputs(pins, chain, deadline) == compiled["ninja"], "fresh compile Ninja pins differ")
    verify_compile_stages(pins, chain, compiled, deadline)
    files = build_files(root, guard_paths, deadline)
    libraries, providers = pin_outputs(pins, root, files, deadline)
    require(libraries == compiled["libraries"] and providers == compiled["providers"],
            "fresh compile library or provider pins differ")
    return compiled, configured, recipe


def execute(configured_path, configured_sha, script_sha):
    started_monotonic = time.monotonic()
    started_utc = dt.datetime.now(dt.timezone.utc)
    pins = load(OWNERS["pin_reader"], "cuda_compile_pins")
    require(configured_path.is_absolute() and configured_path.name == "fresh-configure.json"
            and configured_path.parent.parent == ANCHOR, "absolute reviewed fresh configure path required")
    self_pin = pins.pin(__file__)
    configured_pin = pins.pin(configured_path)
    require(self_pin["sha256"] == script_sha and configured_pin["sha256"] == configured_sha,
            "reviewed compile driver or configure receipt hash differs")
    configured = read_pinned(pins, configured_pin, None)
    recipe = read_pinned(pins, configured["recipe"], None)
    deadline = build_deadline(configured, recipe, now_utc=started_utc, now_monotonic=started_monotonic)
    chain = load_chain(pins, configured_pin, deadline)
    configured, recipe, root, modules = chain
    component = modules["guards"]
    component.guard(recipe, deadline=deadline)
    guard_paths = output_guard(component, recipe)
    destination = root / "fresh-compile.json"
    guard_paths([configured_path, root / "logs"], True, deadline)
    guard_paths([destination], False, deadline)
    require(not destination.exists(), "fresh compile receipt already exists")
    require(not any((root / "logs" / stage).with_suffix(suffix).exists()
                    for stage, _ in STAGE_TARGETS for suffix in (*STAGE_SUFFIXES.values(), ".receipt.json")),
            "fresh build already attempted; retained stages must be preserved")
    files = build_files(root, guard_paths, deadline, include_caches=True)
    require_clean_build(root, files)
    ninja = verify_inputs(pins, chain, deadline)
    tools = recipe["tools"]
    require(Path(sys.executable).resolve() == Path(tools["python"]["path"]),
            "fresh compile is running under a different Python executable")
    env = dict(recipe["environment"])
    require(env["VCTOOLSVERSION"] == tools["msvc_toolset"]
            and env["WINDOWSSDKVERSION"].rstrip("\\/") == tools["windows_sdk"]
            and env.get("CL", "") == env.get("_CL_", "") == "",
            "fresh compiler environment differs from pinned toolchain")
    env.update(TEMP=str(root / "tmp"), TMP=str(root / "tmp"), TMPDIR=str(root / "tmp"),
               CUDA_CACHE_PATH=str(root / "cuda-cache"), PYTHONNOUSERSITE="1")
    commands = build_commands(root, tools["cmake"]["path"])
    stages = []
    retained = []
    for stage, argv in commands:
        proof = modules["process_stage"].run_stage(
            argv=argv, cwd=root, root=root, stage=stage, environment=env, cpu=modules["cpu"],
            guard_paths=guard_paths, power_shell=recipe["powershell"],
            tree_waiter=recipe["owners"]["tree_waiter"], deadline=deadline)
        retained.append(verify_stage(pins, modules["cpu"], root, proof, stage, argv, deadline, deadline))
        stages.append(proof)
        files = build_files(root, guard_paths, deadline, include_caches=True)
    require(verify_inputs(pins, chain, deadline) == ninja, "fresh compile graph changed during build")
    pins.frozen_inputs(recipe, deadline)
    for row in (self_pin, configured_pin, configured["recipe"]):
        pins.check_pin(row, deadline)
    libraries, providers = pin_outputs(pins, root, files, deadline)
    finished = dt.datetime.now(dt.timezone.utc)
    receipt = {
        "schema": COMPILED_SCHEMA, "status": "COMPILED_UNQUALIFIED", "qualification": False,
        "driver": self_pin, "configured": configured_pin, "recipe": configured["recipe"],
        "build_root": str(root / "b/Release"), "toolchain": tools,
        "configured_graph": configured["configured_graph"], "collector_source": configured["collector_source"],
        "ninja": ninja, "stages": stages, "stage_receipts": retained,
        "providers": providers, "libraries": libraries,
        "original_deadline_monotonic": configured["configure"]["original_deadline_monotonic"],
        "deadline_monotonic": deadline, "started_utc": started_utc.isoformat(),
        "finished_utc": finished.isoformat(), "elapsed_seconds": time.monotonic() - started_monotonic,
    }
    verify_compile_stages(pins, chain, receipt, deadline)
    pins.deadline_check(deadline)
    component.write_new(recipe, destination, receipt)
    return pins.pin(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configured", required=True, type=Path)
    parser.add_argument("--configured-sha256", required=True)
    parser.add_argument("--approved-script-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(execute(args.configured, args.configured_sha256, args.approved_script_sha256)))


if __name__ == "__main__":
    main()
