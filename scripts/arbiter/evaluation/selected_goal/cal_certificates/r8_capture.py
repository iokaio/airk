# SPDX-License-Identifier: Apache-2.0
"""Issue certificates from the retained, naturally closed Windows R8 CAL100 fixture."""

import argparse
from collections import Counter
import json
import math
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from capture_closure import settled
from certify import certificate
from phased_closure import ANCHOR, ANCHOR_FILES, check_warm, installation_identity
import r6_epoch
from r6_group import TWO_GROUPS, check_group, check_mutex
from r6_questions import phase_events
from reference_inputs import CAMPAIGN, checked, digest, load_inputs, parsed, read, reference, require

CAPTURE = HERE.parent / "cal_capture"
sys.path.insert(0, str(CAPTURE / "grouped"))
from r8_full_capture import make_plan, original_plan, validate_clock_and_census
from r8_timing_handoff import verify_frozen
from dispatch_proof import read_journal

ROOT = CAMPAIGN / "cal100-focus1024-capture-r8"
PLAN_SCHEMA = "airk.private.cal100-r8-two-lifetime-plan/1"
FIXTURE_SCHEMA = "airk.private.cal100-r8-two-lifetime-fixture/1"
PLAN_SHA = "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c"
LIFETIME_SHAS = (
    "6fcb20e036c29be9506d754e44aa6260728543dad78d409431495af46fe312de",
    "3ae182d20680159addc3eb5ada60fc6101ffb8879b8242a4bacce979b803a77e",
)
TERMINAL_SHAS = (
    "0376582ce510f54030f3397f677760bf93b8e9eca33fcc498ceeed1a82740214",
    "d2a5895792446f9fd65881c7d4ae41128e6c477a33abb0eb70a3576678edc39e",
    "ec3d059030c394163b27551284a699ae1840808d3626d1bf202de515ff8f566a",
)
AUDIT_SHA = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
SERVICE_SHA = "77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
FOCUS = "arbiter-focus-qwen3-reranker-06b"
CONTEXT = "arbiter-context-qwen3-17b"
DISPATCH_SHA = "75b370b0446e0be1946896cb8978e8028aed0fb939320d4c72426f40386ea027"


def verify_terminal(plan_ref, lifetime_shas, terminal):
    require(Path(plan_ref["path"]) == ROOT / "lifetime-plan.json" and plan_ref["sha256"] == PLAN_SHA
            and tuple(lifetime_shas) == LIFETIME_SHAS and tuple(terminal) == TERMINAL_SHAS,
            "exact complete R8 receipt chain required")
    outer_sha, fixture_sha, _ = terminal
    lifetimes = [reference(ROOT / f"lifetime-{i:02}/lifetime-closure.json", sha)
                 for i, sha in enumerate(lifetime_shas)]
    outer = read(reference(ROOT / "outer-owned.json", outer_sha))
    fixture = read(reference(ROOT / "fixture-closure.json", fixture_sha))
    require(settled(outer) and outer["argv"][1:] == [
        "-I", "-B", "-S", str(CAPTURE / "grouped/r8_full_capture.py"), "--output", str(ROOT),
        "--node", "C:\\Program Files\\nodejs\\node.exe", "--owned-plan-sha256", PLAN_SHA,
    ] and fixture["schema"] == FIXTURE_SCHEMA and fixture["plan"] == plan_ref
        and fixture["lifetimes"] == lifetimes and fixture["ordered_questions"] == 100,
        "incomplete, reordered or foreign R8 fixture")
    return fixture


def verify_plan(plan):
    validate_clock_and_census(plan)
    audit_ref = reference(r6_epoch.NEW_AUDIT, AUDIT_SHA)
    require(plan["schema"] == PLAN_SCHEMA and plan["input_audit"] == audit_ref,
            "R8 launch audit changed")
    prior_ref, _ = original_plan()
    _, audit = r6_epoch.preflight(r6_epoch.R7_ROOT, prior_ref, audit_ref, SERVICE_SHA)
    handoff = plan["restart_handoff"]
    verify_frozen(handoff)
    require(read(handoff["timing"]["release"])["at_ms"] <= plan["start_ms"]
            and read(plan["predecessor_control"], ROOT / "predecessor-control.json") == handoff["control_before"]
            and plan == make_plan(plan["start_ms"], plan["installation_mutex"], plan["host_admission"],
                                  plan["predecessor_control"], handoff),
            "R8 plan differs from the original input and final timing handoff")
    for pin in plan["sources"]:
        checked(pin)
    checked(reference(CAPTURE / "dispatch_proof.py", DISPATCH_SHA))
    return audit


def original_installation(plan, audit):
    anchor = plan["installation"]
    expected_restart = plan["restart_handoff"]["native_restart"]
    require(all(anchor[key] == expected_restart[key] for key in ("ready", "closed", "status")),
            "R8 native restart references a different installation")
    records = {}
    for key in ("ready", "closed", "terminal"):
        suffix, sha = ANCHOR_FILES[key]
        require(anchor[key]["sha256"] == sha, "changed R3 installation anchor")
        records[key] = read(anchor[key], ANCHOR / suffix)
    original = read(anchor["request"], ANCHOR / "live-request.json")
    base = read(anchor["bindings"], ANCHOR / "fixture/enable-context.bindings.json")
    template = read(original["answer"], ANCHOR / "fixture/context-request.json")
    custody = read(anchor["custody"], CAMPAIGN / "cal100-focus1024-capture-r3-custody.json")
    require(anchor["custody"]["sha256"] == "6fe3f0a9ef5d88c7d3ccda9791165ffd5d055252db8b95ac2ab6d91277ac4f1e"
            and custody["changed_input_count"] == 0 and original["ready"] == anchor["ready"]
            and original["bindings"] == anchor["bindings"] and original["start_receipt"] == anchor["start"]
            and template["budget"]["input_tokens"] == 8192 and template["budget"]["deadline_ms"] == 120000
            and template["response_mode"] == "full" and template["include_status"] is True,
            "original R3 custody or Answer template differs")
    ready, closed, outer = (records[key] for key in ("ready", "closed", "terminal"))
    require(outer["exit_code"] == 1 and outer["owned_descendants_settled"] is True
            and outer["owned_job_final_member_pids"] == [] and outer["owned_job_forced_termination"] is False
            and closed["service_lease_reacquired"] is True and closed["shutdown_completed"] is True
            and ready["health"]["instance"] == closed["instance"] == base["instance"]
            and ready["inputs_sha256"] == audit["inputs"]["inputs"]["sha256"]
            and math.ceil(outer["elapsed_seconds"] * 1000) == plan["setup_elapsed_ms"],
            "unclosed R3 installation or changed setup debit")
    work = ANCHOR / "fixture"
    read(anchor["dispatch_summary"], work / "composed-observer.closed.json")
    read_journal(work, audit["profiles"][FOCUS], setup_only=False)
    warm = read(anchor["warm_summary"], work / "warm-completions.closed.json")
    require(warm["complete"] is True and warm["observer_id"] == ready["warm_observer_id"]
            and warm["records"] == warm["last_sequence"] == 2
            and warm["bytes"] == (work / "warm-completions.jsonl").stat().st_size
            and warm["dropped"] == 0 and warm["error"] is None, "original R3 warm evidence differs")
    return base, ready, template


def observation_refs(directory, owner):
    # The historical phase reader consumes the same pinned transcript and
    # verifies each native exchange. Retain its observation references here.
    work = directory / "fixture/context-cal-admissions"
    refs = {}
    question = None
    for line in checked(owner["transcript"]).splitlines():
        if b'"window_grant"' not in line and b'"native_observation"' not in line:
            continue
        event = parsed(line)
        if event["kind"] == "window_grant":
            question = event["question_id"]
        elif event["kind"] == "native_observation":
            refs.setdefault(question, []).append(reference(work / event["name"], event["sha256"]))
    return refs


def verify_lifetime(directory, lifetime, plan, audit, context, instances, fingerprints):
    anchor, ready, stage, identity, template, sources, index_root = context
    work = directory / "fixture"
    restart = read(lifetime["restart"], directory / "restart-request.json")
    require(restart == {k: {n: anchor[k][n] for n in ("path", "sha256")} for k in ("ready", "closed", "status")},
            "R8 restart did not bind the original installation and timing idle state")
    owner_path = work / "restart-owner.json"
    require(owner_path.stat().st_size <= 64 << 10 and parsed(owner_path.read_bytes()) == {
        "control": ready["control_root"], "configuration": stage["configuration"],
        "prior_instance": ready["health"]["instance"],
    }, "R8 native restart owner differs")
    live = read(lifetime["live_request"], directory / "live-request.json")
    started = read(live["ready"], work / "ready.json")
    clock = read(live["start_receipt"], directory / "fixture-start.json")
    closed = read(lifetime["closed"], work / "closed.json")
    bound = read(live["bindings"], work / "enable-context.bindings.json")
    instance = lifetime["instance"]
    require(instance not in instances and instance == started["health"]["instance"] == closed["instance"] == bound["instance"]
            and live["fixture_root"] == str(work) and clock["start_ms"] == lifetime["start_ms"]
            and clock["deadline_ms"] - clock["start_ms"] == plan["common_ms"]
            and clock["input_audit_sha256"] == AUDIT_SHA and started["health"]["process_id"] == clock["pid"]
            and started["health"]["executable_sha256"] == SERVICE_SHA
            and all(started[key] == ready[key] for key in ("inputs_sha256", "control_root", "profiles"))
            and closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True,
            "changed, reused or unclosed R8 native service")
    current, fingerprint = installation_identity(bound)
    require(current == identity and fingerprint not in fingerprints
            and read(live["answer"], work / "context-request.json") == template,
            "R8 named scope reused or changed its source binding or request")
    instances.add(instance)
    fingerprints.add(fingerprint)
    binding = sources["ResidentBinding"](work / "enable-context.bindings.json", source_root=index_root)
    require(binding.publication == lifetime["publication"]
            and lifetime["dispatch"] == read_journal(work, audit["profiles"][FOCUS], setup_only=False),
            "R8 lifetime publication or dispatch journal differs")
    for side in ("before", "after"):
        status = parsed((directory / "commands" / f"stage-{side}.stdout").read_bytes())
        require(status["configuration"] == stage["configuration"] and all(
            status[key]["availability"] == "ready" and status[key]["selected_profile"] ==
            status[key]["active_profile"] == audit["profiles"][profile]
            for key, profile in (("focus", FOCUS), ("context", CONTEXT))), "R8 native stage differs")
    check_warm(lifetime["warm"], work, started)
    return binding


def collect(plan_ref, lifetime_shas, terminal):
    """Return the original ordered, fully settled R8 evidence for both downstream readers."""
    fixture = verify_terminal(plan_ref, lifetime_shas, terminal)
    plan = read(plan_ref, ROOT / "lifetime-plan.json")
    audit = verify_plan(plan)
    _, ids, tasks, _, _, _, sources, index_root = load_inputs(
        reference(r6_epoch.ORIGINAL, r6_epoch.ORIGINAL_SHA), audit["profiles"][FOCUS])
    require(plan["census"] == audit["census"] and read(plan["census"])["question_ids"] == ids
            and plan["phase_ids"] == [ids[i:i+8] for i in range(0, 100, 8)],
            "R8 exact CAL question predeclaration differs")
    base, ready, template = original_installation(plan, audit)
    stage = read(plan["installation"]["status"])
    baseline = read(plan["scope_baseline"])
    identity, retired_fingerprint = installation_identity(baseline)
    _, anchor_fingerprint = installation_identity(base)
    fingerprints = {anchor_fingerprint, retired_fingerprint}
    instances = set(plan["restart_handoff"]["retired_instances"])
    require(len(fingerprints) == 2 and len(instances) == 5
            and len({ready["health"]["instance"], baseline["instance"]}) == 2
            and {ready["health"]["instance"], baseline["instance"]} <= instances,
            "R8 handoff did not retire the original and timing scope")
    publication = base["bindings"][0]["publication"]
    require(fixture["publication"] == baseline["bindings"][0]["publication"] == publication,
            "R8 original publication changed")
    anchor = plan["installation"]
    context = (anchor, ready, stage, identity, template, sources, index_root)
    prior = anchor["closed"]
    last = plan["start_ms"]
    observations = []
    for group, pin in enumerate(fixture["lifetimes"]):
        directory = ROOT / f"lifetime-{group:02}"
        lifetime = read(pin, directory / "lifetime-closure.json")
        require(lifetime["plan"] == plan_ref and lifetime["prior"] == prior
                and lifetime["publication"] == publication, "R8 native lifetime predecessor or publication differs")
        phase_pins = check_group(directory, lifetime, group, plan, last)
        binding = verify_lifetime(directory, lifetime, plan, audit, context, instances, fingerprints)
        observed_names = set()
        group_rows = []
        worker = Path(plan["candidate_service"]["sources"]["worker.cjs"]["path"])
        for phase, phase_pin in zip(TWO_GROUPS[group], phase_pins, strict=True):
            # Verify the whole phase before carrying any observation reference to a consumer.
            rows = list(phase_events(directory, phase, phase_pin, ids, binding, template, tasks, audit["pipeline"],
                                     worker=worker, request_sha=lifetime["live_request"]["sha256"],
                                     instance=lifetime["instance"], fingerprints=fingerprints,
                                     observed_names=observed_names))
            refs = observation_refs(directory, read(phase_pin, directory / f"phase-{phase:02}.owned.json"))
            for evidence, window in rows:
                pins = refs.pop(evidence["id"], [])
                require(len(pins) == len(evidence["captured"]), "R8 native observation pins omitted")
                group_rows.append((evidence | {"capture_refs": pins}, window))
            require(not refs, "unassociated R8 native observation pins")
        binding.verify()
        require(lifetime["pipeline_searches"] == sum(e["pipeline_searches"] for e, _ in group_rows)
                and lifetime["probes"] == sum(len(e["probes"]) for e, _ in group_rows)
                and observed_names == {p.name for p in (directory / "fixture/context-cal-admissions").glob("record-*.json")},
                "R8 native lifetime omitted an observation, probe or Search")
        observations.extend(group_rows)
        last, prior = lifetime["end_ms"], pin

    check_complete(plan, fixture, observations, ids, instances, fingerprints, last)
    outer_sha, _, release_sha = terminal
    check_mutex(ROOT, plan, outer_sha, release_sha, plan["start_ms"] + fixture["elapsed_ms"] - plan["setup_elapsed_ms"])
    return plan, audit, observations


def check_complete(plan, fixture, observations, ids, instances, fingerprints, last):
    require([e["id"] for e, _ in observations] == ids and len(ids) == len(set(ids)) == 100
            and len(instances) == 7 and len(fingerprints) == 104
            and 0 < fixture["pipeline_searches"] == sum(e["pipeline_searches"] for e, _ in observations)
            and 0 <= fixture["probes"] == sum(len(e["probes"]) for e, _ in observations) <= 500
            and last <= plan["deadline_ms"]
            and last - plan["start_ms"] + plan["setup_elapsed_ms"] <= fixture["elapsed_ms"] <= plan["common_ms"],
            "incomplete R8 population, fingerprints, probe/Search census or common clock")


def report(plan_ref, lifetime_shas, terminal):
    _, audit, observations = collect(plan_ref, lifetime_shas, terminal)
    return report_verified(plan_ref, audit, observations, lifetime_shas, terminal)


def report_verified(plan_ref, audit, observations, lifetime_shas, terminal):
    """Apply the original scoring rules to a complete result from collect()."""
    focus = audit["profiles"][FOCUS]
    _, ids, tasks, ledger, utility, policy, _, _ = load_inputs(reference(r6_epoch.ORIGINAL, r6_epoch.ORIGINAL_SHA), focus)
    rows = [certificate(evidence, tasks[evidence["id"]], audit, binding, policy, ledger, utility)
            for evidence, binding in observations]
    require([r["id"] for r in rows] == ids, "unclosed original CAL population")
    outer, fixture, release = terminal
    inputs = audit["inputs"]
    source = (
        Path(__file__), HERE / "r6_epoch.py", HERE / "r6_group.py", HERE / "r6_questions.py",
        HERE / "r6_effective_sources.py", HERE / "certify.py", HERE / "phased_closure.py",
        HERE / "grouped_closure.py", HERE / "capture_closure.py", HERE / "reference_inputs.py",
        CAPTURE / "dispatch_proof.py", *(CAPTURE / "grouped" / f"r8_{name}.py"
                                        for name in ("full_capture", "timing_handoff", "predecessor")),
    )
    return {
        "schema": "airk.private-selected-Context-CAL-reference-preflight/1", "complete": True,
        "qualified_profiles": [], "Focus_configuration": "ON", "Focus_profile": focus,
        "MiniLM_pipeline": audit["pipeline"], "corpus_sha256": inputs["corpus"]["sha256"],
        "source_manifest_sha256": inputs["manifest"]["sha256"], "rubric_sha256": inputs["rubric"]["sha256"],
        "task_policy": policy, "human_semantic_gold_count": 0, "capture_sha256": fixture,
        "outer_owned_sha256": outer, "lifetime_plan_sha256": plan_ref["sha256"],
        "lifetime_closure_sha256": list(lifetime_shas), "mutex_release_sha256": release,
        "launch_audit_sha256": AUDIT_SHA, "focus_tokens": 1024, "service_sha256": SERVICE_SHA,
        "source_sha256": {p.name: digest(p.read_bytes()) for p in source},
        "native_capture_status_counts": Counter(r["native_capture_status"] for r in rows),
        "scope": "Original human support gain from additional verified reads only. Terminal labels unavailable.",
        "rows": rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode and sys.flags.no_site, "checked isolated Python required")
    value = report(reference(ROOT / "lifetime-plan.json", PLAN_SHA), LIFETIME_SHAS, TERMINAL_SHAS)
    encoded = json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")
    sys.stdout.buffer.write(encoded + b"\n")


if __name__ == "__main__":
    main()
