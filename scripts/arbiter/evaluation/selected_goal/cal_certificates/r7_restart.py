# SPDX-License-Identifier: Apache-2.0
"""Authenticate the original retired R6 instance and the frozen R7 scope."""
from pathlib import Path

from phased_closure import installation_identity
from reference_inputs import CAMPAIGN, read, reference, require

R6 = CAMPAIGN / "cal100-focus1024-capture-r6"
R7 = CAMPAIGN / "cal100-focus1024-capture-r7"
GROUP = R6 / "lifetime-00"
WORK = GROUP / "fixture"
OWNER_SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
INPUTS = {
    "plan": (R6 / "lifetime-plan.json", "213c57218699b8fc184a95386cfaab7f745a12f9cb2f9ac9ac12fece3388cadf"),
    "outer": (R6 / "outer-owned.json", "c29f7db5040ef9d4face6a6fea6365de36aaf51adf8ad940bb6f8cb39fd359db"),
    "release": (R6 / "control-mutex-released.json", "d976fbe1a642530b6e57ef470a44827cb89df7d0bf03ddeb12a04b086543c74e"),
    "custody": (R6.with_name(R6.name + "-custody.json"), "c805f6228ad84ec4aff40b9699231be83507a40fe78d88ec16a3922000db543f"),
    "ready": (WORK / "ready.json", "4345262004b31daaa606622de4521ee952f351c70f7764eccff3efea60b17fb2"),
    "closed": (WORK / "closed.json", "63428142b9ba0b89f0f1bbd9f13b16708b078a68b0e9035553034758bf1055ea"),
    "idle": (GROUP / "commands/failure-stage-at-shutdown.stdout", "30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"),
    "bindings": (WORK / "enable-context.bindings.json", "b7e0812efb0a0c14d7c70b7eb4c07a84f854e18c16487569dcf8cc55e644c364"),
    "dispatch": (WORK / "composed-observer.closed.json", "16326c02f9f6a2ed9a322a4ddb6e8c88b6374ff7f85d63552058b3b5996d8471"),
}


def validate_binding(bound, compared, ready, anchor):
    identity, fingerprint = installation_identity(bound)
    expected, previous = installation_identity(compared)
    (old_row, old_status, old_scope), original = installation_identity(anchor)
    row, status, scope = identity
    clock = ready["health"]["catalog_clock"]
    require(identity == expected and fingerprint != previous and
            bound["instance"] == ready["health"]["instance"] != compared["instance"] and
            all(bound[key] == compared[key] for key in
                ("control_root", "evaluation", "release_qualified", "namespace", "schema", "scopes_sha256")) and
            bound["catalog_revision"] == compared["catalog_revision"] == clock["revision"] == 8 and
            bound["namespace"] == clock["namespace"] and
            scope["selection"]["catalog_revision"] == clock["revision"] and
            old_row["window_scope_selection"] | {"expected_catalog_revision": 8} == row["window_scope_selection"] and
            old_row | {"window_scope_selection": row["window_scope_selection"]} == row and old_status == status and
            old_scope | {"selection": scope["selection"]} == scope and
            old_scope["selection"] | {"catalog_revision": 8,
                                      "scope_identity": scope["selection"]["scope_identity"]} == scope["selection"] and
            old_scope["selection"]["scope_identity"] != scope["selection"]["scope_identity"] and
            anchor["catalog_revision"] == 4 and anchor["namespace"] == bound["namespace"] and
            original not in (fingerprint, previous),
            "retired R6 scope differs from the original revision-eight Search binding")


def validate_restart(plan, audit, compared, search_control):
    handoff = plan["restart_handoff"]
    require(set(handoff) == {*INPUTS, "control"}, "incorrect retired R6 evidence roster")
    records = {}
    for name, (path, sha) in INPUTS.items():
        require(handoff[name] == reference(path, sha), "different retired R6 receipt: " + name)
        records[name] = read(handoff[name], path)
    retired, outer, release, custody = (records[k] for k in ("plan", "outer", "release", "custody"))
    require(retired["schema"] == "airk.private.cal100-r6-two-lifetime-plan/1" and
            retired["input_audit"] == plan["input_audit"] and
            retired["physical_handoff"] == plan["physical_handoff"] and
            retired["installation"] | {"status": handoff["idle"]} == plan["installation"] and
            retired["candidate_service"]["image"] == audit["inputs"]["service"] and
            retired["candidate_service"]["provenance"] == plan["candidate_service"]["provenance"] and
            str(R6) in outer["argv"] and handoff["plan"]["sha256"] in outer["argv"],
            "R6 used a different installation, original Search, service or admitted plan")
    require(outer["exit_code"] == custody["exit_code"] == 1 and
            outer["child_assigned_before_resume"] and outer["owned_descendants_settled"] and
            not outer["timed_out"] and not outer["log_oversized"] and not outer["owned_job_forced_termination"] and
            outer["owned_job_final_active_processes"] == 0 and not outer["owned_job_final_member_pids"] and
            custody["owner_sid"] == OWNER_SID and custody["changed_input_count"] == 0 and
            not custody["changed_input_paths"] and custody["output_count"] > 0 and
            not (R6 / "fixture-closure.json").exists() and not list(GROUP.glob("phase-*")),
            "R6 did not retire naturally before phase zero")
    acquired = read(retired["installation_mutex"], R6 / "control-mutex-acquired.json")
    require(release["acquired"] == retired["installation_mutex"] and release["outer"] == handoff["outer"] and
            acquired["name"] == release["name"] == r"Local\AIRK-20261116-Focus1024-Reuse" and
            acquired["owner_pid"] == release["owner_pid"] and
            acquired["at_ms"] <= retired["start_ms"] < release["at_ms"] <= plan["start_ms"],
            "retired R6 installation mutex was not released before R7")
    ready, closed, dispatch = (records[k] for k in ("ready", "closed", "dispatch"))
    require(ready["health"]["instance"] == closed["instance"] and
            ready["health"]["executable_sha256"] == audit["inputs"]["service"]["sha256"] and
            ready["inputs_sha256"] == audit["inputs"]["inputs"]["sha256"] and
            ready["control_root"] == closed["control_root"] and closed["shutdown_completed"] and
            closed["service_lease_reacquired"] and dispatch["complete"] and dispatch["error"] is None and
            all(dispatch[key] == 0 for key in ("records", "bytes", "last_sequence", "dropped", "writer_failed")) and
            (WORK / "composed-observer.jsonl").stat().st_size == 0 and
            not any((WORK / "context-cal-admissions").iterdir()),
            "retired R6 native instance, lease or zero-dispatch proof differs")
    require(plan["scope_baseline"] == handoff["bindings"] and plan["deadline_ms"] <= 1795194600000,
            "wrong R7 named-scope baseline or campaign deadline")
    anchor = read(plan["installation"]["bindings"])
    validate_binding(records["bindings"], compared, ready, anchor)
    validate_control(read(handoff["control"], R7 / "predecessor-control.json"), search_control)
    return records["bindings"]


def validate_control(current, previous):
    require(set(current) == {"control_root", "directories", "files"} and
            current["control_root"] == previous["control_root"] and
            current["directories"] == sorted(previous["directories"]) and
            current["files"].keys() == previous["files"].keys(),
            "stopped R6 installation roster differs")
    changed = {key for key, sha in current["files"].items() if sha != previous["files"][key]}
    require(changed == {"http-1/endpoint.json", "logs-1.4/current.jsonl", "service-1.4.sqlite3"} and
            all(current["files"][key] == sha for key, sha in previous["immutable"].items()),
            "stopped R6 installation has unexpected changed bytes")
