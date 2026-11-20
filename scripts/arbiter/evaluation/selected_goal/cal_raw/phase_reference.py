# SPDX-License-Identifier: Apache-2.0
"""Independent CPU Compact QKV and publisher vectors for one verified CAL lifetime."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import runpy
import struct
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
SCIENCE = ROOT / "scripts/arbiter-fp16-execution"
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
AUDIT = "87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661"
PLAN = "9259b4be9e340ac8e050060e904f53acffee27e3807eca6145a9a8f3be3e299d"
GRAPH = "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14"
WEIGHTS = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
PROFILE = "67cc4ddcd715c20daf9ed7462b03777002ef2c06db59f9069575b1c5d14bc6bc"
ORIGINAL = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"
MARKER = "AIRK_CAL_CPU_RESULT "
MIB = 1 << 20


def require(value, reason):
    if not value:
        raise ValueError(reason)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def checked(ref, maximum=256*MIB):
    path = Path(ref["path"])
    require(path.is_absolute() and path.resolve() == path and path.is_relative_to(CAMPAIGN)
            and path.stat().st_size <= maximum, "unapproved private input extent or location")
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == ref["sha256"]
            and len(raw) == ref.get("bytes", len(raw)), "private input bytes differ")
    return raw


def source_modules():
    sources = {"representative_compact_reference.py": "e574ed3d2b7dd3f736891bcf5e40c2eb45edc43f1fbb73bec902c330aded83e1",
               "replay_compact_t1024_suffix.py": "2ccd33badbfddb795f34f0e52ffce0091f9290956336fc135b0657c029441ff0"}
    for name, expected in sources.items():
        require(digest(SCIENCE/name) == expected, "unreviewed reference math or owner")
    base = runpy.run_path(str(SCIENCE/"representative_compact_reference.py"))
    owner = base["owner"]
    replay = owner.module("replay_layer0.py", owner.REPLAY_SHA)
    wider = owner.module("verify_projection_candidate_t32.py", replay.WIDER_REFERENCE_SHA)
    return base, owner, replay, wider


MODELS = {"compact": ("arbiter-context-qwen3-17b", ORIGINAL),
          "large": ("arbiter-context-qwen3-4b", "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307")}


def phase_rows(phase_ref, *, model="compact", retain_budget=False):
    model_name, original_manifest = MODELS[model]
    phase = json.loads(checked(phase_ref))
    n = phase["phase"]
    require(type(n) is int and 0 <= n < 13 and phase["input_audit_sha256"] == AUDIT
            and phase["plan_sha256"] == PLAN and phase["certificate_count"] == 0,
            "foreign or uncertified CAL phase")
    root = CAMPAIGN / "cal100-focus1024-capture-r4"
    plan = json.loads(checked({"path":str(root/"lifetime-plan.json"), "sha256":PLAN}))
    lifetime = json.loads(checked({"path":str(root/f"lifetime-{n:02}"/"lifetime-closure.json"),
                                   "sha256":phase["lifetime_sha256"]}))
    require(lifetime["phase_closure"] == phase["phase_owner"], "phase owner changed")
    owned = json.loads(checked(phase["phase_owner"]))
    require(phase["source_transcript"] == owned["transcript"], "phase transcript changed")
    transcript = (json.loads(line) for line in checked(phase["source_transcript"]).splitlines())
    delivered = [event["evidence"] for event in transcript if event["kind"] == "cal_question"]
    ids = plan["phase_ids"][n]
    require(len(ids) == phase["closed_questions"] == len(phase["rows"]) == len(delivered),
            "incomplete phase population")
    rows = []
    for offset, (case, event, expected_id) in enumerate(zip(phase["rows"], delivered, ids, strict=True)):
        require(case["ordinal"] == 8*n+offset and case["question_id"] == event["id"] == expected_id,
                "case order changed")
        records = [json.loads(checked(ref)) for ref in case["captures"]]
        require(records == event["captured"], "source capture mismatch")
        row = {"ordinal":case["ordinal"], "id":expected_id, "captures":case["captures"]}
        rows.append(row)
        if event["answer"]["refused"] or not records:
            row["status"] = "no_native_assessment_request"
            continue
        require(len(records) == 1, "ambiguous delivered assessment")
        record = records[0]
        request = record["request"]
        require(record["Context_NN_dispatch"] is False and record["Focus_configuration"] == "ON"
                and record["actual_Context_profile"]["manifest"] == ORIGINAL
                and request["question"] == event["answer"]["body"]["question"]
                and request["records"] == event["answer"]["body"]["records"][:len(request["records"])]
                and request["scope"] == event["answer"]["body"]["assessment"]["scope"],
                "foreign delivered native request")
        census, = [c for c in record["compiler_census"]
                   if c["profile"] == original_manifest and c["model"] == model_name]
        require(census["NN_invocations"] == 0, "scored census refused")
        row["roster"] = request["roster"]
        remaining = min(request["remaining_ms"], record["remaining_ns_at_observation"]//1000000)
        if retain_budget:
            row.update(remaining_ms=remaining, remaining_input_tokens=request["remaining_input_tokens"])
        if not census["fits_remaining_allowance"] or remaining <= 0:
            row["status"] = "compiler_refused_remaining_allowance"
            continue
        compiled = census["compiled"]
        require(0 < len(compiled["input_ids"]) <= request["remaining_input_tokens"] and
                1 <= len(compiled["candidate_positions"]) == len(row["roster"]["cards"]) <= 8, "original bounds differ")
        row.update(status="eligible", compiled=compiled)
    return rows


def score(phase_ref, ordinal, deadline, modules):
    def check():
        require(time.monotonic() < deadline, "absolute reference deadline expired")

    rows = [row for row in phase_rows(phase_ref) if row["ordinal"] == ordinal]
    require(len(rows) == 1 and rows[0]["status"] == "eligible", "only one originally eligible ordinal may load a model")
    base, owner, replay, wider = modules
    legacy = replay.read_only_bootstrap()
    proof, proof_pin = legacy.read_pinned_json(CAMPAIGN/"compact-fp16-qkv-fp32-wider-reference-r1"/wider.WIDER_LEAF,
                                              owner.REFERENCE_SHA, check)
    require(proof["candidate"]["graph"]["sha256"] == GRAPH and proof["candidate"]["weights"]["sha256"] == WEIGHTS,
            "reference recipe does not match registered scorer")
    runtime, runtime_pin = legacy.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, check)
    for ref in proof["publisher_files"].values():
        require(replay.file_pin(Path(ref["path"]), check) == ref, "publisher bytes changed")
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(wider.WIDER_CPU_THREADS)
    torch.set_num_interop_threads(1)
    half = owner.module("../arbiter-fp16-reference.py", legacy.REFERENCES[legacy.REFERENCE_OWNER])
    pub = owner.module("../"+legacy.PUBLISHER_OWNER.removeprefix("scripts/"), legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    original, config = pub.load_authenticated(legacy.PUBLISHER_ROOT, "Qwen/Qwen3-1.7B")
    model = half.HalfReference(torch, config, original.text.state_dict(), False, "cpu")
    with torch.inference_mode():
        control = proof["references"][0]
        require(len(control["input"]["input_ids"]) == 128, "missing original control")
        matching = base["matching"](half, wider, model, control["input"], check)
        fp32 = base["publisher_scores"](torch, original, control["input"])
        require(struct.pack("<8f", *matching) == struct.pack("<8f", *control["matching_qkv_fp32"])
                and max(abs(a-b) for a,b in zip(fp32,control["original_fp32"],strict=True)) <= .001,
                "T128 independent reference regression")
        for row in rows:
            if row["status"] != "eligible":
                continue
            start = time.monotonic()
            compiled = row["compiled"]
            half.validate_native(torch, config, False, compiled)
            row["matching_fp16"] = base["matching"](half, wider, model, compiled, check)
            check()
            row["original_fp32"] = base["publisher_scores"](torch, original, compiled)
            check()
            row["seconds"] = time.monotonic()-start
            require(all(len(row[k]) == len(row["roster"]["cards"]) and all(map(math.isfinite,row[k]))
                        for k in ("matching_fp16","original_fp32")), "invalid full reference vectors")
    require(not torch.cuda.is_initialized() and [r for r in phase_rows(phase_ref) if r["ordinal"] == ordinal] ==
            [{k:v for k,v in row.items() if k not in {"matching_fp16","original_fp32","seconds"}} for row in rows],
            "unexpected CUDA initialization or changed phase")
    pins = dict(runtime["files"])
    for ref in proof["reference_sources"]+[replay.file_pin(Path(__file__)),replay.file_pin(SCIENCE/"representative_compact_reference.py")]:
        pins[ref["path"]] = ref
    legacy.loaded_runtime(pins, replay.file_pin, check, [sys.modules[k] for k in sys.modules.keys()-before])
    return {"phase":phase_ref,"rows":rows,"profile":PROFILE,"graph":GRAPH,"weights":WEIGHTS,
            "runtime":runtime_pin,"reference_basis":proof_pin,"candidate_forwards":0}

def owned(args, phase_ref, modules):
    _, sci, replay, wider = modules
    legacy, _ = wider.load_base()
    deadline = time.monotonic() + legacy.MAX_WALL_SECONDS
    def check():
        require(time.monotonic() < deadline, "launch deadline expired")
    plan,_ = legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],
                                     wider.BASE_ARGUMENTS["conversion_plan_sha256"], check)
    wrapper = Path(plan["wrapper"]["path"])
    if replay.file_pin(wrapper) != plan["wrapper"]:
        wrapper = CAMPAIGN/"compact-conversion-owner-311d13cc"/"arbiter-fp16-execution.py"
    require(digest(wrapper) == plan["wrapper"]["sha256"], "conversion/custody owner changed")
    conversion = legacy.load_module(wrapper,"cal_cpu_custody")
    token = conversion.windows_owner()
    require(replay.file_pin(Path(sys.executable)) == plan["python"] and token["owner_sid"] == plan["human_sid"],
            "unexpected Python executable or execution owner")
    helper_root = ROOT/"scripts/cuda-native-execution/helpers"
    for name, expected in conversion.HELPERS.items():
        require(digest(helper_root/name)==expected,"owned launcher source changed")
    sys.path.insert(0,str(helper_root))
    parent=legacy.load_module(helper_root/"reference_parent_no_site_r2.py","cal_cpu_parent")
    job=legacy.load_module(helper_root/"reference_owned.py","cal_cpu_job")
    out=args.output
    require(out.is_absolute() and out==out.resolve() and out.parent==CAMPAIGN and not out.exists(),
            "fresh private campaign destination required")
    require(any(r["ordinal"] == args.ordinal and r["status"] == "eligible" for r in phase_rows(phase_ref)), "no eligible phase ordinal")
    cache=out/"temp"
    for path in (out,cache):
        conversion.guard(plan,path,deadline=deadline)
        path.mkdir()
        conversion.guard(plan,path,True,deadline=deadline)
    log,receipt=out/"worker.log",out/"execution.json"
    for path in (log,receipt):
        conversion.guard(plan,path,deadline=deadline)
    env={key:val for key,val in os.environ.items() if not key.upper().startswith(("AIRK_","ORT_","PYTHON","CUDA_","CUDNN_","TORCH_","HF_","TRITON_"))}
    env.update(CUDA_VISIBLE_DEVICES="",HF_HUB_OFFLINE="1",TRANSFORMERS_OFFLINE="1",TOKENIZERS_PARALLELISM="false",
               TORCH_HOME=str(cache),TORCHINDUCTOR_CACHE_DIR=str(cache),TRITON_CACHE_DIR=str(cache),
               TEMP=str(cache),TMP=str(cache),TMPDIR=str(cache))
    script=Path(__file__).resolve()
    source=replay.file_pin(script)
    argv=[sys.executable,"-I","-B","-S","-X","pycache_prefix="+plan["cache_prefix"],str(script),
          "--ordinal",str(args.ordinal),"--phase-input",phase_ref["path"],"--phase-sha256",phase_ref["sha256"],
          "--worker-deadline",repr(deadline),"--source-sha256",source["sha256"]]
    def suspended(api,handle,info,child_deadline):
        proof=parent.suspended_authentication(api,info,deadline,child_deadline,conversion.SID,plan["python"],job.image_proof)
        require(conversion.windows_owner(process=info.process)==token and job.job_snapshot(api,handle)==([info.pid],1,1),
                "foreign suspended child or Job")
        return proof
    with open(os.devnull,"rb") as stdin,log.open("xb") as stream:
        conversion.guard(plan,log,True,deadline=deadline)
        observation=job.run_windows_owned_training(argv,ROOT,stream,deadline-time.monotonic(),env,stdin,32<<30,
                                                   before_resume=suspended,absolute_deadline=deadline,cleanup_seconds=5)
    for path in [out,cache,log,*cache.rglob("*")]:
        conversion.guard(plan,path,True,deadline=time.monotonic()+10)
    require(log.stat().st_size<=16*MIB,"reference output limit exceeded")
    results=[line[len(MARKER):] for line in log.read_text(encoding="utf8").splitlines() if line.startswith(MARKER)]
    complete=parent.settled(observation) and len(results)==1 and replay.file_pin(script)==source
    with receipt.open("x",encoding="utf8") as stream:
        json.dump({"schema":"airk.private.compact-cal-cpu-phase/1","complete":complete,"phase":phase_ref,"source":source,
                   "command":argv,"owned":observation,"result":json.loads(results[0]) if complete else None},stream,allow_nan=False)
    conversion.guard(plan,receipt,True,deadline=time.monotonic()+10)
    require(complete,"incomplete CPU reference Job; inspect retained log and receipt")


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    mode=p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check-only",action="store_true")
    mode.add_argument("--output",type=Path)
    mode.add_argument("--worker-deadline",type=float)
    p.add_argument("--ordinal",type=int)
    p.add_argument("--phase-input",type=Path,required=True)
    p.add_argument("--phase-sha256",required=True)
    p.add_argument("--source-sha256")
    args=p.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked Windows Python -I -B -S required")
    ref={"path":str(args.phase_input),"sha256":args.phase_sha256}
    if args.check_only:
        rows=phase_rows(ref)
        print(json.dumps({"questions":len(rows),"eligible_tokens":[len(r["compiled"]["input_ids"]) for r in rows if r["status"]=="eligible"]}))
    elif args.output:
        owned(args,ref,source_modules())
    else:
        require(args.source_sha256==digest(Path(__file__)) and 0<args.worker_deadline-time.monotonic()<=600,
                "unsealed source or reference deadline")
        print(MARKER+json.dumps(score(ref,args.ordinal,args.worker_deadline,source_modules()),separators=(",",":"),allow_nan=False))