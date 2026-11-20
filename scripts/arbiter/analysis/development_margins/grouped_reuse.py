# SPDX-License-Identifier: Apache-2.0
"""Authenticate the grouped CAL installation handoff before using its last native idle status."""
import hashlib
import json
from pathlib import Path


MUTEX = r"Local\AIRK-20261116-Focus1024-Reuse"
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(path, seal=None):
    require(path.resolve(strict=True) == path and path.is_file() and all(not p.is_symlink() and not p.is_junction()
            for p in (path, *path.parents)), "foreign grouped evidence path")
    raw = path.read_bytes()
    actual = hashlib.sha256(raw).hexdigest()
    require(seal is None or seal == actual, "grouped evidence digest changed")
    return json.loads(raw), {"path": str(path), "sha256": actual}


def pinned_at(pin, path):
    require(pin["path"] == str(path), "grouped reference escaped its original path")
    return read(path, pin["sha256"])[0]


def check_release(release, acquired, outer, custody):
    require(release["name"] == acquired["name"] == MUTEX and release["owner_pid"] == acquired["owner_pid"] > 0
            and acquired["at_ms"] <= release["at_ms"] and release.get("outer"), "missing natural grouped mutex release")
    require(outer["owned_descendants_settled"] and not outer["owned_job_forced_termination"]
            and not outer["timed_out"] and outer["owned_job_final_active_processes"] == 0
            and outer["owned_job_final_member_pids"] == [] and custody["exit_code"] == outer["exit_code"]
            and custody["owner_sid"] == SID and custody["changed_input_count"] == 0 and custody["output_count"] > 0,
            "grouped owner, source custody or native process settlement differs")


def group_layout(plan, freeze):
    three = "airk.private.cal100-grouped-plan/1"
    two = "airk.private.cal100-two-lifetime-plan/1"
    require(plan["schema"] in (two, three), "unknown grouped CAL layout")
    is_two = plan["schema"] == two
    phases = [list(range(7)), list(range(7, 13))] if is_two else [list(range(4)), list(range(4, 8)), list(range(8, 13))]
    require(plan["phase_groups"] == phases, "grouped phase order differs")
    root = Path(__file__).resolve().parents[4]
    base = root / "scripts/arbiter/evaluation/selected_goal/cal_capture"
    names = ("controller.py", "prepare.py", "phase.cjs", "worker.cjs", "window_scope.cjs", "dispatch_proof.py", "launch.py",
             "phased/capture.py", "phased/lifetime.py", "grouped/run_grouped.py", "grouped/group_service.py", "grouped/installation_mutex.py")
    expected = [base / n for n in names] + [root / "scripts/provision-static-onnx-runtime.py", base.parents[1] / "run_compact_answer.py",
        base.parent / "replay_cal_population.py", root / "scripts/arbiter/analysis/public_source_admission/native_cohort_source_reads.py"]
    if is_two:
        expected.append(base / "grouped/two_lifetimes.py")
    frozen = {r["path"]: r for r in freeze["sources"]}
    require(len(frozen) == len(freeze["sources"]) and len(plan["sources"]) == len(expected)
            and {r["path"] for r in plan["sources"]} == {str(p) for p in expected}
            and all(frozen.get(r["path"]) == r for r in plan["sources"]), "grouped source closure differs")
    schema = "airk.private.cal100-two-lifetime-fixture/1" if is_two else "airk.private.cal100-grouped-fixture/1"
    return len(phases), schema


def verify(campaign, release_sha, custody_sha, installation):
    r5 = campaign / "cal100-focus1024-capture-r5"
    release, _ = read(r5 / "control-mutex-released.json", release_sha)
    acquired = pinned_at(release["acquired"], r5 / "control-mutex-acquired.json")
    require(release.get("outer"), "release lacks the original outer Job")
    outer = pinned_at(release["outer"], r5 / "outer-owned.json")
    custody, custody_pin = read(campaign / "cal100-focus1024-capture-r5-custody.json", custody_sha)
    check_release(release, acquired, outer, custody)
    freeze, _ = read(campaign / "cal100-focus1024-capture-r5-freeze.json", custody["freeze_sha256"])
    args = outer["argv"]
    require(args.count("--owned-plan-sha256") == 1 and args.count("--output") == 1
            and args[args.index("--output") + 1] == str(r5), "grouped original outer command differs")
    plan, plan_pin = read(r5 / "lifetime-plan.json", args[args.index("--owned-plan-sha256") + 1])
    groups, fixture_schema = group_layout(plan, freeze)
    require(plan["installation"] == installation and plan["installation_mutex"] == release["acquired"],
            "grouped installation, mutex or phase order differs")
    r4 = pinned_at(plan["prior_attempt"], campaign / "cal100-focus1024-capture-r4/outer-owned.json")
    require(r4["exit_code"] != 0 and r4["owned_descendants_settled"]
            and r4["owned_job_final_active_processes"] == 0 and not r4["owned_job_final_member_pids"],
            "grouped predecessor is not a settled unsuccessful R4")
    original = pinned_at(installation["ready"], campaign / "cal100-focus1024-capture-r3/fixture/ready.json")
    original_status = pinned_at(installation["status"], campaign / "cal100-focus1024-capture-r3/commands/cal-ready.stdout")
    folders = sorted(r5.glob("lifetime-[0-9][0-9]"))
    require(0 < len(folders) <= groups and folders == [r5 / f"lifetime-{i:02}" for i in range(len(folders))],
            "grouped lifetime roster incomplete or unordered")
    closures = None
    if outer["exit_code"] == 0:
        result, _ = read(r5 / "fixture-closure.json")
        require(result["schema"] == fixture_schema and result["plan"]["sha256"] == plan_pin["sha256"]
                and result["ordered_questions"] == 100 and len(result["lifetimes"]) == len(folders) == groups,
                "successful grouped terminal lacks its native lifetimes")
        closures = result["lifetimes"]
    seen = {original["health"]["instance"]}
    for i, folder in enumerate(folders):
        work = folder / "fixture"
        opened, _ = read(work / "ready.json")
        closed, close_pin = read(work / "closed.json")
        instance = opened["health"]["instance"]
        require(instance not in seen and instance == closed["instance"] and closed["shutdown_completed"]
                and closed["service_lease_reacquired"] and original["control_root"] == opened["control_root"] == closed["control_root"]
                and original["inputs_sha256"] == opened["inputs_sha256"] and original["profiles"] == opened["profiles"]
                and original["health"]["executable_sha256"] == opened["health"]["executable_sha256"],
                "grouped physical control, model, service incarnation or native closure differs")
        seen.add(instance)
        lifetime = folder / "lifetime-closure.json"
        completed_group = closures is not None or lifetime.is_file()
        if completed_group:
            row = pinned_at(closures[i], lifetime) if closures is not None else read(lifetime)[0]
            require(row["group"] == i and row["phases"] == plan["phase_groups"][i] and row["instance"] == instance
                    and row["closed"]["sha256"] == close_pin["sha256"] and row["plan"]["sha256"] == plan_pin["sha256"],
                    "grouped final report differs from its actual native closure")
        # This command was executed after all work in a successful group. A
        # failed group needs its own observed status while its service was alive.
        label = "stage-after"
        if not completed_group:
            label = "failure-stage-at-shutdown"
        command, _ = read(folder / "commands" / (label + ".command.json"))
        require(command["exit_code"] == 0 and command["argv"][1:] == ["--output", "json", "--service-runtime-dir",
                original["control_root"], "arbiter", "status-legacy"], "fresh native idle status command differs")
        status, status_pin = read(folder / "commands" / (label + ".stdout"))
        require(status["configuration"] == original_status["configuration"] and status.get("initialization") is None
                and all(status[k]["queued"] == status[k]["running"] == 0 and status[k]["availability"] == "ready"
                        and status[k]["active_profile"] == original_status[k]["active_profile"] for k in ("focus", "context")),
                "grouped terminal stage status is not settled on original configuration")
    return status_pin, custody_pin
