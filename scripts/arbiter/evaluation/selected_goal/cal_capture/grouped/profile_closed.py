# SPDX-License-Identifier: Apache-2.0
"""Read only closed R4 CAL lifetimes and report aggregate timing components in seconds."""
import json
from pathlib import Path
import sys

PARENT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PARENT))
from controller import CAMPAIGN, require

RUN = CAMPAIGN/"cal100-focus1024-capture-r4"


def document(path):
    return json.loads(path.read_bytes())


def lines(path):
    with path.open("rb") as stream:
        for line in stream:
            yield json.loads(line)


def seconds(path):
    return path.stat().st_mtime_ns/1e9


def measure(folder):
    close = document(folder/"lifetime-closure.json")
    phase = close["phase"]
    worker = document(folder/f"phase-{phase:02}.owned.json")
    transcript = folder/f"phase-{phase:02}.jsonl"
    questions = [r["evidence"] for r in lines(transcript) if r["kind"] == "cal_question"]
    work = folder/"fixture"
    start = close["start_ms"]/1000
    warm = max(r["observed_unix_ms"] for r in lines(work/"warm-completions.jsonl"))/1000
    worker_end = seconds(transcript)
    worker_start = worker_end-worker["elapsed_seconds"]
    cmds = {p.stem.split(".")[0]:document(p)["elapsed_ns"]/1e9 for p in (folder/"commands").glob("*.command.json")}
    admitted = {}
    for r in lines(work/"composed-observer.jsonl"):
        if r["kind"] == "dispatch_admitted":
            admitted[r["dispatch_identity"]] = [r["unix_ms"],r["original_deadline_unix_ms"]]
        elif r["kind"] == "score_returned":
            require(r["outcome"] == "success","aggregate timing requires observed finite returns")
            admitted[r["dispatch_identity"]].append(r["unix_ms"])
    entries = list(admitted.values())
    cursor = 0
    counts = dict(questions=0,probes=0,search_probes=0,initial_reads=0)
    total = dict(answer_forward=0.0,probe_forward=0.0,source_to_first_scored_probe=0.0,
                 first_probe_to_following_status=0.0,probe_prep=0.0,reported_compile=0.0)
    for i,q in enumerate(questions):
        status = q["scope"]["observed_unix_ms"]/1000
        next_status = questions[i+1]["scope"]["observed_unix_ms"]/1000 if i+1 < len(questions) else worker_end
        counts["questions"] += 1
        aw = q["answer"]["body"]["work"]
        counts["initial_reads"] += aw["calls"]
        probes = q["probes"]
        counts["probes"] += len(probes)
        actions = [("answer",aw)]
        for probe in probes:
            if probe["probe"]["tool"] == "search":
                counts["search_probes"] += 1
                focus = probe["result"].get("body",{}).get("focus",{})
                actions.append(("probe",focus.get("receipt",{}).get("work",{})))
        first_scored = None
        for kind, native_work in actions:
            tokens = native_work.get("input_tokens",0)
            require(tokens % 1024 == 0,"timing cannot associate fractional static pairs")
            count = tokens//1024
            rows = entries[cursor:cursor+count]
            cursor += count
            require(len(rows) == count and all(status-0.1 <= begin/1000 <= end/1000 <= next_status+0.1
                                               for begin,_,end in rows),"timing rows cross original question")
            total[kind+"_forward"] += sum(end-begin for begin,_,end in rows)/1000
            total["reported_compile"] += native_work.get("compile_us",0)/1e6
            if kind == "probe" and rows:
                begin,deadline,_ = rows[0]
                original = deadline/1000-120
                require(status-0.1 <= original <= begin/1000 <= next_status+0.1, "probe original clock differs")
                total["probe_prep"] += begin/1000-original
                if first_scored is None:
                    first_scored = original
        if first_scored is not None:
            total["source_to_first_scored_probe"] += first_scored-status
            total["first_probe_to_following_status"] += next_status-first_scored
    require(cursor == len(entries),"native work denominator differs")
    first_status = questions[0]["scope"]["observed_unix_ms"]/1000
    end = close["end_ms"]/1000
    return {"phase":phase, **counts, "admissions":cursor,"lifetime":end-start,"worker":worker["elapsed_seconds"],
            "birth_to_warm":warm-start,"warm_to_worker":worker_start-warm,"worker_to_first_status":first_status-worker_start,
            "worker_end_to_closure":end-worker_end,"native_binding":cmds["bind-original-scope"],
            "stage_status_shutdown":sum(cmds[k] for k in ("stage-before","stage-after","shutdown")),**total}


if __name__ == "__main__":
    rows = [measure(path.parent) for path in sorted(RUN.glob("lifetime-*/lifetime-closure.json"))]
    print(json.dumps(rows))
