# SPDX-License-Identifier: Apache-2.0
"""Reopen the private fresh Windows CUDA aggregate evidence under a caller deadline."""

import hashlib
import importlib.util
import math
from pathlib import Path
import stat
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PIN_READER = REPO / "scripts/cuda-static-incremental.py"
COMPILE_OWNER = HERE / "fresh_compile.py"
AGGREGATE_OWNER = HERE / "fresh_aggregate.py"
ARCHIVE_OWNER = REPO / "scripts/cuda-static-archive.py"
ANCHOR = Path("C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z")
RECEIPT_SCHEMA = "airk.private.cuda-fresh-aggregate/1"
RECIPE_SCHEMA = "airk.private.cuda-fresh-aggregate-recipe/1"
BOOTSTRAP_JSON_LIMIT = 32 << 20
RECEIPT_FIELDS = {
    "schema", "status", "recipe", "compile_receipt", "configure_receipt", "plan", "plan_sha256", "tool",
    "argv", "compile_verification", "started_utc", "started_monotonic", "original_deadline_monotonic",
    "deadline_monotonic", "qualification", "error", "response", "response_sha256", "command",
    "stage_receipt", "coff", "output", "finished_utc", "finished_monotonic", "elapsed_seconds",
}
RECIPE_FIELDS = {
    "schema", "repository_root", "compile_receipt", "owners", "archiver",
    "wall_seconds", "execution_authorized", "qualification",
}
VERIFICATION_FIELDS = {
    "started_utc", "finished_utc", "started_monotonic", "finished_monotonic", "elapsed_seconds",
    "original_deadline_monotonic", "effective_deadline_monotonic",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


def read_bootstrap(pins, row, deadline):
    require(isinstance(row, dict) and set(row) == {"path", "bytes", "sha256"}
            and isinstance(row["path"], str) and Path(row["path"]).is_absolute()
            and type(row["bytes"]) is int and 0 < row["bytes"] <= BOOTSTRAP_JSON_LIMIT,
            "invalid fresh aggregate bootstrap FilePin")
    pins.deadline_check(deadline)
    path = Path(row["path"])
    for entry in (*reversed(path.parents), path):
        observed = entry.lstat()
        require(not stat.S_ISLNK(observed.st_mode)
                and not getattr(observed, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                "reparse fresh aggregate bootstrap input refused: " + str(entry))
    require(stat.S_ISREG(observed.st_mode) and observed.st_size == row["bytes"],
            "fresh aggregate bootstrap file or length differs")
    with path.open("rb") as stream:
        raw = stream.read(row["bytes"] + 1)
    require(len(raw) == row["bytes"] and hashlib.sha256(raw).hexdigest() == row["sha256"],
            "consumed fresh aggregate bootstrap bytes differ")
    pins.deadline_check(deadline)
    return pins.parse_json(raw)


def verify_clocks(receipt, compiled, configured, configure_recipe, recipe, compile_owner):
    cutoff_utc, original = compile_owner.configure_window(configured, configure_recipe)
    verification = receipt["compile_verification"]
    require(isinstance(verification, dict) and set(verification) == VERIFICATION_FIELDS,
            "fresh aggregate compile verification clock fields differ")
    pre_start = verification["started_monotonic"]
    pre_finish = verification["finished_monotonic"]
    start = receipt["started_monotonic"]
    finish = receipt["finished_monotonic"]
    parent = receipt["original_deadline_monotonic"]
    effective = receipt["deadline_monotonic"]
    values = (pre_start, pre_finish, start, finish, parent, effective, verification["elapsed_seconds"],
              receipt["elapsed_seconds"], verification["original_deadline_monotonic"],
              verification["effective_deadline_monotonic"])
    require(all(compile_owner.valid_seconds(value) for value in values),
            "fresh aggregate monotonic clock is invalid")
    require(verification["original_deadline_monotonic"] == compiled["original_deadline_monotonic"] == original
            and verification["effective_deadline_monotonic"] == parent <= original
            and pre_start < pre_finish == start < finish < effective
            and effective == min(start + recipe["wall_seconds"], parent)
            and verification["elapsed_seconds"] == pre_finish - pre_start
            and receipt["elapsed_seconds"] == finish - start,
            "fresh aggregate original window differs")
    tree = receipt["command"]["actual_tree_result"]
    utc = compile_owner.utc
    require(utc(compiled["finished_utc"]) <= utc(verification["started_utc"])
            <= utc(verification["finished_utc"]) == utc(receipt["started_utc"])
            <= utc(tree["start_utc"]) <= utc(tree["finished_utc"])
            <= utc(receipt["finished_utc"]) < cutoff_utc,
            "fresh aggregate retained UTC chronology differs")


def verify_receipt(aggregate_pin, deadline):
    """Return observed current facts without reopening an execution window or issuing a proof."""
    require(type(deadline) in (int, float) and math.isfinite(deadline) and time.monotonic() < deadline,
            "explicit unexpired fresh verification deadline required")
    pins = load(PIN_READER, "fresh_verify_pins")
    require(isinstance(aggregate_pin, dict) and isinstance(aggregate_pin.get("path"), str),
            "reviewed fresh aggregate FilePin required")
    receipt_path = Path(aggregate_pin["path"])
    require(receipt_path.parent.parent == ANCHOR and receipt_path.name == "fresh-aggregate.json",
            "fresh aggregate receipt outside approved build root")
    receipt = read_bootstrap(pins, aggregate_pin, deadline)
    require(isinstance(receipt, dict) and set(receipt) == RECEIPT_FIELDS
            and receipt["schema"] == RECEIPT_SCHEMA
            and receipt["status"] == "AGGREGATED_UNQUALIFIED" and receipt["qualification"] is False
            and receipt["error"] is None, "completed unqualified fresh aggregate receipt required")
    recipe_pin = receipt["recipe"]
    recipe = read_bootstrap(pins, recipe_pin, deadline)
    require(isinstance(recipe, dict) and set(recipe) == RECIPE_FIELDS
            and recipe["schema"] == RECIPE_SCHEMA
            and recipe["repository_root"] == str(REPO) and recipe["execution_authorized"] is True
            and recipe["qualification"] is False and recipe["compile_receipt"] == receipt["compile_receipt"]
            and type(recipe["wall_seconds"]) is int and 0 < recipe["wall_seconds"] <= 300,
            "fresh aggregate recipe scope differs")
    owners = {
        "driver": AGGREGATE_OWNER, "archive": ARCHIVE_OWNER, "compile": COMPILE_OWNER,
        "pin_reader": PIN_READER, "projection": HERE / "fresh_projection.py",
    }
    require(isinstance(recipe["owners"], dict) and set(recipe["owners"]) == set(owners),
            "fresh aggregate owner roster differs")
    for name, owner in owners.items():
        require(pins.check_pin(recipe["owners"][name], deadline) == owner,
                "fresh aggregate owner path differs: " + name)

    compile_owner = load(COMPILE_OWNER, "fresh_verify_compile")
    aggregate_owner = load(AGGREGATE_OWNER, "fresh_verify_aggregate")
    require(compile_owner.ANCHOR == ANCHOR and aggregate_owner.RECEIPT_SCHEMA == RECEIPT_SCHEMA
            and aggregate_owner.RECIPE_SCHEMA == RECIPE_SCHEMA, "fresh aggregate owner boundary differs")
    compile_pin = recipe["compile_receipt"]
    compiled, configured, configure_recipe = compile_owner.verify_receipt(compile_pin, deadline)
    root = Path(configure_recipe["fresh_output"])
    build_root = Path(compiled["build_root"])
    require(receipt_path == root / "fresh-aggregate.json" and build_root == root / "b/Release"
            and receipt["configure_receipt"] == compiled["configured"],
            "fresh aggregate compile or configure ancestry differs")
    verification_cpu = Path(configure_recipe["owners"]["cpu"]["path"])
    guard_owner = Path(configure_recipe["owners"]["guards"]["path"])
    cpu = load(verification_cpu, "fresh_verify_cpu")
    component = load(guard_owner, "fresh_verify_guards")
    guard_paths = compile_owner.output_guard(component, configure_recipe)
    component.guard(configure_recipe, deadline=deadline)
    guard_paths([receipt_path, Path(recipe_pin["path"])], True, deadline)
    cpu.plain(receipt_path, regular=True)
    cpu.plain(recipe_pin["path"], regular=True)
    verify_clocks(receipt, compiled, configured, configure_recipe, recipe, compile_owner)

    archiver_pin = recipe["archiver"]
    archiver = pins.check_pin(archiver_pin, deadline)
    compiler = Path(configure_recipe["tools"]["compiler"]["path"])
    observed = [row for row in configure_recipe["compiler_tools"] if Path(row["path"]) == archiver]
    require(archiver == compiler.with_name("lib.exe") and observed == [archiver_pin]
            and receipt["tool"] == archiver_pin, "fresh aggregate recorded archiver differs")
    cpu.plain(archiver, regular=True)
    archive_owner = pins.bound_module(load(ARCHIVE_OWNER, "fresh_verify_coff"), deadline)
    projection = load(owners["projection"], "fresh_verify_projection")
    build = projection.project_build(compile_pin, compiled, configured, configure_recipe, canonical=pins.canonical)
    plan = aggregate_owner.select_plan(build_root, compiled, compile_pin, cpu, archive_owner, pins, deadline,
                                       producer_proof_sha256=pins.canonical(build))
    require(plan == receipt["plan"] and pins.canonical(plan) == receipt["plan_sha256"],
            "fresh aggregate observed graph plan differs")

    work = root / "gpu-aggregate"
    response = work / "inputs.rsp"
    output = work / "onnxruntime.lib"
    argv = [str(archiver), "/NOLOGO", "/OUT:" + str(output), "@" + str(response)]
    require(receipt["argv"] == argv, "fresh aggregate recorded argument vector differs")
    stage = root / "logs/fresh-aggregate"
    stage_files = [stage.with_suffix(suffix) for suffix in compile_owner.STAGE_SUFFIXES.values()]
    guard_paths([work, response, output, stage.with_suffix(".receipt.json"), *stage_files], True, deadline)

    response_pin = receipt["response"]
    require(Path(response_pin["path"]) == response and receipt["response_sha256"] == response_pin["sha256"],
            "fresh aggregate recorded response pin differs")
    cpu.plain(response, regular=True)
    response_bytes = aggregate_owner.response_bytes(plan)
    require(response_pin["bytes"] == len(response_bytes), "fresh aggregate response length differs")
    pins.check_pin(response_pin, deadline)
    require(response.read_bytes() == response_bytes, "fresh aggregate observed response bytes differ")

    command = receipt["command"]
    owned = command["owned"]
    require(type(owned.get("exit_code")) is int and owned["exit_code"] == 0
            and owned.get("timed_out") is False and owned.get("log_oversized") is False
            and owned.get("owned_descendants_settled") is True and owned.get("owned_job_forced_termination") is False
            and owned.get("parent_assigned_to_job") is False and owned.get("child_assigned_before_resume") is True
            and "cleanup_refusal" not in owned, "fresh aggregate command lacks natural owned settlement")
    aggregate_owner.verify_command_logs(command, root, cpu.MAX_LOG)
    stage_pin = compile_owner.verify_stage(
        pins, cpu, root, command, "fresh-aggregate", argv, receipt["deadline_monotonic"], deadline,
    )
    require(stage_pin == receipt["stage_receipt"], "fresh aggregate retained stage receipt pin differs")

    relative_output = receipt["output"]
    require(isinstance(relative_output, dict) and relative_output.get("path") == "gpu-aggregate/onnxruntime.lib"
            and type(relative_output.get("bytes")) is int
            and 65536 < relative_output["bytes"] <= archive_owner.MAX_ARCHIVE,
            "fresh aggregate output path or size differs")
    output_pin = {**relative_output, "path": str(output)}
    cpu.plain(output, regular=True)
    pins.check_pin(output_pin, deadline)
    selected = plan["inputs"] + [entry["pin"] for entry in plan["excluded_provider_imports"]]
    guard_paths([Path(row["path"]) for row in selected], True, deadline)
    objects = aggregate_owner.inspect_inputs(plan, cpu, archive_owner, pins, deadline)
    require(aggregate_owner.inspect_output(output, objects, archive_owner) == receipt["coff"],
            "fresh aggregate observed COFF proof differs")
    for pin in (aggregate_pin, recipe_pin, compile_pin, compiled["configured"], compiled["recipe"],
                archiver_pin, response_pin, output_pin, stage_pin, *compiled["ninja"], *selected, *compiled["providers"],
                *recipe["owners"].values()):
        pins.check_pin(pin, deadline)
    pins.deadline_check(deadline)
    return {
        "aggregate_receipt": aggregate_pin,
        "aggregate": receipt,
        "archive": output_pin,
        "compile": compiled,
        "configure": configured,
        "aggregate_recipe": recipe,
        "configure_recipe": configure_recipe,
    }
