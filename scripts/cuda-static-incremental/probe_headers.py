# SPDX-License-Identifier: Apache-2.0
"""Observe the full include roster of the unchanged collector base translation unit."""

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from importlib import import_module

producer = import_module("cuda-static-incremental")
component = import_module("cuda-component-execution")
PREFIX = "Note: including file:"


def observed_headers(log, cwd):
    paths = set()
    for line in log.read_bytes().decode("utf-8").splitlines():
        if line.startswith(PREFIX):
            paths.add((cwd / line[len(PREFIX):].strip()).resolve(strict=True))
    return paths


def run(recipe_path, recipe_sha, output, *, skip_devshell_telemetry=False):
    started = time.monotonic()
    deadline = started + 120
    recipe_pin = producer.pin(recipe_path, deadline)
    producer.require(recipe_pin["sha256"] == recipe_sha, "baseline recipe pin differs")
    recipe = producer.read(recipe_path)
    producer.require(recipe["repo_root"] == str(ROOT), "baseline repository differs")
    producer.check_pin(recipe["base_source"], deadline)
    compiler = next(row for row in recipe["compiler_tool_files"]
                    if row["path"] == recipe["commands"]["compile"][0])
    producer.check_pin(compiler, deadline)
    component.guard(recipe, deadline=deadline)
    component.mkdir(recipe, output, deadline=deadline)
    temp = output / "tmp"
    component.mkdir(recipe, temp, deadline=deadline)
    log = output / "preprocess.log"
    component.guard(recipe, log, deadline=deadline)
    environment = dict(recipe["environment"], TMP=str(temp), TEMP=str(temp))
    if skip_devshell_telemetry:
        environment["VSCMD_SKIP_SENDTELEMETRY"] = "1"
    preprocessed = output / "base.i"
    component.guard(recipe, preprocessed, deadline=deadline)
    argv = producer.base_header_command(recipe, preprocessed)
    cwd = Path(recipe["parent_context"]["destination"]) / "b/Release"
    parent, owner = component.base().helpers(recipe)

    def authenticate(api, job, info, child_deadline):
        proof = parent.suspended_authentication(
            api, info, deadline, child_deadline, producer.SID, compiler, owner.image_proof)
        producer.require(component.token_owner(info.process) == producer.SID,
                         "preprocessor child default owner differs")
        proof.update(default_owner_sid=producer.SID, component_job=component.configure_job(api, job))
        return proof

    member_images = {}
    launcher_path = owner.run_windows_owned_training.__code__.co_filename

    def record_job_handles(frame, event, handles):
        # Observe the maintained owner's handles after it confirms Job
        # membership and before it waits or terminates the members.
        if (event != "return" or frame.f_code.co_filename != launcher_path
                or frame.f_code.co_name != "retain_owned_members" or not handles):
            return
        state = frame.f_locals
        for handle in handles:
            pid = state["retained_pid_handles"][handle]
            if pid in member_images:
                continue
            image = ctypes.create_unicode_buffer(32768)
            size = wintypes.DWORD(len(image))
            ok = state["api"].QueryFullProcessImageNameW(handle, 0, image, ctypes.byref(size))
            member_images[pid] = {"path": image.value if ok else None,
                                  "winerror": None if ok else ctypes.get_last_error()}

    with open(os.devnull, "rb") as initial, log.open("xb") as stream:
        previous_profile = sys.getprofile()
        sys.setprofile(record_job_handles)
        try:
            result = owner.run_windows_owned_training(
                argv, cwd, stream, deadline - time.monotonic(), environment, initial, 16 << 30,
                before_resume=authenticate, cleanup_seconds=5, absolute_deadline=deadline)
        finally:
            sys.setprofile(previous_profile)
    component.guard_many(recipe, [output, *output.rglob("*")], True, deadline=deadline)
    component.write_new(recipe, output / "job.json", {
        "command": argv, "recipe": recipe_pin, "log": producer.pin(log, deadline),
        "owned": result, "retained_member_images": member_images, "qualification": False,
        "skip_devshell_telemetry": skip_devshell_telemetry,
    })
    producer.settled(result, compiler)
    observed = observed_headers(log, cwd)
    ninja = {Path(row["path"]).resolve(strict=True) for row in recipe["compiler_dependencies"]}
    producer.require(ninja <= observed, "unchanged source omitted a frozen Ninja dependency")
    roots = tuple(Path(path).resolve() for path in environment["INCLUDE"].split(";") if path)
    supplement = observed - ninja
    producer.require(all(any(path.is_relative_to(root) for root in roots) for path in supplement),
                     "unchanged source reported a supplemental header outside pinned toolchain roots")
    headers = [producer.pin(path, deadline) for path in sorted(observed)]
    for row in headers:
        producer.check_pin(row, deadline)
    producer.check_pin(recipe["base_source"], deadline)
    producer.check_pin(compiler, deadline)
    value = {
        "schema": "airk.private.collector-base-header-observation/1",
        "recipe": recipe_pin, "source": recipe["base_source"], "compiler": compiler,
        "probe_source": producer.pin(__file__, deadline),
        "command": argv, "cwd": str(cwd), "log": producer.pin(log, deadline),
        "owned": result, "compiler_dependencies": len(ninja),
        "toolchain_headers": [row for row in headers if Path(row["path"]) in supplement],
        "all_header_count": len(headers), "qualification": False,
        "skip_devshell_telemetry": skip_devshell_telemetry,
        "elapsed_seconds": time.monotonic() - started,
    }
    producer.deadline_check(deadline)
    component.write_new(recipe, output / "observation.json", value)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", type=Path, required=True)
    parser.add_argument("--recipe-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--skip-devshell-telemetry", action="store_true")
    args = parser.parse_args()
    result = run(args.recipe, args.recipe_sha256, args.output,
                 skip_devshell_telemetry=args.skip_devshell_telemetry)
    print(json.dumps({key: result[key] for key in ("all_header_count", "elapsed_seconds", "qualification")}))


if __name__ == "__main__":
    main()
