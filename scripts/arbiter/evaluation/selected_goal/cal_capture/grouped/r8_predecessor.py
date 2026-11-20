# SPDX-License-Identifier: Apache-2.0
"""Reopen the exact failed R7 custody chain for a prospective new CAL attempt."""

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import two_lifetimes as grouped
from dispatch_proof import read_journal
from installation_mutex import InstallationMutex, MUTEX_NAME
from lifetime import binding_identity, warm_closure
from r7_handoff import PLAN_SCHEMA, R7, inventory, verify_scope

CAMPAIGN = R7.parent
ANCHOR = CAMPAIGN / "cal100-focus1024-capture-r3/fixture"
FIRST = R7 / "lifetime-00"
LAST = R7 / "lifetime-01"
WINDOW_MS = 10_800_000
SETUP_MS = 545_806
PINS = {
    "plan": ("lifetime-plan.json", "c04be2262a32d8318f9aa222e73ea9bf078ad80d7975d907eebefe486895a2eb"),
    "outer": ("outer-owned.json", "964a76d94ad627b7712e97e6af743d3a4bc48c514ceab4a7123d69605b5d3539"),
    "acquired": ("control-mutex-acquired.json", "4130f722bed8aabd6061811a70a319cf74143767cb620e0d4369f0331afa9b28"),
    "release": ("control-mutex-released.json", "409ca5e2cc7551e743c729ba32c67c5593555fcb7b6251c2f729901a3a855800"),
    "control": ("predecessor-control.json", "2d5278f60cd261e0c06d5cbd400e458bf8e3a5ca7ebb8248fec649b53b7284c2"),
    "first_lifetime": ("lifetime-00/lifetime-closure.json", "c3ffb5749ed457e285d2380d181d5bd5dfafed6ff1a7be6c0b807f9ef853871e"),
    "first_ready": ("lifetime-00/fixture/ready.json", "306727fa6b100d40ed63f56c12c41fa5c158fdfc559d8b317c11f85e279e10db"),
    "first_closed": ("lifetime-00/fixture/closed.json", "9603091a040177e5ca7bcd83c09a647d20632ea393265b01941023a0921678ce"),
    "final_start": ("lifetime-01/fixture-start.json", "0fc4b9a529ae2684d7920a3def79240ad9e43e7d1cb5ff5eb3129544bf77e23d"),
    "final_request": ("lifetime-01/live-request.json", "33826a1d778157a5777fe6dc9e85b1184e3361522287e6b189a34158a6a3f043"),
    "phase": ("lifetime-01/phase-12.owned.json", "9c544fe1c19ba37c9f8b91fc4f6b26f570d5302130aaf552b1b301da671cff38"),
    "transcript": ("lifetime-01/phase-12.jsonl", "b701c5582ec1bc2d31daa103a9ae40eddfa63093acff373b515d5fd8dce47b79"),
    "final_ready": ("lifetime-01/fixture/ready.json", "161da72e7b2e47485c92ef39664b9ec1f7473222a7799369bbf6208cddfac422"),
    "final_closed": ("lifetime-01/fixture/closed.json", "9b2725d44d8533d899311daef4a7eba8ad9949e4ac7b930541de1c6b21278feb"),
    "idle": ("lifetime-01/commands/failure-stage-at-shutdown.stdout", "30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"),
    "idle_command": ("lifetime-01/commands/failure-stage-at-shutdown.command.json", "e9880ad0b87ef2bf76ddfe4f26bbebc9666e8f07016119d29402f6fa81415ee4"),
    "shutdown": ("lifetime-01/commands/shutdown.command.json", "819daad1b707c14dbe41b436187faa6b7805fc7c25ca8dbe1cf62d077cf49f06"),
    "binding": ("lifetime-01/fixture/enable-context.bindings.json", "31c2b8a77d0e57ae48e4cc91e1c20fb1e2039bc2f333fb2650e58b24360d4f2a"),
    "warm": ("lifetime-01/fixture/warm-completions.closed.json", "ed7911c0d17850eb4ff1874466372de8238ca36f7fb15398dd3697f57012aac7"),
    "warm_journal": ("lifetime-01/fixture/warm-completions.jsonl", "e4e0fcf89e1b0a8be4b946a7a3cb0a91f032b2f63bbdccceec2c560147b08071"),
    "dispatch": ("lifetime-01/fixture/composed-observer.closed.json", "18fcd6255206926c917937de95578ce815e5206253ae132c4b1400f4d34aac8b"),
    "dispatch_journal": ("lifetime-01/fixture/composed-observer.jsonl", "b524216770e5df474b2648a425a33836c9c82a31991aff3d60902994f18ea674"),
    "failure": ("lifetime-01/service-failure.json", "a61f5255e7ea19a78aba23501ce2a4539dcd924fc09d7a0534a5402c63e97559"),
}
require = grouped.require


def naturally_failed(owner):
    return (owner["exit_code"] == 1 and owner["child_assigned_before_resume"] is True
            and owner["owned_descendants_settled"] is True
            and owner["timed_out"] is False and owner["log_oversized"] is False
            and owner["owned_job_forced_termination"] is False
            and owner["owned_job_final_active_processes"] == 0
            and owner["owned_job_final_member_pids"] == [])


def verify_custody(refs, plan, outer, acquired, release, phase, start, first):
    ids = plan["phase_ids"]
    require(plan["schema"] == PLAN_SCHEMA and plan["common_ms"] == WINDOW_MS
            and plan["clock_decision"] is None and plan["setup_elapsed_ms"] == SETUP_MS
            and plan["deadline_ms"] == plan["start_ms"] + WINDOW_MS - SETUP_MS
            and plan["phase_groups"] == [list(group) for group in grouped.PHASE_GROUPS]
            and [len(chunk) for chunk in ids] == [8] * 12 + [4]
            and len({key for chunk in ids for key in chunk}) == 100,
            "R7 common clock, setup debit or original census differs")
    argv = outer["argv"]
    require(naturally_failed(outer) and argv[1:5] == ["-I", "-B", "-S", str(HERE / "r6_capture.py")]
            and argv.count("--resume-r6") == 1 and str(R7) in argv
            and argv[argv.index("--owned-plan-sha256") + 1] == refs["plan"]["sha256"],
            "original R7 outer Job did not fail and settle naturally")
    require(plan["installation_mutex"] == release["acquired"] == refs["acquired"]
            and release["outer"] == refs["outer"]
            and acquired["name"] == release["name"] == MUTEX_NAME
            and acquired["owner_pid"] == release["owner_pid"]
            and acquired["at_ms"] == plan["start_ms"] < release["at_ms"] <= plan["deadline_ms"],
            "original installation mutex was not released by its owner")
    worker = plan["candidate_service"]["sources"]["worker.cjs"]
    require(naturally_failed(phase) and phase["phase"] == 12 and phase["deadline_ms"] == plan["deadline_ms"]
            and phase["request_sha256"] == refs["final_request"]["sha256"]
            and phase["argv"][1:5] == [worker["path"], refs["final_request"]["path"],
                                       refs["final_request"]["sha256"], "12"]
            and worker in phase["sources"] and phase["transcript"] == refs["transcript"]
            and first["end_ms"] <= start["start_ms"] <= phase["start_ms"] < phase["end_ms"] <= release["at_ms"]
            and phase["deadline_ms"] - phase["start_ms"] <= 1_800_000
            and 125 < float(phase["argv"][-1]) <= (phase["deadline_ms"] - phase["start_ms"]) / 1000
            and start["deadline_ms"] == start["start_ms"] + WINDOW_MS,
            "original R7 phase 12 failure, request or clock differs")


def verify_service_lease(ready, closed, anchor, image_sha):
    require(ready["schema"] == "airk.private-arbiter-fixture/1"
            and closed["schema"] == "airk.private-arbiter-fixture-closed/1"
            and ready["evaluation"] is closed["evaluation"] is True
            and ready["release_qualified"] is closed["release_qualified"] is False
            and ready["health"]["executable_sha256"] == image_sha
            and all(ready[key] == anchor[key] for key in ("control_root", "inputs_sha256", "profiles"))
            and closed["control_root"] == ready["control_root"]
            and closed["instance"] == ready["health"]["instance"]
            and closed["shutdown_completed"] is closed["service_lease_reacquired"] is True,
            "native service instance, installation identity or settled lease differs")


def verify_transcript(data):
    lines = data.splitlines()
    require(lines and lines[-1] == b"original phase or fixture no longer available",
            "original phase 12 final worker error differs")
    events = [json.loads(line) for line in lines[:-1]]
    kinds = Counter(row["kind"] for row in events)
    require(kinds["window_grant"] == 4 and kinds["window_release"] == kinds["cal_question"] == 3
            and not kinds["phase_complete"] and kinds["session_event"] == 1
            and events[-1] == {"kind": "session_event", "event": {"kind": "session_lost", "code": "session_disposed"}},
            "terminal phase transcript did not retain the original incomplete grant and disposal")


def verify_control(before):
    control = ANCHOR / "control"
    require(before["control_root"] == str(control) and control.resolve(strict=True) == control,
            "different original physical control root")

    def seal(path):
        require(path.is_file() and path.stat().st_nlink == 1, "nonordinary CAL control entry")
        return grouped.link(path)

    with InstallationMutex():
        return inventory(control, before, seal)


def observe(*, check_control=False):
    refs = {key: grouped.pin(R7 / rel, sha) for key, (rel, sha) in PINS.items()}

    def record(key):
        return grouped.linked(refs[key])

    plan = record("plan")
    outer, acquired, release = (record(key) for key in ("outer", "acquired", "release"))
    phase, start, first = (record(key) for key in ("phase", "final_start", "first_lifetime"))
    verify_custody(refs, plan, outer, acquired, release, phase, start, first)
    require(not (R7 / "fixture-closure.json").exists() and not (LAST / "lifetime-closure.json").exists(),
            "original R7 failed capture has an unexpected scientific closure")
    transcript = Path(refs["transcript"]["path"]).read_bytes()
    require(hashlib.sha256(transcript).hexdigest() == refs["transcript"]["sha256"], "original phase transcript changed")
    verify_transcript(transcript)

    installation = plan["installation"]
    require(Path(installation["ready"]["path"]) == ANCHOR / "ready.json"
            and Path(installation["closed"]["path"]) == ANCHOR / "closed.json"
            and Path(installation["bindings"]["path"]) == ANCHOR / "enable-context.bindings.json",
            "original R3 native restart anchor differs")
    anchor, anchor_closed, anchor_binding = (grouped.linked(installation[k]) for k in ("ready", "closed", "bindings"))
    physical_control = ANCHOR / "control"
    require(physical_control.resolve(strict=True) == physical_control
            and physical_control.samefile(anchor["control_root"]), "original physical installation differs")
    verify_service_lease(anchor, anchor_closed, anchor, anchor["health"]["executable_sha256"])

    image_sha = plan["candidate_service"]["image"]["sha256"]
    audit = grouped.linked(plan["input_audit"])
    profiles = audit["profiles"]
    focus = profiles["arbiter-focus-qwen3-reranker-06b"]
    context = profiles["arbiter-context-qwen3-17b"]
    require(audit["inputs"]["service"] == plan["candidate_service"]["image"]
            and anchor["inputs_sha256"] == audit["inputs"]["inputs"]["sha256"],
            "R7 candidate launch audit differs from its original installation")
    ready0, closed0, ready1, closed1 = (record(k) for k in ("first_ready", "first_closed", "final_ready", "final_closed"))
    for ready, closed in ((ready0, closed0), (ready1, closed1)):
        verify_service_lease(ready, closed, anchor, image_sha)
    require(len({anchor_closed["instance"], closed0["instance"], closed1["instance"]}) == 3,
            "native service reused an earlier lifetime identity")
    require(first["closed"] == refs["first_closed"] and first["instance"] == closed0["instance"]
            and grouped.linked(first["live_request"])["ready"] == refs["first_ready"],
            "first lifetime identifiers differ from its closed lease")
    publication = anchor_binding["bindings"][0]["publication"]
    grouped.verify_group(refs["first_lifetime"], refs["plan"], installation["closed"], 0, publication)
    require(warm_closure(FIRST / "fixture", ready0) == first["warm"]
            and read_journal(FIRST / "fixture", focus, setup_only=False) == first["dispatch"],
            "first lifetime warm or dispatch settlement differs")

    request = record("final_request")
    require(request["fixture_root"] == start["fixture_root"] == str(LAST / "fixture")
            and request["ready"] == refs["final_ready"] and request["bindings"] == refs["binding"]
            and request["start_receipt"] == refs["final_start"] and start["pid"] == ready1["health"]["process_id"]
            and start["input_audit_sha256"] == plan["input_audit"]["sha256"],
            "final R7 native restart request differs")
    idle, idle_command, shutdown = (record(k) for k in ("idle", "idle_command", "shutdown"))
    command = [audit["inputs"]["cli"]["path"], "--output", "json", "--service-runtime-dir", anchor["control_root"]]
    require(idle_command["argv"] == [*command, "arbiter", "status-legacy"] and idle_command["exit_code"] == 0
            and shutdown["argv"] == [*command, "service", "shutdown"] and shutdown["exit_code"] == 0
            and refs["idle"]["sha256"] == installation["status"]["sha256"]
            and idle["configuration"]["revision"] == 13
            and all(idle[key]["availability"] == "ready" and idle[key]["running"] == idle[key]["queued"] == 0
                    and idle[key]["active_profile"] == idle[key]["selected_profile"] == model
                    for key, model in (("focus", focus), ("context", context))),
            "final stage did not report the original idle configuration and naturally shut down")
    failure = record("failure")
    require(failure["settlement"] == [] and failure["terminal_failed"] is True,
            "phase failure was accompanied by a native settlement failure")

    binding = record("binding")
    baseline = grouped.linked(plan["scope_baseline"])
    comparison = grouped.linked(plan["physical_handoff"]["comparison"])
    search = grouped.linked(comparison["fixture"]["enable-context.bindings.json"])
    verify_scope(anchor_binding, search, binding, ready1)
    require(binding["instance"] == ready1["health"]["instance"] and binding_identity(baseline) == binding_identity(binding),
            "latest native binding differs from the admitted R7 scope")
    require(warm_closure(LAST / "fixture", ready1) == {"summary": refs["warm"], "journal": refs["warm_journal"]},
            "final native warm observer did not close")
    dispatch = read_journal(LAST / "fixture", focus, setup_only=False)
    require(dispatch["summary_sha256"] == refs["dispatch"]["sha256"]
            and dispatch["journal_sha256"] == refs["dispatch_journal"]["sha256"]
            and dispatch["records"] == 3960 and dispatch["focus_dispatches"] == 792
            and dispatch["context_dispatches"] == 0,
            "final native Focus dispatch or charge settlement differs")
    before = record("control")
    require(before["control_root"] == str(physical_control) and plan["restart_handoff"]["control"] == refs["control"],
            "original R7 pre-start control inventory differs")
    return {
        "schema": "airk.private.cal100-r7-retired-predecessor/1",
        "receipts": refs,
        "native_restart": {"ready": installation["ready"], "closed": installation["closed"], "status": refs["idle"]},
        "scope_baseline": refs["binding"],
        "historical_common_ms": WINDOW_MS,
        "historical_setup_ms": SETUP_MS,
        "failed_phase": 12,
        "capture_complete": False,
        "launch_authorized": False,
        "control_observation": verify_control(before) if check_control else None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-control", action="store_true", help="hash the stopped original control tree under its mutex")
    args = parser.parse_args()
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked Windows Python -I -B -S required")
    print(json.dumps(observe(check_control=args.check_control), sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
