# SPDX-License-Identifier: Apache-2.0
"""Independently recompile both protected models' closed R7 inputs without inference."""
import argparse
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from r7_chronological_inputs import AUDIT, SCHEMA, SERVICE, original
from reference_inputs import CAMPAIGN, ROOT, digest, parsed, read, reference, require
from run_compiler_batch import SELECTOR, job, parent

BUILD = CAMPAIGN / "windows-r6-chronological-checks-r4/large-image-r1.json"
CAL = ("crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/"
       "cuda_qualification/fp16_compact/cal.rs")


def emitted_image(build_ref, log_ref, owner_ref, selected):
    receipt = read(build_ref, BUILD)
    require(receipt["passed"] is True and receipt["inputs_unchanged"] is True and receipt["exit_code"] == 0
            and Path(receipt["target_directory"]) == ROOT / "target"
            and receipt["groups"]["native"]["files"][CAL] == digest((ROOT / CAL).read_bytes())
            and receipt["log"] == "large-image-r1.log", "unverified chronological compiler build or Rust source")
    owner = read(owner_ref, BUILD.parent / "owned-image/execution.json")
    require(owner["exit_code"] == 0 and owner["child_assigned_before_resume"]
            and owner["owned_descendants_settled"] and owner["owned_job_final_active_processes"] == 0
            and owner["owned_job_final_member_pids"] == [] and not owner["owned_job_forced_termination"]
            and not owner["timed_out"] and not owner["log_oversized"] and str(BUILD.parent) in owner["argv"]
            and "--worker" in owner["argv"] and "image" in owner["argv"],
            "compiler image did not originate in the settled build Job")
    log = BUILD.with_name(receipt["log"])
    require(Path(log_ref["path"]) == log and log_ref["bytes"] <= 4 << 20,
            "foreign chronological compiler log")
    text = log.read_bytes()
    require(len(text) == log_ref["bytes"] and digest(text) == log_ref["sha256"], "compiler log changed")
    emitted = []
    for line in text.splitlines():
        if line.startswith(b"{"):
            row = parsed(line)
            if (row.get("reason") == "compiler-artifact" and row["target"]["name"] == "airk_inference"
                    and row["profile"]["test"] and row.get("executable")):
                emitted.append(Path(row["executable"]))
    require(emitted == [selected] and selected.resolve(strict=True) == selected
            and selected.parent == ROOT / "target/debug/deps", "compiler did not emit the selected native image")
    return selected


def projection_inputs(projection_ref):
    path = Path(projection_ref["path"])
    require(path.parent.parent == CAMPAIGN and path.name == "projection-receipt.json",
            "foreign R7 projection receipt path")
    projection = read(projection_ref, path)
    count = projection["original_requests"]
    audit = projection["launch_audit"]
    require(projection["schema"] == SCHEMA and projection["service_sha256"] == SERVICE
            and audit["sha256"] == AUDIT
            and projection["source_sha256"] == digest((HERE / "r7_chronological_inputs.py").read_bytes())
            and projection["NN_invocations"] == projection["gain_certificates"] == 0
            and count in (56, 100) and (count == 100) == bool(projection["terminal"]),
            "foreign or incomplete R7 projection")
    require(read(audit)["inputs"]["service"]["sha256"] == SERVICE, "R7 compiler audit changed")
    env = {"AIRK_CUDA_CAL_LAUNCH_AUDIT": audit["path"], "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT}
    expected, phases = {}, None
    for model in ("compact", "large"):
        pin = projection["models"][model]
        batch = read(pin, path.with_name(model + ".json"))
        manifest_id, manifest = original.MODELS[model]
        eligible = batch["statuses"].get("eligible", 0)
        require(batch["model"] == manifest_id and batch["original_manifest"] == manifest
                and batch["launch_audit_sha256"] == AUDIT and batch["original_requests"] == count
                and batch["phase_plan_sha256"] == projection["plan_sha256"] and batch["NN_invocations"] == 0
                and [row["ordinal"] for row in batch["rows"]] == list(range(count))
                and len(batch["phases"]) == (7 if count == 56 else 13)
                and (phases is None or phases == batch["phases"]) and 0 < eligible <= count,
                "model batch differs from the original closed R7 projection")
        phases = batch["phases"]
        key = "AIRK_CUDA_CAL_CHRONOLOGICAL_" + model.upper()
        env[key], env[key + "_SHA256"] = pin["path"], pin["sha256"]
        expected[model] = (f"ContextQwen{model.capitalize()} original_requests={count} "
                           f"native_recompiled={eligible} retained_refusals={count - eligible}")
    return env, expected


def file_sha(path):
    import hashlib

    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def execute(args):
    deadline = time.monotonic() + 120
    conversion = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution.py"))
    token = conversion["windows_owner"]()
    require(token["user_sid"] == token["owner_sid"] == conversion["SID"], "wrong human default owner")
    build = reference(BUILD, args.build_sha256)
    cargo_log = reference(BUILD.with_name("large-image-r1.log"), args.build_log_sha256)
    owner = reference(BUILD.parent / "owned-image/execution.json", args.build_owner_sha256)
    image = emitted_image(build, cargo_log, owner, args.image)
    require(file_sha(image) == args.image_sha256, "native compiler image bytes changed")
    native = {"path": str(image), "sha256": args.image_sha256}
    projection = reference(args.projection, args.projection_sha256)
    settings, expected = projection_inputs(projection)
    out = args.output
    require(out.resolve(strict=True) == out and out.parent == CAMPAIGN and not any(out.iterdir()),
            "fresh guarded R7 compiler output required")
    names = ["native.log", "native.json"]
    original.check_outputs(out, names, created=False)
    temp = Path(os.environ["TEMP"])
    require(temp == Path(os.environ["TMP"]) and temp == Path(os.environ["TMPDIR"])
            and temp.resolve(strict=True) == temp and temp.parent == CAMPAIGN
            and temp.name == out.name + "-temp", "matched private D compiler scratch required")
    original.check_outputs(temp.parent, [temp.name], created=True)
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "CUDA_", "ORT_"))}
    env.update(settings, CUDA_VISIBLE_DEVICES="")
    log = out / "native.log"

    def authenticate(api, handle, info, child_deadline):
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, token["user_sid"], native, job.image_proof)
        require(conversion["windows_owner"](process=info.process) == token
                and job.job_snapshot(api, handle) == ([info.pid], 1, 1), "foreign child or unexpected Job membership")
        return proof

    argv = [str(image), "--exact", SELECTOR, "--ignored", "--nocapture", "--test-threads=1"]
    with open(os.devnull, "rb") as stdin, log.open("xb") as stdout:
        observed = job.run_windows_owned_training(
            argv, out, stdout, deadline - time.monotonic(), env, stdin, 2 << 30,
            before_resume=authenticate, absolute_deadline=deadline)
    original.check_outputs(out, ["native.log"], created=True)
    require(log.stat().st_size <= 1 << 20, "native compiler log limit exceeded")
    lines = log.read_text(encoding="utf-8").splitlines()
    passed = (parent.settled(observed) and file_sha(image) == native["sha256"]
              and projection_inputs(projection) == (settings, expected)
              and all(sum(line.endswith(summary) for line in lines) == 1 for summary in expected.values())
              and any("1 passed; 0 failed;" in line for line in lines))
    result = {"passed": passed, "command": argv, "image": native, "projection": projection,
              "build": build, "build_log": cargo_log, "build_owner": owner, "summary": expected, "launch_audit_sha256": AUDIT,
              "job": observed, "log_sha256": file_sha(log), "source_sha256": file_sha(Path(__file__)),
              "NN_invocations": 0}
    with (out / "native.json").open("x", encoding="utf-8") as stream:
        json.dump(result, stream, allow_nan=False)
    original.check_outputs(out, ["native.json"], created=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("image", "projection", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("image", "projection", "build", "build-log", "build-owner"):
        parser.add_argument("--" + name + "-sha256", required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    result = execute(args)
    print(json.dumps(result, allow_nan=False))
    sys.exit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
