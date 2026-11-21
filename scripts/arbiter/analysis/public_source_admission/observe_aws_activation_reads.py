# SPDX-License-Identifier: Apache-2.0
"""Observe existing-worker kernel read stages after proving a stable native store baseline."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
old = runpy.run_path(str(HERE / "observe_aws_kernel_store.py"))
entry = runpy.run_path(str(HERE / "aws_kernel_entry_evidence.py"))
normal, n, E = old["normal"], old["n"], old["E"]
require = n["require"]
PRIOR = E / "aws-kernel-census-r1"
IMAGE_DIR = E / "aws-activation-build-r1"
PROOF = "d7450c37520b05ff674471ba3f1ff4050dd0956f2e563b2edb9927af4921ef0f"


def observed_inputs(w, owner):
    stopped = PRIOR / "owned.json"
    n["checked"](stopped, "b289664c6cd0054b56631d00db3d1d6bddb6aff6cdc43ff8825814fb30a42e4d")
    closed = w["read_json"](stopped)
    require(closed["exit_code"] == 1 and closed["owned_descendants_settled"]
            and closed["owned_job_final_active_processes"] == 0 and not closed["owned_job_forced_termination"],
            "original verifier outcome or settlement differs")
    after = PRIOR / "store-after.json"
    n["checked"](after, "85dbfd84bf5e5d59ce765c88c9306763c16b97b69ca69eedf66102230718b2f4")
    register = old["ORIGIN"] / "commands/register.stdout"
    n["checked"](register, old["REGISTER"])
    _, registration = (json.loads(line) for line in register.read_bytes().splitlines())
    project = registration["receipt"]["request"]["registration"]
    store = owner.plain(old["ORIGIN"] / "index")
    incarnation = old["REVISION"].split(":")[1]
    directory = store / "collections-1.4" / project["collection"] / "incarnations" / incarnation
    n["checked"](IMAGE_DIR / "observer-image.private.json", PROOF)
    proof = w["read_json"](IMAGE_DIR / "observer-image.private.json")
    src = n["ROOT"] / "crates/airk-kernel/examples/observe_service_activation_reads.rs"
    n["checked"](src, proof["source_sha256"])
    build = IMAGE_DIR / "aws-activation-build-r1.json"
    n["checked"](build, proof["build_sha256"])
    data = w["read_json"](build)
    require(data["passed"] and data["inputs_unchanged"] and data["exit_code"] == 0
            and proof["artifact"]["target"]["name"] == "observe_service_activation_reads"
            and proof["artifact"]["target"]["src_path"] == str(src), "activation image proof differs")
    exe = owner.plain(IMAGE_DIR / "observe_service_activation_reads.exe", regular=True)
    n["checked"](exe, proof["image_sha256"])
    command = [str(exe), project["root"], project["data_root"], project["collection"], project["binding"], old["REVISION"]]
    return store, directory, command, w["read_json"](after)


def worker(w, owner, output, mode, deadline, member):
    store, directory, command, previous = observed_inputs(w, owner)
    with old["deny_database_writes"](directory):
        base = entry["capture"](store, output / "before.rows", deadline)
        w["write_json"](output / "before.json", base)
        if mode == "control":
            repeat = entry["capture"](store, output / "repeat.rows", deadline)
            w["write_json"](output / "repeat.json", repeat)
            diff = entry["compare"](output / "before.rows", output / "repeat.rows")
            w["write_json"](output / "control.json", {"previous": previous, "before": base, "repeat": repeat, **diff})
            require(base == repeat and diff["different_rows"] == 0, "no-op control changed")
            return
        require(base == previous, "current inventory differs from sealed prior post-read state")
        code = None
        forced = False
        started = time.monotonic_ns()
        env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "PYTHON", "RUST_LOG"))}
        try:
            with (output / "phases.jsonl").open("xb") as stdout, (output / "observer.stderr").open("xb") as stderr:
                child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, env=env,
                                         cwd=n["ROOT"], creationflags=subprocess.CREATE_NO_WINDOW)
                try:
                    member(child.pid)
                    code = child.wait(timeout=min(120, max(0, deadline - time.monotonic())))
                finally:
                    if child.poll() is None:
                        forced = True
                        child.kill()
                    code = child.wait()
                    w["write_json"](output / "command.json", {"command": command, "pid": child.pid, "exit_code": code,
                                    "forced": forced, "elapsed_ns": time.monotonic_ns() - started})
        finally:
            after = entry["capture"](store, output / "after.rows", deadline)
            w["write_json"](output / "after.json", after)
            diff = entry["compare"](output / "before.rows", output / "after.rows")
            w["write_json"](output / "comparison.json", diff)
            require(after == base and not diff["different_rows"], "activation read changed store")
        require(code == 0 and not forced, "native read stages unavailable")
        phases = [json.loads(line) for line in (output / "phases.jsonl").read_bytes().splitlines()]
        require([r["phase"] for r in phases[:-1]] == ["open_existing_service_observer", "service_rebuild_receipt",
                "refresh_service_snapshot_and_configured_roots", "inspect_service", "status", "service_recovery_required"]
                and all(r["ok"] for r in phases[:-1]) and phases[-1]["revision"] == old["REVISION"]
                and phases[-1]["files"] == 35_400, "native phase coverage differs")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--mode", choices=("control", "activation"), required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--control-sha", help="original settled stable, matching no-op control.json")
    p.add_argument("--worker", action="store_true")
    args = p.parse_args()
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    output = owner.plain(args.output)
    require(output.parent == E and output.name.startswith("aws-activation-" + args.mode + "-"), "wrong private output")
    if args.mode == "activation":
        path = E / "aws-activation-control-r1/control.json"
        require(args.control_sha and len(args.control_sha) == 64, "sealed control required")
        n["checked"](path, args.control_sha)
        control = w["read_json"](path)
        require(control["previous"] == control["before"] == control["repeat"] and control["different_rows"] == 0,
                "no stable authenticated baseline for this selector")
        normal["stage"]["require_settlement"](w, w["read_json"](path.parent / "owned.json"))
    if args.worker:
        require(os.environ["AIRK_AWS_PHASE_OWNER"] == str(output), "original Job worker required")
        with normal["capped"](w, output, 2 << 30) as (member, _):
            worker(w, owner, output, args.mode, float(os.environ["AIRK_AWS_PHASE_DEADLINE"]), member)
        return
    require(not output.exists(), "fresh output required")
    wall = min(3600, int(1795194600 - time.time()))
    require(wall > 120, "insufficient campaign time")
    output.mkdir()
    argv = [sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"]
    result = owner.run_evaluation_fixture(argv, n["ROOT"], output / "outer.log", wall, os.environ | {
        "AIRK_AWS_PHASE_OWNER": str(output), "AIRK_AWS_PHASE_DEADLINE": str(time.monotonic() + wall)})
    w["write_json"](output / "owned.json", result)
    normal["stage"]["require_settlement"](w, result)


if __name__ == "__main__":
    main()
