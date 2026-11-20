# SPDX-License-Identifier: Apache-2.0
"""Authenticate the stopped R6 native lifetime before a fresh CAL100 attempt."""
from pathlib import Path

import hashlib

import r6_candidate
from r6_preflight import CAMPAIGN, require
from lifetime import binding_identity

R6 = CAMPAIGN / "cal100-focus1024-capture-r6"
R7 = CAMPAIGN / "cal100-focus1024-capture-r7"
PLAN_SCHEMA = "airk.private.cal100-r7-two-lifetime-plan/1"
FIXTURE_SCHEMA = "airk.private.cal100-r7-two-lifetime-fixture/1"
MUTABLE = {"http-1/endpoint.json", "logs-1.4/current.jsonl", "service-1.4.sqlite3"}
ORIGINALS = {
    "plan": (R6 / "lifetime-plan.json", "213c57218699b8fc184a95386cfaab7f745a12f9cb2f9ac9ac12fece3388cadf"),
    "outer": (R6 / "outer-owned.json", "c29f7db5040ef9d4face6a6fea6365de36aaf51adf8ad940bb6f8cb39fd359db"),
    "release": (R6 / "control-mutex-released.json", "d976fbe1a642530b6e57ef470a44827cb89df7d0bf03ddeb12a04b086543c74e"),
    "custody": (R6.with_name(R6.name + "-custody.json"), "c805f6228ad84ec4aff40b9699231be83507a40fe78d88ec16a3922000db543f"),
    "ready": (R6 / "lifetime-00/fixture/ready.json", "4345262004b31daaa606622de4521ee952f351c70f7764eccff3efea60b17fb2"),
    "closed": (R6 / "lifetime-00/fixture/closed.json", "63428142b9ba0b89f0f1bbd9f13b16708b078a68b0e9035553034758bf1055ea"),
    "idle": (R6 / "lifetime-00/commands/failure-stage-at-shutdown.stdout", "30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"),
    "bindings": (R6 / "lifetime-00/fixture/enable-context.bindings.json", "b7e0812efb0a0c14d7c70b7eb4c07a84f854e18c16487569dcf8cc55e644c364"),
    "dispatch": (R6 / "lifetime-00/fixture/composed-observer.closed.json", "16326c02f9f6a2ed9a322a4ddb6e8c88b6374ff7f85d63552058b3b5996d8471"),
}
SEARCH_BINDING = "216bfe2464aeaf5d54cea45a8c68cba47b72cb6cc8d6aae5da189e3fb18867ff"


def verify_scope(anchor, search, retired, ready):
    old_row, old_status, old_scope = binding_identity(anchor)
    current_row, current_status, scope = binding_identity(retired)
    revision = retired["catalog_revision"]
    namespace = retired["namespace"]
    require(binding_identity(search) == (current_row, current_status, scope)
            and old_status == current_status
            and {k: v for k, v in old_row.items() if k != "window_scope_selection"} ==
                {k: v for k, v in current_row.items() if k != "window_scope_selection"}
            and current_row["window_scope_selection"] ==
                (old_row["window_scope_selection"] | {"expected_catalog_revision": revision})
            and scope == (old_scope | {"selection": scope["selection"]})
            and scope["selection"] == (old_scope["selection"] | {
                "catalog_revision": revision, "scope_identity": scope["selection"]["scope_identity"]})
            and revision == 8 and anchor["catalog_revision"] == 4
            and scope["selection"]["scope_identity"] != old_scope["selection"]["scope_identity"]
            and current_row["window_scope_selection"]["namespace"] == namespace == anchor["namespace"]
            and ready["health"]["catalog_clock"] == {"namespace": namespace, "revision": revision},
            "closed R6 scope does not match the measured candidate epoch and original CAL coverage")


def inventory(control, baseline, link):
    files, directories = {}, []
    for path in sorted(control.rglob("*")):
        require(not path.is_symlink() and not path.is_junction(), "linked CAL control entry")
        name = path.relative_to(control).as_posix()
        if path.is_dir():
            directories.append(name)
        else:
            files[name] = link(path)["sha256"]
    require(sorted(directories) == sorted(baseline["directories"])
            and files.keys() == baseline["files"].keys()
            and {key for key in files if files[key] != baseline["files"][key]} == MUTABLE,
            "stopped R6 control roster or unchanged file identities differ")
    return {"files": files, "directories": sorted(directories), "control_root": str(control)}


def evidence(plan, epoch, r5, *, create=False, live=False):
    refs = {key: r5.pin(path, digest) for key, (path, digest) in ORIGINALS.items()}
    read = r5.linked
    prior_plan, outer, release, custody, ready, closed, idle, binding, dispatch = (
        read(refs[key]) for key in ORIGINALS
    )
    root = R6 / "lifetime-00"
    acquired = read(release["acquired"])
    initial = plan["installation"]
    comparison = read(plan["physical_handoff"]["comparison"])
    search_binding = comparison["fixture"]["enable-context.bindings.json"]
    require(search_binding["sha256"] == SEARCH_BINDING
            and prior_plan["physical_handoff"] == plan["physical_handoff"]
            and prior_plan["input_audit"] == epoch["audit"]
            and prior_plan["phase_ids"] == plan["phase_ids"]
            and prior_plan["candidate_service"]["image"] == epoch["image"]
            and prior_plan["setup_elapsed_ms"] == plan["setup_elapsed_ms"]
            and prior_plan["installation"] == initial
            and refs["plan"]["sha256"] in outer["argv"] and str(R6) in outer["argv"]
            and outer["exit_code"] == 1 and not outer["timed_out"] and not outer["log_oversized"]
            and not outer["owned_job_forced_termination"] and outer["owned_descendants_settled"]
            and outer["owned_job_final_active_processes"] == 0 and outer["owned_job_final_member_pids"] == []
            and release["acquired"] == prior_plan["installation_mutex"] and release["outer"] == refs["outer"]
            and acquired["name"] == release["name"] == r5.MUTEX_NAME
            and acquired["owner_pid"] == release["owner_pid"]
            and acquired["at_ms"] == prior_plan["start_ms"] < release["at_ms"] <= prior_plan["deadline_ms"]
            and custody["exit_code"] == 1 and custody["changed_input_count"] == 0
            and custody["owner_sid"] == "S-1-5-21-3678370870-3858642417-826968105-1002",
            "R6 did not naturally release the same installation and frozen population")
    anchor = read(initial["ready"])
    require(ready["health"]["executable_sha256"] == epoch["image"]["sha256"]
            and all(ready[k] == anchor[k] for k in ("control_root", "inputs_sha256", "profiles"))
            and ready["health"]["instance"] == closed["instance"] == binding["instance"]
            and closed["control_root"] == ready["control_root"] == binding["control_root"]
            and closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True
            and idle == read(initial["status"]) and dispatch["complete"] is True
            and all(dispatch[k] == 0 for k in ("records", "bytes", "last_sequence", "dropped", "writer_failed"))
            and (root / "fixture/composed-observer.jsonl").stat().st_size == 0
            and not list(root.glob("phase-*")) and not (R6 / "fixture-closure.json").exists(),
            "R6 never closed, changed its installation, or issued calibration")
    verify_scope(read(initial["bindings"]), read(search_binding), binding, ready)
    baseline = read(plan["physical_handoff"]["control_after"])
    control = Path(baseline["control_root"])
    require(control.samefile(ready["control_root"]), "wrong stopped CAL root")
    path = R7 / "predecessor-control.json"
    if create:
        r5.native.write_json(path, inventory(control, baseline, r5.link))
    refs["control"] = r5.link(path)
    if live:
        require(read(refs["control"]) == inventory(control, baseline, r5.link),
                "R6 control tree changed after predecessor admission")
    return refs


def snapshots(epoch, output, pin, create):
    sources = r6_candidate.projected(epoch, output)
    sources["group_service.py"] = r6_candidate.replace_bytes(
        sources["group_service.py"], 'linked(original["bindings"])', 'linked(plan["scope_baseline"])')
    if create:
        output.mkdir()
        for name, content in sources.items():
            with (output / name).open("xb") as file:
                file.write(content)
    require({p.name for p in output.iterdir()} == set(sources), "wrong R7 candidate snapshot roster")
    return {name: pin(output / name, hashlib.sha256(content).hexdigest()) for name, content in sources.items()}
