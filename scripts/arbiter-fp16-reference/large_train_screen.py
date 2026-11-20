# SPDX-License-Identifier: Apache-2.0
"""Preregister the four exposed Large TRAIN menus and interpret their original scores."""
import hashlib
import json
import math
from pathlib import Path

# Ordered genuine admissions from the existing exposed TRAIN roster, selected before scoring.
CASES = (
    (111,1193,"2b9d0f236fa317d8547ddc1ae4becb65222ec1c752b94bfafdc95c334c5520a5","faa9bc39acf0442741ad08bfe7bdc56cba629351debdbbe5db4bf46e8b5fdfbb","a740b495da0cfbbc358c6c9b0d475b97e5323e245fa8b0a1cdc3589feed28ef7"),
    (117,1230,"de5515b4676af3df7a106240b625e8d5824e778cffcdeed2458fbaaaa7fd93f5","098d697c067b12f591c22995cf8c82ff89506fce2f7444d3c1f6a1e9e3441759","8b15dff6142577db9d240a0e90503c1f2e19c01f26090425a38edd803fd30bb3"),
    (202,1158,"04f9d587a714a7ab362f062f2fe7bf17167b544600b9b7cc3b2770f69aa24e67","ca01a66c474afdd1b3d8d56aab2d4ac799fcb086a45be674833b85b38b81259d","4a397df735ec5055ff643f7ba45b6963433656536f42ab817b7b76134d60e6d3"),
    (227,1609,"3c41f7748cd1d4f0dec03083dfdd6674520c8f52f8804493237ad8828ecb8da1","3297aa67af7c8fac35e503900b60883153abfeec8ded4c1b9943456704b6ff37","deb34557aa6c70fb0e167c1f431cbf8f61dbd0208c9e93ef2e97998f19b156aa"),
)
THRESHOLD = 0.020
PROFILE = "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"

def read(path, sha):
    if path.resolve(strict=True)!=path or any(p.is_symlink() or p.is_junction() for p in (path,*path.parents)):
        raise ValueError("indirect private source")
    raw=path.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=sha:
        raise ValueError("changed original "+path.name)
    return json.loads(raw), {"path":str(path),"sha256":sha,"bytes":len(raw)}

def inputs(campaign):
    rows=[]
    for ordinal,width,cap_sha,record_sha,digest in CASES:
        case=f"TRAIN_Q{ordinal}"
        directory=campaign/f"compact-development-{case}-focus1024-run-r1"
        cap,cp=read(directory/"capture.private.json",cap_sha)
        r,rp=read(directory/"fixture/context-cal-admissions/record-0001.json",record_sha)
        large=[v for v in r["compiler_census"] if v["profile"]==PROFILE and v["model"]=="arbiter-context-qwen3-4b"]
        if not (cap["case_id"]==case and cap["judged"] is False and r["Context_NN_dispatch"] is False and
                r["ordinal_in_private_run"]==1 and len(large)==1 and large[0]["NN_invocations"]==0 and
                large[0]["fits_remaining_allowance"] and large[0]["input_tokens"]==width):
            raise ValueError("not an authentic exposed predispatch Large admission")
        x=large[0]["compiled"]
        if not (x["input_digest"]==digest and len(x["input_ids"])==width and x["position_ids"]==list(range(width)) and
                x["attention_mask"]==[1]*width and x["decision_position"]==width-1 and x["option_mask"]==[True]*7
                and x["candidate_positions"]==list(range(32,39)) and len(r["request"]["roster"]["cards"])==7
                and r["request"]["remaining_input_tokens"]>=width and 0<r["remaining_ns_at_observation"]<=120_000_000_000):
            raise ValueError("wrong original Large row or remaining allowance")
        rows.append({"case":case,"input":x,"roster":r["request"]["roster"],"remaining_tokens":r["request"]["remaining_input_tokens"],
                     "remaining_ns":r["remaining_ns_at_observation"],"pins":[cp,rp]})
    return rows

def plan(rows):
    return {"population":"all four preselected exposed original TRAIN menus","ordered_cases":[r["case"] for r in rows],
            "input_digests":[r["input"]["input_digest"] for r in rows],"original_sources":[r["pins"] for r in rows],
            "selector":"first publisher top-two raw margin at or below threshold in frozen order",
            "threshold":THRESHOLD,"limit":4,"heldout_cases":0,"candidate_forwards":0}

def ranked(rows, scores):
    if len(scores)!=len(CASES) or len(rows)!=len(CASES):
        raise ValueError("incomplete finite TRAIN screen")
    result=[]
    for row,values in zip(rows,scores,strict=True):
        if len(values)!=7 or not all(map(math.isfinite,values)):
            raise ValueError("seven finite original scores required")
        first,second=sorted(range(7), key=values.__getitem__, reverse=True)[:2]
        gap=values[first]-values[second]
        result.append({"case":row["case"],"original_fp32":values,"winner":first,"runner_up":second,"margin":gap,
                       "meets_screen":gap<=THRESHOLD})
    return result, next((r["case"] for r in result if r["meets_screen"]),None)

PLAN = "17468da0b10a2332b756cbe35afafa9932cd6d01368edefd614784c46691921a"

def population(campaign):
    rows = inputs(campaign)
    recorded,pin = read(campaign/"large-exposed-publisher-screen-r1-plan.json",PLAN)
    if recorded != plan(rows):
        raise ValueError("preregistered original screen changed")
    return {"input":rows[0]["input"], "screen":rows,
            "pins":[pin, *(p for row in rows for p in row["pins"])]}

def publisher(torch,prep,meta,pop,check,train):
    src = train["ROOT"]/prep["OWNER"]
    if hashlib.sha256(src.read_bytes()).hexdigest() != train["PUBLISHER"]:
        raise ValueError("official Large source changed")
    import runpy
    owner=runpy.run_path(str(src))
    owner["freeze_math"]()
    folder,config,index=owner["authenticate"](prep["PUBLISHER"],"Qwen/Qwen3-4B")
    if len(index)!=398 or len(set(index.values()))!=3 or not config["tie_word_embeddings"]:
        raise ValueError("original Qwen3-4B state differs")
    def phase(name,index):
        check()
        if index==35:
            print("AIRK_LARGE_SCREEN_FORWARD completed 36 decoder layers",flush=True)
    wrapper,trace=train["streamed_official_model"](torch,owner["make_wrapper"],config,folder,index,check,phase)
    control,=[r for r in meta["rows"] if len(r["input"]["input_ids"])==32]
    base=train["score_publisher"](torch,wrapper,control["input"])
    error=max(abs(x-y) for x,y in zip(base,control["reference_original_fp32"],strict=True))
    if error>0.001 or trace!=list(range(36)):
        raise ValueError("original official T32 positive control differs")
    values=[]
    for row in pop["screen"]:
        check()
        values.append(train["score_publisher"](torch,wrapper,row["input"]))
        if trace!=list(range(36))*(len(values)+1):
            raise ValueError("decoder order or actual forward count differs")
    ranks,selected=ranked(pop["screen"],values)
    return {"model":"Qwen/Qwen3-4B","revision":owner["REVISIONS"]["Qwen/Qwen3-4B"],
            "plan_sha256":PLAN,"official_forwards":5,"candidate_forwards":0,
            "t32_control":base,"t32_max_abs":error,"results":ranks,"selected":selected,
            "original_vectors":values,"decoder_trace":trace,"original_source_sha256":train["PUBLISHER"]}


if __name__=="__main__":
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign",type=Path)
    parser.add_argument("destination",type=Path)
    args=parser.parse_args()
    frozen=plan(inputs(args.campaign.resolve(strict=True)))
    with args.destination.open("x",encoding="utf8") as file:
        json.dump(frozen,file,indent=2,allow_nan=False)
        file.write("\n")
