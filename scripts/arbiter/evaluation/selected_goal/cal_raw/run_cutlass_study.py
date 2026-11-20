# SPDX-License-Identifier: Apache-2.0
"""Materialize the verified R5 input and execute the matched private Kernel study in one owned Job."""
import argparse
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from cutlass_study_inputs import CAMPAIGN, GPU, RECIPE, checked, chronology, digest, json_pin, prepare, require
from run_compiler_batch import ROOT, job, parent

SELECTOR = "inference_owner::context_action_calibration_tests::selected::cutlass_study::study_cutlass_generic_read"
MODULE = "crates/airk-kernel/src/inference_owner/context_action_calibration_tests"
OWNERS = [MODULE+".rs", MODULE+"/selected.rs", MODULE+"/selected/cutlass_study.rs",
          "crates/airk-core/src/arbiter/context_action/calibration.rs",
          "crates/airk-core/src/arbiter/context_action/calibration/terminal.rs"]


def image_from(descriptor):
    build = json_pin(descriptor["build"])
    log = checked(descriptor["cargo_log"])
    require(build["passed"] is True and build["inputs_unchanged"] is True and
            build["command_inputs_unchanged"] is True and "--no-run" in build["command"] and
            Path(descriptor["build"]["path"]).parent / build["log"] == Path(descriptor["cargo_log"]["path"]),
            "genuine unchanged original cargo build required")
    for path in OWNERS:
        require(build["groups"]["native"]["files"][path] == digest(ROOT/path), "native study source changed")
    paths = []
    for line in log.splitlines():
        if line.startswith(b"{"):
            item = json.loads(line)
            if item.get("reason") == "compiler-artifact" and item["target"]["name"] == "airk_kernel" and item["profile"]["test"] and item.get("executable"):
                paths.append(item["executable"])
    staging = json_pin(descriptor["staged_images"])
    if isinstance(staging,dict):
        require(staging["build_sha256"] == descriptor["build"]["sha256"] and staging["study_selector"] == SELECTOR
                and staging["study_invocations"] == 0 and staging["unit_sha256"] ==
                "3bc65c36b05d70e32ca81c693972d6cd6831413f7fa0b0e68b4bb3c5b37c4480",
                "incorrect dedicated native study staging")
        staging = [staging]
    matches = [image for image in staging if image["name"] == "airk_kernel"]
    require(len(paths) == len(matches) == 1 and Path(matches[0]["source"]) == Path(paths[0]),
            "different staging or linked Kernel image")
    pin = matches[0]
    image = Path(pin["path"])
    require(image.is_absolute() and image.resolve() == image and image.is_relative_to(CAMPAIGN) and
            digest(image) == pin["sha256"] and image.stat().st_size == pin["bytes"], "staged image differs")
    return {"path":str(image),"sha256":pin["sha256"]}


def execute(descriptor, out):
    require(out.resolve() == out and out.parent == CAMPAIGN and out.is_dir() and not any(out.iterdir()),
            "fresh private output required")
    names = ["verified-study-input.json","native.log","execution.json"]
    chronology.check_outputs(out,names,created=False)
    image = image_from(descriptor)
    data = prepare(descriptor["evidence"])
    source, log, closure = [out/name for name in names]
    with source.open("x",encoding="utf-8") as file:
        json.dump(data,file,allow_nan=False,separators=(",",":"))
    chronology.check_outputs(out,[source.name],created=True)
    packet_sha = digest(source)
    owner = runpy.run_path(str(ROOT/"scripts/arbiter-fp16-execution.py"))
    token = owner["windows_owner"]()
    require(token["owner_sid"] == token["user_sid"] == owner["SID"], "wrong human token")
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","CUDA_","ORT_"))}
    env.update(CUDA_VISIBLE_DEVICES="",AIRK_CUTLASS_STUDY_INPUT=str(source),AIRK_CUTLASS_STUDY_INPUT_SHA256=packet_sha)
    argv = [image["path"],"--exact",SELECTOR,"--ignored","--nocapture","--test-threads=1"]
    deadline = time.monotonic()+120
    def authenticate(api, handle, info, child_deadline):
        proof = parent.suspended_authentication(api,info,deadline,child_deadline,token["user_sid"],image,job.image_proof)
        require(owner["windows_owner"](process=info.process) == token and job.job_snapshot(api,handle) == ([info.pid],1,1),
                "foreign native child or Job")
        return proof
    with open(os.devnull,"rb") as stdin, log.open("xb") as file:
        observed = job.run_windows_owned_training(argv,out,file,deadline-time.monotonic(),env,stdin,2<<30,
                                                   before_resume=authenticate,absolute_deadline=deadline)
    chronology.check_outputs(out,[log.name],created=True)
    require(log.stat().st_size <= 16<<20,"study output exceeded bound")
    lines = log.read_text("utf-8").splitlines()
    marker = "AIRK_CUTLASS_READ_STUDY="
    reports = [json.loads(line.split(marker,1)[1]) for line in lines if marker in line]
    complete = parent.settled(observed) and image_from(descriptor) == image and digest(source) == packet_sha and len(reports)==1 and any("1 passed; 0 failed;" in line for line in lines)
    if complete:
        result = reports[0]
        complete = (result["input_sha256"] == packet_sha and result["graph"] == GPU["GRAPH"] and
                    result["weights"] == GPU["WEIGHTS"] and result["recipe"] == RECIPE and
                    result["registered_profile"] is result["release_qualified"] is False and
                    [r["id"] for r in result["outcomes"]] == [r["id"] for r in data["raw"]["rows"]])
    with closure.open("x",encoding="utf-8") as file:
        json.dump({"complete":complete,"descriptor":descriptor,"source":str(source),"input_sha256":packet_sha,
                   "image":image,"job":observed,"command":argv,"log_sha256":digest(log),
                   "report":reports[0] if complete else None},file,allow_nan=False)
    chronology.check_outputs(out,[closure.name],created=True)
    require(complete,"study incomplete; see original Job receipt")
    print(digest(closure))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--descriptor",required=True)
    p.add_argument("--sha256",required=True)
    p.add_argument("--output",type=Path,required=True)
    args = p.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use original checked Windows Python -I -B -S")
    execute(json_pin({"path":args.descriptor,"sha256":args.sha256}),args.output)
