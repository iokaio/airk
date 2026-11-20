# SPDX-License-Identifier: Apache-2.0
"""Reopen the failed CU18 R2 chain and plan a separate direct-Job continuation."""

import argparse
import datetime as dt
import json
import math
from pathlib import Path
import runpy
import subprocess
import time

HERE = Path(__file__).resolve().parent
R2_REVIEW_SHA = "5505f1c1c8cd9fc9f3db3d1c60c1985ca5c339a075fb8da82fa502fe6c483ccf"
R2_ADMISSION_SHA = "b01f171200633e4616905fea737656af3da5881d39e680ad69e98724ee0bd4e1"
ORIGINAL_FILES = {
    "logs/configure.request.json": "c9cd69b1827b209b9a8572ea892e7d3ed76e2c3bdb26362f201efe9b737b1151",
    "logs/configure.started.json": "9bb000fd43d332cc0cf0f61c3f8e006b2647d2bd41cea672b2118f461bda155b",
    "logs/configure.stdout.log": "c89bb4f573d39aea96c5b9af69eb30ed50820bd3f4e45441c2cc3146767c7ebd",
    "logs/configure.stderr.log": "bbd4427191527c8ee59d4499fc11d4cc7061bdfa0f67d6bb2fa46646edbe689a",
    "b/Release/build.ninja": "cd022733fdc5033ade0a17ea51d606cc1d8fdcd59ef186880bf13f31795d85f2",
    "b/Release/CMakeCache.txt": "6f3c1fa0ccee07525b6e44077b67429f432c2480e4917513e3bd30601528e0ec",
    "b/Release/CMakeFiles/rules.ninja": "8fe4a1d54dc43ac3be19f6eccc495a57e962c7f07ce9fca66ae211d684767f2e",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def utc(text):
    value = dt.datetime.fromisoformat(text)
    require(value.utcoffset() == dt.timedelta(), "an original UTC clock is required")
    return value


def deadline(first, retry, campaign_end, now, monotonic):
    first_start, retry_start = utc(first["start"]), utc(retry["start"])
    cutoff_utc = min(first_start + dt.timedelta(hours=3), campaign_end)
    first_end, retry_end = first["deadline"], retry["deadline"]
    require(first_start <= retry_start <= now < cutoff_utc
            and all(type(value) in (float, int) and math.isfinite(value) and value > 0
                    for value in (first_end, retry_end, monotonic))
            and retry_end <= first_end, "original CU18 clock or admission differs")
    cutoff = min(first_end, retry_end, monotonic + (cutoff_utc - now).total_seconds())
    require(monotonic < cutoff, "original CU18 recovery window exhausted")
    return cutoff_utc, cutoff


def require_failed_settled(job):
    require(type(job["exit_code"]) is int and job["exit_code"] != 0
            and job["owned_descendants_settled"] is True
            and type(job["owned_job_final_active_processes"]) is int
            and job["owned_job_final_active_processes"] == 0
            and job["owned_job_final_member_pids"] == []
            and job["parent_assigned_to_job"] is False
            and job["child_assigned_before_resume"] is True
            and "cleanup_refusal" not in job, "original failed Job is not proved settled")


def commands(root, recipe, configure, compiler):
    output = root / "direct-r3"
    build = root / "b/Release"
    tools = recipe["tools"]
    ninja = tools["ninja"]["path"]
    workers = compiler.parallel_workers(recipe)
    archiver = str(Path(tools["compiler"]["path"]).with_name("lib.exe"))
    response = output / "inputs.rsp"
    archive = output / "onnxruntime.lib"
    cases = [
        ("configure", configure),
        ("core-session", [ninja, "-C", str(build), "-j", str(workers), "onnxruntime_session"]),
        ("provider", [ninja, "-C", str(build), "-j", str(workers), "all", "re2"]),
        ("archive", [archiver, "/NOLOGO", "/OUT:" + str(archive), "@" + str(response)]),
    ]
    return [{"name": name, "argv": argv, "cwd": str(root), "log": str(output / (name + ".log"))}
            for name, argv in cases]


def plan(execution_sha, stage_sha):
    supervisor = runpy.run_path(str(HERE / "fresh_windows_d_job.py"))
    d = supervisor["d"]
    root, run = supervisor["OUT"], supervisor["RUN"]
    custody, base, _, _, guard = supervisor["runtime"]()
    review, recipe = supervisor["checked_plan"](R2_REVIEW_SHA, base)
    d["exact_output"](root)
    core, compiler, _, _ = d["owners"]()
    pins = compiler.load(compiler.OWNERS["pin_reader"], "cu18_direct_plan_pins")
    admission_pin = base.pin(run / "admission.json")
    require(admission_pin["sha256"] == R2_ADMISSION_SHA, "original R2 admission differs")
    admission = compiler.read_pinned(pins, admission_pin, None)
    cutoff_utc, cutoff = deadline(supervisor["first_admission"](), admission, d["END"],
                                  dt.datetime.now(dt.timezone.utc), time.monotonic())
    expected_worker = [recipe["tools"]["python"]["path"], "-I", "-B", str(HERE / "fresh_windows_d_job.py"),
                       "worker", "--sha256", R2_REVIEW_SHA]
    require(admission["argv"] == expected_worker and admission["host_cap"] == supervisor["HOST_CAP"]
            and admission["disk_reservation"] == supervisor["DISK_CAP"], "original R2 owner or caps differ")
    original_stage = root / "logs/configure.receipt.json"
    execution = run / "execution.json"
    wrapper_log = root / "logs/configure.outer.log"
    reviewed_paths = [execution, original_stage, wrapper_log, *(root / leaf for leaf in ORIGINAL_FILES)]
    custody["guard_many"](guard, [run, root, admission_pin["path"], *reviewed_paths], True, deadline=cutoff)
    for path, sha in ((execution, execution_sha), (original_stage, stage_sha)):
        require(base.pin(path)["sha256"] == sha, "explicit original failed receipt pin differs: " + str(path))
    execution_pin, stage_pin = base.pin(execution), base.pin(original_stage)
    outer, stage = (compiler.read_pinned(pins, pin, cutoff) for pin in (execution_pin, stage_pin))
    require(outer["passed"] is False and outer["binding_sha256"] == R2_REVIEW_SHA
            and "outputs" not in outer and "worker_result" not in outer, "genuine failed R2 required")
    outer_job, stage_job = outer["job"], stage["owned"]
    require_failed_settled(outer_job)
    require_failed_settled(stage_job)
    require(outer_job["owned_leader_pid"] == 45876 and outer_job["exit_code"] == 1
            and outer_job["owned_job_forced_termination"] is False
            and stage_job["exit_code"] == 0xFFFFFFFF and stage_job["owned_job_forced_termination"] is True
            and all(job["timed_out"] is False and job["log_oversized"] is False for job in (outer_job, stage_job)),
            "original stalled waiter or R2 supervisor outcome differs")
    require(stage["stage"] == "configure" and stage["actual_tree_result"] is None
            and not (root / "logs/configure.tree.json").exists()
            and not any((root / leaf).exists() for leaf in ("fresh-configure.json", "fresh-compile.json",
                                                          "fresh-aggregate.json", "direct-r3")),
            "unexpected successful or conflicting configure evidence")
    require(type(stage["original_deadline_monotonic"]) in (int, float)
            and cutoff <= stage["original_deadline_monotonic"], "stage cutoff is earlier than the direct proposal")
    originals = {leaf: base.pin(root / leaf) for leaf in ORIGINAL_FILES}
    require(all(pin["sha256"] == ORIGINAL_FILES[leaf] for leaf, pin in originals.items()),
            "original configure logs or graph bytes changed since the failed attempt")
    for filename in ("configure.request.json", "configure.started.json", "configure.stdout.log", "configure.stderr.log"):
        require(originals["logs/" + filename] in stage["files"], "original configure stage file roster differs")
    require(base.pin(wrapper_log) in stage["files"], "original configure wrapper log lost")
    request = compiler.read_pinned(pins, originals["logs/configure.request.json"], cutoff)
    started = compiler.read_pinned(pins, originals["logs/configure.started.json"], cutoff)
    require((started["pid"], started["wrapper_pid"]) == (33024, 54288), "stalled configure PID binding differs")
    cpu = compiler.load(compiler.OWNERS["cpu"], "cu18_direct_plan_cpu")
    r8 = compiler.load(compiler.OWNERS["r8"], "cu18_direct_plan_r8")
    configure = core.configure_command(cpu, r8, root, recipe["tools"], root / "frontend")
    require(stage["argv"] == configure and stage["cwd"] == request["cwd"] == str(root)
            and request["executable"] == configure[0]
            and request["arguments"] == subprocess.list2cmdline(configure[1:]),
            "original locked configure command differs")
    planned = commands(root, recipe, configure, compiler)
    require(planned[-1]["argv"][0] in {row["path"] for row in recipe["compiler_tools"]},
            "archive tool not admitted in exact original compiler census")
    pins.deadline_check(cutoff)
    return {
        "schema": "airk.private.cu18-direct-recovery-plan/1", "qualification": False,
        "execution_authorized": False, "cutoff_utc": cutoff_utc.isoformat(),
        "cutoff_monotonic": cutoff, "configure_recipe": review["configure"],
        "failed_r2_outer": execution_pin, "failed_r2_stage": stage_pin,
        "original_stage_inputs": originals, "recovery_dir": str(root / "direct-r3"),
        "original_build_dir": str(root / "b/Release"), "host_cap": supervisor["HOST_CAP"],
        "descendant_limit": 64, "archive_wall_seconds": 300, "stages": planned,
        "plan_owner": base.pin(Path(__file__).resolve()),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--r2-execution-sha256", required=True)
    parser.add_argument("--r2-stage-sha256", required=True)
    options = parser.parse_args()
    result = plan(options.r2_execution_sha256, options.r2_stage_sha256)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
