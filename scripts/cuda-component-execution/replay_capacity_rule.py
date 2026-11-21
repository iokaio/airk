# SPDX-License-Identifier: Apache-2.0
"""Reclassify the retained pressure-fit receipts under the baseline-relative capacity rule.

Offline only. It rereads retained WDDM samples and launches nothing. Its rows are not live trials,
grant no admission and cannot serve as the prior fit of a pressure-fixed run.
"""
import argparse
import json
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from capacity_ballast import CAMPAIGN, ROOT, pin
from capacity_ballast_worker import GIB, require
from capacity_sweep import receipt
from protected_capacity_sweep import CAPACITY_RULE, CONTROL_SOURCE, MARGIN, RESERVE, judge
from verify_protected_pressure_matrix import RUNS

SCHEMA = "airk.private.cuda-capacity-rule-replay/1"
# The five Compact pairs belong to the matrix reader. The Large pair is the one retained Large fit.
RETAINED = [("compact", tier, f"compact-capacity-protected-{tier}-gib-pressure-fit-r{attempt}", outer, native)
            for tier, attempt, outer, native in RUNS] + [
    ("large", 24, "large-capacity-direct-pressure-24-gib-fit-r1",
     "35d07a14f48961adb3eccdb58969e8402357cf4e5297badbf25c419b6f7e39fd",
     "b99509409a067f1fec52f71ed7d142defd472a7896668008325188bf54c6f3b1")]


def classify(outer, native, model, tier):
    """Judge one authenticated outer and native receipt pair. Pure; no device or file access."""
    trial, owned = outer["trial"], native["owned"]
    host_limit = owned["suspended_assignment"]["observed_limits"]["process_memory_limit_bytes"]
    require(outer["tier_gib"] == tier and trial["mode"] == native["mode"] == "pressure-fit"
            and trial.get("model", "compact") == model
            and trial["candidate_pid"] == owned["owned_leader_pid"] == outer["admission"]["candidate_pid"]
            and trial["image"]["sha256"] == native["image"]["sha256"]
            and host_limit in (12*GIB, 16*GIB, 22*GIB) and trial.get("host_limit_bytes", host_limit) == host_limit,
            "retained outer and native receipts differ")
    memory, rule = judge(native, outer["admission"], tier, host_limit)
    native_complete = native["complete"] is True and owned["exit_code"] == 0
    pressure_complete = outer["complete"] is True
    if not native_complete:
        outcome = "native_run_incomplete"
    elif not rule["satisfied"]:
        outcome = rule["refusal"]
    elif not pressure_complete:
        outcome = "pressure_run_incomplete"
    else:
        outcome = "admitted"
    return {"model": model, "tier_gib": tier, "historical_outcome": trial["observed_outcome"],
            "historical_capacity_admitted": trial["capacity_admitted"],
            "probe_source_sha256": trial["probe_source_sha256"], "image_sha256": trial["image"]["sha256"],
            "host_limit_bytes": host_limit, "native_complete": native_complete, "native_failure": native.get("error"),
            "pressure_run_complete": pressure_complete,
            "pressure_run_failure": outer.get("sample_error") or outer.get("failure"),
            "candidate_wddm": memory, "capacity_rule": rule,
            "reclassified_outcome": outcome, "reclassified_admitted": outcome == "admitted"}


def reclassify(model, tier, folder, outer_sha, native_sha):
    outer = receipt(CAMPAIGN/folder/"execution.json", outer_sha)
    native = receipt(CAMPAIGN/(folder+"-native")/"execution.json", native_sha)
    require(outer["trial"]["native_execution"]["sha256"] == native_sha, "outer receipt names another native receipt")
    return {"directory": folder, "outer_sha256": outer_sha, "native_sha256": native_sha,
            **classify(outer, native, model, tier)}


def replay():
    return {"schema": SCHEMA, "evidence": "offline_reclassification_of_retained_samples", "live_trial": False,
            "device_work": False, "grants_admission": False, "usable_as_prior_fit": False,
            "rule": {"name": CAPACITY_RULE, "control": CONTROL_SOURCE, "margin_bytes": MARGIN,
                     "reserve_bytes": RESERVE},
            "sources": [pin(path) for path in (Path(__file__).resolve(), HERE/"protected_capacity_sweep.py",
                                               HERE/"capacity_sweep.py", HERE/"verify_protected_pressure_matrix.py")],
            "rows": [reclassify(*row) for row in RETAINED]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="fresh capacity-rule-* campaign child; omit to print only")
    args = parser.parse_args()
    result = replay()
    if args.output is None:
        print(json.dumps(result, indent=2))
        return
    out = args.output
    require(out.is_absolute() and out.parent == CAMPAIGN and not out.exists()
            and out.name.startswith("capacity-rule-"), "fresh private campaign child required")
    component = runpy.run_path(str(ROOT/"scripts/cuda-component-execution.py"))
    base = component["base"]()
    guard = {"checked_guard": base.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),
             "powershell": base.pin(Path("C:/Program Files/PowerShell/7/pwsh.exe"))}
    component["mkdir"](guard, out)
    component["write_new"](guard, out/"execution.json", result)
    print(json.dumps({"receipt": pin(out/"execution.json"), "evidence": result["evidence"],
                      "rows": [{key: row[key] for key in ("model", "tier_gib", "reclassified_outcome")}
                               for row in result["rows"]]}, indent=2))


if __name__ == "__main__":
    main()
