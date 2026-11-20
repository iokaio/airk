# SPDX-License-Identifier: Apache-2.0
"""Run a pinned Windows CAL compiler batch under the existing suspended one-child Job owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from phase_reference import AUDIT, CAMPAIGN, ROOT, checked, digest, require

HELPERS = ROOT / "scripts/cuda-native-execution/helpers"
sys.path.insert(0, str(HELPERS))
import reference_owned as job
import reference_parent_no_site_r2 as parent

SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
            "fp16_compact::cal::chronological_original_context_compiler_batch")


def execute(args):
    deadline = time.monotonic() + 120
    conversion = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution.py"))
    token = conversion["windows_owner"]()
    require(token["user_sid"] == token["owner_sid"] == conversion["SID"], "wrong human/default owner")
    image = args.image.resolve(strict=True)
    require(image == args.image and (image.is_relative_to(CAMPAIGN) or image.is_relative_to(ROOT/"target"/"debug"/"deps"))
            and digest(image) == args.image_sha256, "unapproved native image or digest")
    cwd = args.log.parent
    require(args.log.is_absolute() and args.log.resolve() == args.log and cwd.parent == CAMPAIGN
            and not args.log.exists(), "fresh private log required")
    native_pin = {"path": str(image), "sha256": args.image_sha256}
    env = {k:v for k,v in os.environ.items() if not k.startswith(("AIRK_", "CUDA_", "ORT_"))}
    env["CUDA_VISIBLE_DEVICES"] = ""
    env["AIRK_CUDA_CAL_LAUNCH_AUDIT"] = str(CAMPAIGN/"cal100-focus1024-launch-r1"/"cal100-launch-audit.json")
    env["AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256"] = AUDIT
    expected = {}
    inputs = {}
    for model in ("compact", "large"):
        path, sha = getattr(args, model)
        pin = {"path":path,"sha256":sha}
        value = json.loads(checked(pin))
        key = "AIRK_CUDA_CAL_CHRONOLOGICAL_" + model.upper()
        env[key], env[key+"_SHA256"] = path, sha
        inputs[model] = pin
        eligible = value["statuses"].get("eligible", 0)
        count = value["original_requests"]
        require(len(value["rows"]) == count and len(value["phases"]) in range(1,14), "incomplete input")
        expected[model] = f"ContextQwen{model.capitalize()} original_requests={count} native_recompiled={eligible} retained_refusals={count-eligible}"
    def authenticate(api, handle, info, child_deadline):
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, token["user_sid"], native_pin, job.image_proof)
        require(conversion["windows_owner"](process=info.process) == token
                and job.job_snapshot(api,handle) == ([info.pid],1,1), "foreign child or Job")
        return proof
    argv = [str(image), "--exact", SELECTOR, "--ignored", "--nocapture", "--test-threads=1"]
    with open(os.devnull, "rb") as stdin, args.log.open("xb") as log:
        observed = job.run_windows_owned_training(argv, cwd, log, deadline-time.monotonic(), env, stdin, 2 << 30,
                                                   before_resume=authenticate, absolute_deadline=deadline)
    require(args.log.stat().st_size <= 1 << 20, "compiler log exceeded one MiB")
    lines = args.log.read_text(encoding="utf-8").splitlines()
    valid = (parent.settled(observed) and digest(image) == args.image_sha256
             and all(any(line.endswith(summary) for line in lines) for summary in expected.values())
             and any("1 passed; 0 failed;" in line for line in lines))
    return {"passed":valid, "command":argv, "image":native_pin, "inputs":inputs, "summary":expected,
            "launch_audit_sha256":AUDIT, "job":observed, "log_sha256":digest(args.log),
            "source_sha256":digest(Path(__file__)), "NN_invocations":0}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--image", type=Path, required=True)
    p.add_argument("--image-sha256", required=True)
    p.add_argument("--compact", nargs=2, required=True, metavar=("PATH","SHA256"))
    p.add_argument("--large", nargs=2, required=True, metavar=("PATH","SHA256"))
    p.add_argument("--log", type=Path, required=True)
    args = p.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    result = execute(args)
    print(json.dumps(result, allow_nan=False, separators=(",",":")))
    sys.exit(0 if result["passed"] else 1)
