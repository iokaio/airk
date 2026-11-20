# SPDX-License-Identifier: Apache-2.0
"""Score the complete authentic R5 Compact panel with the original native FP32 CPU scorer."""
import argparse
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from close_original import ORIGINAL, PROFILE, close
from cutlass_study_inputs import AUDIT, CAMPAIGN, chronology, digest, future_rows, json_pin, require, require_full, settled, verified_certificate
from run_compiler_batch import ROOT, job, parent
from run_cutlass_study import image_from

MODEL = "arbiter-context-qwen3-17b"
SELECTOR = "inference_owner::context_action_tests::selected::selected_context_cal_complete_readout_raw_scores"
HOST = 32 << 30
INPUT_RECEIPT = CAMPAIGN/"cal100-focus1024-launch-r1/cal100-launch-audit.json"
KERNEL_IMAGE = "ecbbc8c4a63876c76bb0a7a9866328baf04847998995dc512f735e247d100cc3"


def pin(path):
    return {"path":str(path),"sha256":digest(path)}


def compiled_rows(projected, certificates):
    require_full(projected)
    require([r["id"] for r in certificates] == [r["id"] for r in projected], "foreign original CAL order")
    result = []
    for row,source in zip(projected,certificates,strict=True):
        native = {"id":row["id"],"split":"calibration","source_group":source["source_group"]}
        request = source["request"]
        if request is None:
            require(row["status"] == "no_native_assessment_request","invented CPU request")
            native["status"] = row["status"]
        else:
            census, = [c for c in source["compiler_census"] if c["profile"] == ORIGINAL]
            require(row["roster"] == request["roster"] and row["remaining_ms"] == source["remaining_ms_at_observation"],
                    "original request roster or CPU allowance differs")
            native["roster"] = row["roster"]
            if row["status"] == "compiler_refused_remaining_allowance":
                require(not census["fits_remaining_allowance"] or row["remaining_ms"] <= 0, "unjustified compiler refusal")
                native.update(status=row["status"],census=census)
            else:
                require(row["status"] == "eligible" and census["fits_remaining_allowance"] and
                        row["compiled"] == census["compiled"] and row["remaining_ms"] > 0, "unexpected eligible CPU input")
                native.update(status="compiled_original_input",compiled=row["compiled"],remaining_ms=row["remaining_ms"])
        result.append(native)
    return result


def prepare(descriptor):
    full = json_pin(descriptor["full"])
    require(len(full["terminal"]) == 3 and len(full["lifetimes"]) == 2, "original R5 whole closure required")
    rows,_ = future_rows(descriptor["full"])
    pre = verified_certificate(descriptor["certificate"],full["terminal"])
    audit_ref = {"path":str(INPUT_RECEIPT),"sha256":AUDIT}
    audit = json_pin(audit_ref)
    model = full["models"]["compact"]
    compiler = json_pin(full["original_compiler_verification"])
    require(compiler["passed"] is True and settled(compiler["job"]) and compiler["NN_invocations"] == 0 and
            compiler["launch_audit_sha256"] == AUDIT and compiler["image"]["sha256"] ==
            "b3d20acfea363da2afd1780078e07f46f4c3bcd52da5515aa4ed29c0cc192351" and compiler["inputs"]["compact"] == {k:model[k] for k in ("path","sha256")},
            "missing authentic full original compiler")
    roster = audit["inputs"]["inputs"]
    original, = [r for r in json_pin(roster) if r["profile_sha256"] == PROFILE]
    profile = {"path":original["profile"],"sha256":PROFILE}
    p = json_pin(profile)
    require(p["model"] == MODEL and p["manifest"] == ORIGINAL and p["runtime"] == "onnxruntime-cpu/1.30.0" and
            p["qualification"] is None and p["memory_reservation_bytes"] == 16<<30 and
            p["cpu_fit"] == {"minimum_physical_cores":4,"intra_threads":4,"inter_threads":1,"sequential":True,"spinning":False},
            "wrong original numeric FP32 adapter")
    prepared = {"schema":"airk.private.compact-cal-original-compiled-inputs/1","complete":True,"NN_invocations":0,
                "manifest":ORIGINAL,"original_profile":profile,"launch_audit":audit_ref,"preflight":descriptor["certificate"],
                "capture_sha256":pre["capture_sha256"],"full_projection":descriptor["full"],
                "rows":compiled_rows(rows,pre["rows"])}
    return prepared,roster,pre["corpus_sha256"]


def execute(descriptor, out):
    start = time.monotonic()
    deadline = start + 1200
    require(out == out.resolve(strict=True) and out.parent == CAMPAIGN and not any(out.iterdir()),"fresh human output required")
    names = ["native.log","control","original-compiled.json","raw.json","owned.json","execution.json"]
    chronology.check_outputs(out,names,created=False)
    image = image_from(descriptor)
    require(image["sha256"] == KERNEL_IMAGE, "wrong retained original Kernel executable")
    build = json_pin(descriptor["build"])
    for owner in ("context_action_tests/selected.rs","context_action_tests.rs"):
        name = "crates/airk-kernel/src/inference_owner/"+owner
        require(digest(ROOT/name) == build["groups"]["native"]["files"][name],"original selected native owner changed")
    prepared,roster,corpus = prepare(descriptor)
    def write(name,data):
        with (out/name).open("x",encoding="utf-8") as file:
            json.dump(data,file,separators=(",",":"),allow_nan=False)
        chronology.check_outputs(out,[name],created=True)
    write("original-compiled.json",prepared)
    control = out/"control"
    control.mkdir()
    chronology.check_outputs(out,["control"],created=True)
    token_owner = runpy.run_path(str(ROOT/"scripts/arbiter-fp16-execution.py"))
    token = token_owner["windows_owner"]()
    require(token["user_sid"] == token["owner_sid"] == token_owner["SID"],"wrong human token")
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","CUDA_","ORT_"))}
    env.update(CUDA_VISIBLE_DEVICES="",AIRK_SELECTED_CONTEXT_MODEL=MODEL,AIRK_CONTEXT_ACTION_PRIVATE_CONTROL=str(control),
               AIRK_SELECTED_CONTEXT_PREFLIGHT=descriptor["certificate"]["path"],
               AIRK_SELECTED_CONTEXT_PREFLIGHT_SHA256=descriptor["certificate"]["sha256"],
               AIRK_SELECTED_CONTEXT_PROFILES=roster["path"],AIRK_SELECTED_CONTEXT_PROFILES_SHA256=roster["sha256"],
               AIRK_SELECTED_CONTEXT_CORPUS_SHA256=corpus,AIRK_SELECTED_CONTEXT_RAW_OUTPUT=str(out/"raw.json"))
    argv = [image["path"],"--exact",SELECTOR,"--ignored","--nocapture","--test-threads=1"]
    def authenticate(api,handle,info,child_deadline):
        proof = parent.suspended_authentication(api,info,deadline,child_deadline,token["user_sid"],image,job.image_proof)
        require(token_owner["windows_owner"](process=info.process) == token and job.job_snapshot(api,handle) == ([info.pid],1,1),
                "foreign suspended CPU scorer")
        return proof
    require(time.monotonic() < deadline,"original whole deadline exhausted during admission")
    with open(os.devnull,"rb") as stdin,(out/"native.log").open("xb") as log:
        observed = job.run_windows_owned_training(argv,out,log,deadline-time.monotonic(),env,stdin,HOST,
                                                   before_resume=authenticate,absolute_deadline=deadline)
    write("owned.json",observed)
    chronology.check_outputs(out,[str(p.relative_to(out)) for p in out.rglob("*")],created=True)
    require(settled(observed) and observed["peak_host_committed_bytes"] <= HOST and digest(Path(image["path"])) == KERNEL_IMAGE
            and (out/"native.log").stat().st_size <= 16<<20 and "1 passed; 0 failed;" in (out/"native.log").read_text("utf-8"),
            "native FP32 raw scoring did not complete under the original limits")
    result = close({"original_compiled":pin(out/"original-compiled.json"),"raw":pin(out/"raw.json"),
                    "owned_job":pin(out/"owned.json"),"producer_binary":image})
    write("execution.json",{"closure":result,"raw":pin(out/"raw.json"),"descriptor":descriptor,"command":argv,
                            "host_limit_bytes":HOST,"wall_limit_seconds":1200,"elapsed_seconds":time.monotonic()-start,
                            "product_qualified":False,"calibration_fitted":False})
    return pin(out/"execution.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--descriptor",type=Path,required=True)
    parser.add_argument("--sha256",required=True)
    parser.add_argument("--output",type=Path,required=True)
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"checked Python -I -B -S required")
    print(json.dumps(execute(json_pin({"path":str(args.descriptor),"sha256":args.sha256}),args.output)))
