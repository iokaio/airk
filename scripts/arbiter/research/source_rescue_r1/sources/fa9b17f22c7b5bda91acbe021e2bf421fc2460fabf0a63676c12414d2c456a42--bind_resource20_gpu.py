# SPDX-License-Identifier: Apache-2.0
"""Bind native-emitted GPU resource documents into the existing private parent plan.
No native DTO is constructed and no native process is launched.
"""
import argparse, hashlib, json
from pathlib import Path
import launch_resource20_gpu as owner

SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::"
            "cuda_qualification::cuda_resources::cuda_selected_native_resources20")
def file_pin(path, sha):
    p=owner.pin(path,sha)
    return {"path":str(p),"bytes":p.stat().st_size,"sha256":sha}
def bind(template, wrapper_pin, policy_pin, emission_pin, observer_pin, attention_required):
    # Template retains the genuine image/source-bridge/runtime/OS-custody inputs.
    # Root must supply current reviewed source bindings; none are silently dropped.
    result=json.loads(json.dumps(template))
    wrapper=owner.document(wrapper_pin)
    request=owner.document(wrapper["gpu_input"])
    expected=owner.document(wrapper["expectations"])
    profile=owner.document(request["profile"])
    policy=owner.document(policy_pin)
    host,job=owner.gpu_policy.model_pair(policy,profile["model"])
    result.update(schema="airk.private.cuda-resource20-owned-plan/1",mode="gpu-resource",
        native_input=wrapper_pin,native_policy=policy_pin,native_emission_receipt=emission_pin,
        model=profile["model"],profile=profile["manifest"],cpu_fit=profile["cpu_fit"],
        reservation_bytes=host,policy=request["policy"],policy_digest=request["policy_digest"],
        expected_resource_input=expected["expected_resource_input"],
        native_operation_roster=expected["native_operation_roster"],forward_roster=expected["forward_roster"],
        limits={"whole_process_seconds":3600,"host_committed_bytes":job,"log_bytes":64<<20,
                "operation_journal_bytes":131072,"per_operation_ms":120000},
        command_selector=SELECTOR,attention_observer=observer_pin,
        attention_observation_required=attention_required,
        scope="Actual native GPU latency/host/placement Resource20 only; GPU process peak unavailable; no numerical, coexistence, final GPU qualification or product grant.")
    # Do not invent native output/ownership authority from a filename or a bool.
    owner.document(emission_pin)
    required=[wrapper_pin,wrapper["gpu_input"],wrapper["expectations"],policy_pin,emission_pin,
              request["profile"],request["source_manifest"],wrapper["runtime_manifest"],observer_pin]
    assets={a["name"]:a for a in profile["assets"]}
    if not assets or len(assets)!=len(profile["assets"]) or set(assets)!=set(request["files"]):
        raise ValueError("exact native profile asset/file closure required")
    required += [{"path":request["files"][name],"bytes":a["bytes"],"sha256":a["sha256"]}
                 for name,a in assets.items()]
    required += [file_pin(Path(__file__).parent/name,owner.digest(Path(__file__).parent/name))
                 for name in ("bind_resource20_gpu.py","launch_resource20_gpu.py","gpu_parent_policy.py",
                              "capacity_resource20_gpu.py","watch_resource20_gpu.py","native_operations_watchdog_gpu.py")]
    seals=dict(result["input_seals"])
    for row in required:
        prior=seals.get(row["path"])
        if prior is not None and prior!=row["sha256"]:
            raise ValueError("existing frozen source/input seal would be overwritten")
        seals[row["path"]]=row["sha256"]
    result["input_seals"]=seals
    owner.gpu_policy.documents(result,wrapper,request,profile,expected,policy)
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for key in ("template","request","policy","emission","observer"):
        parser.add_argument("--"+key,required=True)
        parser.add_argument("--"+key+"-sha256",required=True)
    parser.add_argument("--attention-required",action="store_true")
    parser.add_argument("--output",required=True)
    a=parser.parse_args()
    template_pin=file_pin(a.template,a.template_sha256)
    pairs=[file_pin(getattr(a,k),getattr(a,k+"_sha256")) for k in ("request","policy","emission","observer")]
    plan=bind(owner.document(template_pin),*pairs,a.attention_required)
    plan["input_seals"][template_pin["path"]]=template_pin["sha256"]
    # Existing owner source/image checks still execute before any native child.
    owner.write(a.output,plan)
    print(json.dumps({"plan":file_pin(a.output,owner.digest(a.output)),
                      "native_model_Cargo_runs":0,"Root_review_required_before_execution":True}))
if __name__=="__main__":
    main()
