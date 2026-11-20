# SPDX-License-Identifier: Apache-2.0
"""Owned Windows Large compensated-attention operand, final graph and four frozen numerical controls."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_large::compensated::large_compensated_"
MANIFEST = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
STAGE = "e6a6227446662e33c47ea006e1c0f6e2710b6434245bb23f4205ace8299d794c"
ORIGINAL = "c01cd56af84f90a6b842c5318f0c3e8eb757f14a9ce3387752c928c2f835c8c7"

def require(ok, message):
    if not ok:
        raise ValueError(message)

def reviewed(wrapper, shared, direct=False):
    observed = wrapper["result"]
    require(observed["route_refusal"] is None and observed["cpu_refusal"] is None, "Large placement or bounded CPU dataflow refused")
    graph = observed["graph"]
    require(graph["complete"] and wrapper["placement"]["complete"], "incomplete actual Large graph")
    if direct:
        proof = runpy.run_path(str(HERE / "large_direct_placement.py"))["verify"](graph)
    else:
        proof = shared["validate_projection_graph"](graph, large_qkv=True)
        proof.update(runpy.run_path(str(HERE / "large_body_placement.py"))["verify"](graph))
    rows = [r for r in graph["rows"] if r["graph"] == 0]
    edges = {(r["node"],r["role"],r["slot"]):r["value"] for r in rows if r["kind"]=="edge"}
    tensors = {r["value"]:r for r in rows if r["kind"]=="value"}
    expected = {f"/airk_large_cutlass/{i}/efficient" for i in range(36)}
    for node in (r for r in rows if r["kind"]=="node" and r["op"]=="MultiHeadAttention"):
        require(node["domain"]=="com.microsoft" and node["ep"]=="CUDAExecutionProvider" and node["name"] in expected, "unexpected Large MHA")
        expected.remove(node["name"])
        for role, slot in ((0,0),(0,1),(0,2),(2,0)):
            value = tensors[edges[node["node"],role,slot]]
            require(value["rank"]==3 and value["element_type"]==1, "FP32 MHA input/output ABI differs")
        require((node["node"],2,1) not in edges, "unapproved second attention output")
    require(not expected, "missing Large CUDA MHA")
    return dict(proof, efficient_fp32_attention_nodes=36, cpu_payload_limit=12067)

def observation(raw, mode):
    lines=raw.decode("utf8").replace("\x1b[36m","").replace("\x1b[0m","").splitlines()
    marker="AIRK_LARGE_COMPENSATED="
    values=[json.loads(line[len(marker):]) for line in lines if line.startswith(marker)]
    require(len(values)==1 and any("1 passed; 0 failed" in line for line in lines), "one successful original native selector required")
    kernels=[s for s in lines if s.startswith("Operator=")]
    scratch=[s for s in lines if s.startswith("flash=")]
    names=(["airk_large_cutlass_t32_operand"] if mode=="operand" else
           [f"/airk_large_cutlass/{i}/efficient" for i in range(36)] if mode=="score" else [])
    expected=[f"Operator=MultiHeadAttention Node={name} DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION" for name in names]
    require(sorted(kernels)==sorted(expected) and len(scratch)==len(expected) and all(
        "efficient=1" in line and "has_qkv_workspace=0, workspace=0" in line for line in scratch),
        "actual efficient FP32 dispatches or bounded attention workspace differ")
    return values[0], kernels, scratch

def run(args):
    end=time.monotonic()+120
    shared=runpy.run_path(str(HERE/"probe_compact.py"))
    components=runpy.run_path(str(ROOT/"scripts/cuda-component-execution.py"))
    capacity=runpy.run_path(str(ROOT/"scripts/cuda-component-execution/wddm_capacity_snapshot.py"))
    wddm=runpy.run_path(str(ROOT/"scripts/cuda-component-execution/wddm_process_samples.py"))
    base=components["base"]()
    parent,owner=base.helpers({"ownership_helper_root":str(base.MAINTAINED_HELPER_ROOT)})
    exact=lambda path,digest:shared["exact_pin"](path,digest,base)
    require(os.name=="nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and parent.worker.windows_sid()==base.SID==components["token_owner"](ctypes.c_void_p(-1)),"verified isolated human required")
    guard={"checked_guard":base.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),"powershell":exact(args.powershell,args.powershell_sha256)}
    def audit(paths,created=True,*,end=end):
        components["guard_many"](guard,list(paths),created,deadline=end)
    out=args.output
    require(out.is_absolute() and out.parent==CAMPAIGN and out.parent.resolve(strict=True)==out.parent and not out.exists(),"fresh private output required")
    manifest_path=args.image.with_name("airk-cuda-runtime.json")
    audit([args.build,args.image,args.snapshot,args.lineage,manifest_path])
    image,build=exact(args.image,args.image_sha256),exact(args.build,args.build_sha256)
    manifest=exact(manifest_path,MANIFEST)
    lineage=exact(args.lineage,args.lineage_sha256)
    chain=base.read(lineage["path"])
    require(chain["schema"]=="airk.private.compact-static-cargo-custody/1" and chain["complete"] and chain["native_invocations"]==0
            and chain["image"]==image and chain["build"]==build and chain["runtime_manifest"]==manifest
            and len(chain["compile_time_cuda_approvals"])==3,"wrong static linked-image chain")
    log,linked,sources=shared["linked_image"](build,image,base,audit,"metadata",source_snapshot=args.snapshot,cargo_chain=chain)
    approvals=shared["decision_build_approvals"](log,linked,manifest,base,audit,cargo_chain=chain)
    files=base.read(build["path"],32<<20)["groups"]["native"]["files"]
    retrieval="crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/cutlass"
    inference="crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/fp16_large"
    required_native = [retrieval+".rs", retrieval+"/large.rs", retrieval+"/model.rs", inference+".rs", inference+"/compensated.rs"]
    if retrieval+"/large_routes.rs" in files:
        required_native.append(retrieval+"/large_routes.rs")
    for key in required_native:
        path=args.snapshot/(files[key]+".source")
        audit([path])
        sources[key]=exact(path,files[key])
    backend=runpy.run_path(str(HERE/"compact_cutlass_attention.py"))["backend_sources"](args.image.parent)
    backend={key:backend[key] for key in ("producer","provider_sources","sm80_gemm_type","internal_math")}
    pins=[backend["producer"],backend["sm80_gemm_type"],*backend["provider_sources"].values(),lineage,base.pin(Path(__file__).resolve()),base.pin(HERE/"probe_compact.py"),base.pin(HERE/"large_body_placement.py"),
          exact(HERE/"score_comparison.py","24f264d1989c105c9b284f8eccb104f4f786398c5605ef23be59b0e4bd5428c1"), base.pin(HERE/"compact_cutlass_attention.py")]
    sources_to_reopen = [ROOT/"scripts/cuda-component-execution.py", ROOT/"scripts/cuda-native-execution.py",
                         ROOT/"scripts/cuda-component-execution/wddm_capacity_snapshot.py",
                         ROOT/"scripts/cuda-component-execution/wddm_process_samples.py"]
    pins += [base.pin(path) for path in sources_to_reopen]
    pins += [exact(base.MAINTAINED_HELPER_ROOT/name, sha) for name,sha in base.HELPERS.items()]
    pins.extend(guard.values())
    prep=exact(CAMPAIGN/"large-cutlass-t32-operands-r1/prepare.json","f9a45a949357610f6a7310b8df2b87ae07225382422fcc56f375fba00df8ccea")
    original=base.read(prep["path"])
    pins += [prep,*original["assets"].values()]
    mode=args.mode
    require(not args.direct or mode!="operand", "original operand control is common")
    host=(4 if mode=="operand" else 22)<<30
    gate=None
    if mode!="operand":
        stage=(exact(CAMPAIGN/"large-direct-fp32-stage-r2/stage.json","c2ac1951a3c0b5817380c1c6164b25e4e2f3b679ba569b727a7ebbbbf1ea9a35") if args.direct else
               exact(CAMPAIGN/"large-cutlass-body-stage-r1/stage.json",STAGE))
        if args.direct:
            pins.append(base.pin(HERE/"large_direct_placement.py"))
        arm=base.read(stage["path"])
        pins += [stage,arm["graph"],arm["weights"]]
        control_pin=exact(args.operand_receipt,args.operand_sha256)
        control=base.read(control_pin["path"])
        require(control["mode"]=="operand" and control["complete"] and parent.settled(control["owned"])
                and control["build"]==build and control["image"]==image and control["result"]["result"]["values"]==131072
                and control["operand_matches"],"same-image genuine operand control required")
        pins.append(control_pin)
    if mode=="score":
        gp=exact(args.graph_receipt,args.graph_sha256)
        g=base.read(gp["path"],64<<20)
        require(g["mode"]=="graph" and g["complete"] and parent.settled(g["owned"]) and g["build"]==build and g["image"]==image
                and g.get("direct",False)==args.direct and g["typed_graph_validation"]==reviewed(g["result"],shared,args.direct),"same-image measured Large graph required")
        gate=g["result"]["result"]["typed_graph_sha256"]
        refp=exact(args.reference,args.reference_sha256)
        closure=base.read(refp["path"])
        require(closure["complete"] and parent.settled(closure["owned"]) and not closure["whole_phase_expired"],"matching independent Job not closed")
        ref=base.read(closure["proof"]["path"])
        if args.width == 1588:
            require(args.direct and ref.get("train_phase")=="matching", "original Large TRAIN protected reference required")
            case_source=exact(HERE.parent/"arbiter-fp16-reference/large_train_case.py","722baf8ad63976edea9c389f16a06be8ebf33d185ff96b4e3fd6a06e6dd6aa3b")
            case=runpy.run_path(case_source["path"])["original"](CAMPAIGN)
            retained=ref["case_pins"]
            require(len(retained)==4 and retained[:2]==case["pins"], "original TRAIN source or publisher closure differs")
            for pin in retained:
                base.pinned(pin)
            pub=base.read(retained[2]["path"])
            require(pub["complete"] and parent.settled(pub["owned"]) and not pub["whole_phase_expired"]
                    and pub["proof"]==retained[3], "original publisher Job not settled")
            p=base.read(retained[3]["path"])
            require(p["train_phase"]=="publisher" and p["case_pins"]==case["pins"] and p["publisher"]["official_forwards"]==2
                    and p["publisher"]["t32_max_abs"]<=0.001 and p["input_digest"]==case["input"]["input_digest"]
                    and len(p["original_fp32"])==len(ref["matching_fp16"])==7, "wrong publisher row or incomplete matching data")
            row=dict(case,reference_original_fp32=p["original_fp32"])
            pins.extend([case_source,*retained])
        else:
            original_inputs=exact(CAMPAIGN/"large-fp16-independent-reference-inputs-r1.json",ORIGINAL)
            rows=[r for r in base.read(original_inputs["path"])["rows"] if len(r["input"]["input_ids"])==args.width]
            require(len(rows)==1, "ambiguous frozen Large width")
            row=rows[0]
            pins.append(original_inputs)
        require(ref["input_digest"]==row["input"]["input_digest"] and ref["tokens"]==args.width
                and ref["candidate"]["graph"]==arm["graph"] and ref["candidate"]["protected_routes_in_model_order"]==252
                and ref["recipe"]==("large-weight-only-fp32-sm86-compensated-causal-mha" if args.direct else "large-body252-fp32-sm86-compensated-causal-mha") and ref["original_fp32"]==row["reference_original_fp32"]
                and ref["cpu_placement"]["torch_intraop_threads"]==2,"frozen Large reference/input differs")
        pins += [gp,refp,closure["proof"]]
    audit([Path(p["path"]) for p in pins])
    for pin in pins:
        base.pinned(pin)
    selector=PREFIX.replace("::large_compensated_","::large_direct_") if args.direct else PREFIX
    command=[str(args.image),"--exact",selector+mode,"--ignored","--nocapture"]
    receipt={"schema":"airk.private.large-compensated-cuda/1","complete":False,"qualification":False,"mode":mode,"direct":args.direct,"width":args.width,
             "command":command,"image":image,"build":build,"linked_image":linked,"compile_time_cuda_approvals":approvals,"build_log":log,
             "runtime_manifest":manifest,"sources":sources,"backend_sources":backend,"inputs":pins,"host_bytes":host,"seconds":120}
    components["mkdir"](guard,out,deadline=end)
    temp,stdin,stdout,dst=(out/n for n in ("temp","stdin.txt","stdout-stderr.log","execution.json"))
    try:
        components["mkdir"](guard,temp,deadline=end)
        audit([stdin,stdout,dst],False)
        env={k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","ORT_","PYTHON","CUDA_","CUDNN_"))}
        env.update(PATH=r"C:\Windows\System32;C:\Windows",TEMP=str(temp),TMP=str(temp),TMPDIR=str(temp),AIRK_PRIVATE_CENSUS_BASE=str(temp),
                   CUDA_CACHE_PATH=str(temp),ORT_CUDA_GEMM_OPTIONS="2",ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO="1")
        if gate is not None:
            env.update(AIRK_LARGE_CUTLASS_TYPED_GRAPH=gate,AIRK_LARGE_CUTLASS_WIDTH=str(args.width))
        begin=capacity["snapshot"](None)
        luid=begin["dxgi"]["luid"]
        receipt["before_load_adapter"]=begin
        baseline=None
        def suspended(api,job,info,child_end):
            nonlocal baseline
            proof=parent.suspended_authentication(api,info,end,child_end,base.SID,image,owner.image_proof)
            require(components["token_owner"](info.process)==base.SID and owner.job_snapshot(api,job)==([info.pid],1,1),"wrong suspended owner or Job")
            with wddm["ProcessSampler"](info,luid) as sampler:
                baseline=sampler.sample()
            return dict(proof,default_owner_sid=base.SID,observed_limits=shared["job_limits"](api,job,host))
        def observing(api,job,info,expiry):
            return {"wddm":wddm["observe_process"](api,info,expiry,luid,baseline)},[]
        with stdin.open("xb"):
            pass
        with stdin.open("rb") as src,stdout.open("xb") as sink:
            audit([stdin,stdout])
            receipt["owned"]=owner.run_windows_owned_training(command,ROOT,sink,end-time.monotonic(),env,src,host,
                    before_resume=suspended,after_resume=observing,absolute_deadline=end,cleanup_seconds=5)
        require(parent.settled(receipt["owned"]),"native Large lifetime did not settle successfully")
        payload,records,scratch=observation(stdout.read_bytes(),mode)
        native=payload["native"]
        result=native["result"]
        receipt.update(result=native,actual_job=payload["job"],kernel_records=records,scratch_records=scratch)
        if mode!="operand":
            require(result["model_sha256"]==arm["graph"]["sha256"] and result["weight_sha256"]==arm["weights"]["sha256"],"native model inputs differ")
        if mode=="operand":
            require(result["values"]==131072,"missing measured operand values")
            receipt["operand_matches"]=result["maximum_absolute_error"]<=0.00001
        elif mode=="graph":
            receipt["typed_graph_validation"]=reviewed(native,shared,args.direct)
        else:
            require(result["typed_graph_sha256"]==gate and result["score"]["compiled_input_digest"]==ref["input_digest"],"scored wrong original input")
            receipt["numerical"]=runpy.run_path(str(HERE/"score_comparison.py"))["numerical"](
                result["score"]["raw_scores"],ref["matching_fp16"],ref["original_fp32"])
            if args.width == 1588:
                def ranking(values):
                    first,second = sorted(range(len(values)), key=values.__getitem__, reverse=True)[:2]
                    return {"first":first, "runner_up":second, "raw_margin":values[first]-values[second]}
                receipt["training_development"] = {
                    "case":row["name"], "input_digest":ref["input_digest"], "remaining_ns":row["remaining_ns"],
                    "remaining_input_tokens":row["remaining_input_tokens"], "ordered_roster":row["roster"],
                    "rankings":{k:ranking(receipt["numerical"][k]) for k in ("candidate_scores","matching_scores","original_scores")}}
        receipt["after_release_adapter"]=capacity["snapshot"](None)
        require(receipt["after_release_adapter"]["dxgi"]["luid"]==luid,"physical adapter changed")
        for item in [image,build,manifest,log,linked,*approvals,*sources.values(),*pins]:
            base.pinned(item)
        receipt["complete"]=True
    except Exception as exc:
        receipt["error"]=type(exc).__name__+": "+str(exc)
    finally:
        if stdout.exists():
            receipt["original_stdout"]=base.pin(stdout)
        audit(out.rglob("*"),end=time.monotonic()+15)
        with dst.open("x",encoding="utf8") as file:
            json.dump(receipt,file,allow_nan=False)
            file.write("\n")
        audit([dst],end=time.monotonic()+10)
    require(receipt["complete"],receipt.get("error","invalid Large receipt"))

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--mode",choices=("operand","graph","score"),required=True)
    p.add_argument("--direct",action="store_true")
    p.add_argument("--width",type=int,choices=(32,128,1024,1588,8192),default=32)
    for field in ("image","build","powershell","lineage","operand-receipt","graph-receipt","reference"):
        required=field in ("image","build","powershell","lineage")
        p.add_argument("--"+field,type=Path,required=required)
        p.add_argument("--"+field.replace("-receipt","")+"-sha256",required=required)
    for field in ("snapshot","output"):
        p.add_argument("--"+field,type=Path,required=True)
    run(p.parse_args())
