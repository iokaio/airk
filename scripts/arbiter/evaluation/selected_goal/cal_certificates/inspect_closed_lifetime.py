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

def inspect(root, phase, plan_sha, lifetime_sha, *, all_phases=False):
    require(root.parent == CAMPAIGN and 0 <= phase < 13, "foreign diagnostic input")
    ref = reference(root/"lifetime-plan.json",plan_sha)
    plan = read(ref)
    import grouped_closure as groups
    grouped = plan["schema"] in groups.PLAN_FOR_FIXTURE.values()
    group = next(i for i, phases in enumerate(groups.shape(plan["schema"])) if phase in phases) if grouped else phase
    directory = root/f"lifetime-{group:02}"
    lifetime = read(reference(directory/"lifetime-closure.json",lifetime_sha))
    require(lifetime["plan"] == ref, "wrong lifetime plan")
    if grouped:
        groups.check_plan(plan)
        acquired = read(plan["installation_mutex"],root/"control-mutex-acquired.json")
        require(acquired["name"] == r"Local\AIRK-20261116-Focus1024-Reuse"
                and type(acquired["owner_pid"]) is int and acquired["owner_pid"] > 0
                and acquired["at_ms"] <= plan["start_ms"], "wrong installation owner")
    else:
        require(plan["schema"] == "airk.private.cal100-lifetime-plan/1" and lifetime["phase"] == phase,
                "wrong lifetime")
    audit_sha = plan["input_audit"]["sha256"]
    anchor = plan["installation"]
    bound = read(anchor["bindings"])
    audit = read(plan["input_audit"])
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    _, ids, tasks, _, _, _, sources, index_root = load_inputs(plan["input_audit"],focus)
    prior = read(lifetime["prior"])
    prior_end = plan["start_ms"] if group == 0 else prior["end_ms"]
    if grouped:
        require((group == 0 and lifetime["prior"] == anchor["closed"]) or
                (group > 0 and prior["group"] == group-1 and prior["plan"] == ref and
                 lifetime["prior"]["path"] == str(root/f"lifetime-{group-1:02}/lifetime-closure.json")),
                "incorrect group predecessor")
        phase_refs = groups.check_group(directory,lifetime,group,plan,prior_end)
        phases = groups.shape(plan["schema"])[group]
    else:
        require((phase==0 and lifetime["prior"]==anchor["closed"]) or
                (phase>0 and prior["phase"]==phase-1 and prior["plan"]==ref),"incorrect predecessor")
        phase_interval(lifetime["start_ms"],lifetime["end_ms"],prior_end,plan["deadline_ms"])
        phase_refs = [lifetime["phase_closure"]]
        phases = (phase,)
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
    if grouped:
        import runpy
        from reference_inputs import HERE, digest
        proof = HERE.parent/"cal_capture/dispatch_proof.py"
        require(digest(proof.read_bytes()) == "75b370b0446e0be1946896cb8978e8028aed0fb939320d4c72426f40386ea027",
                "dispatch proof changed")
        require(lifetime["dispatch"] == runpy.run_path(str(proof))["read_journal"](work,focus,setup_only=False)
                and lifetime["publication"] == binding.publication, "changed group ledger or publication")
    census.update(phase_closures=phase_refs,request_sha256=lifetime["live_request"]["sha256"])
    seen = {old_scope,new_scope}
    events = question_events(directory,lifetime,census,ids,binding,read(live["answer"]),tasks,audit["pipeline"],
                             first_phase=phases[0],lifetime_fingerprints=seen)
    counts = [(row["id"],len(row["probes"]),row["pipeline_searches"]) for row,_ in events]
    require(plan["phase_ids"] == [ids[i:i+8] for i in range(0,100,8)]
            and [key for key,_,_ in counts] == [q for p in phases for q in plan["phase_ids"][p]],
            "incomplete original question roster")
    wanted = phases if all_phases else (phase,)
    reports = []
    for number in wanted:
        observed = plan["phase_ids"][number]
        selected = [c for c in counts if c[0] in observed]
        reports.append(phase_report(directory,work,number,phase_refs[phases.index(number)],observed,
                                    selected,lifetime_sha,plan_sha,audit_sha))
    return reports if all_phases else reports[0]


def phase_report(directory,work,phase,phase_ref,observed,selected,lifetime_sha,plan_sha,audit_sha):
    owner = read(phase_ref,directory/f"phase-{phase:02}.owned.json")
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
    return {"phase":phase,"closed_questions":len(observed),"probes":sum(probes for _,probes,_ in selected),"rows":roster,
            "source_transcript":owner["transcript"],"phase_owner":phase_ref,
            "pipeline_searches":sum(searches for _,_,searches in selected),"certificate_count":0,
            "lifetime_sha256":lifetime_sha,"plan_sha256":plan_sha,"input_audit_sha256":audit_sha}

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root",type=Path,required=True)
    p.add_argument("--phase",type=int,required=True)
    p.add_argument("--plan-sha256",required=True)
    p.add_argument("--lifetime-sha256",required=True)
    p.add_argument("--all-phases",action="store_true",help="emit each phase in a settled native lifetime after one full journal verification")
    a=p.parse_args()
    require(a.root.parent==CAMPAIGN and 0<=a.phase<13,"foreign diagnostic input")
    print(json.dumps(inspect(a.root,a.phase,a.plan_sha256,a.lifetime_sha256,all_phases=a.all_phases),sort_keys=True))
