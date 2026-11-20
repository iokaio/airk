# SPDX-License-Identifier: Apache-2.0
"""Bind the previously selected real TRAIN Q543 row, its sparse readout and official Large FP32 control."""
import hashlib
import json
import math
import runpy
from pathlib import Path

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
INPUT="af125f4c7ef1cd3776ea0cf7308743724a958744a921559746d907d0278868a4"
RECORD="50ed40cec62a39b0067515b9aab2cd97c2849ef547dad149c49d86a32a863912"
CAPTURE="0eb6a5b4f433e9022179a1c950095f1466e9d8ca0b111ecd63ea2a0d7eeaaa47"
PROFILE="d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"
PUBLISHER="4c6f43d0d81539cfc6bf7b6ddf5ae69273e61508d0db3d22a5e7488c60560b6b"
STREAM="75df989310931603b3a1fd55cc70dca2e34543638c94ce9520284c1b4eec1c75"

def require(ok,reason):
    if not ok:
        raise ValueError(reason)

def pinned(path,sha,expected_bytes):
    path=path.absolute()
    require(path.resolve(strict=True)==path and not any(p.is_symlink() or p.is_junction() for p in (path,*path.parents))
            and path.stat().st_size==expected_bytes,"wrong original path or length: "+path.name)
    with path.open("rb") as source:
        raw=source.read(expected_bytes+1)
    require(len(raw)==expected_bytes,"original case grew or truncated: "+path.name)
    require(hashlib.sha256(raw).hexdigest()==sha,"changed original "+path.name)
    return json.loads(raw),{"path":str(path),"bytes":len(raw),"sha256":sha}

def original(campaign):
    root=campaign/"compact-development-TRAIN_Q543-focus1024-run-r1"
    cap,cp=pinned(root/"capture.private.json",CAPTURE,1624)
    r,rp=pinned(root/"fixture/context-cal-admissions/record-0001.json",RECORD,204733)
    require(cap["case_id"]=="TRAIN_Q543" and cap["judged"] is False and r["Context_NN_dispatch"] is False
            and r["ordinal_in_private_run"]==1 and r["request"]["remaining_input_tokens"]==4096 and
            r["remaining_ns_at_observation"]==100253589300,"original TRAIN development or allowance differs")
    rows=[v for v in r["compiler_census"] if v["profile"]==PROFILE and v["model"]=="arbiter-context-qwen3-4b"]
    require(len(rows)==1 and rows[0]["NN_invocations"]==0 and rows[0]["input_tokens"]==1588 and rows[0]["fits_remaining_allowance"],
            "original Large compiler entry missing")
    x=rows[0]["compiled"]
    require(x["input_digest"]==INPUT and len(x["input_ids"])==1588 and x["attention_mask"]==[1]*1588 and
            x["position_ids"]==list(range(1588)) and x["decision_position"]==1587 and
            x["candidate_positions"]==list(range(32,39)) and x["option_mask"]==[True]*7 and len(r["request"]["roster"]["cards"])==7,
            "wrong compiled native Large row")
    return {"name":"TRAIN_Q543","input":x,"roster":r["request"]["roster"],"remaining_ns":r["remaining_ns_at_observation"],
            "remaining_input_tokens":r["request"]["remaining_input_tokens"],"pins":[cp,rp]}

def score_publisher(torch, model, row):
    i64=torch.int64
    args=[torch.tensor([row[key]],dtype=i64) for key in ("input_ids","attention_mask","position_ids")]
    args.extend([torch.tensor([row["decision_position"]],dtype=i64),torch.tensor([row["candidate_positions"]],dtype=i64),
                 torch.tensor([row["option_mask"]],dtype=torch.bool)])
    with torch.inference_mode():
        scores,hidden=model(*args)
    require(hidden.device.type=="cpu" and tuple(hidden.shape)==(1,len(row["input_ids"]),2560),
            "official Large publisher hidden output differs")
    require(scores.device.type=="cpu" and scores.dtype==torch.float32 and tuple(scores.shape)==(1,len(row["option_mask"])),
            "official Large publisher sparse output differs")
    values=scores[0].tolist()
    require(all(math.isfinite(v) for v in values),"nonfinite publisher output")
    return values

def publisher(torch,prep,meta,case,check):
    from safetensors import safe_open
    src=ROOT/prep["OWNER"]
    reader=HERE/"large_publisher.py"
    require(hashlib.sha256(src.read_bytes()).hexdigest()==PUBLISHER and hashlib.sha256(reader.read_bytes()).hexdigest()==STREAM,
            "official Large publisher or original BF16 reader changed")
    owner=runpy.run_path(str(src))
    owner["freeze_math"]()
    folder,config,index=owner["authenticate"](prep["PUBLISHER"],"Qwen/Qwen3-4B")
    require(len(index)==398 and len(set(index.values()))==3 and config["tie_word_embeddings"],"Large publisher identity differs")
    def phase(name,detail=None):
        check()
        print("AIRK_LARGE_REFERENCE_PHASE "+json.dumps({"stage":name,"detail":detail}),flush=True)
    loader=runpy.run_path(str(reader))["PublisherState"](folder,index,torch,safe_open,phase)
    state=dict(loader.items())
    require(len(state)==398 and all(t.dtype==torch.float32 and t.device.type=="cpu" for t in state.values()),"missing logical FP32 source parameters")
    official=owner["make_wrapper"](config,state)
    del state
    check()
    control,=[r for r in meta["rows"] if len(r["input"]["input_ids"])==32]
    base=score_publisher(torch,official,control["input"])
    error=max(abs(a-b) for a,b in zip(base,control["reference_original_fp32"],strict=True))
    require(error<=0.001,"official Large T32 positive control differs")
    values=score_publisher(torch,official,case["input"])
    check()
    return {"original_fp32":values,"t32_control":base,"t32_max_abs":error,"official_forwards":2,
            "official_source_sha256":PUBLISHER,"stream_source_sha256":STREAM,"input_digest":INPUT,
            "model":"Qwen/Qwen3-4B","revision":owner["REVISIONS"]["Qwen/Qwen3-4B"]}
