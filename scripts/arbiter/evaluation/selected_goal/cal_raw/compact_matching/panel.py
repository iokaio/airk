# SPDX-License-Identifier: Apache-2.0
"""Join independently settled Compact CPU Job roles on every fresh original R5 row."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from inputs import CAMPAIGN, ORIGINAL, STAGE_SHA, checked, chronology, future_rows, ref, require, row_key
from run import source_set, verify_result
from assemble import settled

GRAPH_PROOF = ref(CAMPAIGN/"compact-cutlass-full-model-graph-r1/execution.json",
                  "cbcc3706b621225507c9e1e955116dd0aa35d162db1f6c1c97a031974457b40b")
IDENTITY = {
    "model_sha256":"147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9",
    "weight_sha256":"0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3",
    "typed_graph_sha256":"6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581",
    "runtime_manifest_sha256":"9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
    "matching_recipe_sha256":"7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394"}


def numeric_identity():
    proof = json.loads(checked(GRAPH_PROOF))
    require(proof["complete"] is True and proof["mode"] == "graph" and settled(proof["owned"]) and
            all(proof["result"][k] == IDENTITY[k] for k in ("model_sha256","weight_sha256","typed_graph_sha256")) and
            proof["runtime_manifest"]["sha256"] == IDENTITY["runtime_manifest_sha256"],"protected graph/runtime evidence differs")
    require(proof["image"]["sha256"] == proof["linked_image"]["sha256"] and
            proof["sources"] and proof["backend_sources"],"incomplete native image or source closure")
    return IDENTITY | {"original_typed_graph_job":GRAPH_PROOF,
                       "native_build":{key:proof[key] for key in (
                           "image","linked_image","build","runtime_manifest","sources","backend_sources")}}


def reference_identity(result):
    basis = result["reference_basis"]
    require(basis["sha256"] == "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14" and
            result["original_manifest"] == ORIGINAL and result["protected_stage"]["sha256"] == STAGE_SHA and
            result["runtime"]["sha256"] == "a557be764ebcf8a2b777403ac349f2d3ecadfabb5d65e16b8c47026239644c95",
            "original publisher or protected arithmetic runtime differs")
    require(result["publisher_files"] == json.loads(checked(basis))["publisher_files"] and
            result["candidate_graph"]["sha256"] == IDENTITY["model_sha256"] and
            result["candidate_weights"]["sha256"] == IDENTITY["weight_sha256"] and
            result["matching_recipe_sha256"] == IDENTITY["matching_recipe_sha256"],
            "wrong publisher files, matching recipe or candidate weights")


def role_rows(jobs,original,role):
    total = []
    evidence = {}
    sources = source_set()
    for pin in jobs:
        job = json.loads(checked(pin))
        require(job["schema"] == "airk.private.compact-protected-cal-cpu-batch/1" and job["complete"] is True and
                job["role"] == role and job["sources"] == sources and settled(job["owned"]) and
                job["first"] == len(total) and 1 <= job["count"] <= 8,"missing, repeated or unreviewed independent reference")
        input_sha = job["input"]["sha256"]
        if input_sha not in evidence:
            receipt = json.loads(checked(job["input"]))
            source = json.loads(checked(receipt["models"]["compact"]))
            require(receipt["plan_sha256"] == chronology.PLAN and receipt["acquired_sha256"] == chronology.ACQUIRED and
                    source["phase_plan_sha256"] == chronology.PLAN and source["original_requests"] in (56,100) and
                    source["rows"] == original[:source["original_requests"]],
                    "source group changed relative to the independently verified final R5 census")
            evidence[input_sha] = job["input"]
        rows = original[job["first"]:job["first"]+job["count"]]
        result = job["result"]
        verify_result(result,rows,"r5_original",role)
        reference_identity(result)
        for measured in result["rows"]:
            total.append((measured,pin))
    require(len(total) == len(original),"incomplete original ordinal census")
    return total


def assemble_panel(descriptor):
    projection = json.loads(checked(descriptor["projection"]))
    native = json.loads(checked(projection["original_compiler_verification"]))
    require(native["passed"] is True and native["NN_invocations"] == 0 and settled(native["job"]) and
            native["source_sha256"] == "c7d7d8f27a603691171a1bdb15855709132dbee9cdc41d3fba2ab7a6b3aaed98" and
            native["image"]["sha256"] == "b3d20acfea363da2afd1780078e07f46f4c3bcd52da5515aa4ed29c0cc192351" and
            all(native["inputs"][model] == {k:projection["models"][model][k] for k in ("path","sha256")}
                for model in ("compact","large")),"original native compiler evidence differs")
    original,mode = future_rows(descriptor["projection"])
    require(mode == "r5_original" and len(original) == 100,"complete verified original R5 required")
    publisher = role_rows(descriptor["publisher_jobs"],original,"publisher_fp32")
    matching = role_rows(descriptor["matching_jobs"],original,"protected_matching")
    rows = []
    for source,(a,ap),(b,bp) in zip(original,publisher,matching,strict=True):
        row = source | {"input_row_sha256":row_key(source),"original_job":ap,"matching_job":bp}
        if source["status"] == "eligible":
            row.update(original_fp32=a["scores"],matching_fp16=b["scores"])
        rows.append(row)
    return {"schema":"airk.private.compact-protected-cal-independent-panel/1","complete":True,"rows":rows,
            "input_projection":descriptor["projection"],"identity":numeric_identity(),"candidate_forwards":0,
            "publisher_jobs":descriptor["publisher_jobs"],"matching_jobs":descriptor["matching_jobs"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--descriptor",type=Path,required=True)
    parser.add_argument("--sha256",required=True)
    parser.add_argument("--output",type=Path,required=True)
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"isolated checked Python required")
    out = args.output
    require(out.is_absolute() and out.parent.resolve(strict=True) == out.parent and
            out.parent.parent == CAMPAIGN and not out.exists(),"fresh private campaign panel path required")
    chronology.check_outputs(out.parent,[out.name],created=False)
    result = assemble_panel(json.loads(checked(ref(args.descriptor,args.sha256))))
    with out.open("x",encoding="utf-8") as stream:
        json.dump(result,stream,allow_nan=False,separators=(",",":"))
    chronology.check_outputs(out.parent,[out.name],created=True)
    print(json.dumps({"path":str(out),"rows":len(result["rows"]),"sha256":chronology.digest(out.read_bytes())}))
