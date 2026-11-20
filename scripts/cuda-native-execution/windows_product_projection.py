# SPDX-License-Identifier: Apache-2.0
"""Issue and verify current Windows NN0 package metadata for the pristine CU15 root."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import runpy
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from windows_product_observation import ROOT, CAMPAIGN, checked_image, require

SOURCE = CAMPAIGN / "windows-product-runtime-r1"
STAGING_SHA = "6c9a629f8ad653bba631ac1e574a0bdd3d95a7e962c31b12edc2692041c1182a"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
REQUEST = PREFIX + "emit_cuda_component_probe_request"
EMIT = PREFIX + "emit_cuda_runtime_package_projection_from_held_native_closure"
SCHEMA = "airk.private.current-windows-package-projection/1"


def exact(base, path, sha):
    pin = base.pin(path)
    require(pin["sha256"] == sha, "changed input: "+str(path))
    return pin


def check_settlement(base, parent, value, selector):
    owned = value["owned"]
    image = base.pinned(value["image"])
    require(value["complete"] is True and parent.settled(owned)
            and value["command"] == [str(image), "--exact", selector, "--ignored", "--nocapture", "--test-threads=1"]
            and 0 < owned["peak_host_committed_bytes"] <= 8 << 30 and 0 < value["elapsed_seconds"] < 120,
            "original bounded single native child required")
    proof = owned["suspended_assignment"]
    require(proof["pid"] == owned["owned_leader_pid"] and proof["execution_user_sid"] == base.SID
            and proof["image"] == {k:value["image"][k] for k in ("path","sha256")}, "suspended identity differs")


def census(base, assembly, parent):
    from windows_product_observation import SELECTOR, ASSEMBLY, PRODUCT_SOURCE
    record = base.read(base.pinned(assembly))
    require(record["schema"] == "airk.private.owned-windows-product-observation/1"
            and record["input"]["sha256"] == ASSEMBLY and record["sources"]["sha256"] == PRODUCT_SOURCE,
            "actual product source observation required")
    check_settlement(base, parent, record, SELECTOR)
    for key in ("input","sources","parent","native_emission","descriptor","provenance","stdout"):
        base.pinned(record[key])
    inner = base.read(record["native_emission"]["path"])
    require(inner["schema"] == "airk.private.windows-product-runtime-observation/1"
            and inner["pid"] == record["owned"]["owned_leader_pid"] and inner["execution_user"] == base.SID
            and all(inner[k] is False for k in ("model_requests_dispatched","activation","qualification"))
            and all(base.core_pin_matches(inner[k],record[v]) for k,v in
                    (("descriptor","descriptor"),("provenance","provenance"),("input","input"),("source_input","sources"))),
            "native assembly facts differ")
    staging = base.read(exact(base, SOURCE / "staging.json", STAGING_SHA)["path"])
    runtime = SOURCE / "runtime"
    expected = {v["name"]: (v["bytes"],v["sha256"]) for v in staging["files"]}
    require(len(expected) == 62 and staging["runtime"] == str(runtime),"foreign source stage")
    for key,name in (("descriptor","airk-cuda-runtime.json"),("provenance","airk-cuda-build-provenance.json")):
        expected[name] = (record[key]["bytes"],record[key]["sha256"])
    rows = base.census(runtime)
    require(len(rows)==64 and {str(runtime/k):v for k,v in expected.items()} ==
            {v["path"]:(v["bytes"],v["sha256"]) for v in rows}, "unexpected runtime custody bytes")
    return {"repository_root":str(ROOT),"runtime_root":str(runtime),"runtime_files":rows,
            "descriptor":base.pin(runtime/"airk-cuda-runtime.json")}


def verified_request(base, parent, pin, plan, image):
    request = base.read(base.pinned(pin))
    require(request["schema"] == SCHEMA and request["stage"] == "request" and request["image"] == image
            and request["plan"] == plan and request["activation"] is False and request["qualification"] is False,
            "foreign package request")
    check_settlement(base,parent,request,REQUEST)
    output = request["output"]
    data = base.read(base.pinned(output))
    require(data.get("kind") == "package_projection" and data.get("directory") == plan["runtime_root"]
            and base.core_pin_matches(data.get("descriptor"),plan["descriptor"]) and data.get("deadline_ms") == 120000,
            "native package request differs")
    return output


def run(args):
    start = time.monotonic()
    deadline = start + 120
    require(os.name=="nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use isolated Windows Python")
    api = runpy.run_path(str(ROOT/"scripts/cuda-component-execution.py"))
    base = api["base"]()
    parent, job = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    require(parent.worker.windows_sid()==base.SID and api["token_owner"](ctypes.c_void_p(-1))==base.SID,"human parent required")
    guard = {"checked_guard":base.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),
             "powershell":exact(base,args.powershell,args.powershell_sha256)}
    def audit(paths, created=True):
        api["guard_many"](guard,paths,created,deadline=deadline)
    image, build, assembly = [exact(base,getattr(args,k),getattr(args,k+"_sha256")) for k in ("image","build","assembly")]
    audit([Path(v["path"]) for v in (image,build,assembly)])
    lineage = checked_image(base,build,image,audit)
    plan = census(base,assembly,parent)
    audit([Path(row["path"]) for row in plan["runtime_files"]])
    out = args.output
    require(out.parent==CAMPAIGN and not out.exists(), "fresh campaign directory required")
    request, request_evidence = None, None
    if args.stage=="emit":
        request_evidence=exact(base,args.request,args.request_sha256)
        request=verified_request(base,parent,request_evidence,plan,image)
    api["mkdir"](guard,out,deadline=deadline)
    temp=out/"temp"
    api["mkdir"](guard,temp,deadline=deadline)
    result_path = out/("request.json" if request is None else "projection.json")
    log, stdin = out/"stdout.log", out/"stdin"
    paths = [result_path,stdin,log] + ([result_path.with_suffix(".receipt.json")] if request else [])
    audit(paths,False)
    with stdin.open("xb"):
        pass
    env={k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","ORT_","CUDA_","CUDNN_","PYTHON"))}
    env.update(PATH=r"C:\Windows\System32;C:\Windows",TMP=str(temp),TEMP=str(temp),TMPDIR=str(temp))
    if request is None:
        env.update(AIRK_CUDA_REQUEST_KIND="package_projection", AIRK_CUDA_REQUEST_OUTPUT=str(result_path),
                   AIRK_CUDA_RUNTIME_ROOT=plan["runtime_root"],AIRK_CUDA_RUNTIME_MANIFEST=plan["descriptor"]["path"],
                   AIRK_CUDA_RUNTIME_MANIFEST_SHA256=plan["descriptor"]["sha256"],AIRK_CUDA_DEADLINE_MS="120000")
    else:
        env.update(AIRK_CUDA_QUALIFICATION_INPUT=request["path"],AIRK_CUDA_QUALIFICATION_INPUT_SHA256=request["sha256"],
                   AIRK_CUDA_QUALIFICATION_OUTPUT=str(result_path))
    selector = EMIT if request else REQUEST
    argv = [image["path"],"--exact",selector,"--ignored","--nocapture","--test-threads=1"]
    result = {"schema":SCHEMA,"stage":args.stage,"plan":plan,"assembly":assembly,"input":request,
              "request_evidence":request_evidence,"image":image,"build":build,"lineage":lineage,"guard":guard,
              "command":argv,"complete":False,"activation":False,"qualification":False,
              "parent":base.pin(Path(__file__)),"observation_owner":base.pin(Path(__file__).with_name("windows_product_observation.py"))}
    def authorize(child_api, handle, info, child_deadline):
        proof = parent.suspended_authentication(child_api,info,deadline,child_deadline,base.SID,image,job.image_proof)
        require(api["token_owner"](info.process)==base.SID,"child token owner mismatch")
        return proof
    try:
        with stdin.open("rb") as initial,log.open("xb") as stream:
            result["owned"]=job.run_windows_owned_training(argv,ROOT,stream,deadline-time.monotonic(),env,initial,
                8 << 30,before_resume=authorize,absolute_deadline=deadline)
        require(parent.settled(result["owned"]),"native request/projection did not naturally settle")
        audit(paths)
        require(result_path.stat().st_size<=1<<20 and log.stat().st_size<=64<<20,"native output exceeds cap")
        result["output"]=base.pin(result_path)
        if request:
            result["native_emission"]=base.pin(result_path.with_suffix(".receipt.json"))
            base.emission_facts(base.read(result["native_emission"]["path"],1<<20),dict(plan,native_input=request),
                                result["output"],result["owned"])
        require(census(base,assembly,parent)==plan and not list(temp.iterdir()),"input drift or live control")
        for pin in [image,build,lineage["artifact"],lineage["log"]]:
            base.pinned(pin)
        result["complete"]=True
    except Exception as error:
        result["failure"]={"type":type(error).__name__,"message":str(error)}
        if hasattr(error,"owned_cleanup"):
            result["cleanup"]=error.owned_cleanup
        raise
    finally:
        if log.exists() and log.stat().st_size<=64<<20:
            result["stdout"]=base.pin(log)
        result["elapsed_seconds"]=time.monotonic()-start
        if result["elapsed_seconds"]>=120:
            result.update(complete=False,deadline_expired=True)
        api["write_new"](guard,out/"execution.json",result)
    require(result["complete"],"original whole deadline expired")


def verify_execution(path,sha,projection_pin):
    api=runpy.run_path(str(ROOT/"scripts/cuda-component-execution.py"))
    base=api["base"]()
    record=base.read(exact(base,path,sha)["path"])
    parent,_=base.helpers({"ownership_helper_root":str(base.MAINTAINED_HELPER_ROOT)})
    require(record["schema"]==SCHEMA and record["stage"]=="emit" and record["output"]==projection_pin
            and record["activation"] is False and record["qualification"] is False,"foreign package evidence")
    check_settlement(base,parent,record,EMIT)
    for key in ("parent","observation_owner","image","build","output","native_emission","stdout"):
        base.pinned(record[key])
    require(record["parent"]==base.pin(Path(__file__)) and record["observation_owner"]==
            base.pin(Path(__file__).with_name("windows_product_observation.py")),"custody owner drift")
    guard=record["guard"]
    require(guard["checked_guard"]==base.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),"actual output guard differs")
    base.pinned(guard["powershell"])
    def audit(paths):
        api["guard_many"](guard,paths,True)
    lineage=checked_image(base,record["build"],record["image"],audit)
    plan=census(base,record["assembly"],parent)
    require(plan==record["plan"] and lineage==record["lineage"],"original build or payload drift")
    request=verified_request(base,parent,record["request_evidence"],plan,record["image"])
    require(request==record["input"],"original typed request differs")
    base.emission_facts(base.read(record["native_emission"]["path"]),dict(plan,native_input=request),projection_pin,record["owned"])
    return plan,record


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--stage",choices=["request","emit"],required=True)
    for key in ("output","image","build","assembly","powershell"):
        p.add_argument("--"+key,type=Path,required=True)
    for key in ("image","build","assembly","powershell"):
        p.add_argument("--"+key+"-sha256",required=True)
    p.add_argument("--request",type=Path)
    p.add_argument("--request-sha256")
    run(p.parse_args())
