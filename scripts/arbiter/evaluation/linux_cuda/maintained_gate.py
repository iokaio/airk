# SPDX-License-Identifier: Apache-2.0
"""Run the merged Retrieval CUDA source from the maintained checkout in the existing GNU target."""
import argparse
import hashlib
import json
import os
import re
import runpy
import stat
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
    repo = REPO
    dependencies = {}
    if options.signed_r7:
        import signed_gate
        repo = signed_gate.ROOT
        joint = signed_gate.authenticate()
        dependencies = signed_gate.native()
    before = source_inventory(repo)
    if options.signed_r7:
        before.update(joint=joint,native=dependencies)
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
    scan.mkdir(0o700)
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
    if options.signed_r7:
        from shipping_build import monitored
        reserve = (128 if options.gate == "clippy" else 512) << 20
        result,_,_,bounded = monitored(command,repo,scan,env,reserve,uid,timeout=1800,log_name="cargo.log")
    else:
        result = process["run_owned"](["/usr/bin/nice", "-n", "10", "/usr/bin/prlimit", "--as=8589934592",
                                       "--", *command], repo, scan / "cargo.log", 1800, env)
    after = source_inventory(repo)
    if options.signed_r7:
        after.update(joint=signed_gate.authenticate(),native=signed_gate.native())
    log = (scan / "cargo.log").read_text(errors="replace")
    summary = re.search(r"Summary \[[^\]]+\]\s+(\d+) tests? run: (\d+) passed", log)
    count = int(summary[1]) if summary else 0
    audit(scan, uid)
    for folder in ("target", "cargo", "tmp", "private-census"):
        audit(BASE / folder, uid)
    return {"success": before == after and bounded and process["successful"](result) and
            (options.gate in ("clippy", "product-clippy") or summary is not None and count > 0 and count == int(summary[2])),
            "scope": "airk-retrieval", "incremental": False, "target": env["CARGO_TARGET_DIR"],
            "sources_before": before["sha256"], "sources_after": after["sha256"], "source_receipt": options.freeze_sha256,
            "selected_tests": count if options.gate == "nextest" else None, "command": result,
            "log": str(scan / "cargo.log"), "log_sha256": sha(scan / "cargo.log"),
            "compiled_against": env["ORT_LIB_LOCATION"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gate", choices=("inspect", "clippy", "product-clippy", "nextest"), required=True)
    parser.add_argument("--signed-r7",action="store_true")
    parser.add_argument("--attempt", type=int, default=1)
    parser.add_argument("--freeze", type=Path)
    parser.add_argument("--freeze-sha256")
    print(json.dumps(run(parser.parse_args()), sort_keys=True))
