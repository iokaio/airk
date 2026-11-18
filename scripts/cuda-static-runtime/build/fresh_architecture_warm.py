# SPDX-License-Identifier: Apache-2.0
"""Continue the exact interrupted cu17 CUDA compile using direct owned Ninja."""

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import time
import types

SELF = Path(__file__).resolve()
HERE = SELF.parent
ARCHITECTURE_OWNER = HERE / "fresh_architectures.py"
ARCHITECTURE_SHA = "865f88d629127f8931b3763e68096c0908c01f1eaaac3acfdc6abc2dbc4288bb"
CONFIGURED_SHA = "ff74214cb702747e124b8950ea179de5f0ec701f41e1d830d3fcf9c6f4fecbe1"
FAILED_STAGE_SHA = "fd3e8159527f8f0dcefd1531a4f10dc42465b68d7c112beece43aeac62055974"
SCHEMA = "airk.private.cuda-architecture-warm-compile/1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_architecture():
    with ARCHITECTURE_OWNER.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(0 < len(raw) <= 2 << 20 and hashlib.sha256(raw).hexdigest() == ARCHITECTURE_SHA,
            "exact original architecture owner required")
    module = types.ModuleType("fresh_cuda_warm_architecture_owner")
    module.__file__ = str(ARCHITECTURE_OWNER)
    exec(compile(raw, str(ARCHITECTURE_OWNER), "exec", dont_inherit=True), module.__dict__)
    return module


def empty_owned_job(owned):
    return (type(owned.get("exit_code")) is int
            and owned.get("owned_descendants_settled") is True
            and type(owned.get("owned_job_final_active_processes")) is int
            and owned["owned_job_final_active_processes"] == 0
            and owned.get("owned_job_final_member_pids") == []
            and owned.get("parent_assigned_to_job") is False
            and owned.get("child_assigned_before_resume") is True
            and "cleanup_refusal" not in owned)


def failed_session(prior, root, recipe, compiler, original, guard, pins, deadline):
    name = "core-session-build"
    cwd = root
    argv = dict(compiler.build_commands(root, recipe["tools"]["cmake"]["path"], recipe))[name]
    prefix = root / "logs" / name
    request = prefix.with_suffix(".request.json")
    owners = recipe["owners"]
    wrapped = [recipe["powershell"]["path"], "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
               owners["tree_waiter"]["path"], "-Request", str(request)]
    owned = prior["owned"]
    require(prior["stage"] == name and prior["argv"] == argv and Path(prior["cwd"]) == cwd
            and prior["original_deadline_monotonic"] == original
            and prior["actual_tree_result"] is None and owned["argv"] == wrapped
            and owned["log"] == str(prefix.with_suffix(".outer.log"))
            and empty_owned_job(owned), "original unfinished tree stage lacks exact settled Job evidence")
    paths = [prefix.with_suffix(suffix) for suffix in compiler.STAGE_SUFFIXES.values()
             if suffix != ".tree.json"]
    files = {Path(row["path"]): row for row in prior["files"]}
    require(len(files) == len(prior["files"]) == len(paths) and files.keys() == set(paths),
            "original failed stage evidence roster differs")
    guard(paths, True, deadline)
    for row in prior["files"]:
        pins.check_pin(row, deadline)
    actual = compiler.read_pinned(pins, files[request], deadline)
    require(actual["schema"] == "airk.private.process-tree-command/1" and actual["executable"] == argv[0]
            and actual["arguments"] == compiler.subprocess.list2cmdline(argv[1:])
            and Path(actual["cwd"]) == root
            and not prefix.with_suffix(".tree.json").exists(),
            "original missing tree proof or actual request identity changed")
    return prior["files"]


def ninja_command(tool, build, jobs, targets):
    require(type(jobs) is int and jobs in (2, 8)
            and targets in (("onnxruntime_session",), ("all", "re2")), "unexpected warm Ninja command")
    return [str(tool), "-C", str(build), "-j", str(jobs), *targets]


def run_direct(cpu, pins, guard, root, output, stage, argv, environment, deadline, proof):
    log = output / (stage + ".log")
    guard([log], False, deadline)
    require(not log.exists(), "existing direct stage output must be preserved")
    row = {"stage": stage, "argv": argv, "cwd": str(root), "deadline_monotonic": deadline,
           "started_utc": dt.datetime.now(dt.timezone.utc).isoformat(), "actual_owned": None}
    proof["stages"].append(row)
    remaining = int(deadline - time.monotonic())
    require(remaining > 0, "original warm compilation deadline expired before dispatch")
    try:
        actual = cpu.run_owned(argv, root, log, remaining, environment)
        row["actual_owned"] = actual
        require(actual.get("argv") == argv and actual.get("log") == str(log)
                and empty_owned_job(actual) and cpu.successful(actual),
                "direct child or original owned process tree did not naturally settle")
    finally:
        if log.exists():
            guard([log], True, deadline)
            row["log"] = pins.pin(log, deadline)
        row["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    pins.deadline_check(deadline)
    return row["log"]


def execute(args):
    arch = load_architecture()
    _, compiler, _ = arch.modules()
    pins = compiler.load(compiler.OWNERS["pin_reader"], "warm_architecture_pins")
    source = pins.pin(SELF)
    require(source["sha256"] == args.approved_script_sha256
            and args.configured_sha256 == CONFIGURED_SHA, "exact reviewed warm/configure source pins required")
    root = compiler.ANCHOR / "cu17"
    require(args.configured == root / "fresh-configure.json"
            and args.failed_stage == root / "logs/core-session-build.receipt.json",
            "exact cu17 inputs required")
    output = args.output
    require(output.parent == root and re.fullmatch(r"warm-continuation-[a-z0-9-]{1,32}", output.name)
            and not output.exists(), "fresh direct warm-continuation output required")
    configured_pin = pins.pin(args.configured)
    require(configured_pin["sha256"] == CONFIGURED_SHA, "original cu17 configure receipt changed")
    configured = compiler.read_pinned(pins, configured_pin, None)
    recipe = compiler.read_pinned(pins, configured["recipe"], None)
    deadline = compiler.build_deadline(configured, recipe)
    _, original = compiler.configure_window(configured, recipe)
    chain = compiler.load_chain(pins, configured_pin, deadline)
    configured, recipe, observed_root, owners = chain
    require(observed_root == root and recipe["cuda_architectures"] == arch.ARCHITECTURES
            and recipe["cuda_architecture_owner"]["path"] == str(ARCHITECTURE_OWNER)
            and recipe["cuda_architecture_owner"]["sha256"] == ARCHITECTURE_SHA
            and Path(sys.executable).resolve() == Path(recipe["tools"]["python"]["path"]),
            "cu17 current configure, architecture owner or interpreter identity differs")
    component, cpu = owners["guards"], owners["cpu"]
    component.guard(recipe, deadline=deadline)
    guard = compiler.output_guard(component, recipe)
    guard([args.failed_stage], True, deadline)
    prior_pin = pins.pin(args.failed_stage, deadline)
    require(prior_pin["sha256"] == args.failed_stage_sha256 == FAILED_STAGE_SHA, "exact reviewed original stage pin required")
    prior = compiler.read_pinned(pins, prior_pin, deadline)
    prior_files = failed_session(prior, root, recipe, compiler, original, guard, pins, deadline)
    for path in (root / "fresh-compile.json", root / "fresh-cuda-code.json",
                 root / "logs/build.request.json", root / "logs/build.receipt.json"):
        require(not path.exists(), "original run already advanced beyond interrupted session tree")
    graph = compiler.verify_inputs(pins, chain, deadline)
    compiler.build_files(root, guard, deadline)
    build = root / "b/Release"
    session = build / "onnxruntime_session.lib"
    original_session = pins.pin(session, deadline)
    ninja = pins.check_pin(recipe["tools"]["ninja"], deadline)
    inspector = pins.check_pin(recipe["cuda_code_inspector"], deadline)
    require(Path(inspector) == arch.INSPECTOR, "exact local original CUDA inspector required")
    jobs = compiler.parallel_workers(recipe)
    component.mkdir(recipe, output, deadline=deadline)
    temporary = output / "tmp"
    cache = output / "cuda-cache"
    component.mkdir_siblings(recipe, [temporary, cache], deadline=deadline)
    receipt = output / "warm-compile.json"
    guard([receipt], False, deadline)
    environment = dict(recipe["environment"])
    environment.update(TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary),
                       CUDA_CACHE_PATH=str(cache), PYTHONNOUSERSITE="1")
    proof = {"schema": SCHEMA, "status": "FAILED_PRESERVED_NO_RETRY", "qualification": False,
             "configure": configured_pin, "configure_recipe": configured["recipe"],
             "architecture_owner": recipe["cuda_architecture_owner"], "recovery_owner": source,
             "failed_session_stage": prior_pin, "ninja_graph": graph,
             "original_session_archive": original_session, "original_deadline_monotonic": original,
             "effective_deadline_monotonic": deadline, "stages": [], "error": None}
    try:
        for stage, targets in (("warm-session", ("onnxruntime_session",)), ("warm-provider", ("all", "re2"))):
            run_direct(cpu, pins, guard, root, output, stage, ninja_command(ninja, build, jobs, targets),
                       environment, deadline, proof)
        files = compiler.build_files(root, guard, deadline)
        libraries, providers = compiler.pin_outputs(pins, root, files, deadline)
        selected = [row for row in providers if Path(row["path"]).name == "onnxruntime_providers_cuda.dll"]
        require(len(selected) == 1, "actual warm CUDA provider absent")
        provider = pins.check_pin(selected[0], deadline)
        raw = []
        for stage, flag in (("cuda-cubin-census", "--list-elf"), ("cuda-ptx-census", "--list-ptx")):
            report = run_direct(cpu, pins, guard, root, output, stage,
                                [str(inspector), "--all-fatbin", flag, str(provider)], environment, deadline, proof)
            path = pins.check_pin(report, deadline)
            payload = arch.exact_bytes(path, report["sha256"])
            require(len(payload) == report["bytes"], "observed CUDA census report length differs")
            raw.append(payload.decode("utf-8"))
        proof["code_inventory"] = arch.code_inventory(*raw)
        proof["code_provider"] = selected[0]
        proof["code_inspector"] = recipe["cuda_code_inspector"]
        proof["libraries"], proof["providers"] = libraries, providers
        require(compiler.verify_inputs(pins, chain, deadline) == graph, "original configured graph changed during build")
        compiler.verify_frozen_inputs(pins, recipe, deadline)
        for row in [source, prior_pin, configured_pin, *prior_files, *providers, *libraries]:
            pins.check_pin(row, deadline)
        guard(list(compiler.tree_paths(output, deadline)), True, deadline)
        require(not (root / "logs/core-session-build.tree.json").exists(), "original missing tree proof was altered")
        pins.deadline_check(deadline)
        proof["status"] = "WARM_COMPILED_UNQUALIFIED"
    except BaseException as error:
        proof["error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        active_error = sys.exc_info()[1]
        proof["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        try:
            component.write_new(recipe, receipt, proof)
        except BaseException as retention_error:
            if active_error is None:
                raise
            active_error.add_note("warm continuation receipt retention: " + str(retention_error))
    pins.deadline_check(deadline)
    return pins.pin(receipt, deadline)


def main():
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("configured", "failed-stage", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("configured-sha256", "failed-stage-sha256", "approved-script-sha256"):
        parser.add_argument("--" + name, required=True)
    print(json.dumps(execute(parser.parse_args()), allow_nan=False))


if __name__ == "__main__":
    main()
