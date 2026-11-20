# SPDX-License-Identifier: Apache-2.0
"""Run the two native model-free selectors on an authenticated Linux package stage."""
import argparse
import json
import os
from pathlib import Path
import runpy
import stat

from configure_ort import BASE, CPU, CPU_SHA, audit, sha
from unpack import guarded
from verified_inputs import identity

NAMESPACE = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"


def run(options):
    uid = identity()
    os.umask(0o077)
    if sha(CPU) != CPU_SHA or sha(options.stage) != options.stage_sha256:
        raise ValueError("unreviewed process owner or payload stage")
    stage = json.loads(options.stage.read_bytes())
    root = BASE / "linux-cuda-package-r1"
    runtime = root / "runtime"
    image = options.executable
    if Path(stage["runtime"]) != runtime or image.parent != BASE / "target/debug/deps":
        raise ValueError("unexpected native execution input root")
    for folder in (BASE.parents[1], BASE.parent, BASE, BASE / "tmp", root, runtime, image.parent):
        guarded(folder, uid, stat.S_ISDIR)
    guarded(image, uid, stat.S_ISREG)
    if sha(image) != options.executable_sha256 or not 1 <= options.attempt <= 99:
        raise ValueError("unreviewed Linux metadata emitter or attempt")
    out = root / ("assembly-" + str(options.attempt))
    out.mkdir(0o700)
    guarded(out,uid,stat.S_ISDIR)
    request = out / "request.json"
    descriptor = out / "scientific.json"
    producer = "airk-linux-cuda-build-provenance.json"
    if sha(runtime / producer) != stage["provenance"]:
        raise ValueError("staged provenance changed")
    env = {"PATH":"/usr/bin:/bin", "LC_ALL":"C", "HOME":str(BASE.parents[1]), "TMPDIR":str(BASE / "tmp")}
    owner = runpy.run_path(str(CPU))
    records = []
    def selector(name, extra, deadline):
        select = NAMESPACE + name
        log = out / (name + ".log")
        result = owner["run_owned"]([str(image), "--exact", select, "--ignored", "--nocapture", "--test-threads=1"],
                                     out, log, deadline, dict(env, **extra))
        audit(out,uid)
        if not owner["successful"](result) or "1 passed; 0 failed" not in log.read_text():
            raise ValueError("native selector failed: " + str(log))
        records.append({"command":result, "log_sha256":sha(log)})
    selector("emit_cuda_component_probe_request", {
        "AIRK_CUDA_REQUEST_KIND":"assembly", "AIRK_CUDA_RUNTIME_ROOT":str(runtime),
        "AIRK_CUDA_PRODUCER_NAME":producer, "AIRK_CUDA_PRODUCER_SHA256":stage["provenance"],
        "AIRK_CUDA_MODULE_NAMES":";".join(stage["module_names"]),
        "AIRK_CUDA_LOAD_ORDER":";".join(stage["load_order"]),
        "AIRK_CUDA_SYSTEM_IMPORTS":";".join(stage["system_imports"]),
        "AIRK_CUDA_DEADLINE_MS":"120000", "AIRK_CUDA_REQUEST_OUTPUT":str(request)}, 30)
    selector("emit_cuda_runtime_manifest_from_held_native_closure", {
        "AIRK_CUDA_QUALIFICATION_INPUT":str(request), "AIRK_CUDA_QUALIFICATION_INPUT_SHA256":sha(request),
        "AIRK_CUDA_QUALIFICATION_OUTPUT":str(descriptor)}, 125)
    receipt = descriptor.with_suffix(".receipt.json")
    observation = json.loads(receipt.read_bytes())
    if (observation["manifest"]["sha256"] != sha(descriptor) or observation["input"]["sha256"] != sha(request)
        or observation["fits_empty"] is not True or observation["approved"] is not False
        or observation["model_requests_dispatched"] is not False or observation["runtime_admission_dispatched"] is not False):
        raise ValueError("native metadata emitter did not confirm its actual inputs and outputs")
    audit(out,uid)
    audit(BASE / "tmp",uid)
    return {"stage":options.stage_sha256, "executable":sha(image), "commands":records, "observation":observation,
            "receipt_sha256":sha(receipt), "manifest":str(descriptor), "manifest_sha256":sha(descriptor)}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--stage", type=Path, required=True)
    p.add_argument("--stage-sha256", required=True)
    p.add_argument("--executable", type=Path, required=True)
    p.add_argument("--executable-sha256", required=True)
    p.add_argument("--attempt", type=int, default=1)
    print(json.dumps(run(p.parse_args()), sort_keys=True))
