# SPDX-License-Identifier: Apache-2.0
"""Run the merged Retrieval CUDA source from the maintained checkout in the existing GNU target."""
import argparse
import hashlib
import json
import os
import re
import runpy
import stat
import time
from pathlib import Path

from cargo_gate import ENV_PIN
from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, REPO, audit, sha
from unpack import guarded
from verified_inputs import identity


def source_inventory(repo=REPO):
    names = {repo / name for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                                      ".cargo/config.toml", ".config/nextest.toml")}
    for crate in ("airk-core", "airk-process", "airk-retrieval", "airk-store"):
        root = repo / "crates" / crate
        names.update(path for path in root.rglob("*") if path.is_file() and
                     (path.suffix in {".rs", ".toml"} or path.name == "CONTRACT.md"))
    if any(not p.is_file() or p.is_symlink() or p.stat().st_uid != os.getuid() for p in names):
        raise ValueError("incomplete or unowned maintained Retrieval inputs")
    rows = {str(p.relative_to(repo)): sha(p) for p in sorted(names)}
    digest = hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"sha256": digest, "files": rows}


def run(options):
    uid = identity()
    os.umask(0o077)
    if options.revised_host_volume_log is not None and (not options.signed_r7 or options.gate != "nextest"):
        raise ValueError("revised host volume monitor is scoped to signed R7 Nextest")
    repo = REPO
    dependencies = {}
    verify_source = None
    if options.signed_r7 or options.signed_joint:
        import signed_gate
        if options.signed_joint:
            import joint_source
            repo = REPO / "artifacts/v190/final-plan/gnu-joint-source"
            verify_source = lambda: joint_source.authenticate(repo)
        else:
            repo = signed_gate.ROOT
            verify_source = signed_gate.authenticate
        dependencies = signed_gate.native()
    before = source_inventory(repo)
    if verify_source:
        before.update(joint=verify_source(),native=dependencies)
    if options.gate == "inspect":
        return before
    if not options.freeze or sha(options.freeze) != options.freeze_sha256 or before != json.loads(options.freeze.read_bytes()):
        raise ValueError("maintained source changed before the native gate")
    env_file = CAMPAIGN / "linux-driver-fix-checks-r1/gate-preflight.json"
    if sha(CPU) != CPU_SHA or sha(env_file) != ENV_PIN:
        raise ValueError("unreviewed native process or cache environment")
    for parent in (BASE.parents[1], BASE.parent, BASE):
        guarded(parent, uid, stat.S_ISDIR)
    scan = BASE / ("merged-retrieval-" + options.gate + "-r" + str(options.attempt))
    if not 1 <= options.attempt <= 99 or scan.exists():
        raise ValueError("fresh GNU gate required")
    env = json.loads(env_file.read_bytes())["environment"]
    env.update(HOME=str(BASE.parents[1]), LC_ALL="C", CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="1",
               CARGO_INCREMENTAL="0", RAYON_NUM_THREADS="1", RUST_TEST_THREADS="1")
    env.update(dependencies)
    for folder in ("target", "cargo", "tmp", "private-census"):
        guarded(BASE / folder, uid, stat.S_ISDIR)
        audit(BASE / folder, uid)
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            if proc.stat().st_uid == uid and (proc / "comm").read_text().strip() in {"cargo", "cargo-clippy", "rustc",
                                                                                      "cargo-nextest", "clippy-driver"}:
                raise ValueError("another Linux Cargo target owner is active")
        except (FileNotFoundError, ProcessLookupError):
            continue
    cargo = BASE / "rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin/cargo"
    args = ["--locked", "--offline", "--timings", "-p", "airk-retrieval"]
    if options.signed_joint:
        args.extend(["-p", "airk-inference"])
    if options.gate == "product-clippy":
        command = [cargo, "clippy", *args, "--features", "gpu-cuda", "--lib", "--", "-D", "warnings"]
    elif options.gate == "clippy":
        args.append("--all-features")
        command = [cargo, "clippy", *args, "--all-targets", "--", "-D", "warnings"]
    else:
        command = [cargo, "nextest", "run", *args, "--all-features", "--profile", "ci", "--test-threads", "1",
                   "-E", ("test(arbiter::runtime::cuda) | test(cpu_source_transition)" if options.signed_r7 else
                          "test(arbiter::runtime::cuda)")]
    process = runpy.run_path(str(CPU))
    bounded = True
    r7_nextest = None
    if options.signed_joint and options.gate == "nextest":
        command[-1] = ("test(arbiter::runtime::cuda) | test(cpu_source_transition) | "
                       "test(original_reference_paths_keep_a_bounded_source_root) | test(fp16_compact::) | test(fp16_large::)")
    if verify_source:
        from shipping_build import monitored, space
        reserve = (128 if options.gate == "clippy" else 512) << 20
        volume_guard = None
        timeout = 1800
        if options.revised_host_volume_log is not None:
            from r7_volume_budget import CUTOFF, original_guest_baseline, read_monitor, revised_capacity
            baseline = original_guest_baseline()
            def volume_guard(sample, initial, admitting):
                observation = read_monitor(options.revised_host_volume_log, uid, time.time())
                sample["host_monitor"] = observation
                return revised_capacity(baseline, sample, observation, initial, reserve, admitting=admitting)

            initial = space()
            timeout = min(timeout, CUTOFF - time.time())
            if timeout <= 0 or not volume_guard(initial, initial, True):
                raise ValueError("signed R7 storage reservation or campaign clock is unavailable")
        scan.mkdir(0o700)
        guarded(scan, uid, stat.S_ISDIR)
        if options.signed_r7 and options.gate == "nextest":
            from r7_nextest import prepare
            r7_nextest = prepare(repo, scan, before["files"][".config/nextest.toml"], uid)
            command[3:3] = ["--config-file", r7_nextest["effective"]["path"]]
        result,_,_,bounded = monitored(command,repo,scan,env,reserve,uid,timeout=timeout,log_name="cargo.log",
                                       volume_guard=volume_guard)
    else:
        scan.mkdir(0o700)
        guarded(scan, uid, stat.S_ISDIR)
        result = process["run_owned"](["/usr/bin/nice", "-n", "10", "/usr/bin/prlimit", "--as=8589934592",
                                       "--", *command], repo, scan / "cargo.log", 1800, env)
    after = source_inventory(repo)
    if verify_source:
        after.update(joint=verify_source(),native=signed_gate.native())
    log = (scan / "cargo.log").read_text(errors="replace")
    summary = re.search(r"Summary \[[^\]]+\]\s+(\d+) tests? run: (\d+) passed(?:, (\d+) skipped)?", log)
    if r7_nextest is not None:
        from r7_nextest import result_junit
        r7_nextest["junit"] = result_junit(r7_nextest, uid)
        bounded = bounded and r7_nextest["junit"] is not None
    count = int(summary[1]) if summary else 0
    audit(scan, uid)
    for folder in ("target", "cargo", "tmp", "private-census"):
        audit(BASE / folder, uid)
    return {"success": before == after and bounded and process["successful"](result) and
            (options.gate in ("clippy", "product-clippy") or summary is not None and count > 0 and count == int(summary[2])),
            **({"stage": options.gate, "source_view": before["joint"]} if options.signed_joint else {}),
            "scope": "airk-retrieval,airk-inference" if options.signed_joint else "airk-retrieval",
            "incremental": False, "target": env["CARGO_TARGET_DIR"],
            "sources_before": before["sha256"], "sources_after": after["sha256"], "source_receipt": options.freeze_sha256,
            "selected_tests": count if options.gate == "nextest" else None, "command": result,
            **({"nextest_store": r7_nextest, "skipped_tests": int(summary[3] or 0) if summary else None}
               if r7_nextest is not None else {}),
            "log": str(scan / "cargo.log"), "log_sha256": sha(scan / "cargo.log"),
            "compiled_against": env["ORT_LIB_LOCATION"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gate", choices=("inspect", "clippy", "product-clippy", "nextest"), required=True)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--signed-r7",action="store_true")
    source.add_argument("--signed-joint",action="store_true")
    parser.add_argument("--attempt", type=int, default=1)
    parser.add_argument("--freeze", type=Path)
    parser.add_argument("--freeze-sha256")
    parser.add_argument("--revised-host-volume-log", type=Path)
    print(json.dumps(run(parser.parse_args()), sort_keys=True))
