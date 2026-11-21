# SPDX-License-Identifier: Apache-2.0
"""Run one reviewed Large CAL native Cargo phase inside a four-GiB Windows Job.

Successor of large_scientific_build.py for the image that carries the Large
direct CAL selector. The original file keeps its digest; its template pins,
pinned-toolchain checks and compiler-helper wait are reused from it unchanged.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ElementTree

HERE = Path(__file__).resolve().parent
ORIGINAL = HERE / "large_scientific_build.py"
ORIGINAL_SHA = "8d8f14b80450cf668c7c79e5524e63dc80460f1e3f87ca07ae1dca7b1c9af65b"
CAP_OWNER_SHA = "bf2850980a395de97a9cae888f910fe7e95f1f9f679a72641c32940a8ac497f1"
PREFIX = "windows-large-direct-cal-checks-"
CAL_TESTS = (
    "large_direct_cal_keeps_original_final_phase_and_refuses_foreign_rows",
    "large_cal_refuses_more_than_one_phase_even_if_collector_ignores_error",
)
# The Large selector calls into fp16_compact/cal.rs, which changed after the last Large image.
DEPENDENCY_TESTS = (
    "chronological_cal_rejects_foreign_audit_before_opening_file",
    "compact_cal_population_rejects_reordered_and_duplicate_ordinals",
)
# Approved D: campaign storage: original free-space baseline, growth budget and physical floor.
D_BASELINE_FREE = 1_233_246_265_344
D_BUDGET = 195_421_011_968
D_FLOOR = 107_374_182_400
D_CUSHION = 512 << 20
PHASE_SECONDS = 1800
MINIMUM_SECONDS = 120
UNSUCCESSFUL = {"failure", "error", "skipped", "flakyFailure", "flakyError", "rerunFailure", "rerunError"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def disk_floor():
    """Least D: free space the approved budget and the physical floor both allow."""
    return max(D_FLOOR, D_BASELINE_FREE - D_BUDGET)


def require_disk(free):
    require(type(free) is int and free >= disk_floor() + D_CUSHION,
            "D free space is below the approved campaign budget floor")


def parse_not_after(text):
    """Unix seconds of a run-supplied UTC cutoff, or None when the run has none."""
    if text is None:
        return None
    require(re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", text) is not None,
            "--not-after takes UTC as YYYY-MM-DDTHH:MM:SSZ")
    moment = datetime.datetime.strptime(text, "%Y-%m-%dT%H:%M:%SZ")
    return int(moment.replace(tzinfo=datetime.timezone.utc).timestamp())


def wall_seconds(not_after, now):
    """The original 1,800-second phase limit, shortened by an optional run cutoff."""
    wall = PHASE_SECONDS if not_after is None else min(PHASE_SECONDS, int(not_after - now))
    require(wall >= MINIMUM_SECONDS, "run cutoff leaves insufficient native time")
    return wall


def cal_filter(expression):
    names = (*CAL_TESTS, *DEPENDENCY_TESTS)
    require(not any(f"test({name})" in expression for name in names),
            "CAL regression is already in the original roster")
    return expression + "".join(f" | test({name})" for name in names)


def phase_argv(original, phase, directory):
    """The original reviewed receipt-runner command, with the CAL regressions added to Nextest."""
    argv = original["phase_argv"](phase, directory)
    if phase == "nextest":
        argv[-1] = cal_filter(argv[-1])
    return argv


def confirm_regressions(junit_text, names):
    """Require every named test to have run and passed exactly once in the Nextest JUnit report."""
    root = ElementTree.fromstring(junit_text)
    require(root.tag == "testsuites" and root.get("failures") == "0" and root.get("errors") == "0",
            "Nextest reported a failed or errored test")
    cases = list(root.iter("testcase"))
    for name in names:
        matched = [case for case in cases if case.get("name", "").rsplit("::", 1)[-1] == name]
        require(len(matched) == 1 and not any(child.tag in UNSUCCESSFUL for child in matched[0]),
                f"required CAL regression did not run and pass exactly once: {name}")
    return len(cases)


def load_original():
    require(hashlib.sha256(ORIGINAL.read_bytes()).hexdigest() == ORIGINAL_SHA,
            "reviewed original Large build launcher changed")
    return runpy.run_path(str(ORIGINAL))


def run(args):
    original = load_original()
    root, campaign, host = original["ROOT"], original["CAMPAIGN"], original["HOST"]
    shared = runpy.run_path(str(root / "scripts/arbiter/evaluation/run_compact_answer.py"))
    owner = shared["owner_module"]()
    directory = owner.plain(args.output)
    require(directory.parent == campaign and directory.name.startswith(PREFIX),
            "fresh Large CAL receipt directory required")
    require(Path(os.environ["CARGO_HOME"]) == campaign / "windows-offline-cargo-r1/home" and
            os.environ.get("CARGO_BUILD_JOBS") == "1" and os.environ.get("CARGO_NET_OFFLINE") == "true" and
            Path(os.environ["TEMP"]).is_relative_to(directory),
            "isolated D Cargo/temporary storage and single worker required")
    phase_dir = owner.plain(directory / f"owned-{args.phase}")
    cmd = phase_argv(original, args.phase, directory)
    not_after = parse_not_after(args.not_after)
    if args.worker:
        require(os.environ["AIRK_LARGE_CARGO_PHASE"] == str(phase_dir), "original owned Cargo parent required")
        cap_path = root / "scripts/arbiter/analysis/public_source_admission/aws_normal_service.py"
        require(hashlib.sha256(cap_path.read_bytes()).hexdigest() == CAP_OWNER_SHA,
                "original nested Windows resource owner changed")
        cap = runpy.run_path(str(cap_path))["capped"]
        with cap(shared, phase_dir, host) as (member, _):
            child = subprocess.Popen(cmd, cwd=root)
            member(child.pid)
            while child.poll() is None:
                require_disk(shutil.disk_usage(root).free)
                try:
                    child.wait(timeout=0.3)
                except subprocess.TimeoutExpired:
                    continue
            require(child.returncode == 0, "original Cargo phase refused")
            drained = original["await_tool_exit"](float(os.environ["AIRK_LARGE_CARGO_EXPIRES"]), child.pid)
            shared["write_json"](phase_dir / "tool-exits.json", drained)
        return
    require(not any(phase_dir.iterdir()), "fresh outer Cargo phase directory required")
    require_disk(shutil.disk_usage(root).free)
    worker = [sys.executable, "-I", "-B", "-S", str(Path(__file__).resolve()), *sys.argv[1:], "--worker"]
    wall = wall_seconds(not_after, time.time())
    result = owner.run_evaluation_fixture(
        worker, root, phase_dir / "outer.log", wall,
        os.environ | {"AIRK_LARGE_CARGO_PHASE": str(phase_dir),
                      "AIRK_LARGE_CARGO_EXPIRES": str(time.monotonic() + wall)})
    shared["write_json"](phase_dir / "execution.json", result)
    require(result["exit_code"] == 0 and result["owned_descendants_settled"]
            and result["owned_job_final_active_processes"] == 0
            and not result["timed_out"] and not result["owned_job_forced_termination"], "Cargo phase did not settle")
    limits = json.loads((phase_dir / "limits.json").read_bytes())
    require(limits["host_cap"] == host and 0 < limits["peak_job_bytes"] <= host and limits["below_normal"],
            "four-GiB Job not verified")
    receipt = json.loads((directory / f"large-{args.phase}-r1.json").read_bytes())
    require(receipt["passed"] and receipt["inputs_unchanged"] and receipt["exit_code"] == 0
            and Path(receipt["target_directory"]) == root / "target", "scoped Cargo receipt differs")
    summary = {"phase": args.phase, "limits": limits, "cargo": receipt["passed"], "observer_seconds": wall,
               "not_after": args.not_after, "d_free_floor_bytes": disk_floor() + D_CUSHION}
    if args.phase == "nextest":
        report = receipt.get("junit")
        require(isinstance(report, dict) and Path(report["path"]).name == report["path"],
                "this Nextest run retained no JUnit report")
        raw = (directory / report["path"]).read_bytes()
        require(hashlib.sha256(raw).hexdigest() == report["sha256"], "retained Nextest JUnit report changed")
        summary["tests_run"] = confirm_regressions(raw.decode("utf-8"), (*CAL_TESTS, *DEPENDENCY_TESTS))
        summary["selected_tests"] = receipt["selected_tests"]
        summary["required_regressions"] = [*CAL_TESTS, *DEPENDENCY_TESTS]
    print(json.dumps(summary))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", choices=("clippy", "nextest", "image"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--not-after", help="optional run cutoff, UTC YYYY-MM-DDTHH:MM:SSZ; omit when no cutoff applies")
    parser.add_argument("--worker", action="store_true")
    run(parser.parse_args())
