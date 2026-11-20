# SPDX-License-Identifier: Apache-2.0
"""Overlay reviewed Linux runtime sources and run one scoped, owned offline Cargo gate."""
import argparse
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import runpy
import stat
import subprocess

from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, REPO, audit, sha
from unpack import directory, guarded

PREFIX = "crates/airk-retrieval/src/arbiter/runtime/"
QUALIFICATION = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/"
FILES = (QUALIFICATION.rstrip("/")+".rs", PREFIX.rstrip("/")+".rs", QUALIFICATION+"fp16_compact.rs", QUALIFICATION+"fp16_large.rs", QUALIFICATION+"fp16_compact/linux.rs", QUALIFICATION+"fp16_compact/timing.rs",
         "Cargo.lock", "crates/airk-retrieval/examples/linux_cuda_provenance.rs", "crates/airk-retrieval/Cargo.toml", *[PREFIX + x for x in (
    "bounded_hybrid.rs", "bounded_hybrid/layer0.rs", "bounded_hybrid/tests.rs", "cuda.rs", "cuda/run.rs", "cuda/diagnostic.rs", "cuda/elf.rs", "cuda/elf/tests.rs", "cuda/eligibility.rs", "cuda/placement.rs",
    "cuda/graph_metadata.rs", "cuda/tests.rs", "cuda/linux.rs", "cuda/linux/driver.rs", "cuda/linux/assembly.rs", "cuda/linux/package.rs",
    "cuda/linux/held.rs", "cuda/linux/mappings.rs", "cuda/linux/tests.rs", "cuda/manifest.rs", "cuda/manifest/linux.rs",
    "cuda/manifest/linux/policy.rs", "cuda/manifest/linux/tests.rs", "cuda/manifest/linux/assembly.rs")])
CHECKOUT = REPO / "artifacts/v190/final-plan/linux-loader-worktree-r1"
STAGE = BASE / "linux-runtime-source-r1"
ENV_PIN = "7b4ebf9e50cdb3b4b591b579d210a61578ca5ecad704f3be07058cfe24be1651"


def run(gate, attempt):
    uid = os.getuid()
    if (uid, os.geteuid(), os.getgid(), pwd.getpwuid(uid).pw_name) != (1000, 1000, 1000, "tyler"):
        raise ValueError("verified Linux human required")
    if sha(CPU) != CPU_SHA or not 1 <= attempt <= 99:
        raise ValueError("unexpected stage owner/attempt")
    os.umask(0o077)
    root = STAGE / "source"
    base = subprocess.run(["/usr/bin/git", "-C", str(root), "rev-parse", "HEAD"], check=True,
                          capture_output=True, text=True, timeout=5).stdout.strip()
    if base != "20b893d82fe3c46d591af37285901bc3b2e8ac25":
        raise ValueError("unexpected source base")
    for p in (BASE.parents[1], BASE.parent, BASE, STAGE, root):
        guarded(p, uid, stat.S_ISDIR)
    snap = STAGE / ("gate-" + str(attempt))
    if snap.exists():
        raise ValueError("fresh gate receipt required")
    snap.mkdir(0o700)
    old = {}
    if attempt > 1:
        old = json.loads((STAGE / ("gate-" + str(attempt-1)) / "overlay.json").read_bytes())
    rows = {}
    for relative in FILES:
        source = CHECKOUT / relative
        out = root / relative
        directory(out.parent, root, uid)
        data = source.read_bytes()
        if out.exists():
            guarded(out, uid, stat.S_ISREG)
            if relative in old:
                if sha(out) != old[relative]:
                    raise ValueError("staged overlay changed: " + relative)
            else:
                prior = subprocess.run(["/usr/bin/git", "-C", str(root), "show", "HEAD:" + relative],
                                       check=True, capture_output=True, timeout=10).stdout
                if prior != out.read_bytes():
                    raise ValueError("staged baseline changed: " + relative)
        out.write_bytes(data)
        guarded(out, uid, stat.S_ISREG)
        rows[relative] = sha(out)
        if rows[relative] != sha(source):
            raise ValueError("overlay changed during transfer")
    inventory = snap / "overlay.json"
    inventory.write_text(json.dumps(rows, sort_keys=True))
    guarded(inventory, uid, stat.S_ISREG)
    env_file = CAMPAIGN / "linux-driver-fix-checks-r1/gate-preflight.json"
    if sha(env_file) != ENV_PIN:
        raise ValueError("unexpected historical Linux tool/input/cache paths")
    env = json.loads(env_file.read_bytes())["environment"]
    env.update(HOME=str(BASE.parents[1]), LC_ALL="C", CARGO_INCREMENTAL="0",
               CARGO_BUILD_JOBS="1", RAYON_NUM_THREADS="1", CARGO_NET_OFFLINE="true")
    for folder in ("target", "cargo", "tmp", "private-census"):
        guarded(BASE/folder, uid, stat.S_ISDIR)
        audit(BASE/folder, uid)
    cargo = BASE / "rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin/cargo"
    cmd = ([cargo, "clippy", "--locked", "--offline", "--timings", "-p", "airk-retrieval", "-p", "airk-inference", "--all-features",
            "--all-targets", "--", "-D", "warnings"] if gate == "clippy" else
           [cargo, "nextest", "run", "--locked", "--offline", "--profile", "ci", "--timings", "-p",
            "airk-retrieval", "-p", "airk-inference", "--all-features", "-E",
            "test(arbiter::runtime::cuda) | test(original_reference_paths_keep_a_bounded_source_root) | test(fp16_compact::) | test(fp16_large::)"])
    if gate == "producer":
        cmd = [cargo, "build", "--locked", "--offline", "--timings", "-p", "airk-retrieval",
               "--features", "test-support,gpu-cuda", "--example", "linux_cuda_provenance"]
    owner = runpy.run_path(str(CPU))
    command = ["/usr/bin/nice", "-n", "10", "/usr/bin/prlimit", "--as=8589934592", "--", *cmd]
    result = owner["run_owned"](command, root, snap / "cargo.log", 1800, env)
    for folder in ("target", "cargo", "tmp", "private-census"):
        audit(BASE/folder, uid)
    audit(snap, uid)
    if any(sha(root / rel) != dig or sha(CHECKOUT / rel) != dig for rel, dig in rows.items()):
        raise ValueError("source drift during Linux gate")
    log = (snap / "cargo.log").read_text(errors="replace")
    passed = owner["successful"](result)
    evidence = {}
    selected = None
    if gate == "nextest":
        match = re.search(r"Summary \[[^\]]+\]\s+(\d+) tests? run: (\d+) passed", log)
        selected = int(match[1]) if match else 0
        passed = passed and match is not None and selected > 0 and int(match[2]) == selected
        report = root/"target/nextest/ci/junit.xml"
        if match and report.is_file():
            shutil.copyfile(report,snap/"nextest-junit.xml")
            evidence["junit"] = sha(snap/"nextest-junit.xml")
    timing = re.search(r"Timing report saved to ([^\n]*?cargo-timing-[0-9A-Za-z-]+\.html)", log)
    if timing:
        report = Path(timing[1])
        if not report.resolve(strict=True).is_relative_to(BASE/"target/cargo-timings"):
            raise ValueError("unexpected Cargo timing output")
        shutil.copyfile(report,snap/"cargo-timing.html")
        evidence["cargo_timing"] = sha(snap/"cargo-timing.html")
    if gate == "producer" and passed:
        example = BASE/"target/debug/examples/linux_cuda_provenance"
        guarded(example,uid,stat.S_ISREG)
        roster = subprocess.run([str(example),"--payload-names"],env=env,text=True,capture_output=True,
                                check=True,timeout=15).stdout.splitlines()
        if len(roster) != len(set(roster)) or len(roster) < 3:
            raise ValueError("native example emitted an invalid payload inventory")
        example_sha = sha(example)
        evidence["example"] = {"path":str(example),"sha256":example_sha,"payload_names":roster}
        retained_root = BASE/"target/airk-linux-cuda-producers"
        directory(retained_root, BASE/"target", uid)
        retained = retained_root / (example_sha + "-linux_cuda_provenance")
        if not retained.exists():
            with example.open("rb") as source, retained.open("xb") as output:
                shutil.copyfileobj(source, output)
            retained.chmod(0o700)
        guarded(retained, uid, stat.S_ISREG)
        if retained.stat().st_nlink != 1 or sha(retained) != example_sha or sha(example) != example_sha:
            raise ValueError("retained GNU provenance author differs from Cargo output")
        evidence["retained_example"] = {"path":str(retained),"sha256":example_sha}
    audit(snap, uid)
    return {"stage": gate, "selected_tests": selected, "reports": evidence, "base": base, "target": env["CARGO_TARGET_DIR"], "incremental": False,
            "compiled_against": env["ORT_LIB_LOCATION"], "overlay": rows, "command": result,
            "log": str(snap / "cargo.log"), "log_sha256": sha(snap / "cargo.log"),
            "success": bool(passed)}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--gate", choices=["clippy", "nextest", "producer"], required=True)
    p.add_argument("--attempt", type=int, required=True)
    a = p.parse_args()
    print(json.dumps(run(a.gate, a.attempt), sort_keys=True))
