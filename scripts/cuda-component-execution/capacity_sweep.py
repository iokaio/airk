# SPDX-License-Identifier: Apache-2.0
"""Run one reviewed capacity tier and one original Compact native selector."""

import argparse
import json
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from capacity_ballast import CAMPAIGN, ROOT, SAMPLE_NS, SMALL, TIERS, natural, pin, run
from capacity_ballast_worker import GIB, require, verify

MASTER = CAMPAIGN / "compact-fp16-qkv-resource20-r2/execution.json"
MASTER_SHA = "b43d40d0b1edce0cf747a935dc4e28f7f6bcc7cd0b92fc1f981640a68f98c79a"
PROBE = ROOT / "scripts/arbiter-fp16-execution/probe_compact.py"


def receipt(path, expected):
    require(path.is_absolute() and path.parent.parent == CAMPAIGN and path.name == "execution.json"
            and path.is_file() and not path.is_symlink() and not path.parent.is_junction()
            and path.stat().st_nlink == 1 and path.stat().st_size <= 64 << 20, "foreign private execution receipt")
    raw = path.read_bytes()
    import hashlib
    require(hashlib.sha256(raw).hexdigest() == expected, "execution receipt changed")
    return json.loads(raw)


def require_proof(result, small, tier=None):
    require(result["complete"] is True and result["small_model_free"] is small and natural(result["job"])
            and result["all_workers_joined"] is True and result["sample_error"] is None and result["samples"],
            "incomplete original pressure evidence")
    if small:
        require(result["held"]["touched"] == SMALL and result["maximum_bytes"] == SMALL,
                "64 MiB checked model-free proof required")
    else:
        require(result["tier_gib"] == tier and result["trial"]["mode"] == "qkv-fit-probe" and
                result["trial"]["capacity_admitted"] is True and result["trial"]["candidate_pid"] ==
                result["admission"]["candidate_pid"], "complete T8192 fit required at the exact floor")


def candidate_samples(proof, admission, *, limit_field="job_limits", host_limit=32*GIB, require_shared_clock=False,
                      shared_ceiling=None):
    """Summarize the owned candidate's WDDM process samples.

    By default any shared byte or unreadable shared sample leaves residency unverified. A caller that judges
    shared use against a same-run model-free control passes that control plus its margin as shared_ceiling.
    """
    require(shared_ceiling is None or (type(shared_ceiling) is int and shared_ceiling >= 0), "invalid shared ceiling")
    owned = proof["owned"]
    limits = owned["suspended_assignment"][limit_field]
    require(limits["process_memory_limit_bytes"] == limits["job_memory_limit_bytes"] == host_limit,
            "candidate original Job caps differ")
    observation = owned["before_GO"]["wddm"]
    require(observation["pid"] == admission["candidate_pid"] and
            observation["adapter_luid"] == admission["snapshot"]["dxgi"]["luid"], "candidate GPU/PID differs")
    rows = observation["samples"]
    positions = [i for i,row in enumerate(rows) if type(row["process"]["Dedicated Usage"]) is int
                 and row["process"]["Dedicated Usage"] > 0]
    # PDH_NO_INSTANCE specifically identifies an absent process counter. INVALID_DATA
    # has a different meaning and cannot establish zero shared use.
    def checked_shared(row):
        shared = row["process"]["Shared Usage"]
        if type(shared) is int and shared >= 0:
            return shared
        fields = ("Dedicated Usage", "Shared Usage", "Total Committed")
        if all(row["process"].get(key) is None and
               row.get("process_counter_status", {}).get(key) == 0x800007d1 for key in fields):
            return 0
        return None
    before_resume = observation.get("before_resume")
    captured = ([before_resume] if isinstance(before_resume,dict) else []) + rows
    all_shared = [checked_shared(row) for row in captured]
    unavailable = sum(value is None for value in all_shared)
    peak_shared = max((value for value in all_shared if value is not None), default=None)
    shared_evidence = {"peak_shared_bytes":peak_shared,"unknown_shared_samples":unavailable,
                       "all_process_samples":len(captured)}
    if not positions:
        return {**shared_evidence,"verified":False, "reason":"no positive dedicated observation"}
    values = rows[positions[0]:positions[-1]+1]
    times = [row["end_monotonic_ns"] for row in values]
    fields = ("Dedicated Usage","Shared Usage") if require_shared_clock else ("Dedicated Usage",)
    series = [[row["counter_filetime_100ns"].get("process/"+field) for row in values] for field in fields]
    fresh = len(values) >= 2 and all(0 < b-a <= SAMPLE_NS for a,b in zip(times,times[1:])) and all(
        all(type(t) is int for t in counters) and all(b > a for a,b in zip(counters,counters[1:])) for counters in series)
    shared = [r["process"]["Shared Usage"] for r in values]
    local = [r["process"]["Dedicated Usage"] for r in values]
    known = all(type(v) is int for v in shared+local)
    peak = max(local) if known else None
    if shared_ceiling is None:
        within, relative = unavailable == 0 and peak_shared == 0, {}
    else:
        # Unreadable samples outside the live interval are counted above and do not refuse here.
        # Any unreadable sample inside it clears `known`, exactly as in the default reading.
        within = peak_shared is not None and peak_shared <= shared_ceiling
        relative = {"shared_ceiling_bytes":shared_ceiling,
                    "unknown_live_samples":sum(type(a) is not int or type(b) is not int for a,b in zip(shared,local))}
    return {**shared_evidence,"verified":fresh and known and within and 0 < peak <= 24*GIB,
            "peak_dedicated_bytes":peak,
            "observations":len(values),"continuous_samples":fresh,**relative}


def segment_evidence(observation):
    """Evaluate both supplemental segment groups across the full owned process lifetime."""
    rows = ([observation["before_resume"]] if observation.get("before_resume") else []) + observation["samples"]
    evidence = {"segment_observations":len(rows)}
    for field, label in (("Local Usage","local"),("Non Local Usage","nonlocal")):
        values = []
        unknown = stale = 0
        confirmed_peak = None
        previous = None
        for row in rows:
            amount = row.get("segment_bytes",{}).get(field)
            status = row.get("segment_counter_status",{}).get(field)
            if amount is None and status == 0x800007d1 and all(
                    row.get("process_counter_status",{}).get(key) == 0x800007d1
                    for key in ("Dedicated Usage","Shared Usage","Total Committed")):
                values.append(0)
                continue
            if type(amount) is not int or amount < 0:
                unknown += 1
                continue
            values.append(amount)
            ticks = row["counter_filetime_100ns"].get("segment_bytes/"+field)
            reference = row["counter_filetime_100ns"].get("process/Dedicated Usage")
            if (type(ticks) is not int or type(reference) is not int or ticks <= 0
                    or abs(ticks-reference)*100 > SAMPLE_NS or (previous is not None and ticks <= previous)):
                stale += 1
            else:
                confirmed_peak = amount if confirmed_peak is None else max(confirmed_peak,amount)
            if type(ticks) is int:
                previous = ticks
        evidence["peak_"+label+"_bytes"] = max(values,default=None)
        evidence["peak_fresh_"+label+"_bytes"] = confirmed_peak
        evidence["unknown_"+label+"_samples"] = unknown
        evidence["stale_"+label+"_samples"] = stale
    evidence["nonlocal_conflict"] = (evidence["peak_fresh_nonlocal_bytes"] or 0)>0
    evidence["dedicated_segment_evidence"] = (evidence["peak_nonlocal_bytes"] == 0
        and (evidence["peak_local_bytes"] or 0)>0
        and all(evidence[key]==0 for key in ("unknown_local_samples","unknown_nonlocal_samples",
                                             "stale_local_samples","stale_nonlocal_samples")))
    return evidence


def trial_options(args, master):
    paths = {"request": master["request"], "evidence": master["reviewed_evidence"],
             "graph_receipt": master["reviewed_graph"]["owned"], "projection_stage": master["projection_stage"],
             "projection_reference": master["projection_reference"],
             "powershell":{"path":"C:/Program Files/PowerShell/7/pwsh.exe",
                           "sha256":"e94166ed9f376559c1359bafb08bc49e463a494c330f67644ec1d9f1ffeb5ee5"},
             "image":{"path": str(args.image), "sha256":args.image_sha256},
             "build_receipt":{"path":str(args.build_receipt),"sha256":args.build_receipt_sha256}}
    opts = argparse.Namespace(mode=args.mode,output=args.output.with_name(args.output.name+"-native"),
                              source_snapshot=args.build_receipt.parent/"retained-native-sources")
    opts.retained_cargo_proof = args.retained_cargo_proof
    opts.retained_cargo_proof_sha256 = args.retained_cargo_proof_sha256
    for name, value in paths.items():
        setattr(opts,name, Path(value["path"]))
        setattr(opts,name+"_sha256", value["sha256"])
    return opts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode",choices=("qkv-fit-probe","qkv-resource20"))
    parser.add_argument("--tier",type=int,choices=TIERS,required=True)
    parser.add_argument("--maximum-gib",type=int,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--probe-source-sha256",required=True)
    for name in ("small-proof","image","build-receipt","admitted-fit"):
        parser.add_argument("--"+name,type=Path,required=name!="admitted-fit")
        parser.add_argument("--"+name+"-sha256",required=name!="admitted-fit")
    parser.add_argument("--retained-cargo-proof",type=Path)
    parser.add_argument("--retained-cargo-proof-sha256")
    args=parser.parse_args()
    require(bool(args.retained_cargo_proof) == bool(args.retained_cargo_proof_sha256),
            "retained Cargo proof requires its digest")
    require(0 < args.maximum_gib <= 48 - args.tier,"allocation exceeds reviewed tier envelope")
    verify(PROBE,args.probe_source_sha256)
    small = receipt(args.small_proof,args.small_proof_sha256)
    require_proof(small,True)
    require(small["worker"]==pin(HERE/"capacity_ballast_worker.py") and
            all(pin(Path(ref["path"])) == ref for ref in small["source_pins"]),
            "model-free proof's execution sources differ")
    master = receipt(MASTER,MASTER_SHA)
    opts = trial_options(args,master)
    original = runpy.run_path(str(PROBE))
    component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = component["base"]()
    parent, _ = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": original["exact_pin"](opts.powershell,opts.powershell_sha256,base)}
    def audit(paths, created=True):
        component["guard_many"](guard,paths,created)
    manifest_path = opts.image.with_name("airk-cuda-runtime.json")
    audit([opts.image,opts.build_receipt,manifest_path])
    image = original["exact_pin"](opts.image,opts.image_sha256,base)
    build = original["exact_pin"](opts.build_receipt,opts.build_receipt_sha256,base)
    manifest = base.pin(manifest_path)
    chain = None
    if opts.retained_cargo_proof:
        _, chain = original["retained_cargo_chain"](opts.retained_cargo_proof,
            opts.retained_cargo_proof_sha256,build,image,manifest,base,audit,parent)
    log, artifact, _ = original["linked_image"](build,image,base,audit,opts.mode,
        source_snapshot=opts.source_snapshot,cargo_chain=chain)
    original["decision_build_approvals"](log,artifact,manifest,base,audit,cargo_chain=chain)
    prior = None
    if args.mode == "qkv-resource20":
        require(args.admitted_fit and args.admitted_fit_sha256, "fixed20 requires prior fit closure")
        first = receipt(args.admitted_fit,args.admitted_fit_sha256)
        require_proof(first,False,args.tier)
        require(first["trial"]["image_sha256"]==args.image_sha256 and
                first["trial"]["build_sha256"]==args.build_receipt_sha256 and
                first["trial"]["probe_source_sha256"]==args.probe_source_sha256, "first fit image/source differs")
        prior=pin(args.admitted_fit)
    else:
        require(not args.admitted_fit and not args.admitted_fit_sha256, "unexpected prior fit")
    def candidate(authorize):
        completed = original["run"](opts,capacity_gate=authorize)
        path=opts.output/"execution.json"
        retained=pin(path)
        proof=receipt(path,retained["sha256"])
        admission=proof.get("capacity_before_resume")
        require(isinstance(admission,dict) and proof["mode"]==opts.mode and proof["wall_seconds"]==120
                and proof["complete"] is completed and proof["image"]["sha256"]==args.image_sha256
                and proof["build"]["sha256"]==args.build_receipt_sha256, "foreign candidate outcome or admission")
        owned=proof.get("owned") or {}
        require(owned.get("owned_leader_pid")==admission["candidate_pid"] and
                owned.get("owned_descendants_settled") is True and owned.get("owned_job_final_active_processes")==0
                and owned.get("owned_job_final_member_pids")==[] and owned.get("owned_job_forced_termination") is False,
                "candidate Job or source did not settle naturally")
        memory = candidate_samples(proof,admission)
        return {"mode":opts.mode,"native_complete":completed,"capacity_admitted":completed and memory["verified"],
                "candidate_wddm":memory,"candidate_pid":admission["candidate_pid"],
                "native_execution":retained,"original_small_proof":pin(args.small_proof),"admitted_fit":prior,
                "image_sha256":args.image_sha256,"build_sha256":args.build_receipt_sha256,
                "probe_source_sha256":args.probe_source_sha256,"failure":proof.get("failure")}
    run(args.output,args.tier,args.maximum_gib*GIB,trial=candidate)


if __name__=="__main__":
    main()
