# SPDX-License-Identifier: Apache-2.0
"""Replay five frozen selection policies through the same provenance-bearing finalist Gather envelope."""
import argparse
from dataclasses import dataclass
import json
from pathlib import Path
from time import perf_counter
from types import SimpleNamespace

import bundled_range_extension as extension
import common
import finalist_gather_contract as contract
import finalist_spare_context as spare
import prompt_need_audit as audit
import qwen_range_choice_trial as range_trial
import range_choice
from agent_cost_batch import working_set


FREEZE = "3ff6bc5c5380cf9a35953dd7681cd32be5753913ae84536686f8f40ac2305161"


@dataclass(frozen=True)
class Candidate:
    at: str
    text: str
    supports: list


def archived_identity(campaign, repository):
    p = campaign/f"agent-cost/E2E-r1/runs/{repository}-E2E-N2.native/session.json"
    proof = common.read_json(p)["research_frozen_index"]
    if not proof["valid"]:
        raise ValueError("Unverified archived proof")
    before = proof["before"]
    if before["index"]!=proof["after"]["index"] or before["publication"]!=proof["after"]["publication"]:
        raise ValueError("Archived publication moved")
    return before["index"], {k:before[k] for k in ("publication","scope_fingerprint")}, audit.digest(p.read_bytes())


def frame_args(args, cap):
    if cap=="default":
        if "budget" in args or contract.reply_cap(args)!=131072:
            raise ValueError("Expected actual no-override request and the frozen default frame ceiling")
        return args
    native = contract.native_limits()["default_budget"]
    original = args.get("budget",native)
    return {**args,"budget":{**original,"max_bytes":min(original["max_bytes"],cap)}}


def repository(campaign, item, inventory):
    repo,pool = range_trial.case_inputs(campaign,item,inventory)
    repo.digests = {r["file"]:bytes.fromhex(r["source_sha256"]) for r in pool}
    return repo


def range_candidates(frozen, repo, args, cap):
    scores, nominations = [], []
    for row in frozen["scores"]:
        path,a,b = audit.parse_location(row["returned_at"])
        if path!=row["file"] or repo.digests[path].hex()!=row["source_sha256"].lower():
            raise ValueError("Scored passage source differs")
        lines = tuple(repo.texts[path][a-1:b])
        if len(lines)!=b-a+1:
            raise ValueError("Scored range exceeds source")
        src=SimpleNamespace(at=row["returned_at"],path=path,lines=lines)
        nominations.append(SimpleNamespace(need=row["need"],source=src,scored_at=row["scored_at"]))
        scores.append(row["score"])
    result=range_choice.assemble(args["needs"],nominations,scores,byte_limit=cap)
    candidates=[]
    for p in result["passages"]:
        support=[{"need":s["need"],"scored_at":s["scored_at"]} for s in result["scored_support"] if s["returned_at"]==p["at"]]
        if [s["need"] for s in support]!=p["needs"]:
            raise ValueError("Missing true need link")
        candidates.append(Candidate(p["at"],p["text"],support))
    if cap==14000 and (result["scored_support"]!=frozen["scored_support"] or
            [{"at":p["at"],"needs":p["needs"],"sha256":audit.digest(p["text"].encode())}
             for p in result["passages"]]!=frozen["passages"]):
        raise ValueError("Original range selection differs")
    return candidates,len({x.source.at for x in nominations})


def analyze(body, blob, original_blob, source, notes, links, limit, elapsed):
    passages = [{"at":p["at"],"needs":p["needs"],"sha256":p["text_sha256"]} for p in body["passages"]]
    supports = [{"need":s["need"],"scored_at":s["scored_at"],"returned_at":p["at"]}
                for p in body["passages"] for s in p["support"]]
    size = sum(len(p["text"].encode()) for p in body["passages"])
    score = audit.score_variant({"passages":passages,"scored_support":supports,"passage_bytes":size,"budget":limit},
                               source,notes,links,expected_budget=limit)
    return {"passages":passages,"scored_support":supports,"wire_bytes":len(blob),"wire_sha256":audit.digest(blob),
            "original_wire_sha256":audit.digest(original_blob),"passage_bytes":size,"totals":audit.totals(score),
            "whole_needs":[n["index"] for n in score["needs"] if n["whole_refs"]],
            "attributed_whole_needs":[n["index"] for n in score["needs"] if n["attributed_whole_refs"]],
            "whole_notes":sorted(k for k,v in score["notes"].items() if v=="whole"),
            "whole_essential":sorted(k for k in score["essential"] if score["notes"][k]=="whole"),"seconds":elapsed}


def run(campaign, directory):
    freeze = directory/"freeze.json"
    identity=audit.digest(freeze.read_bytes())
    if identity not in (FREEZE,"5342af801a03a6089bf4303b2fe450e5b7b59f6770cf1bc4554bc027a18a100d"):
        raise ValueError("Wrong preregistration")
    rules = common.read_json(freeze)
    private=campaign/"replay/qwen-paired-cards-r1"
    files = {"bundled":private/"paired-complete.jsonl","range":private/"range-choice-v1.jsonl",
             "mixed":private/"range-source-window-v1.jsonl","references":campaign/"replay/qwen-need-reference-r2.json"}
    if any(audit.digest(p.read_bytes())!=rules[k] for k,p in files.items()):
        raise ValueError("Frozen score or independent reference identity differs")
    annotations=common.read_json(files["references"])["tasks"]
    inventory={r["id"]:r for r in working_set(campaign)}
    datasets={}
    for group in ("bundled","mixed","range"):
        head,*rows=common.read_jsonl(files[group])
        if len(rows)!=20 or sorted(x["task"] for x in rows)!=sorted(inventory):
            raise ValueError("Incomplete score panel")
        if group!="bundled" and head["paired_sha256"].lower()!=rules["bundled"]:
            raise ValueError("Unmatched scored input")
        datasets[group]={r["task"]:r for r in rows}
    reports={}
    for key,meta in inventory.items():
        item=datasets["bundled"][key]
        src=Path(item["source"]).resolve(strict=True)
        repo=repository(campaign,item,inventory)
        args=item["arguments"]
        index,metrics,proof=archived_identity(campaign,meta["repository"])
        source_revision=common.read_json(meta["snapshot"].with_suffix(".json"))["commit"]
        truth=common.read_json(meta["task"])
        if (truth.get("source_commit",truth.get("commit"))!=source_revision or audit.digest(meta["task"].read_bytes())!=
                annotations[key]["scenario_sha256"] or annotations[key]["scenario_sha256"]!=item["task_sha256"]):
            raise ValueError("Source or grading identity differs")
        notes={n["id"]:n for n in truth["notes"]}
        links=annotations[key]["need_notes"]
        stream=extension.bundled(item,src)
        outcomes={}
        for width in rules["passage_caps"]:
            for policy in rules["treatments"]:
                mode="need_ranges"
                if policy=="bundled":
                    selected=extension.select(stream,width)
                    candidates=[Candidate(p.at,p.text,[]) for p in selected]
                    if width==14000 and [p.at for p in candidates]!=[p["at"] for p in item["variants"]["control"]["passages"]]:
                        raise ValueError("Bundled selection drift")
                    count=len(stream)
                    mode="bundled_file"
                else:
                    family=policy.rstrip("048")
                    variant="range"+policy[len(family):]
                    frozen=datasets[family][key]["variants"][variant]
                    candidates,count=range_candidates(frozen,repo,args,width)
                for cap in rules["caps"]:
                    request=frame_args(args,cap)
                    start=perf_counter()
                    blob,kept=contract.render(request,repo,index,metrics,candidates,count,1,"line",attribution=mode)
                    initial_s=perf_counter()-start
                    start=perf_counter()
                    more,expanded=spare.widen(request,repo,index,metrics,count,1,"line",blob,kept,
                                              passage_limit=width,attribution=mode)
                    extra_s=perf_counter()-start
                    if len(kept)!=len(expanded):
                        raise ValueError("Original response lost slots")
                    orig_body=json.loads(json.loads(blob)["result"]["content"][0]["text"])
                    new_body=json.loads(json.loads(more)["result"]["content"][0]["text"])
                    for p,q in zip(orig_body["passages"],new_body["passages"],strict=True):
                        path,a,b=audit.parse_location(p["at"])
                        path2,x,y=audit.parse_location(q["at"])
                        if path!=path2 or not x<=a<=b<=y or p["needs"]!=q["needs"] or p["support"]!=q["support"]:
                            raise ValueError("Spare frame displaced source lines or evidence links")
                    for arm,body,wire_blob,time in (("original",orig_body,blob,initial_s),
                                                     ("spare16",new_body,more,initial_s+extra_s)):
                        label=f"{width}/{cap}/{policy}/{arm}"
                        outcomes[label]=analyze(body,wire_blob,blob,src,notes,links,width,time)
                    for field in ("whole_notes","whole_needs","whole_essential","attributed_whole_needs"):
                        base=outcomes[f"{width}/{cap}/{policy}/original"][field]
                        if not set(base)<=set(outcomes[label][field]):
                            raise ValueError("Scored evidence regressed")
        reports[key]={"archived_scope_proof_sha256":proof,"set":meta["set"],"outcomes":outcomes}
    return {"freeze_sha256":identity,"runner_sha256":audit.digest(Path(__file__).read_bytes()),
            "contract_sha256":audit.digest(Path(contract.__file__).read_bytes()),
            "spare_sha256":audit.digest(Path(spare.__file__).read_bytes()),"tasks":reports}


if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("campaign",type=Path)
    ap.add_argument("directory",type=Path)
    ap.add_argument("output",type=Path)
    args=ap.parse_args()
    result=run(args.campaign,args.directory)
    with args.output.open("x",encoding="utf-8") as fp:
        json.dump(result,fp,separators=(",",":"))
        fp.write("\n")
