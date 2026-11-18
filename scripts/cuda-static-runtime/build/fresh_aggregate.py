# SPDX-License-Identifier: Apache-2.0
"""Aggregate a reviewed fresh Windows static build and retain private build evidence."""

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
TARGET = "x86_64-pc-windows-msvc"
RECIPE_SCHEMA = "airk.private.cuda-fresh-aggregate-recipe/1"
RECEIPT_SCHEMA = "airk.private.cuda-fresh-aggregate/1"
EXPORTS = ("airk_ort_collect_final_graph_metadata_v1", "airk_ort_collect_final_placement_v1")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


def select_plan(build_root, compiled, compile_pin, cpu, archive, pins, deadline, *, producer_proof_sha256):
    require(isinstance(producer_proof_sha256, str) and len(producer_proof_sha256) == 64
            and all(ch in "0123456789abcdef" for ch in producer_proof_sha256),
            "reviewed fresh build proof SHA-256 required")
    ninja = build_root / "build.ninja"
    matches = [row for row in compiled["ninja"] if Path(row["path"]) == ninja]
    require(len(matches) == 1 and matches[0]["sha256"] == compiled["configured_graph"]["ninja_sha256"]
            and matches[0]["bytes"] <= 64 << 20, "fresh build graph identity differs")
    cpu.plain(ninja, regular=True)
    pins.check_pin(matches[0], deadline)
    roots = [f"onnxruntime_{name}.lib" for name in cpu.STATIC_COMPONENTS]
    roots += ["_deps/re2-build/re2.lib", "_deps/pytorch_cpuinfo-build/cpuinfo.lib"]
    raw = ninja.read_bytes()
    require(len(raw) == matches[0]["bytes"] and hashlib.sha256(raw).hexdigest() == matches[0]["sha256"],
            "consumed fresh build graph bytes differ")
    graph = archive.runtime_closure(raw.decode("utf-8"), roots)
    pins.deadline_check(deadline)
    require(len(graph["static_inputs"]) == 96, "fresh static input roster must contain 96 archives")
    recorded = {Path(row["path"]): row for row in compiled["libraries"]}
    require(len(recorded) == len(compiled["libraries"]), "duplicate fresh archive census path")
    inputs = []
    for relative in graph["static_inputs"]:
        path = cpu.child(build_root, relative)
        require(path in recorded, "graph-selected archive missing from the fresh compile census: " + relative)
        inputs.append(recorded[path])
    imports = []
    for module, info in graph["provider_modules"].items():
        relative = info["import_library"]
        if relative is not None:
            path = cpu.child(build_root, relative)
            require(path in recorded, "provider import interface missing from the fresh compile census")
            imports.append({"module": module, "pin": recorded[path]})
    require(sum(row["bytes"] for row in inputs) <= archive.MAX_ARCHIVE, "fresh aggregate input size exceeds bound")
    return {
        "schema": "airk.private.cuda-fresh-aggregate-plan/1",
        "target": TARGET,
        "compile_receipt": compile_pin,
        "producer_proof_sha256": producer_proof_sha256,
        "ninja": matches[0],
        "graph": graph,
        "inputs": inputs,
        "excluded_provider_imports": imports,
        "required_export_directives": list(EXPORTS),
        "qualification": False,
    }


def response_bytes(plan):
    paths = [row["path"] for row in plan["inputs"]]
    require(all(Path(path).is_absolute() and not any(mark in path for mark in '\r\n"')
                for path in paths), "unsafe archive response path")
    return ("\n".join(f'"{path}"' for path in paths) + "\n").encode("utf-8")


def inspect_inputs(plan, cpu, archive, pins, deadline):
    def inspect(row):
        path = cpu.plain(row["path"], regular=True)
        pins.check_pin(row, deadline)
        return archive.inspect_archive(path)

    observed = [inspect(row) for row in plan["inputs"]]
    require(all(row["kind"] == "static" for row in observed),
            "nonstatic archive entered the fresh static closure")
    exports = {symbol for row in observed for symbol in row["export_directives"]}
    require(set(EXPORTS) <= exports, "fresh static inputs lack both collector exports")
    for excluded in plan["excluded_provider_imports"]:
        value = inspect(excluded["pin"])
        require(value["kind"] == "import_or_mixed"
                and all(dll == excluded["module"].lower() for dll in value["imported_dlls"]),
                "fresh provider import archive is not its declared DLL interface")
    return observed


def inspect_output(output, inputs, archive):
    observed = archive.inspect_archive(output)
    archive.verify_member_closure(inputs, observed)
    require(set(EXPORTS) <= set(observed["export_directives"]), "fresh aggregate lacks both collector exports")
    return observed


def verify_command_logs(command, root, limit):
    log_paths = {str(root / "logs" / ("fresh-aggregate." + name + ".log"))
                 for name in ("stdout", "stderr", "outer")}
    logs = [row for row in command["files"] if row["path"] in log_paths]
    require(len(logs) == len({row["path"] for row in logs}) == len(log_paths)
            and all(type(row["bytes"]) is int and row["bytes"] >= 0 for row in logs)
            and sum(row["bytes"] for row in logs) <= limit, "fresh archive command log bound differs")
    owned = command["owned"]
    require(type(owned.get("owned_job_final_active_processes")) is int
            and owned["owned_job_final_active_processes"] == 0
            and owned.get("owned_job_final_member_pids") == [], "fresh archive command Job is not empty")


def retain_receipt(path, receipt, guard_paths, pins, deadline):
    guard_paths([path], False, deadline)
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(receipt, stream, indent=2, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    guard_paths([path], True, deadline)
    return pins.pin(path, deadline)


def execute(recipe_path, expected_sha):
    verification_started = time.monotonic()
    verification_started_utc = dt.datetime.now(dt.timezone.utc)
    bootstrap_deadline = verification_started + 10800
    pin_reader = REPO / "scripts/cuda-static-incremental.py"
    pins = load(pin_reader, "fresh_aggregate_pins")
    require(recipe_path.stat().st_size <= 32 << 20, "fresh aggregate recipe exceeds size bound")
    recipe_pin = pins.pin(recipe_path, bootstrap_deadline)
    require(recipe_pin["sha256"] == expected_sha, "reviewed fresh aggregate recipe hash differs")
    recipe_bytes = recipe_path.read_bytes()
    require(len(recipe_bytes) == recipe_pin["bytes"] and hashlib.sha256(recipe_bytes).hexdigest() == expected_sha,
            "consumed fresh aggregate recipe bytes differ")
    recipe = pins.parse_json(recipe_bytes)
    require(os.name == "nt"
            and set(recipe) == {"schema", "repository_root", "compile_receipt", "owners", "archiver",
                               "wall_seconds", "execution_authorized", "qualification"}
            and recipe["schema"] == RECIPE_SCHEMA and recipe["repository_root"] == str(REPO)
            and recipe["execution_authorized"] is True and recipe["qualification"] is False,
            "fresh aggregate recipe scope differs")
    require(type(recipe["wall_seconds"]) is int and 0 < recipe["wall_seconds"] <= 300,
            "fresh aggregate exceeds the 300-second operation bound")
    owners = {
        "driver": Path(__file__),
        "archive": REPO / "scripts/cuda-static-archive.py",
        "compile": HERE / "fresh_compile.py",
        "pin_reader": pin_reader,
        "projection": HERE / "fresh_projection.py",
    }
    require(set(recipe["owners"]) == set(owners), "fresh aggregate owner roster differs")
    for name, path in owners.items():
        require(pins.check_pin(recipe["owners"][name], bootstrap_deadline) == path,
                "fresh aggregate owner path differs")
    compile_owner = load(owners["compile"], "fresh_aggregate_compile")
    bootstrap_deadline = min(
        bootstrap_deadline,
        verification_started + (compile_owner.CAMPAIGN_END - verification_started_utc).total_seconds(),
    )
    pins.deadline_check(bootstrap_deadline)
    compile_pin = recipe["compile_receipt"]
    retained_compile = compile_owner.read_pinned(pins, compile_pin, bootstrap_deadline)
    retained_configure = compile_owner.read_pinned(pins, retained_compile["configured"], bootstrap_deadline)
    retained_recipe = compile_owner.read_pinned(pins, retained_configure["recipe"], bootstrap_deadline)
    parent_deadline = compile_owner.build_deadline(retained_configure, retained_recipe)
    compiled, configured, configure_recipe = compile_owner.verify_receipt(compile_pin, parent_deadline)
    require(compiled == retained_compile and configured == retained_configure and configure_recipe == retained_recipe,
            "fresh archive compile lineage changed during verification")
    verification_finished_utc = dt.datetime.now(dt.timezone.utc)
    verification_finished = time.monotonic()
    require(verification_finished < parent_deadline, "original build deadline expired during preverification")
    started = verification_finished
    started_utc = verification_finished_utc
    deadline = min(started + recipe["wall_seconds"], parent_deadline)
    pins.deadline_check(deadline)

    build_root = Path(compiled["build_root"])
    root = Path(configure_recipe["fresh_output"])
    require(build_root == root / "b/Release", "fresh aggregate build root differs")
    archiver = pins.check_pin(recipe["archiver"], deadline)
    compiler = Path(configure_recipe["tools"]["compiler"]["path"])
    require(archiver == compiler.with_name("lib.exe"), "fresh archiver must be the verified compiler sibling")
    configured_archiver = [row for row in configure_recipe["compiler_tools"] if Path(row["path"]) == archiver]
    require(configured_archiver == [recipe["archiver"]], "fresh archiver absent or differs from the configured tool census")

    config_owners = configure_recipe["owners"]
    for name, path in {
        "cpu": REPO / "scripts/provision-static-onnx-runtime.py",
        "guards": REPO / "scripts/cuda-component-execution.py",
        "process_stage": HERE / "fresh_process.py",
    }.items():
        require(pins.check_pin(config_owners[name], deadline) == path, "fresh configure owner path differs")
    cpu = load(config_owners["cpu"]["path"], "fresh_aggregate_cpu")
    component = load(config_owners["guards"]["path"], "fresh_aggregate_component")
    process = load(config_owners["process_stage"]["path"], "fresh_aggregate_process")
    archive = pins.bound_module(load(owners["archive"], "fresh_aggregate_coff"), deadline)
    cpu.plain(archiver, regular=True)

    def guard_paths(paths, created, cutoff):
        for start in range(0, len(paths), 128):
            component.guard_many(configure_recipe, paths[start:start + 128], created, deadline=cutoff)

    component.guard(configure_recipe, deadline=deadline)
    work = root / "gpu-aggregate"
    receipt_path = root / "fresh-aggregate.json"
    require(not work.exists() and not receipt_path.exists(), "existing fresh aggregate evidence preserved")
    guard_paths([work, receipt_path], False, deadline)

    projection = load(owners["projection"], "fresh_aggregate_projection")
    build = projection.project_build(compile_pin, compiled, configured, configure_recipe, canonical=pins.canonical)
    plan = select_plan(build_root, compiled, compile_pin, cpu, archive, pins, deadline,
                       producer_proof_sha256=pins.canonical(build))
    selected = plan["inputs"] + [entry["pin"] for entry in plan["excluded_provider_imports"]]
    guard_paths([Path(row["path"]) for row in selected], True, deadline)
    response = work / "inputs.rsp"
    output = work / "onnxruntime.lib"
    argv = [str(archiver), "/NOLOGO", "/OUT:" + str(output), "@" + str(response)]
    receipt = {
        "schema": RECEIPT_SCHEMA,
        "status": "FAILED_PRESERVED_NO_RETRY",
        "recipe": recipe_pin,
        "compile_receipt": compile_pin,
        "configure_receipt": compiled["configured"],
        "plan": plan,
        "plan_sha256": pins.canonical(plan),
        "tool": recipe["archiver"],
        "argv": argv,
        "compile_verification": {
            "started_utc": verification_started_utc.isoformat(),
            "finished_utc": verification_finished_utc.isoformat(),
            "started_monotonic": verification_started,
            "finished_monotonic": verification_finished,
            "elapsed_seconds": verification_finished - verification_started,
            "original_deadline_monotonic": compiled["original_deadline_monotonic"],
            "effective_deadline_monotonic": parent_deadline,
        },
        "started_utc": started_utc.isoformat(),
        "started_monotonic": started,
        "original_deadline_monotonic": parent_deadline,
        "deadline_monotonic": deadline,
        "qualification": False,
        "error": None,
    }
    try:
        observed_inputs = inspect_inputs(plan, cpu, archive, pins, deadline)
        component.mkdir(configure_recipe, work, deadline=deadline)
        temporary = work / "tmp"
        component.mkdir(configure_recipe, temporary, deadline=deadline)
        data = response_bytes(plan)
        guard_paths([response, output], False, deadline)
        with response.open("xb") as stream:
            stream.write(data)
        guard_paths([response], True, deadline)
        response_pin = pins.pin(response, deadline)
        require(response.read_bytes() == data, "fresh aggregate response text differs")
        receipt["response"] = response_pin
        receipt["response_sha256"] = response_pin["sha256"]

        environment = dict(configure_recipe["environment"])
        environment.update({name: str(temporary) for name in ("TEMP", "TMP", "TMPDIR")})
        pins.check_pin(recipe["archiver"], deadline)
        receipt["command"] = process.run_stage(
            argv=argv, cwd=root, root=root, stage="fresh-aggregate", environment=environment,
            cpu=cpu, guard_paths=guard_paths, power_shell=configure_recipe["powershell"],
            tree_waiter=config_owners["tree_waiter"], deadline=deadline,
        )
        verify_command_logs(receipt["command"], root, cpu.MAX_LOG)
        receipt["stage_receipt"] = compile_owner.verify_stage(
            pins, cpu, root, receipt["command"], "fresh-aggregate", argv, deadline, deadline,
        )
        output_paths = list(compile_owner.tree_paths(work, deadline))
        guard_paths(output_paths, True, deadline)
        cpu.plain(output, regular=True)
        output_pin = pins.pin(output, deadline)
        require(65536 < output_pin["bytes"] <= archive.MAX_ARCHIVE, "fresh aggregate output size differs")
        receipt["coff"] = inspect_output(output, observed_inputs, archive)
        receipt["output"] = {**output_pin, "path": output.relative_to(root).as_posix()}
        for pin in [recipe_pin, compile_pin, compiled["configured"], compiled["recipe"], recipe["archiver"],
                    response_pin, output_pin, *compiled["ninja"], *selected, *recipe["owners"].values()]:
            pins.check_pin(pin, deadline)
        finished_utc = dt.datetime.now(dt.timezone.utc)
        finished = time.monotonic()
        require(finished < deadline, "original fresh aggregate deadline expired")
        receipt["finished_utc"] = finished_utc.isoformat()
        receipt["finished_monotonic"] = finished
        receipt["elapsed_seconds"] = finished - started
        receipt["status"] = "AGGREGATED_UNQUALIFIED"
    except BaseException as error:
        finished_utc = dt.datetime.now(dt.timezone.utc)
        finished = time.monotonic()
        receipt.update(error={"type": type(error).__name__, "message": str(error)},
                       finished_utc=finished_utc.isoformat(), finished_monotonic=finished,
                       elapsed_seconds=finished - started)
        try:
            retain_receipt(receipt_path, receipt, guard_paths, pins, deadline)
        except Exception as retention_error:
            error.add_note("Fresh aggregate receipt retention failed: " + str(retention_error))
        raise
    return retain_receipt(receipt_path, receipt, guard_paths, pins, deadline)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    parser.add_argument("--recipe-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(execute(args.recipe, args.recipe_sha256)))


if __name__ == "__main__":
    main()
