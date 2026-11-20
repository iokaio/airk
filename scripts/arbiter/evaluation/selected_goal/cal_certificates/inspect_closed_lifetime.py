# SPDX-License-Identifier: Apache-2.0
"""Read one closed phased CAL lifetime for diagnostics. Never calculates gains or issues certificates."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from capture_closure import question_events
from phased_closure import phase_interval, installation_identity, check_warm
from reference_inputs import checked, parsed, read, reference, load_inputs, require, CAMPAIGN

def inspect(root, phase, plan_sha, lifetime_sha):
    directory = root/f"lifetime-{phase:02}"
    ref = reference(root/"lifetime-plan.json",plan_sha)
    plan = read(ref)
    lifetime = read(reference(directory/"lifetime-closure.json",lifetime_sha))
    require(lifetime["plan"]==ref and lifetime["phase"]==phase,"wrong lifetime")
    audit_sha = plan["input_audit"]["sha256"]
    anchor = plan["installation"]
    bound = read(anchor["bindings"])
    audit = read(plan["input_audit"])
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    _, ids, tasks, _, _, _, sources, index_root = load_inputs(plan["input_audit"],focus)
    prior = read(lifetime["prior"])
    require((phase==0 and lifetime["prior"]==anchor["closed"]) or
            (phase>0 and prior["phase"]==phase-1 and prior["plan"]==ref),"incorrect predecessor")
    phase_interval(lifetime["start_ms"],lifetime["end_ms"],plan["start_ms"] if phase==0 else prior["end_ms"],plan["deadline_ms"])
    live = read(lifetime["live_request"],directory/"live-request.json")
    work = directory/"fixture"
    ready = read(live["ready"],work/"ready.json")
    closed = read(lifetime["closed"],work/"closed.json")
    require(ready["health"]["instance"]==lifetime["instance"]==closed["instance"] and
            closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True,"unclosed service")
    native = read(live["bindings"],work/"enable-context.bindings.json")
    base, old_scope = installation_identity(bound)
    current, new_scope = installation_identity(native)
    require(base==current and old_scope!=new_scope, "changed installation or reused lifetime")
    binding = sources["ResidentBinding"](work/"enable-context.bindings.json",source_root=index_root)
    check_warm(lifetime["warm"],work,ready)
    census = {k:lifetime[k] for k in ("probes","pipeline_searches")}
    census.update(phase_closures=[lifetime["phase_closure"]],request_sha256=lifetime["live_request"]["sha256"])
    seen = {old_scope,new_scope}
    events = question_events(directory,lifetime,census,ids,binding,read(live["answer"]),tasks,audit["pipeline"],
                             first_phase=phase,lifetime_fingerprints=seen)
    observed = [row["id"] for row,_ in events]
    require(observed==plan["phase_ids"][phase],"incomplete original question roster")
    owner = read(lifetime["phase_closure"],directory/f"phase-{phase:02}.owned.json")
    roster, pending = [], []
    for line in checked(owner["transcript"]).splitlines():
        event = parsed(line)
        if event["kind"]=="native_observation":
            pending.append(reference(work/"context-cal-admissions"/event["name"],event["sha256"]))
        if event["kind"]=="cal_question":
            evidence = event["evidence"]
            require([read(ref) for ref in pending]==evidence["captured"],"original capture linkage differs")
            roster.append({"ordinal":phase*8+len(roster),"question_id":evidence["id"],"captures":pending})
            pending = []
    require(not pending and [r["question_id"] for r in roster]==observed,"phase ordinal linkage differs")
    return {"phase":phase,"closed_questions":len(observed),"probes":census["probes"],"rows":roster,
            "source_transcript":owner["transcript"],"phase_owner":lifetime["phase_closure"],
            "pipeline_searches":census["pipeline_searches"],"certificate_count":0,
            "lifetime_sha256":lifetime_sha,"plan_sha256":plan_sha,"input_audit_sha256":audit_sha}

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root",type=Path,required=True)
    p.add_argument("--phase",type=int,required=True)
    p.add_argument("--plan-sha256",required=True)
    p.add_argument("--lifetime-sha256",required=True)
    a=p.parse_args()
    require(a.root.parent==CAMPAIGN and 0<=a.phase<13,"foreign diagnostic input")
    print(json.dumps(inspect(a.root,a.phase,a.plan_sha256,a.lifetime_sha256),sort_keys=True))
