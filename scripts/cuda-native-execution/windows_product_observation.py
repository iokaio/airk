# SPDX-License-Identifier: Apache-2.0
"""Observe public Windows runtime metadata using one authenticated native child."""
import argparse
import ctypes
import json
import os
import runpy
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
ASSEMBLY = "74fb838eca96803b489e7462c9aa87241c88bb610bb0d7c53948987c8032f8ef"
PRODUCT_SOURCE = "31dc8ddd6bbe01895d3ceddc1405810738c4c205a22ef24c07b50bcae57dffae"
SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
            "emit_nonactivating_windows_product_runtime")
RETRIEVAL = "crates/airk-retrieval/src/arbiter/runtime/"
SOURCES = ("crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification.rs",
           RETRIEVAL+"cuda.rs", RETRIEVAL+"cuda/windows.rs", RETRIEVAL+"cuda/provenance/projection.rs",
           RETRIEVAL+"cuda/provenance/projection/product_fresh.rs")


def require(value, reason):
    if not value:
        raise ValueError(reason)


def checked_image(base, build, image, audit):
    record = base.read(build["path"], 32 << 20)
    command = record["command"]
    require(record["schema"] == "airk-readiness-command/1" and record["passed"] is True
            and record["inputs_unchanged"] is True and record["exit_code"] == 0
            and all(item in command for item in ["test", "airk-inference", "--no-run",
                                                "--all-features", "--message-format=json"]),
            "current stable all-feature Inference test build required")
    relative = record["log"]
    require(relative and relative == Path(relative).name, "build log sibling required")
    log = Path(build["path"]).with_name(relative)
    audit([log, *[ROOT / name for name in SOURCES]])
    native = record["groups"]["native"]["files"]
    for name in SOURCES:
        require(base.digest(ROOT / name) == native[name], "current native product emitter differs")
    require(0 < log.stat().st_size <= 32 << 20, "bounded Cargo JSON log required")
    with log.open(encoding="utf-8") as stream:
        events = (json.loads(line) for line in stream if line.startswith("{"))
        paths = [Path(event["executable"]) for event in events if event.get("reason") == "compiler-artifact"
                 and event["target"]["name"] == "airk_inference" and event["profile"]["test"] is True
                 and event.get("executable")]
    require(len(paths) == 1, "unique linked Inference image required")
    audit(paths)
    linked = base.pin(paths[0])
    require(all(linked[k] == image[k] for k in ("sha256", "bytes")), "linked native image drift")
    return {"log": base.pin(log), "artifact": linked, "native_group": record["groups"]["native"]["sha256"],
            "sources": {name: native[name] for name in SOURCES}}


def run(args):
    start = time.monotonic()
    expires = start + 120
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "isolated Windows Python -I -B -S required")
    owner_api = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = owner_api["base"]()
    parent, job = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    require(parent.worker.windows_sid() == base.SID
            and owner_api["token_owner"](ctypes.c_void_p(-1)) == base.SID, "human parent token required")
    def exact(path, sha):
        pin = base.pin(path)
        require(pin["sha256"] == sha, "input SHA differs: "+str(path))
        return pin
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": exact(args.powershell, args.powershell_sha256)}
    def audit(paths, created=True):
        owner_api["guard_many"](guard, paths, created, deadline=expires)
    out = args.output
    require(out.parent == CAMPAIGN and not out.exists(), "fresh private campaign child required")
    source_path = CAMPAIGN / "windows-product-notice-sources-r3/product-sources.json"
    audit([args.input, args.image, args.build, source_path])
    assembly = exact(args.input, ASSEMBLY)
    sources = exact(source_path, PRODUCT_SOURCE)
    image = exact(args.image, args.image_sha256)
    build = exact(args.build, args.build_sha256)
    lineage = checked_image(base, build, image, audit)
    owner_api["mkdir"](guard, out, deadline=expires)
    temp = out / "temp"
    owner_api["mkdir"](guard, temp, deadline=expires)
    output = out / "airk-cuda-runtime.json"
    proof = out / "airk-cuda-build-provenance.json"
    inner = output.with_suffix(".receipt.json")
    log, stdin = out / "stdout.log", out / "stdin"
    audit([output, proof, inner, log, stdin], False)
    with stdin.open("xb"):
        pass
    audit([stdin])
    argv = [image["path"], "--exact", SELECTOR, "--ignored", "--nocapture", "--test-threads=1"]
    env = {k: v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_", "ORT_", "CUDA_", "CUDNN_", "PYTHON"))}
    env.update(PATH=r"C:\Windows\System32;C:\Windows", TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
               AIRK_CUDA_QUALIFICATION_INPUT=assembly["path"], AIRK_CUDA_QUALIFICATION_INPUT_SHA256=ASSEMBLY,
               AIRK_CUDA_PRODUCT_SOURCES=sources["path"], AIRK_CUDA_PRODUCT_SOURCES_SHA256=PRODUCT_SOURCE,
               AIRK_CUDA_QUALIFICATION_OUTPUT=str(output))
    result = {"schema":"airk.private.owned-windows-product-observation/1", "input":assembly, "sources":sources,
              "image":image, "build":build, "lineage":lineage, "command":argv, "complete":False,
              "activation":False, "qualification":False, "parent":base.pin(Path(__file__))}
    def authenticate(api, handle, info, deadline):
        observed = parent.suspended_authentication(api, info, expires, deadline, base.SID, image, job.image_proof)
        require(owner_api["token_owner"](info.process) == base.SID, "child default owner differs")
        return observed
    try:
        with stdin.open("rb") as initial, log.open("xb") as stream:
            result["owned"] = job.run_windows_owned_training(argv, ROOT, stream, expires-time.monotonic(), env,
                initial, 8 << 30, before_resume=authenticate, absolute_deadline=expires)
        require(parent.settled(result["owned"]) and 0 < result["owned"]["peak_host_committed_bytes"] <= 8 << 30,
                "native metadata job did not naturally settle")
        audit([output, proof, inner, log])
        require(log.stat().st_size <= 64 << 20 and all(p.stat().st_size <= 1 << 20 for p in [output, proof, inner]),
                "native output/log cap exceeded")
        value = base.read(inner, 1 << 20)
        result.update(descriptor=base.pin(output), provenance=base.pin(proof), native_emission=base.pin(inner))
        require(value["schema"] == "airk.private.windows-product-runtime-observation/1"
                and all(base.core_pin_matches(value[k],result[key]) for k,key in
                        [("input","input"),("source_input","sources"),("descriptor","descriptor"),("provenance","provenance")])
                and value["pid"] == result["owned"]["owned_leader_pid"] and value["execution_user"] == base.SID
                and all(value[k] is False for k in ["model_requests_dispatched","activation","qualification"])
                and type(value["elapsed_us"]) is int and 0 <= value["elapsed_us"] < 120_000_000,
                "native evidence differs")
        for original in [assembly, sources, image, build, lineage["log"], lineage["artifact"]]:
            base.pinned(original)
        require(not list(temp.iterdir()), "private native control still active")
        result["complete"] = True
    except Exception as exc:
        result["failure"] = {"type":type(exc).__name__, "message":str(exc)}
        if hasattr(exc, "owned_cleanup"):
            result["cleanup"] = exc.owned_cleanup
        raise
    finally:
        if log.exists():
            result["stdout"] = base.pin(log)
        result["elapsed_seconds"] = time.monotonic() - start
        if result["elapsed_seconds"] >= 120:
            result.update(complete=False, deadline_expired=True)
        owner_api["write_new"](guard, out / "execution.json", result)
    require(result["complete"], "original whole observation deadline exceeded")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("output","input","image","build","powershell"):
        parser.add_argument("--"+key, type=Path, required=True)
    for key in ("image","build","powershell"):
        parser.add_argument("--"+key+"-sha256", required=True)
    run(parser.parse_args())
