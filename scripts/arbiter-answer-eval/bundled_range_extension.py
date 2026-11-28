# SPDX-License-Identifier: Apache-2.0
"""Offline fixed-score comparison of merged padding and non-displacing source-context expansion."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
from time import perf_counter

from adaptive_passage_budget import Passage, allocation
from agent_cost_batch import working_set
import fixture
import passage_reply_budget as wire
import prompt_need_audit as audit

FROZEN = "635448a36191871b366822bd8625fe2c46f86754c1e3fb519c401b95bf6c12d4"


def make_passage(path, first, last, lines):
    if not 1 <= first <= last <= len(lines):
        raise ValueError("Range outside the source")
    return Passage(f"{path}:{first}-{last}", path, first, last, "\n".join(lines[first-1:last])+"\n", None, None)


def bundled(item, source):
    pool = {r["file"]: r for r in item["pool"]}
    scores = item["variants"]["control"]["scores"]
    if [r["file"] for r in scores] != list(pool) or len(set(pool)) != 40:
        raise ValueError("Frozen control file order differs")
    order = [r["file"] for r in sorted(scores, key=lambda r: -r["score"])]
    first, remaining, files = [], [], set()
    for path in order[:16]:
        actual = (source / path).resolve(strict=True)
        if not actual.is_relative_to(source) or audit.digest(actual.read_bytes()) != pool[path]["source_sha256"].lower():
            raise ValueError("Source path or file digest differs")
        lines = audit.source_lines(source, path)
        for at in pool[path]["locations"]:
            here, a, b = audit.parse_location(at)
            if path != here:
                raise ValueError("Range outside nominated file")
            row = make_passage(path, a, b, lines)
            if path not in files and len(files) < 6:
                first.append(row)
                files.add(path)
            else:
                remaining.append(row)
    return first + remaining


def padded(stream, radius, lines):
    groups = defaultdict(list)
    for n, p in enumerate(stream):
        groups[p.path].append((max(1,p.first-radius), min(len(lines(p.path)),p.last+radius), n))
    output = []
    for path, ranges in groups.items():
        merged = []
        for a,b,n in sorted(ranges):
            if merged and a <= merged[-1][1]:
                prior = merged[-1]
                merged[-1] = (prior[0], max(prior[1],b), min(prior[2],n))
            else:
                merged.append((a,b,n))
        output.extend((n,make_passage(path,a,b,lines(path))) for a,b,n in merged)
    return [p for n,p in sorted(output,key=lambda item:item[0])]


def add_spare(original, cap, lines, body=None, hard=None):
    """The pre-existing ranges are invariant subsets of the same-position outputs."""
    chosen = list(original)
    spent = sum(p.size for p in chosen)
    covered = {(p.path,n) for p in chosen for n in range(p.first,p.last+1)}
    for _ in range(16):
        updated = False
        for i,p in enumerate(chosen):
            src = lines(p.path)
            for left in (True,False):
                n = p.first-1 if left else p.last+1
                if not 1 <= n <= len(src) or (p.path,n) in covered:
                    continue
                size = len(src[n-1].encode())+1
                if spent+size > cap:
                    continue
                next_p = make_passage(p.path,min(n,p.first),max(n,p.last),src)
                if body is not None:
                    old = body["passages"][i]
                    body["passages"][i] = {"at":next_p.at,"needs":[],"text":next_p.text}
                    if len(wire.encode(body,1,"line")) > hard:
                        body["passages"][i] = old
                        continue
                covered.add((p.path,n))
                spent += size
                chosen[i] = p = next_p
                updated = True
        if not updated:
            break
    if any(p.path!=q.path or p.first>q.first or p.last<q.last for p,q in zip(chosen,original,strict=True)):
        raise ValueError("Spare expansion displaced original evidence")
    return chosen


def select(stream, cap):
    result = allocation(stream,cap)
    by_at = {p.at:p for p in stream}
    return [by_at[r["at"]] for r in result["passages"]]


def metrics(rows, cap, source, notes, links, blob=None):
    data = {"passages":[{"at":p.at,"needs":[],"sha256":audit.digest(p.text.encode())} for p in rows],
            "budget":cap,"passage_bytes":sum(p.size for p in rows)}
    if blob is not None:
        body = json.loads(json.loads(blob)["result"]["content"][0]["text"])
        if body["passages"] != [{"at":p.at,"needs":[],"text":p.text} for p in rows]:
            raise ValueError("Evidence differs from bytes on wire")
    score = audit.score_variant(data,source,notes,links,expected_budget=cap)
    return {**data, "wire_bytes":len(blob) if blob else None, "wire_sha256":audit.digest(blob) if blob else None,
            "whole_notes": sorted(k for k,state in score["notes"].items() if state=="whole"),
            "whole_needs": [n["index"] for n in score["needs"] if n["whole_refs"]],
            "whole_essential": sorted(k for k in score["essential"] if score["notes"][k]=="whole"),
            "all_mapped": [n["index"] for n in score["needs"] if n["reference_ids"] and n["whole_refs"]==n["reference_ids"]]}


def replay(campaign, input_path, refs, freeze):
    if audit.digest(freeze.read_bytes()) != FROZEN:
        raise ValueError("Wrong preregistration")
    rules = audit.read_json(freeze)
    if audit.digest(input_path.read_bytes()) != rules["input"] or audit.digest(refs.read_bytes()) != rules["references"]:
        raise ValueError("Incorrect scored input or reference mapping")
    meta = {r["id"]:r for r in working_set(campaign)}
    mapping = audit.read_json(refs)["tasks"]
    header,*items = map(json.loads,input_path.read_bytes().splitlines())
    if sorted(r["task"] for r in items) != sorted(meta) or len(meta)!=20:
        raise ValueError("Incomplete panel")
    results = {}
    for item in items:
        key = item["task"]
        task = meta[key]
        source = (task["snapshot"] if task["repository"]!="bookbuilder" else campaign/"agent-cost/E2E-r1/snapshots/bookbuilder").resolve(strict=True)
        if Path(item["source"]).resolve()!=source or any((source/path).exists() for _,path in fixture.EXCLUSIONS[task["repository"]]):
            raise ValueError("Untrusted source root")
        stream = bundled(item,source)
        control = allocation(stream,14000)
        if any(control[k]!=item["variants"]["control"][k] for k in ("passages","budget","passage_bytes")):
            raise ValueError(f"Historical bundled response differs for {key}")
        reference = audit.read_json(task["task"])
        if audit.digest(task["task"].read_bytes()) != mapping[key]["scenario_sha256"] or item["task_sha256"]!=mapping[key]["scenario_sha256"]:
            raise ValueError("Reference has changed")
        notes = {r["id"]:r for r in reference["notes"]}
        links = mapping[key]["need_notes"]
        args = item["arguments"]
        revision = audit.read_json(task["snapshot"].with_suffix(".json"))["commit"]
        if revision != reference.get("source_commit",reference.get("commit")):
            raise ValueError("Source revision differs")
        def lines(path):
            return audit.source_lines(source,path)
        def framed(rows,count,hard):
            return wire.render(args["task"],args["needs"],revision,rows,count,hard)
        report = {}
        for cap in rules["caps"]:
            for arm in rules["arms"]:
                start = perf_counter()
                if arm in ("original","spare16"):
                    candidates = stream
                else:
                    candidates = padded(stream,{"merge-only":0,"pad8":8,"pad16":16}[arm],lines)
                original = select(candidates,cap)
                chosen = add_spare(original,cap,lines) if arm=="spare16" else original
                configure_seconds = perf_counter()-start
                for wire_cap in [None,*rules["wire"]]:
                    start = perf_counter()
                    final,blob = chosen,None
                    if wire_cap:
                        hard = wire.effective_wire_limit(args,wire_cap)
                        blob, final = framed(original,len(candidates),hard)
                        if arm=="spare16":
                            body = json.loads(json.loads(blob)["result"]["content"][0]["text"])
                            final = add_spare(final,cap,lines,body,hard)
                            blob,checked = framed(final,len(candidates),hard)
                            if checked!=final:
                                raise ValueError("Final wire dropped expanded baseline evidence")
                        if len(blob)>hard:
                            raise ValueError("Reply exceeded effective wire budget")
                    label = f"{cap}/{wire_cap or 'passage'}/{arm}"
                    seconds = perf_counter()-start
                    result = metrics(final,cap,source,notes,links,blob)
                    result["selector_seconds"] = configure_seconds + seconds
                    result["candidate_ranges"] = len(candidates)
                    result["effective_wire_cap"] = hard if wire_cap else None
                    report[label] = result
                    if arm=="spare16":
                        base = report[f"{cap}/{wire_cap or 'passage'}/original"]
                        for k in ("whole_notes","whole_needs","whole_essential","all_mapped"):
                            if not set(base[k])<=set(result[k]):
                                raise ValueError(f"Lost original evidence: {key} {label} {k}")
        results[key] = {"set":task["set"],"outcomes":report}
    return {"freeze_sha256":FROZEN,"scored_receipt_sha256":rules["input"],"reference_sha256":rules["references"],
            "program_sha256":audit.digest(Path(__file__).read_bytes()),"tasks":results}


if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    for name in ("campaign","receipt","refs","freeze","output"):
        ap.add_argument(name,type=Path)
    args=ap.parse_args()
    result=replay(args.campaign,args.receipt,args.refs,args.freeze)
    with args.output.open("x",encoding="utf-8") as fp:
        json.dump(result,fp,separators=(",",":"))
        fp.write("\n")
