# SPDX-License-Identifier: Apache-2.0
"""Run the fixed Compact CUTLASS operator in one original, authenticated native Windows Job."""
import argparse
import ctypes
import json
import os
import shutil
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
SELECTOR = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_compact::full::compact_cutlass_original_t1014_operand"
PREP = CAMPAIGN / "compact-cutlass-fp32-t1014-operands-r1/prepare.json"
PREP_SHA = "bf26d16b2ed80595789441a941c9549aa5ae6fbbdeca3afb17b42aee3b73c501"
MANIFEST_SHA = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
HOST = 4 << 30


def native_output(raw):
    # CU15's attention debug printer wraps complete records in cyan SGR.
    text = raw.decode("utf8").replace("\x1b[36m","").replace("\x1b[0m","")
    lines = text.splitlines()
    prefix = "AIRK_COMPACT_CUTLASS_OPERAND="
    results = [line[len(prefix):] for line in lines if line.startswith(prefix)]
    if len(results) != 1 or not any("1 passed; 0 failed" in line for line in lines):
        raise ValueError("exactly one native selector must pass")
    events = [line for line in lines if line.startswith("Operator=")]
    expected = "Operator=MultiHeadAttention Node=airk_cutlass_gqa_decision_control DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION"
    if events != [expected,expected]:
        raise ValueError("both actual original queries must use CUTLASS FP32 efficient attention")
    scratch = [line for line in lines if line.startswith("flash=")]
    expected_scratch = "flash=0, lean=0, efficient=1, fused_runner=0, fused_cross=0, bias=0, attn_bias=0, mask_dims=0, has_qkv_workspace=0, workspace=0, past=0, present=0"
    if scratch != [expected_scratch,expected_scratch]:
        raise ValueError("unexpected attention scratch or dispatch")
    return json.loads(results[0]), events, scratch


def model_output(raw, mode, shared):
    text = raw.decode("utf8").replace("\x1b[36m","").replace("\x1b[0m","")
    lines = text.splitlines()
    prefix = "AIRK_COMPACT_CUTLASS_MODEL="
    matches = [line[len(prefix):] for line in lines if line.startswith(prefix)]
    require = shared["require"]
    require(len(matches)==1 and any("1 passed; 0 failed" in line for line in lines), "one authentic native model observation required")
    value = json.loads(matches[0])
    records = [line for line in lines if line.startswith("Operator=")]
    scratch = [line for line in lines if line.startswith("flash=")]
    require(value["schema"]=="airk.private.compact-protected-cutlass-model/1"
            and value["model_sha256"]=="147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"
            and value["weight_sha256"]=="0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
            and value["placement"]["complete"], "candidate graph, weights or placement changed")
    if mode in ("fixed","mixed","pressure-fit","pressure-fixed"):
        require(value["graph"] is None and value["score"] is None and value["timing"]["profiled"] is False
                and not records and not scratch, "warm timing must be uninstrumented")
    elif mode=="graph":
        require(value["score"] is None and not records and not scratch, "graph-only probe dispatched numeric work")
        require(value["graph"]["complete"], "typed graph observation incomplete")
    else:
        expected={f"Operator=MultiHeadAttention Node=/airk_compact_cutlass/{layer}/efficient DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION"
                  for layer in range(28)}
        require(len(records)==28 and set(records)==expected and len(scratch)==28 and all(
            "efficient=1" in line and "has_qkv_workspace=0, workspace=0" in line for line in scratch),
            "all 28 measured FP32 memory-efficient dispatches required")
        require(value["score"]["forward_synchronized"] and value["graph"] is None, "score did not return synchronously")
    return value, records, scratch


def reviewed_typed_graph(value,shared):
    require=shared["require"]
    require(value["route_refusal"] is None and value["cpu_refusal"] is None,"native CUDA or bounded CPU route refused")
    rows=value["graph"]["rows"]
    nodes={row["node"]:row for row in rows if row["kind"]=="node"}
    tensors={row["value"]:row for row in rows if row["kind"]=="value"}
    edges={(row["node"],row["role"],row["slot"]):row["value"] for row in rows if row["kind"]=="edge"}
    proof=shared["validate_projection_graph"](value["graph"],full=True)
    rosters={f"/airk_compact_cutlass/{layer}/efficient" for layer in range(28)}
    matched=set()
    for idx,node in nodes.items():
        if node["op"]=="MultiHeadAttention":
            require(node["name"] in rosters and node["name"] not in matched and node["domain"]=="com.microsoft"
                    and node["ep"]=="CUDAExecutionProvider","unexpected finalized attention node")
            for role,slot in ((0,0),(0,1),(0,2),(2,0)):
                t=tensors[edges[idx,role,slot]]
                require(t["element_type"]==1 and t["rank"]==3,"FP32 rank-three MHA input/output required")
            matched.add(node["name"])
    require(matched==rosters,"mandatory 28 FP32 attentions missing")
    return dict(proof, cuda_mha_fp32=len(matched), bounded_cpu_payload_limit=9443)


def original_input(case, base, exact, require):
    if case=="fixed":
        pin=exact(CAMPAIGN/"compact-fp16-full-engineering-score-r1/native-result.json",
                  "735c77839099adf75426cf3ef9884ad69a99b60bb73e5fdf6247b7f321b12ded",base)
        return base.read(pin["path"])["rows"][0], [pin]
    if case=="q543":
        pin=exact(CAMPAIGN/"compact-train-focus1024-reference-q543-r1/execution.json",
                  "1ca4ef30499c75dac97c8a047988e03aaaec7d8e774f7ba82c965ca721aaecd1",base)
        value=base.read(pin["path"])
        require(value["complete"] and value["owned"]["owned_descendants_settled"],"original publisher reference did not close")
        row=value["result"]["rows"][0]["admissions"][0]
        raw=exact(CAMPAIGN/"compact-development-TRAIN_Q543-focus1024-run-r1/fixture/context-cal-admissions/record-0001.json",
                  "50ed40cec62a39b0067515b9aab2cd97c2849ef547dad149c49d86a32a863912",base)
        observations=[c for c in base.read(raw["path"])["compiler_census"] if c["profile"]==row["profile"]]
        require(row["capture"]==raw and len(observations)==1 and observations[0]["compiled"]==row["compiled"]
                and row["ordinal"]==1 and row["remaining_input_tokens"]==4096, "original exposed development row changed")
        return row, [pin,raw]
    if case=="32":
        pin=exact(CAMPAIGN/"compact-fp16-qkv-fp32-reference-r1/compact-qkv-fp32-t32-reference.json",
                  "304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28",base)
        row=base.read(pin["path"])["reference"]
    else:
        index=("128","1024","8192").index(case)
        pin=exact(CAMPAIGN/"compact-fp16-qkv-fp32-wider-reference-r1/compact-qkv-fp32-wider-reference.json",
                  "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14",base)
        row=base.read(pin["path"])["references"][index]
    require(len(row["input"]["input_ids"])==int(case) and len(row["original_fp32"])==8, "wrong fixed control row")
    return dict(row,compiled=row["input"]), [pin]


SCORE_COMPARISON = HERE / "score_comparison.py"
SCORE_COMPARISON_SHA = "24f264d1989c105c9b284f8eccb104f4f786398c5605ef23be59b0e4bd5428c1"

def numerical(result, matching, original):
    return runpy.run_path(str(SCORE_COMPARISON))["numerical"](result, matching, original)


def host_cap(mode, case, cal_epoch=None):
    if mode == "cal" and cal_epoch in ("r5-first56","r5-full100"):
        return 16 << 30
    if mode == "pressure-fit":
        return 12 << 30
    if mode=="operand":
        return HOST
    if mode in ("graph","fixed","mixed","pressure-fixed") or (mode=="score" and case in {"32","128","1024"}):
        return 16<<30
    return 32<<30


def run(args, *, pressure=None):
    deadline = time.monotonic() + 120
    shared = runpy.run_path(str(HERE / "probe_compact.py"))
    component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    wddm = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/wddm_process_samples.py"))
    capacity = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/wddm_capacity_snapshot.py"))
    base = component["base"]()
    parent, owner = base.helpers({"ownership_helper_root":str(base.MAINTAINED_HELPER_ROOT)})
    require, exact = shared["require"], shared["exact_pin"]
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and parent.worker.windows_sid() == base.SID and component["token_owner"](ctypes.c_void_p(-1)) == base.SID,
            "checked original Windows user and isolated interpreter required")
    guard = {"checked_guard":base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell":exact(args.powershell, args.powershell_sha256, base)}
    def audit(paths, created=True, *, end=deadline):
        component["guard_many"](guard, list(paths), created, deadline=end)
    out = args.output
    require(out.is_absolute() and out.parent == CAMPAIGN and out.parent.resolve(strict=True) == out.parent and not out.exists(),
            "fresh campaign output required")
    audit([args.image, args.build, PREP, args.snapshot, args.image.with_name("airk-cuda-runtime.json")])
    image, build = exact(args.image, args.image_sha256, base), exact(args.build, args.build_sha256, base)
    manifest = exact(args.image.with_name("airk-cuda-runtime.json"), MANIFEST_SHA, base)
    prep = exact(PREP, PREP_SHA, base)
    prep_value = base.read(str(PREP))
    inputs = [prep_value[k] for k in ("graph", "operands", "reference")]
    inputs.append(exact(SCORE_COMPARISON, SCORE_COMPARISON_SHA, base))
    audit([Path(p["path"]) for p in inputs])
    for item in inputs:
        base.pinned(item)
    chain=None
    if args.lineage is not None:
        fingerprint_pin=exact(args.lineage,args.lineage_sha256,base)
        saved=base.read(fingerprint_pin["path"])
        require(saved["schema"]=="airk.private.compact-static-cargo-custody/1" and saved["complete"]
                and saved["native_invocations"]==0 and saved["image"]==image and saved["build"]==build
                and saved["runtime_manifest"]==manifest and len(saved["compile_time_cuda_approvals"])==3,
                "unrelated static Cargo artifact lineage")
        chain=dict(saved,build=build)
    log, linked, sources = shared["linked_image"](build, image, base, audit, "metadata", source_snapshot=args.snapshot,cargo_chain=chain)
    files = base.read(build["path"], 32 << 20)["groups"]["native"]["files"]
    for key in ("crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/cutlass.rs",
                "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/fp16_compact/full.rs"):
        path = args.snapshot / (files[key] + ".source")
        audit([path])
        sources[key] = exact(path,files[key],base)
    approvals = shared["decision_build_approvals"](log, linked, manifest, base, audit,cargo_chain=chain)
    if args.mode=="seal":
        require(chain is None,"a new static seal requires original live Cargo artifacts")
        component["mkdir"](guard,out,deadline=deadline)
        target = Path(build["path"]).parent/"retained-cargo-lineage"
        if not target.exists():
            component["mkdir"](guard,target,deadline=deadline)
        audit([target])
        for original in [linked,*approvals]:
            destination=target/(original["sha256"]+".cargo")
            if not destination.exists():
                audit([destination],False)
                with Path(original["path"]).open("rb") as source,destination.open("xb") as sink:
                    shutil.copyfileobj(source,sink,1<<20)
            audit([destination])
            require(base.pin(destination)["sha256"]==original["sha256"],"immutable Cargo byte copy changed")
        for original in [build,image,log,linked,manifest,*approvals]:
            base.pinned(original)
        receipt=out/"execution.json"
        audit([receipt],False)
        with receipt.open("x",encoding="utf8") as file:
            json.dump({"schema":"airk.private.compact-static-cargo-custody/1","complete":True,"native_invocations":0,
                "build":build,"image":image,"runtime_manifest":manifest,"linked_image":linked,"build_log":log,
                "compile_time_cuda_approvals":approvals,"source":base.pin(Path(__file__).resolve())},file)
        audit([receipt])
        return
    recipe = runpy.run_path(str(HERE / "compact_cutlass_attention.py"))["backend_sources"](args.image.parent)
    mode = args.mode
    pressure_trial = mode in ("pressure-fit", "pressure-fixed")
    require(pressure_trial == (pressure is not None), "pressure modes require the owned resident worker")
    timing = mode in ("fixed","mixed") or pressure_trial
    timing_source=base.pin(HERE/"compact_cutlass_timing.py") if timing else None
    timing_owner=runpy.run_path(timing_source["path"]) if timing else None
    if timing:
        inputs.extend([timing_source,base.pin(HERE/"timing_control.py")])
    cal_mode = mode == "cal"
    if cal_mode:
        cal_source = base.pin(HERE / "compact_cutlass_cal.py")
        cal_owner = runpy.run_path(cal_source["path"])
        inputs.append(cal_source)
        for key in cal_owner["SOURCE_KEYS"]:
            path = args.snapshot / (files[key] + ".source")
            audit([path])
            sources[key] = exact(path, files[key], base)
    memory = host_cap(mode,args.case,args.cal_epoch if cal_mode else None)
    selector = SELECTOR if mode=="operand" else SELECTOR.rsplit("::",1)[0]+"::compact_cutlass_protected_"+({"fixed":"fixed_timing","mixed":"mixed_timing","graph":"graph"}.get(mode,"fixed_score"))
    if pressure_trial:
        selector = SELECTOR.rsplit("::", 1)[0] + "::compact_cutlass_" + mode.replace("-", "_")
        for key in ("crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/cutlass/model/pressure.rs",
                    "crates/airk-retrieval/src/arbiter/runtime/cuda/windows.rs"):
            path = args.snapshot / (files[key] + ".source")
            audit([path])
            sources[key] = exact(path, files[key], base)
    if cal_mode:
        selector = cal_owner["SELECTOR"]
    typed_gate = None
    if mode!="operand":
        stage=exact(CAMPAIGN/"compact-cutlass-protected-staged-r1/stage.json","4465463b434f6bb5e9798307c31e622dc3da4d67b6a5f6bf5793daf5c94beabf",base)
        arm=base.read(stage["path"])
        inputs += [stage, arm["candidate_graph"], arm["candidate_weights"]]
        model_source = "crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/cutlass/model.rs"
        file = args.snapshot/(files[model_source]+".source")
        audit([file,*[Path(x["path"]) for x in inputs]])
        sources[model_source]=exact(file,files[model_source],base)
        model_inputs_source = model_source.removesuffix(".rs")+"/inputs.rs"
        if args.case!="fixed" or model_inputs_source in files:
            file=args.snapshot/(files[model_inputs_source]+".source")
            audit([file])
            sources[model_inputs_source]=exact(file,files[model_inputs_source],base)
        for item in inputs:
            base.pinned(item)
    if mode=="score" or timing or cal_mode:
        owner_pin=exact(args.graph_receipt,args.graph_sha256,base)
        observed=base.read(owner_pin["path"],64<<20)
        require(observed["complete"] and observed["mode"]=="graph" and observed["owned"]["exit_code"]==0
                and observed["owned"]["owned_descendants_settled"] and observed["owned"]["owned_job_final_active_processes"]==0
                and observed["image"]==image and observed["build"]==build and observed["typed_graph_validation"]==reviewed_typed_graph(observed["result"],shared),
                "original settled same-image typed graph required")
        typed_gate=observed["result"]["typed_graph_sha256"]
        inputs.append(owner_pin)
    if timing:
        expected_words,raw_pins=timing_owner["inputs"](CAMPAIGN,typed_gate,args.maximum_receipt_sha256,base,exact)
        inputs+=raw_pins
    if mode=="score":
        if args.case=="fixed":
            matching_pin=exact(CAMPAIGN/"compact-cutlass-fixed-publisher-reference-r1/execution.json",
                               "8c59af643b05961a039a3f2546fb3698ea3a5eaa0b4e5a9a01ab700d32e5591e",base)
        else:
            require(args.reference.parent.parent==CAMPAIGN,"matching proof must be in this exact campaign")
            matching_pin=exact(args.reference,args.reference_sha256,base)
        ref=base.read(matching_pin["path"])
        original,original_pins=original_input(args.case,base,exact,require)
        require(ref["complete"] and ref["owned"]["owned_descendants_settled"] and ref["result"]["compiled"]==original["compiled"]
                and ref["result"]["candidate_graph_sha256"]==arm["candidate_graph"]["sha256"],"original compiled reference differs")
        require(ref["owned"]["exit_code"]==0 and ref["owned"]["owned_job_final_active_processes"]==0
                and not ref["owned"]["owned_job_forced_termination"] and ref["result"]["threads"]==2
                and ref["result"]["recipe_sha256"]=="7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394",
                "matching source, quiet worker or closed reference lifetime differs")
        if args.case!="fixed":
            require(ref["result"]["original_binding"]["original_fp32"]==original["original_fp32"],
                    "independent publisher comparator changed")
        inputs += [matching_pin,*original_pins]
    if cal_mode:
        cal_inputs, phase_rows, cal_env = cal_owner["inputs"](args, CAMPAIGN, base, exact)
        inputs.extend(cal_inputs)
        audit([Path(p["path"]) for p in cal_inputs])
    command = [str(args.image),"--exact",selector,"--ignored","--nocapture"]
    receipt = {"schema":"airk.private.compact-cutlass-owned/1", "complete":False, "command":command,"image":image,"build":build,
               "linked_image":linked, "compile_time_cuda_approvals":approvals,"build_log":log,"runtime_manifest":manifest,
               "sources":sources,"preparation":prep,"mode":mode,"case":args.case,"model_inputs":inputs,"original_limits":{"seconds":120,"host_bytes":memory,"campaign_host_ceiling_bytes":HOST if mode=="operand" else 32<<30,"ort_arena_bytes":256<<20 if mode=="operand" else 24<<30},
               "checked_launcher":base.pin(Path(__file__).resolve()),"backend_sources":recipe,"result":None}
    if pressure_trial:
        sampler_source = base.pin(ROOT / "scripts/cuda-component-execution/wddm_process_samples.py")
        inputs.append(sampler_source)
        receipt["pressure_sampler_source"] = sampler_source
        receipt["original_limits"]["ort_arena_bytes"] = pressure.tier * (1 << 30)
    component["mkdir"](guard,out,deadline=deadline)
    temp, stdin, stdout, output = (out / n for n in ("temp","stdin.txt","stdout-stderr.log","execution.json"))
    try:
        component["mkdir"](guard,temp,deadline=deadline)
        audit([stdin, stdout, output],False)
        env = {k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","ORT_","PYTHON","CUDA_","CUDNN_"))}
        env.update(PATH=r"C:\Windows\System32;C:\Windows", TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
                   AIRK_PRIVATE_CENSUS_BASE=str(temp), CUDA_CACHE_PATH=str(temp),
                   ORT_CUDA_GEMM_OPTIONS="2", ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO="1")
        if timing:
            env.update(ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO="0",AIRK_COMPACT_CUTLASS_EXPIRES_NS=str(int(deadline*1_000_000_000)),
                       AIRK_COMPACT_CUTLASS_TEST_IMAGE_SHA=image["sha256"],
                       AIRK_COMPACT_CUTLASS_EXPECTED_WORDS=json.dumps(expected_words,separators=(",",":")))
            audit([temp/"protected.operations.jsonl"],False)
        if pressure_trial:
            env.update(AIRK_COMPACT_PRESSURE_TIER_GIB=str(pressure.tier), AIRK_COMPACT_PRESSURE_NONCE=pressure.nonce)
            audit([temp/"pressure-admission.json"], False)
        if cal_mode:
            env.update(cal_env, AIRK_COMPACT_CUTLASS_EXPIRES_NS=str(int(deadline*1_000_000_000)),
                       AIRK_COMPACT_CUTLASS_TEST_IMAGE_SHA=image["sha256"])
            audit([temp/"protected.operations.jsonl"],False)
        if typed_gate is not None:
            env["AIRK_COMPACT_CUTLASS_TYPED_GRAPH"]=typed_gate
            env["AIRK_COMPACT_CUTLASS_INPUT"]=args.case
        begin = capacity["snapshot"](None)
        luid = begin["dxgi"]["luid"]
        receipt["before_load_adapter"] = begin
        baseline = None
        def suspend(api, job, info, child_deadline):
            nonlocal baseline
            proof = parent.suspended_authentication(api, info, deadline, child_deadline, base.SID, image, owner.image_proof)
            require(component["token_owner"](info.process) == base.SID and owner.job_snapshot(api, job) == ([info.pid],1,1),
                    "unexpected suspended owner or Job membership")
            with wddm["ProcessSampler"](info,luid,segment_fields=pressure_trial) as sampler:
                baseline = sampler.sample()
            limits = shared["job_limits"](api,job,memory)
            if pressure_trial:
                grant = pressure.grant(info.pid, child_deadline, image["sha256"], typed_gate, int(env["AIRK_COMPACT_CUTLASS_EXPIRES_NS"]), baseline)
                path = temp/"pressure-admission.json"
                component["write_new"](guard, path, grant)
                receipt["pressure_grant"] = base.pin(path)
                inputs.append(receipt["pressure_grant"])
                host_path = temp / "pressure-host-admission.json"
                audit([host_path],False)
                host_grant = {"schema":"airk.private.cuda-pressure-host-admission/1",
                              "device_grant_sha256":receipt["pressure_grant"]["sha256"], "candidate_pid":info.pid,
                              "executable_sha256":image["sha256"], "mode":mode, "limits":limits,
                              "campaign_host_ceiling_bytes":32<<30}
                component["write_new"](guard,host_path,host_grant)
                receipt["pressure_host_grant"] = base.pin(host_path)
                inputs.append(receipt["pressure_host_grant"])
            return dict(proof, default_owner_sid=base.SID, observed_limits=limits)
        def observe(api, job, info, expiry):
            return {"wddm":wddm["observe_process"](api,info,expiry,luid,baseline,segment_fields=pressure_trial)}, []
        with stdin.open("xb"):
            pass
        with stdin.open("rb") as source, stdout.open("xb") as sink:
            audit([stdin,stdout])
            receipt["owned"] = owner.run_windows_owned_training(command,ROOT,sink,deadline-time.monotonic(),env,source,memory,
                before_resume=suspend,after_resume=observe,absolute_deadline=deadline,cleanup_seconds=5)
        require(parent.settled(receipt["owned"]), "original one-operator Job did not settle successfully")
        if cal_mode:
            journal = temp / "protected.operations.jsonl"
            audit([journal])
            result, logs, scratch = cal_owner["observation"](stdout.read_bytes(), journal, phase_rows, env,
                                                            receipt["owned"]["owned_leader_pid"], typed_gate)
            receipt["operations"] = base.pin(journal)
            receipt["cal_inputs"] = cal_env
        else:
            result, logs, scratch = native_output(stdout.read_bytes()) if mode=="operand" else model_output(stdout.read_bytes(),mode,shared)
        receipt["result"]=result
        if timing:
            require(result["typed_graph_sha256"]==typed_gate,"unprofiled session graph changed")
            if pressure_trial:
                pressure.verify_native(result["pressure"], receipt["pressure_grant"]["sha256"])
            timing_rows=result["timing"]["rows"]
            receipt["timing_summary"]=timing_owner["summarize"](mode,timing_rows,expected_words)
            journal=temp/"protected.operations.jsonl"
            audit([journal])
            events=[json.loads(line) for line in journal.read_text().splitlines()]
            timing_owner["confirm_journal"](events,timing_rows,result["timing"]["expires_monotonic_ns"],env,image["sha256"])
            receipt["operations"]=base.pin(journal)
            controls=runpy.run_path(str(HERE/"timing_control.py"))
            receipt["memory_by_cycle"]=controls["memory_coverage"](
                {"fixed":"qkv-resource20", "mixed":"qkv-mixed", "pressure-fit":"qkv-fit-probe",
                 "pressure-fixed":"qkv-resource20"}[mode],receipt["owned"]["before_GO"]["wddm"],events,
                receipt["owned"]["owned_leader_pid"],luid,load_recorded=False)
        if mode=="graph":
            receipt["typed_graph_validation"]=reviewed_typed_graph(result,shared)
        if mode=="score":
            require(result["typed_graph_sha256"]==typed_gate and result["score"]["compiled_input_digest"]==original["compiled"]["input_digest"],"original score gate or row changed")
            receipt["numerical"]=numerical(result["score"]["raw_scores"],ref["result"]["matching_fp16"],original["original_fp32"])
            if args.case=="q543":
                cards=original["roster"]["cards"]
                ranks={}
                for label, scores in (("native",result["score"]["raw_scores"]),("matching",ref["result"]["matching_fp16"]),
                                      ("publisher",original["original_fp32"]),("historical",original["matching_fp16"])):
                    require(len(scores)==len(cards),"action and score cardinalities differ")
                    order=sorted(range(len(scores)),key=lambda i:-scores[i])
                    first,second=order[:2]
                    ranks[label]={"order":order,"top_action":cards[first]["action"],
                                  "winner_margin":scores[first]-scores[second]}
                receipt["development_actions"]={"rankings":ranks,"remaining_input_tokens":original["remaining_input_tokens"],
                    "remaining_ns":original["remaining_ns"],"certificate_applied":False,"action_executed":False}
        receipt.update(result=result,kernel_records=logs,scratch_records=scratch,after_release_adapter=capacity["snapshot"](None))
        require(receipt["after_release_adapter"]["dxgi"]["luid"] == luid, "adapter changed across lifetime")
        for item in [image,build,manifest,prep,*inputs,log,linked,*approvals,*sources.values(),receipt["checked_launcher"]]:
            base.pinned(item)
        receipt["complete"] = True
    except Exception as exc:
        receipt["error"] = type(exc).__name__ + ": " + str(exc)
    finally:
        if stdout.exists():
            receipt["original_stdout"] = base.pin(stdout)
        audit([p for p in out.rglob("*")], end=time.monotonic()+15)
        with output.open("x",encoding="utf8") as file:
            json.dump(receipt,file,allow_nan=False)
            file.write("\n")
        audit([output],end=time.monotonic()+10)
    require(receipt["complete"], receipt.get("error","invalid result"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode",choices=("operand","graph","score","seal","fixed","mixed","cal"),default="operand")
    parser.add_argument("--case",choices=("fixed","q543","32","128","1024","8192"),default="fixed")
    parser.add_argument("--cal-epoch", choices=("r4-prefix64","r5-first56","r5-full100"))
    parser.add_argument("--cal-phase", type=int, choices=range(13))
    parser.add_argument("--cal-compiler-sha256")
    parser.add_argument("--cal-projection-receipt-sha256")
    parser.add_argument("--reference",type=Path)
    parser.add_argument("--reference-sha256")
    parser.add_argument("--maximum-receipt-sha256")
    parser.add_argument("--lineage",type=Path)
    parser.add_argument("--lineage-sha256")
    parser.add_argument("--graph-receipt",type=Path)
    parser.add_argument("--graph-sha256")
    for key in ("image","build","snapshot","output","powershell"):
        parser.add_argument("--"+key,type=Path,required=True)
    for key in ("image","build","powershell"):
        parser.add_argument("--"+key+"-sha256",required=True)
    run(parser.parse_args())
