# SPDX-License-Identifier: Apache-2.0
"""Join the two measured epochs by original case and actual native work."""
import json
from pathlib import Path
from affinity import require
from closed_readout import measure, compare_menus


def records(path):
    return [json.loads(line) for line in Path(path).read_bytes().splitlines()]


def paired(plan, out, passes, dispatch, binding_row):
    journal=records(out/"fixture/composed-observer.jsonl")
    returns=[r for r in journal if r["kind"]=="score_returned"]
    require(len(returns)==dispatch["focus_dispatches"]==dispatch["successful_focus_returns"] and
            dispatch["failed_focus_returns"]==dispatch["context_dispatches"]==0,"incomplete true native returns")
    report, groups, slices, orders = [],{},[],[]
    for i,entry in enumerate(passes):
        events=records(entry["transcript"]["path"])
        calls=measure(events,journal,plan["question_ids"],i,binding_row)
        stamps=[e for e in events if e["kind"]=="timing_exchange"]
        args=[e["argv"] for e in events if e["kind"]=="native_exchange"]
        orders.append(args)
        slices.append({"pass_index":i,"calls":calls,"assignments":[e for e in events if e["kind"]=="timing_assignment"]})
        for n in range(4):
            row=[x for x in calls if x["ordinal"]==n+96]
            edges=[x for x in stamps if x["ordinal"]==n+96]
            lo,hi=edges[0]["start"]["unix_ms"],edges[-1]["end"]["unix_ms"]
            native=[r for r in returns if lo<=r["unix_ms"]<=hi]
            seconds=(int(edges[-1]["end"]["monotonic_ns"])-int(edges[0]["start"]["monotonic_ns"]))/1e9
            infer=sum(k["native_inference_us"] for k in row)/1e6
            require(len(native)==sum(k["native_focus_calls"] for k in row) and native,"missing matched Focus work")
            groups[(n,row[0]["arm"])]={
                "seconds":seconds,"inference_seconds":infer,"other_seconds":seconds-infer,
                "native_work":[[r["question_utf8_sha256"],r["tokens"],r["raw_score_bits"]] for r in native],
                "dispatches":len(native),"first_unix_ms":lo,"last_unix_ms":hi,
                "last_call_start_ms":edges[-1]["start"]["unix_ms"],
                "last_call_seconds":row[-1]["cli_elapsed_us"]/1e6}
    compare_menus(slices[0]["calls"], slices[1]["calls"])
    require(len({r["window_fingerprint"] for section in slices for r in section["calls"]}) == 8, "reused window fingerprint across passes")
    require(orders[0]==orders[1],"the two original SDK action/argument sequences differ")
    for n,key in enumerate(plan["question_ids"]):
        full,local=groups[(n,"full")],groups[(n,"local4")]
        require(full["native_work"]==local["native_work"],"matched case changed native inputs or returned score bits")
        report.append({"id":key,"ordinal":96+n,"dispatches":full["dispatches"],
                       "full_seconds":full["seconds"],"local_seconds":local["seconds"],
                       "full_inference_seconds":full["inference_seconds"],"local_inference_seconds":local["inference_seconds"],
                       "full_other_seconds":full["other_seconds"],"local_other_seconds":local["other_seconds"],
                       "seconds_saved":full["seconds"]-local["seconds"],
                       "inference_seconds_saved_per_dispatch":(full["inference_seconds"]-local["inference_seconds"])/full["dispatches"]})
    labels=min(q["inference_seconds_saved_per_dispatch"] for q in report)
    # Both missing operations in the old capture are measured here on the original scheduling arm.
    final=groups[(3,"full")]
    turnover=max((groups[(3,plan["schedule"][i][3])]["first_unix_ms"]-
                  groups[(2,plan["schedule"][i][2])]["last_unix_ms"])/1000 for i in range(2))
    required=(groups[(2,"full")]["last_call_seconds"] + turnover
              +(final["last_call_start_ms"]-final["first_unix_ms"])/1000+125-107.996)
    warm_returns=[r for r in returns if r["unix_ms"]<min(g["first_unix_ms"] for g in groups.values())]
    require(len(warm_returns)>0 and len(warm_returns)+sum(g["dispatches"] for g in groups.values())==len(returns),
            "unexpected native work outside warm-up and original measured calls")
    return {"schema":"airk.private.cal-tail-affinity-readout/1","cases":report,"passes":slices,
            "warmup_native_returns":len(warm_returns),"measured_question_turnover_seconds":turnover,
            "missing_r5_work_to_final_admission_seconds":required,
            "weakest_pair_inference_saving_times_1744":labels*1744,
            "gates":{"all_four_elapsed_faster":all(r["seconds_saved"]>0 for r in report),
                     "additional_original_request_margin_seconds":labels*1744-required-125},
            "uncertainty":"Four matched cases on this host; background and cache effects require inspection.",
            "qualification":False}
