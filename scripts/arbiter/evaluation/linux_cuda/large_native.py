# SPDX-License-Identifier: Apache-2.0
"""Launch the Linux Large metadata or disposable graph selector and retain native resource evidence."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time
import uuid

from configure_ort import BASE, CAMPAIGN, REPO, audit, sha
from fp16_supervisor import Adapter, guarded_observation, verified_file
from large_reference import ORIGINAL
from product_projection import checked_path, image_proof, pin, write
from verified_inputs import identity

HOST = 48 << 30
DEVICE = 44 << 30
MODULE = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_large::"
SELECTORS = {"metadata":"emit_cuda_large_fp16_probe_metadata", "graph":"cuda_large_fp16_load_only_graph_diagnostic"}
CPU_INPUT = "d89161fac7bd3b12a716588d2fc65549b0bcc16aa53ae1abdb8fc14a8bea4b7a"


def confirm_graph(value, parent, metadata, request, driver, helpers):
    if (value["schema"] != "airk.private.cuda-graph-metadata-diagnostic/1" or value["outcome"] != "graph_metadata_observed"
        or any(value[name] is not True for name in ("diagnostic_complete", "session_created", "session_load_dispatched",
                "native_host_charge_release_observed", "native_control_cleanup_observed", "strict_production_unchanged"))
        or any(value[name] is not False for name in ("forward_dispatched", "model_requests_dispatched", "qualification",
                "product_admission", "scorer_returned", "operations_journal_created", "process_cuda_poisoned",
                "native_control_retained_for_unknown_cuda")) or value["forward_count_observed"] is not None
        or value["profile"] != metadata["profile"] or value["source_manifest"] != metadata["source_manifest"]
        or value["native_input"] != request["gpu_input"] or value["adapter_before"]["uuid"] != list(driver.bytes)):
        raise ValueError("incomplete Large graph, resource settlement, scope or adapter identity")
    logs = value["logs"]
    if (type(logs["truncated"]) is not bool or value["selected_logs_complete"] is not (not logs["truncated"])
        or len(logs["rows"]) != logs["retained"] or logs["selected"] != logs["retained"] + logs["omitted"]
        or logs["omitted"] < 0 or (logs["omitted"] and not logs["truncated"])):
        raise ValueError("Large selected diagnostic log coverage differs")
    parent["selected_logs_complete"] = value["selected_logs_complete"]
    parent["selected_logs_omitted"] = logs["omitted"]
    drop = value["teardown"]
    if (drop["pending_claims"] != 0 or drop["poisoned"] or drop["errors"] or any(drop[k] is not True for k in
            ("pre_sync_observed","session_drop_attempted","session_drop_returned","post_sync_observed"))):
        raise ValueError("Large graph device did not settle")
    helpers[0]["validate_graph_metadata"](value["graph_metadata"],value["placement"])
    helpers[1]["validate_large_attention"](value["placement"])
    parent["observed_large_cpu_values"] = helpers[2]["cpu_controls"](value["graph_metadata"])


def run(a):
    uid = identity()
    os.umask(0o077)
    if any(n.startswith("LD_") or n in {"GLIBC_TUNABLES","CUDNN_FRONTEND_CUDART_LIB_NAME"} for n in os.environ):
        raise ValueError("ambient loader overrides present")
    image = checked_path(a.image)
    proof = image_proof(a.image_proof, a.image_proof_sha256, pin(image))
    request_pin = pin(checked_path(a.request))
    if request_pin["sha256"] != a.request_sha256 or request_pin["bytes"] > 1 << 20:
        raise ValueError("native Large request differs")
    request = json.loads(a.request.read_bytes())
    bounds = request["bounds"]
    if (bounds["host_limit_bytes"], bounds["device_limit_bytes"], bounds["deadline_ms"], request["job_limit_bytes"],
        request["cold_bytes"], request["warm_bytes"], request["headroom_bytes"]) != (HOST,DEVICE,120000,HOST,40<<30,8<<30,4<<30):
        raise ValueError("original Large process, device, or wall budgets differ")
    manifest = pin(image.parent / "airk-cuda-runtime.json")
    def small_pin(row):
        return {k:row[k] for k in ("path","sha256")}
    if request["runtime_manifest"] != small_pin(manifest) or manifest["sha256"] != proof["manifest_sha256"]:
        raise ValueError("native request, manifest, or compiled approval changed")
    metadata = None
    backing = 0
    helpers = ()
    if a.mode == "metadata":
        if request["kind"] != "prepare" or request["cpu_input"]["sha256"] != CPU_INPUT:
            raise ValueError("original Linux Large CPU preparation required")
    else:
        meta = pin(checked_path(a.metadata))
        if meta["sha256"] != a.metadata_sha256 or meta["bytes"] > 1 << 20:
            raise ValueError("native Large metadata differs")
        metadata = json.loads(a.metadata.read_bytes())
        if (request["kind"] != "probe" or metadata["probe_request"] != small_pin(request_pin)
            or metadata["qualified"] is not False or metadata["native_binary_sha256"] != proof["image"]["sha256"]):
            raise ValueError("observed Large metadata differs from the prepared probe")
        backing = metadata["private_elf_backing_bytes"]
        if type(backing) is not int or not 0 < backing < HOST or backing % os.sysconf("SC_PAGE_SIZE"):
            raise ValueError("native ELF page reserve differs")
        helper_paths = [REPO/"scripts/cuda-component-execution.py", REPO/"scripts/arbiter-fp16-execution/probe_large.py",
                        REPO/"scripts/arbiter-fp16-execution/large_placement.py"]
        helper_pins = [sha(p) for p in helper_paths]
        if helper_pins != a.validators:
            raise ValueError("reviewed typed graph, attention, or CPU diagnostic owner differs")
        helpers = [runpy.run_path(str(p)) for p in helper_paths]
    out = checked_path(a.directory,True)
    if list(out.iterdir()):
        raise ValueError("fresh private run directory required")
    scratch = out/"tmp"
    scratch.mkdir(0o700)
    guard = Path(__file__).resolve().with_name("fp16_process.py")
    native = out/"native.json"
    env = {"HOME":str(BASE.parents[1]),"PATH":"/usr/bin:/bin","LC_ALL":"C","TMPDIR":str(scratch),
           "AIRK_PRIVATE_CENSUS_BASE":str(scratch),"CUDA_CACHE_PATH":str(scratch),"ORT_CUDA_GEMM_OPTIONS":"2",
           "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO":"1","AIRK_CUDA_QUALIFICATION_INPUT":str(a.request),
           "AIRK_CUDA_QUALIFICATION_INPUT_SHA256":a.request_sha256,"AIRK_CUDA_QUALIFICATION_OUTPUT":str(native)}
    if a.mode == "metadata":
        ref = CAMPAIGN/"large-fp16-baseline-independent-t32-r2/reference.json"
        if sha(ref) != ORIGINAL:
            raise ValueError("original Large CPU reference changed")
        directory = out/"metadata"
        if directory.exists():
            raise ValueError("fresh destination for native metadata selector required")
        env.update(AIRK_CUDA_METADATA_DIRECTORY=str(directory),AIRK_CUDA_FP16_INDEPENDENT_PROOF=str(ref),
                   AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256=ORIGINAL)
    parent = {"mode":a.mode,"qualified":False,"request":request_pin,"image":pin(image),"uid":uid,"forced_termination":False,
              "address_space_limit":HOST-backing,"private_elf_backing_bytes":backing,"device_limit":DEVICE,"samples":[]}
    proc = gpu = None
    try:
        deadline = time.monotonic()+120
        if a.mode == "graph":
            gpu = Adapter(a.gpu_uuid)
            baseline = parent["baseline_adapter"] = gpu.sample()
            if baseline["free"] < DEVICE:
                raise ValueError("insufficient actual device headroom for the original Linux Large budget")
        with verified_file(image,proof["image"]["sha256"]) as held, (out/"stdout.log").open("xb") as log:
            command = [sys.executable,"-IB",str(guard),str(os.getpid()),str(HOST-backing),str(held.fileno()),str(image),
                       "--exact",MODULE+SELECTORS[a.mode],"--ignored","--nocapture","--test-threads=1"]
            proc = subprocess.Popen(command, env=env,cwd=out,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,
                                    pass_fds=(held.fileno(),),start_new_session=True)
            parent["pid"] = proc.pid
            if gpu is None:
                status = proc.wait(timeout=max(0,deadline-time.monotonic()))
            else:
                status = guarded_observation(proc,gpu,deadline,baseline,parent["samples"],parent,device_limit=DEVICE)
                parent["settled_adapter"] = gpu.sample()
        parent["natural_exit_status"] = status
        log = out/"stdout.log"
        if status or time.monotonic()>deadline or log.stat().st_size>64<<20 or b"test result: ok. 1 passed;" not in log.read_bytes():
            raise ValueError("Linux Large selector execution or deadline differs")
        checked_path(native)
        if native.stat().st_size > 16<<20:
            raise ValueError("Large native output cap")
        value = json.loads(native.read_bytes())
        if (value["pid"] != proc.pid or value["execution_user"] != str(uid) or value["native_binary_sha256"] != parent["image"]["sha256"]
            or value["request"] != small_pin(request_pin) or value["runtime_manifest"] != small_pin(manifest)):
            raise ValueError("native process or consumed request differs")
        if a.mode == "graph":
            confirm_graph(value,parent,metadata,request,uuid.UUID(a.gpu_uuid.removeprefix("GPU-")),helpers)
            if [sha(p) for p in helper_paths] != a.validators:
                raise ValueError("Large graph validators changed")
        else:
            if (value["qualified"] is not False or value["model_requests_dispatched"] is not False or
                value["cpu_input"]["sha256"] != CPU_INPUT or value["independent_proof"]["sha256"] != ORIGINAL):
                raise ValueError("native Linux Large metadata scope differs")
            recorded = {key:pin(checked_path(Path(value[key]["path"]))) for key in
                        ("probe_request","native_input","profile_pin","source_manifest","calibration")}
            if any(small_pin(row) != value[k] or Path(row["path"]).parent != directory for k,row in recorded.items()):
                raise ValueError("Linux Large original metadata bytes differ")
            if {Path(r["path"]).name for r in recorded.values()} != {p.name for p in directory.iterdir()}:
                raise ValueError("Linux Large metadata roster incomplete")
        parent["native"] = pin(native)
        parent["parent_complete"] = True
    finally:
        if proc is not None and proc.poll() is None:
            proc.kill()
            proc.wait(timeout=10)
            parent["forced_termination"] = True
        if gpu is not None:
            gpu.close()
        write(out/"parent.json",parent)
        audit(out,uid)
    return parent


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=tuple(SELECTORS))
    for k in ("image","image-proof","request","directory"):
        p.add_argument("--"+k,type=Path,required=True)
    for k in ("image-proof-sha256","request-sha256"):
        p.add_argument("--"+k,required=True)
    p.add_argument("--metadata",type=Path)
    p.add_argument("--metadata-sha256")
    p.add_argument("--gpu-uuid")
    p.add_argument("--validators",nargs=3)
    print(json.dumps(run(p.parse_args()),sort_keys=True))
