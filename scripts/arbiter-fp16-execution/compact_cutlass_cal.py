# SPDX-License-Identifier: Apache-2.0
"""Authenticate the protected Compact Windows CUDA phase and its original chronological CAL population."""
from collections import Counter
import json
import math
from pathlib import Path


MODEL = "arbiter-context-qwen3-17b"
ORIGINAL = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"
AUDIT = "87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661"
PLAN4 = "9259b4be9e340ac8e050060e904f53acffee27e3807eca6145a9a8f3be3e299d"
PLAN5 = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf"
ACQUIRED5 = "f0e60bafcd2e431f8daaa00b75b05b856506fb3afe08752fba48e9af8e324981"
CORRECTED_SELECTOR = "e7a87b85e038ab04f5e2e17b8a1d7804d4ee89969ab956aea6ff0a84a591a413"
R4_COMPILER = "a9920b78d7c9532c66c9cb07ead4d23fccd0084d26f65657b42fb00e9772795e"
R4_BATCH = "1c39ff2046d72f1612c92b477fec1e5322b2482d4dd87a19fd56c6e73966567f"
TYPED = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581"
GRAPH = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"
WEIGHTS = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_compact::cal::"
SELECTOR = PREFIX + "protected::compact_cutlass_original_cal_phase"
RUST = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/"
SOURCE_KEYS = (RUST + "cuda_qualification/fp16_compact/cal.rs", RUST + "cuda_qualification/fp16_compact/cal/protected.rs",
               "crates/airk-retrieval/src/arbiter/scientific_cal.rs")
LAYOUT = {"r4-prefix64": ("windows-cal-chronological-native-batch64-r1",64,PLAN4),
          "r5-first56": ("r5-chronological-native-first56-r1",56,PLAN5),
          "r5-full100": ("r5-chronological-native-full100-r1",100,PLAN5)}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def inputs(args, campaign, base, exact):
    build_files = base.read(str(args.build), 32 << 20)["groups"]["native"]["files"]
    require(build_files[SOURCE_KEYS[1]] == CORRECTED_SELECTOR, "protected selector lacks phase-local operation ordinals")
    return original_inputs(args, campaign, base, exact)


def original_inputs(args, campaign, base, exact):
    require(args.cal_epoch in LAYOUT and type(args.cal_phase) is int, "frozen CAL epoch and ordinal phase required")
    folder, count, plan = LAYOUT[args.cal_epoch]
    root = campaign / folder
    refs = {}
    def keep(path, sha):
        path = Path(path)
        ref = exact(path,sha,base)
        refs[str(path)] = ref
        return ref
    def linked(ref):
        return keep(ref["path"],ref["sha256"])
    old = args.cal_epoch == "r4-prefix64"
    require(0 <= args.cal_phase * 8 < count and (not old or args.cal_phase == 0), "CAL phase outside original retained population")
    audit = keep(campaign / "cal100-focus1024-launch-r1/cal100-launch-audit.json",AUDIT)
    census = linked(base.read(audit["path"])["census"])
    ids = base.read(census["path"])["question_ids"]
    if old:
        require(args.cal_compiler_sha256 in (None,R4_COMPILER) and args.cal_projection_receipt_sha256 is None,
                "R4 engineering input requires its original native receipt")
        compiler = keep(root/"native.json",R4_COMPILER)
        projection = keep(root/"compact-batch.json",R4_BATCH)
    else:
        require(args.cal_compiler_sha256 and args.cal_projection_receipt_sha256, "sealed R5 closure and compiler required")
        wrapper = keep(root/"projection-receipt.json",args.cal_projection_receipt_sha256)
        presentation = base.read(wrapper["path"])
        require(presentation["original_requests"] == count and presentation["NN_invocations"] == presentation["gain_certificates"] == 0
                and presentation["plan_sha256"] == plan and len(presentation["lifetimes"]) == (1 if count==56 else 2)
                and bool(presentation["terminal"]) == (count==100), "R5 population or original closure differs")
        root5 = campaign / "cal100-focus1024-capture-r5"
        keep(root5/"lifetime-plan.json", PLAN5)
        require(presentation["acquired_sha256"] == ACQUIRED5, "wrong R5 original installation acquisition")
        keep(root5/"control-mutex-acquired.json", ACQUIRED5)
        if count == 100:
            outer, fixture, release = presentation["terminal"]
            keep(root5/"outer-owned.json",outer)
            keep(root5/"fixture-closure.json",fixture)
            keep(root5/"control-mutex-released.json",release)
        for i, life in enumerate(presentation["lifetimes"]):
            keep(campaign / f"cal100-focus1024-capture-r5/lifetime-{i:02}/lifetime-closure.json", life)
        compiler = keep(root/"native.json",args.cal_compiler_sha256)
        require(presentation["original_compiler_verification"] == compiler,
                "R5 compiler outside closure")
        projection = keep(root/"compact.json",presentation["models"]["compact"]["sha256"])
        require(presentation["models"]["compact"] == projection, "R5 Compact projection changed")
    native = base.read(compiler["path"])
    job = native["job"]
    require(native["passed"] and native["NN_invocations"] == 0 and native["launch_audit_sha256"] == AUDIT
            and native["inputs"]["compact"] == {"path":projection["path"],"sha256":projection["sha256"]}
            and job["exit_code"] == 0 and not job["timed_out"] and not job["owned_job_forced_termination"]
            and job["owned_descendants_settled"] and job["owned_job_final_active_processes"] == 0
            and job["owned_job_final_member_pids"] == [] and job["owned_leader_pid"] == job["suspended_assignment"]["pid"]
            and job["suspended_assignment"]["image"] == native["image"]
            and native["command"][2] == PREFIX+"chronological_original_context_compiler_batch",
            "unsettled original compiler or wrong model/phase input")
    keep(root/"native.log", native["log_sha256"])
    linked(native["image"])
    source = base.read(projection["path"],128<<20)
    phases = source["phases"]
    all_rows = source["rows"]
    require(source["model"] == MODEL and source["original_manifest"] == ORIGINAL and source["phase_plan_sha256"] == plan
            and source["launch_audit_sha256"] == AUDIT and source["NN_invocations"] == 0 and len(ids) == len(set(ids)) == 100
            and source["original_requests"] == len(all_rows) == count and [r["id"] for r in all_rows] == ids[:count]
            and [r["ordinal"] for r in all_rows] == list(range(count)) and len(phases) == (count+7)//8
            and source["statuses"] == dict(Counter(r["status"] for r in all_rows)), "wrong chronological CAL population")
    eligible = sum(r["status"] == "eligible" for r in all_rows)
    require(native["summary"]["compact"] == f"ContextQwenCompact original_requests={count} native_recompiled={eligible} retained_refusals={count-eligible}",
            "original native compiler census disagrees with population")
    for i,p in enumerate(phases):
        report = base.read(linked(p)["path"],32<<20)
        rows = all_rows[i*8:(i+1)*8]
        require(report["phase"] == i and report["plan_sha256"] == plan
                and report["closed_questions"] == len(rows) and report["certificate_count"] == 0,
                "original phase closure or role changed")
        for row, item in zip(rows,report["rows"],strict=True):
            require((row["ordinal"],row["id"],row["captures"]) == (item["ordinal"],item["question_id"],item["captures"]),
                    "case, source or original capture differs")
            if i == args.cal_phase:
                for original in item["captures"]:
                    cap = base.read(linked(original)["path"], 8<<20)
                    if row["status"] == "eligible":
                        req = cap["request"]
                        typed, = [x for x in cap["compiler_census"] if x["model"] == MODEL]
                        require(cap["Context_NN_dispatch"] is False and cap["actual_Context_profile"]["manifest"] == ORIGINAL
                                and row["remaining_ms"] == min(req["remaining_ms"],cap["remaining_ns_at_observation"]//1_000_000)
                                and row["remaining_input_tokens"] == req["remaining_input_tokens"] and row["roster"] == req["roster"]
                                and typed["compiled"] == row["compiled"] and typed["fits_remaining_allowance"] is True,
                                "original native row budget, cards or input differs")
    phase_rows = all_rows[args.cal_phase*8:(args.cal_phase+1)*8]
    return list(refs.values()),phase_rows,{"AIRK_CUDA_CAL_PHASE":str(args.cal_phase),
            "AIRK_CUDA_CAL_LAUNCH_AUDIT":audit["path"],"AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256":AUDIT,
            "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT":projection["path"],"AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256":projection["sha256"]}


def journal_rows(events, selected, expiry, projection, pid, image):
    require(len(events) == 2*len(selected), "missing or extra original CAL operation")
    previous = 0
    for n,(src,score) in enumerate(selected):
        start, stop = events[2*n:2*n+2]
        for j,(event,stage) in enumerate(((start,"started"),(stop,"returned"))):
            require(event["schema"] == "airk.arbiter.native-operation/1" and event["phase"] == stage
                    and event["sequence"] == 2*n+j+1 and event["kind"] == "numeric" and event["ordinal"] == 1 + src["ordinal"] % 8
                    and event["clock"] == "windows_performance_counter" and event["expires_monotonic_ns"] == expiry
                    and event["operation_input_digest"] == src["compiled"]["input_digest"] and event["identity"] == {
                        "input_sha256":projection,"profile":TYPED,"process_id":pid,"native_binary_sha256":image},
                    "operation ordinal, clock, source or owned PID differs")
        require(previous <= start["started_monotonic_ns"] == start["observed_monotonic_ns"] == stop["started_monotonic_ns"]
                <= stop["observed_monotonic_ns"] < expiry and start["native_call_elapsed_us"] is None
                and start["native_success"] is None and stop["native_success"] is True
                and stop["native_call_elapsed_us"] == score["native_call_elapsed_us"] > 0
                and score["inference_us"] >= score["native_call_elapsed_us"]
                and score["compile_us"] + score["inference_us"] < src["remaining_ms"]*1000,
                "unsynchronized, late or mismatched original observation")
        previous = stop["observed_monotonic_ns"]


def observation(raw, journal, original, env, pid, typed):
    lines = raw.decode("utf-8").replace("\x1b[36m","").replace("\x1b[0m","").splitlines()
    marker = "AIRK_COMPACT_CUTLASS_CAL="
    matched = [line[len(marker):] for line in lines if line.startswith(marker)]
    require(len(matched)==1 and any("1 passed; 0 failed" in line for line in lines) and typed == TYPED, "wrong native CAL selector or graph")
    result = json.loads(matched[0])
    cal = result["cal"]
    require(result["schema"] == "airk.private.compact-protected-cutlass-model/1" and result["model_sha256"] == GRAPH
            and result["weight_sha256"] == WEIGHTS and result["typed_graph_sha256"] == typed and result["placement"]["complete"]
            and result["route_refusal"] is None and result["cpu_refusal"] is None and result["graph"] is None
            and result["score"] is None and result["timing"] is None and not result["product_admission"] and not result["qualification"]
            and cal["phase"] == int(env["AIRK_CUDA_CAL_PHASE"]) and not cal["qualification"] and not cal["calibration_fitted"],
            "original graph, placement or CAL role differs")
    for name, ref in (("AIRK_CUDA_CAL_LAUNCH_AUDIT",cal["launch_audit"]),
                      ("AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT",cal["original_projection"])):
        require(ref["path"] == env[name] and ref["sha256"] == env[name+"_SHA256"], "native CAL source pin differs")
    selected = []
    for row, score in zip(original,cal["rows"],strict=True):
        require(score["ordinal"] == row["ordinal"] and score["id"] == row["id"], "changed native CAL chronology")
        if row["status"] != "eligible":
            require(score == {"ordinal":row["ordinal"],"id":row["id"],"status":row["status"]}, "scores invented for original refusal")
            continue
        require(score["status"] == "raw_scores_only" and all(row[k]==score[k] for k in ("compiled","captures","roster","remaining_ms"))
                and score["input_digest"] == row["compiled"]["input_digest"] and len(row["compiled"]["input_ids"]) <= row["remaining_input_tokens"]
                and len(score["scores"]) == len(row["roster"]["cards"]) == len(row["compiled"]["option_mask"])
                and all(math.isfinite(v) for v in score["scores"]), "changed input, incomplete vector or lost source budget")
        selected.append((row,score))
    require(result["cal_native_forwards"] == len(selected) and journal.is_file(), "native forward count or journal missing")
    journal_rows([json.loads(line) for line in journal.read_bytes().splitlines()], selected,
                 int(env["AIRK_COMPACT_CUTLASS_EXPIRES_NS"]),env["AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256"],
                 pid,env["AIRK_COMPACT_CUTLASS_TEST_IMAGE_SHA"])
    records = [line for line in lines if line.startswith("Operator=")]
    expected = [f"Operator=MultiHeadAttention Node=/airk_compact_cutlass/{n}/efficient DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION" for n in range(28)]
    scratch = [line for line in lines if line.startswith("flash=")]
    require(len(records) == len(scratch) == 28 * len(selected)
            and all(set(records[j:j+28]) == set(expected) for j in range(0,len(records),28))
            and all("efficient=1" in line and "has_qkv_workspace=0, workspace=0" in line for line in scratch),
            "native Compact phase lacks 28 protected attention kernels per original forward")
    return result,records,scratch
