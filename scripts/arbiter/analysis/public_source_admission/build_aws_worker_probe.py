# SPDX-License-Identifier: Apache-2.0
"""Build and seal the real paired Windows candidate for the canceled-job activation regression."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import stat
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
normal = runpy.run_path(str(HERE / "aws_normal_service.py"))
n, require = normal["n"], normal["require"]
ROOT, E = n["ROOT"], n["CAMPAIGN"]
VOLUME_BASE = 1_233_246_265_344
GIB = 1 << 30


def target_usage(path):
    """Logical file sizes counted per directory entry, including hardlink aliases."""
    pending = [path]
    count = size = 0
    while pending:
        with os.scandir(pending.pop()) as entries:
            for entry in entries:
                if entry.is_dir(follow_symlinks=False):
                    pending.append(entry.path)
                else:
                    count += 1
                    size += entry.stat(follow_symlinks=False).st_size
    return {"logical_bytes": size, "entries": count}


def space():
    return shutil.disk_usage(ROOT).free


def artifact(output, name, stage):
    log = output / ("aws-worker-" + stage + "-r1.log")
    rows = []
    with log.open("rb") as stream:
        for line in stream:
            if line.startswith(b"{"):
                event = json.loads(line)
                if event.get("reason") == "compiler-artifact" and event["target"]["name"] == name and event["executable"]:
                    rows.append(event)
    require(len(rows) == 1, "missing or ambiguous genuine Cargo artifact " + name)
    return rows[0]


R1_CHECKS = {"clippy": "d9f359b46a3a62a645b33a4a9e9ded1dc94a36efb96fa1e20387644316cda6cf",
             "nextest": "ef93098f70759e25a846ea59a31f74338b5c7ddd112ea66bb19a2f66f359d444"}


def admitted_checks(owner, target):
    original = E / "aws-worker-checks-r1"
    n["checked"](original / "owned.json", "b39e978e41bbb9d4f6149c20b3324046eb22154a4f4e46c40786cb6d5b44c89f")
    audit_path = owner.plain(original / "owner-audit.json", regular=True)
    n["checked"](audit_path, "c1305f39f76069e40c09d17bb334fb8d992a36f05eb90e8c9886c9a43c051922")
    audit = json.loads(audit_path.read_bytes())
    require(audit["exit_code"] == 1 and audit["paths"] > 0, "original custody receipt absent")
    source = runpy.run_path(str(ROOT / "scripts/dev/receipt_runner.py"))["observe"](["native"])
    for stage, seal in R1_CHECKS.items():
        receipt = original / ("aws-worker-" + stage + "-r1.json")
        n["checked"](owner.plain(receipt, regular=True), seal)
        result = json.loads(receipt.read_bytes())
        require(result["passed"] and result["inputs_unchanged"] and result["exit_code"] == 0
                and Path(result["target_directory"]) == target and result["groups"] == source,
                "prior native source or validated scope changed")
    return dict(R1_CHECKS)


def worker(w, owner, output, resume):
    target = ROOT / "target"
    start_free = space()
    require(Path(os.environ["CARGO_HOME"]).resolve() == E / "windows-offline-cargo-r1/home"
            and Path(os.environ["TEMP"]).resolve() == output / "tmp" and os.environ["CARGO_NET_OFFLINE"] == "true"
            and os.environ["CARGO_BUILD_JOBS"] == "1", "original D-only Cargo settings required")
    budget = (3 if resume else 6) * GIB
    require(Path(os.environ["CARGO_TARGET_DIR"]).resolve() == target
            and VOLUME_BASE - start_free + budget < 120 * GIB, "tooling admission would exceed its cap")
    measure = runpy.run_path(str(HERE / "aws_windows_allocation.py"))["allocation"]
    roots = {"target": target, "cargo_home": Path(os.environ["CARGO_HOME"]), "evidence": output}
    allocation_before = measure(roots)
    baseline = target_usage(target)
    w["write_json"](output / "disk-start.json", {"D_free": start_free, "original_D_free": VOLUME_BASE,
                    "target": baseline, "cache": target_usage(Path(os.environ["CARGO_HOME"])),
                    "allocation": allocation_before, "volume_budget": budget})
    lower = start_free - budget
    minimum = start_free
    common = ["--locked", "--offline", "--timings", "--no-default-features"]
    checks = [
        ("clippy", ["clippy", *common, "-p", "airk-store", "-p", "airk-supervisor", "-p", "airkctl",
                    "--lib", "--tests", "--", "-D", "warnings"]),
        ("nextest", ["nextest", "run", "--profile", "ci", *common, "-p", "airk-store", "-p", "airk-supervisor",
                     "--lib", "-E", "test(service::registry::recovery::tests) | test(project_worker::diagnostic_tests)"]),
        ("pair", ["build", *common, "--message-format=json-render-diagnostics", "-p", "airk-cli", "-p", "airkctl",
                  "--bin", "airk", "--bin", "airkctl"]),
        ("starter", ["test", "--no-run", *common, "--message-format=json-render-diagnostics",
                     "-p", "airkctl", "--test", "start_retained_fixture"]),
    ]
    stages = admitted_checks(owner, target) if resume else {}
    origins = {stage: "aws-worker-checks-r1" for stage in stages}
    with normal["capped"](w, output, 4 * GIB) as (member, _):
        for stage, command in checks:
            if stage in stages:
                continue
            if stage != "clippy" and space() < lower + 512 * (1 << 20):
                raise ValueError("insufficient remaining original disk admission for next stage")
            name = "aws-worker-" + stage + "-r1"
            args = [sys.executable, "-I", "-B", "-S", str(ROOT / "scripts/dev/receipt_runner.py"),
                    "--groups", "native", "--out", str(output), "--target-dir", str(target),
                    "--fixture-base", str(output / "fixture"), name, "--", os.environ["CARGO"], *command]
            process = subprocess.Popen(args, cwd=ROOT)
            member(process.pid)
            # The outer owned Job reaps any surviving children on failure or wall expiry.
            while process.poll() is None:
                minimum = min(minimum, space())
                if minimum < lower + 256 * (1 << 20):
                    sys.exit("stopping before original D growth ceiling")
                try:
                    process.wait(timeout=0.25)
                except subprocess.TimeoutExpired:
                    continue
            require(process.returncode == 0, "native check failed: " + stage)
            receipt = output / (name + ".json")
            data = w["read_json"](receipt)
            require(data["passed"] and data["inputs_unchanged"] and data["exit_code"] == 0
                    and Path(data["target_directory"]) == target, "native check identity changed")
            stages[stage] = owner.digest(receipt)
            origins[stage] = output.name
        pair = output / "pair"
        pair.mkdir()
        built = {}
        for binary, stage in (("airk", "pair"), ("airkctl", "pair"), ("start_retained_fixture", "starter")):
            event = artifact(output, binary, stage)
            source = owner.plain(Path(event["executable"]))
            require(source.is_relative_to(target) and stat.S_ISREG(source.lstat().st_mode),
                    "Cargo artifact is not a regular file in the shared target")
            linked = pair / (binary + ".exe")
            require(space() - source.stat().st_size > lower + (64 << 20), "staging exceeds disk ceiling")
            with source.open("rb") as src, linked.open("xb") as dst:
                shutil.copyfileobj(src, dst, 1 << 20)
            digest = owner.digest(owner.plain(linked, regular=True))
            require(digest == owner.digest(source), "staged native image differs")
            built[binary] = {"path": linked.name, "sha256": digest, "bytes": linked.stat().st_size,
                             "compiler_artifact": event}
        minimum = min(minimum, space())
        w["write_json"](output / "pair-proof.private.json", {"executables": built, "receipts": stages,
                        "receipt_origins": origins, "source_root": str(ROOT), "profile": "no-default-features dev", "local_fixture_only": True})
        w["write_json"](output / "disk-end.json", {"target": target_usage(target), "minimum_D_free": minimum,
                        "peak_volume_growth": start_free - minimum, "final_volume_growth": start_free - space(),
                        "allocation": measure(roots)})


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--output", type=Path, required=True)
    args.add_argument("--worker", action="store_true")
    args.add_argument("--resume-required-pair", action="store_true", help="reuse the sealed successful R1 checks")
    opts = args.parse_args()
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    output = owner.plain(opts.output)
    require(output.parent == E and output.name.startswith("aws-worker-checks-"), "wrong private root")
    if opts.worker:
        require(os.environ["AIRK_AWS_NATIVE_BUILD"] == str(output), "owned worker required")
        worker(w, owner, output, opts.resume_required_pair)
    else:
        require(output.exists() and {x.name for x in output.iterdir()} == {"tmp", "fixture"}, "fresh preflight required")
        wall = min(2400, int(1795194600 - time.time()))
        require(wall > 120, "insufficient campaign time")
        command = [sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"]
        result = owner.run_evaluation_fixture(command, ROOT, output / "outer.log", wall,
                                             os.environ | {"AIRK_AWS_NATIVE_BUILD": str(output)})
        w["write_json"](output / "owned.json", result)
        normal["stage"]["require_settlement"](w, result)


if __name__ == "__main__":
    main()
