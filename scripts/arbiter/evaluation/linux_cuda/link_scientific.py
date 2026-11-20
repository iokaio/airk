# SPDX-License-Identifier: Apache-2.0
"""Link the Linux scientific test harness to the genuine static CUDA aggregate and stage it beside its descriptor."""
import argparse
import json
import os
from pathlib import Path
import runpy
import stat
import subprocess

from cargo_gate import ENV_PIN, STAGE
from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, REPO, audit, sha
from product_projection import checked_path, pin, write
from stage_payload import copy_checked
from unpack import guarded
from verified_inputs import identity


def checked_doc(path, expected):
    if sha(path) != expected:
        raise ValueError("native producer input changed: " + str(path))
    return json.loads(path.read_bytes())


def run(a):
    uid = identity()
    os.umask(0o077)
    core_path = CAMPAIGN / "linux-cuda-ort-final-core-r1.json"
    assembly = checked_doc(a.assembly, a.assembly_sha256)
    gate = checked_doc(a.source_gate, a.source_gate_sha256)
    if gate["stage"] != "nextest" or gate["success"] is not True or gate["selected_tests"] < 1:
        raise ValueError("original checked GNU source gate required")
    joint = gate.get("source_view")
    root = REPO / "artifacts/v190/final-plan/gnu-joint-source" if joint else STAGE / "source"
    def check_sources():
        if joint:
            import joint_source
            if gate["scope"] != "airk-retrieval,airk-inference" or joint_source.authenticate(root) != joint:
                raise ValueError("scientific GNU signed source differs from native Nextest")
            return
        git = ["/usr/bin/git", "--no-optional-locks", "-C", str(root)]
        head = subprocess.check_output([*git, "rev-parse", "HEAD"], timeout=10).decode().strip()
        if head != gate["base"] or head != "20b893d82fe3c46d591af37285901bc3b2e8ac25":
            raise ValueError("scientific GNU source base differs")
        delta = subprocess.check_output([*git, "status", "--porcelain=v1", "-z", "--untracked-files=all"], timeout=30)
        changes = {os.fsdecode(row[3:]) for row in delta.split(b"\0") if row}
        if not changes <= set(gate["overlay"]):
            raise ValueError("unexpected scientific source outside authenticated overlay")
        if any(sha(root / p) != value for p,value in gate["overlay"].items()):
            raise ValueError("reviewed scientific GNU source changed")
    check_sources()
    core = runpy.run_path(str(REPO / "scripts/cuda-linux-package.py"))
    handoff = core["verify_handoff"](core_path, a.core_sha256, REPO)
    image_root = BASE / "linux-cuda-package-r1/runtime"
    checked_path(image_root, True)
    descriptor = checked_path(Path(assembly["manifest"]))
    observed = assembly["observation"]
    if (observed["fits_empty"] is not True or observed["approved"] is not False
        or observed["manifest"]["sha256"] != assembly["manifest_sha256"] or sha(descriptor) != assembly["manifest_sha256"]):
        raise ValueError("actual empty scientific ELF descriptor required")
    target_manifest = image_root / "airk-cuda-runtime.json"
    def check_retained_runtime():
        first = checked_doc(CAMPAIGN / "linux-cuda-linked-scientific-image-r1.json",
                            "93f408f615625bc58dee947adc0b584faf72deb637aa650c962f373bf16629e5")
        if pin(target_manifest) != first["staged_manifest"] or first["staged_manifest"]["sha256"] != assembly["manifest_sha256"]:
            raise ValueError("retained scientific descriptor differs from the original native assembly")
        if pin(image_root / "airk-inference-scientific") != first["image"]:
            raise ValueError("original linked scientific executable changed")
        for row in json.loads(target_manifest.read_bytes())["closure"]["files"]:
            name = row["name"]
            if Path(name).name != name:
                raise ValueError("expected a scientific payload sibling")
            expected = {"path": str(image_root / name), "sha256": row["sha256"], "bytes": row["bytes"]}
            if pin(image_root / name) != expected:
                raise ValueError("retained scientific payload changed: " + name)
    if joint:
        check_retained_runtime()
    if sha(CPU) != CPU_SHA:
        raise ValueError("original observed child owner required")
    env_file = CAMPAIGN / "linux-driver-fix-checks-r1/gate-preflight.json"
    env = checked_doc(env_file, ENV_PIN)["environment"]
    env.update(handoff["environment"], HOME=str(BASE.parents[1]), LC_ALL="C", CARGO_NET_OFFLINE="true",
               CARGO_BUILD_JOBS="1", RAYON_NUM_THREADS="1", CARGO_INCREMENTAL="0",
               AIRK_APPROVED_LINUX_CUDA_MANIFEST_SHA256=assembly["manifest_sha256"],
               AIRK_STATIC_LINUX_CUDA_BUILD_SHA256=a.core_sha256)
    cargo = BASE / "rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin/cargo"
    output = BASE / ("linux-cuda-scientific-link-r" + str(a.attempt))
    if not 1 <= a.attempt <= 99 or output.exists():
        raise ValueError("fresh native image attempt required")
    output.mkdir(0o700)
    checked_path(output, True)
    tool = runpy.run_path(str(CPU))
    symbols = sorted(core["SYMBOLS"])
    args = ["/usr/bin/nice", "-n", "10", "/usr/bin/prlimit", "--as=8589934592", "--", str(cargo), "rustc", "--locked", "--offline", "--timings",
            "--tests", "--profile", "test", "-p", "airk-inference", "--all-features", "--message-format=json", "--"]
    for symbol in symbols:
        args.extend(["-C", "link-arg=-Wl,--undefined=" + symbol + ",--export-dynamic-symbol=" + symbol])
    log = output / "cargo.log"
    result = tool["run_owned"](args,root,log,1800,env)
    if not tool["successful"](result):
        raise ValueError("GNU scientific link did not settle successfully: " + str(log))
    check_sources()
    found = []
    with log.open() as stream:
        for line in stream:
            if not line.startswith("{"):
                continue
            event = json.loads(line)
            if (event.get("reason") == "compiler-artifact" and event["target"]["name"] == "airk_inference"
                    and event["target"]["kind"] == ["lib"] and event["profile"]["test"] and event["executable"]):
                found.append(Path(event["executable"]))
    if len(found) != 1 or found[0].parent != BASE / "target/debug/deps":
        raise ValueError("Cargo did not report exactly one original native test executable")
    native = found[0]
    guarded(native, uid, stat.S_ISREG)
    core["verify_linked_image"](native)
    original = {"path": str(native), "bytes": native.stat().st_size, "sha256": sha(native)}
    name = "airk-inference-scientific"
    if joint:
        check_retained_runtime()
        name += "-" + original["sha256"]
    image = image_root / name
    if os.path.lexists(image) or (not joint and os.path.lexists(target_manifest)):
        raise ValueError("fresh runtime image and manifest siblings required")
    copy_checked(native, image, original["sha256"], uid)
    image.chmod(0o700)
    if not joint:
        copy_checked(descriptor, target_manifest, assembly["manifest_sha256"], uid)
    if core["verify_handoff"](core_path, a.core_sha256, REPO) != handoff:
        raise ValueError("static GNU build changed during compilation")
    core["verify_linked_image"](image)
    proof = {"success": True, "target": "x86_64-unknown-linux-gnu", "image": pin(image), "cargo_output": original,
             "static_core_build": a.core_sha256, "aggregate_static_sha256": handoff["aggregate_sha256"],
             "manifest_sha256": assembly["manifest_sha256"], "source_gate_sha256": a.source_gate_sha256,
             "assembly_sha256": a.assembly_sha256, "command":result, "link_log_sha256":sha(log), "symbols":symbols}
    if joint:
        proof["source_view"] = joint
    write(output / "image-proof.json", proof)
    audit(output,uid)
    return {"image_proof": pin(output/"image-proof.json"), "image": pin(image), "staged_manifest":pin(target_manifest)}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--core-sha256",required=True)
    p.add_argument("--assembly",type=Path,required=True)
    p.add_argument("--assembly-sha256",required=True)
    p.add_argument("--source-gate",type=Path,required=True)
    p.add_argument("--source-gate-sha256",required=True)
    p.add_argument("--attempt",type=int,default=1)
    print(json.dumps(run(p.parse_args()),sort_keys=True))
