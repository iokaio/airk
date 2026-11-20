# SPDX-License-Identifier: Apache-2.0
"""Authenticate the isolated timing shutdown as the latest Windows CAL owner."""

from pathlib import Path

import r7_handoff
import r8_phase_timing as replay
import r8_predecessor as predecessor
import two_lifetimes as grouped

ROOT = replay.OUTPUT
OBSERVATION = "e7bcffbb4ddd2ad27a5b7842a27feefd55c11d511a73e3a95cd678ed6e261360"
require = grouped.require


def retirement():
    historical = predecessor.observe()
    observation = grouped.pin(ROOT / "timing-observation.json", OBSERVATION)
    report = grouped.linked(observation)
    require(report["phase"] == 11 and report["ordered_questions"] == 8 and report["probes"] == 40
            and report["diagnostic_only"] is True and report["capture_complete"] is False,
            "expected the original isolated timing observation")
    plan, outer, closure, release = (grouped.linked(report[key]) for key in
                                     ("plan", "outer", "native_closure", "release"))
    require(plan["schema"] == replay.PLAN_SCHEMA and plan["predecessor"] == historical
            and plan["original_plan"] == historical["receipts"]["plan"] and replay.settled(outer)
            and closure["schema"] == replay.CLOSURE_SCHEMA and closure["plan"] == report["plan"]
            and closure["phase_closures"] == [report["diagnostic_phase"]]
            and release["outer"] == report["outer"] and release["acquired"] == plan["installation_mutex"]
            and closure["end_ms"] <= release["at_ms"] and release["name"] == grouped.MUTEX_NAME,
            "timing owner or its mutex release differs")
    acquired = grouped.linked(release["acquired"])
    require(acquired["owner_pid"] == release["owner_pid"] and acquired["name"] == grouped.MUTEX_NAME
            and acquired["at_ms"] <= plan["start_ms"] <= closure["start_ms"] < closure["end_ms"],
            "timing mutex acquisition differs")
    latest = grouped.linked(closure["live_request"])
    ready, closed = grouped.linked(latest["ready"]), grouped.linked(closure["closed"])
    original_native = historical["native_restart"]
    anchor = grouped.linked(original_native["ready"])
    predecessor.verify_service_lease(ready, closed, anchor, plan["candidate_service"]["image"]["sha256"])
    require(ready["health"]["instance"] == closure["instance"], "timing service identity differs")

    binding = grouped.linked(latest["bindings"])
    original_binding = grouped.linked(historical["scope_baseline"])
    require(binding["instance"] == closure["instance"]
            and predecessor.binding_identity(binding) == predecessor.binding_identity(original_binding),
            "timing service did not retain the admitted R7 scope")
    idle = grouped.pin(replay.WORK / "commands/stage-after.stdout", original_native["status"]["sha256"])
    command = grouped.link(replay.WORK / "commands/shutdown.command.json")
    audit = grouped.linked(plan["input_audit"])
    accepted = grouped.linked(command)
    require(accepted["exit_code"] == 0 and accepted["argv"] == [
        audit["inputs"]["cli"]["path"], "--output", "json", "--service-runtime-dir",
        anchor["control_root"], "service", "shutdown"],
        "timing shutdown did not target the original installation")
    r7_plan = grouped.linked(historical["receipts"]["plan"])
    retired = [anchor["health"]["instance"], closure["instance"]]
    for ref in (r7_plan["restart_handoff"]["ready"], historical["receipts"]["first_ready"],
                historical["receipts"]["final_ready"]):
        retired.append(grouped.linked(ref)["health"]["instance"])
    require(len(set(retired)) == 5, "duplicate retired CAL service identity")
    baseline = grouped.linked(historical["receipts"]["control"])
    return {
        "native_restart": original_native | {"status": idle},
        "scope_baseline": latest["bindings"],
        "retired_instances": sorted(retired),
        "timing": {key: report[key] for key in ("plan", "outer", "native_closure", "release")} | {
            "observation": observation, "ready": latest["ready"], "closed": closure["closed"],
            "idle": idle, "shutdown": command,
        },
    }, baseline


def verify_control_record(current, baseline):
    original = baseline["files"]
    observed = current["files"]
    require(current["control_root"] == baseline["control_root"]
            and current["directories"] == baseline["directories"] and observed.keys() == original.keys()
            and {key for key, sha in observed.items() if sha != original[key]} == r7_handoff.MUTABLE,
            "current control inventory differs from the original CAL installation")


def observe():
    """Verify current files while the full capture parent holds the installation mutex."""
    proof, baseline = retirement()
    control = predecessor.ANCHOR / "control"
    require(control.resolve(strict=True) == control and baseline["control_root"] == str(control),
            "CAL physical anchor changed")

    def seal(path: Path):
        require(path.is_file() and path.stat().st_nlink == 1, "nonordinary CAL control entry")
        return grouped.link(path)

    current = r7_handoff.inventory(control, baseline, seal)
    verify_control_record(current, baseline)
    return proof | {"control_before": current}


def verify_frozen(proof):
    original, baseline = retirement()
    require({key: value for key, value in proof.items() if key != "control_before"} == original,
            "latest timing retirement pins changed")
    verify_control_record(proof["control_before"], baseline)
