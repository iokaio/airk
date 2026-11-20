# SPDX-License-Identifier: Apache-2.0
"""Bind one protected Compact phase to the closed R7 projection and native compiler."""
from collections import Counter
import hashlib
from pathlib import Path
import re
import runpy

HERE = Path(__file__).resolve().parent
RAW = HERE.parent / "arbiter/evaluation/selected_goal/cal_raw"
LEGACY = HERE / "compact_cutlass_cal.py"
LEGACY_SHA = "d451a83c70e6f01ad1102995b82c98e19aae647424d5d4d9a69567ea9a720c07"
if hashlib.sha256(LEGACY.read_bytes()).hexdigest() != LEGACY_SHA:
    raise ValueError("historical protected scorer reader changed")
BASE = runpy.run_path(str(LEGACY))
SOURCE_KEYS = BASE["SOURCE_KEYS"]
SELECTOR = BASE["SELECTOR"]
observation = BASE["observation"]
require = BASE["require"]

AUDIT = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
SERVICE = "77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
PROJECTION = "airk.private.cal100-r7-chronological-projection/1"
PLAN = "airk.private.cal100-r7-two-lifetime-plan/1"
FIXTURE = "airk.private.cal100-r7-two-lifetime-fixture/1"
LAYOUT = {"r7-first56": ("first56", 56), "r7-full100": ("full100", 100)}
REGRESSION_SOURCES = (
    "985b906b4a59cc6a642c88387a2e47fc5ffdc84172e371782bf96486e447e9cb",
    "c660904b548ba1b41d11f996834f8c816dbf19f53231806911bb957521d28cec",
)


def scorer_sources(build):
    files = build["groups"]["native"]["files"]
    require(tuple(files[name] for name in SOURCE_KEYS[:2]) == REGRESSION_SOURCES,
            "R7 scorer lacks the candidate-audit or phase-local selector source")


def settled_capture(job):
    return (job["exit_code"] == 0 and job["child_assigned_before_resume"] and not job["timed_out"]
            and not job["log_oversized"] and not job["owned_job_forced_termination"]
            and job["owned_descendants_settled"] and job["owned_job_final_active_processes"] == 0
            and job["owned_job_final_member_pids"] == [])


def validate_population(batch, ids, count, plan_sha):
    rows = batch["rows"]
    phases = batch["phases"]
    require(batch["model"] == BASE["MODEL"] and batch["original_manifest"] == BASE["ORIGINAL"]
            and batch["phase_plan_sha256"] == plan_sha and batch["launch_audit_sha256"] == AUDIT
            and batch["NN_invocations"] == 0 and batch["original_requests"] == len(rows) == count
            and len(ids) == len(set(ids)) == 100 and [row["id"] for row in rows] == ids[:count]
            and [row["ordinal"] for row in rows] == list(range(count)) and len(phases) == (count+7)//8
            and batch["statuses"] == dict(Counter(row["status"] for row in rows)),
            "wrong chronological R7 Compact population")
    require(all(row["status"] in ("eligible", "no_native_assessment_request", "compiler_refused_remaining_allowance")
                and ("compiled" in row) == (row["status"] == "eligible") and len(row["captures"]) <= 1
                and (row["status"] != "eligible" or len(row["captures"]) == 1) for row in rows),
            "invalid R7 request or refusal shapes")
    return rows, phases


def validate_phase(report, phase, plan_sha, lifetime_pin, lifetime, rows):
    slot = phase - lifetime["phases"][0]
    require(report["phase"] == phase and report["plan_sha256"] == plan_sha
            and report["input_audit_sha256"] == AUDIT and report["closed_questions"] == len(rows)
            and report["certificate_count"] == 0 and report["lifetime_sha256"] == lifetime_pin["sha256"]
            and 0 <= slot < len(lifetime["phases"]) and lifetime["phases"][slot] == phase
            and report["phase_owner"] == lifetime["phase_closures"][slot], "foreign R7 phase report or Job")
    require([(row["ordinal"], row["id"], row["captures"]) for row in rows] ==
            [(row["ordinal"], row["question_id"], row["captures"]) for row in report["rows"]],
            "R7 phase repurposed a chronological row or capture")


def validate_capture(cap, row, published, focus, pipeline):
    require(cap["schema"] == "airk.private-selected-Context-native-admission-observation/1"
            and cap["Context_NN_dispatch"] is False and cap["Focus_configuration"] == "ON"
            and cap["actual_Context_profile"]["manifest"] == BASE["ORIGINAL"] and cap["actual_Focus_profile"] == focus
            and cap["MiniLM_pipeline"] == pipeline and cap["request"]["scope"]["publication"] == published,
            "foreign R7 native admission profile or publication")
    if row["status"] == "eligible":
        req = cap["request"]
        typed, = [entry for entry in cap["compiler_census"]
                  if entry["model"] == BASE["MODEL"] and entry["profile"] == BASE["ORIGINAL"]]
        require(row["remaining_ms"] == min(req["remaining_ms"], cap["remaining_ns_at_observation"]//1_000_000)
                and row["remaining_input_tokens"] == req["remaining_input_tokens"] and row["roster"] == req["roster"]
                and typed["compiled"] == row["compiled"] and typed["fits_remaining_allowance"] is True
                and typed["NN_invocations"] == 0, "R7 native request, compiler budget or card roster changed")


def inputs(args, campaign, base, exact, *, audit):
    require(args.cal_epoch in LAYOUT and type(args.cal_phase) is int, "R7 CAL epoch and ordinal phase required")
    epoch, count = LAYOUT[args.cal_epoch]
    phase = args.cal_phase
    require(0 <= phase * 8 < count and args.cal_projection_receipt_sha256 and args.cal_compiler_sha256,
            "R7 phase is outside the sealed projection or is missing its compiler pin")
    scorer_sources(base.read(str(args.build), 32 << 20))
    projection_root = campaign / f"r7-chronological-{epoch}-r1"
    compiler_root = campaign / f"r7-chronological-native-{epoch}-r1"
    capture = campaign / "cal100-focus1024-capture-r7"
    refs = {}

    def keep(files):
        todo = []
        for path, sha in files:
            path = Path(path)
            if path in refs:
                require(refs[path]["sha256"] == sha, "contradictory R7 input pin")
            else:
                todo.append((path, sha))
        if todo:
            audit([path for path, _ in todo])
            for path, sha in todo:
                refs[path] = exact(path, sha, base)
        return [refs[Path(path)] for path, _ in files]

    def linked(pin, path):
        require(pin["path"] == str(path), "foreign R7 closure path")
        ref, = keep([(path, pin["sha256"])])
        require(ref == pin, "foreign R7 closure extent")
        return ref

    wrapper, compiler, _ = keep([
        (projection_root/"projection-receipt.json", args.cal_projection_receipt_sha256),
        (compiler_root/"native.json", args.cal_compiler_sha256), (LEGACY, LEGACY_SHA),
    ])
    presented = base.read(wrapper["path"])
    native = base.read(compiler["path"])
    plan_sha = presented["plan_sha256"]
    require(presented["schema"] == PROJECTION and presented["service_sha256"] == SERVICE
            and presented["original_requests"] == count and presented["NN_invocations"] == presented["gain_certificates"] == 0
            and len(presented["lifetimes"]) == (1 if count == 56 else 2)
            and bool(presented["terminal"]) == (count == 100) and native["projection"] == wrapper
            and native["passed"] is True and native["NN_invocations"] == 0 and native["launch_audit_sha256"] == AUDIT,
            "R7 projection or native compiler receipt differs")
    plan_path = capture/"lifetime-plan.json"
    require(presented["plan"]["sha256"] == plan_sha, "incorrect R7 plan digest")
    plan = base.read(linked(presented["plan"], plan_path)["path"])
    require(plan["schema"] == PLAN and plan["input_audit"] == presented["launch_audit"]
            and plan["input_audit"]["sha256"] == AUDIT and plan["candidate_service"]["image"]["sha256"] == SERVICE
            and plan["installation_mutex"]["sha256"] == presented["acquired_sha256"], "foreign R7 plan or launch audit")
    acquired = linked(plan["installation_mutex"], capture/"control-mutex-acquired.json")
    active = base.read(linked(presented["launch_audit"],
                             campaign/"cal100-fused-candidate-launch-r1/cal100-launch-audit.json")["path"])
    ids = base.read(linked(active["census"], Path(active["census"]["path"]))["path"])["question_ids"]
    models = presented["models"]
    require(set(models) == {"compact", "large"}, "both R7 model compiler inputs are required")
    for model, pin in sorted(models.items()):
        linked(pin, projection_root/f"{model}.json")

    lifetimes = keep([(capture/f"lifetime-{i:02}/lifetime-closure.json", sha)
                      for i, sha in enumerate(presented["lifetimes"])])
    groups = [base.read(pin["path"]) for pin in lifetimes]
    for i, group in enumerate(groups):
        require(group["schema"] == "airk.private.cal100-group-lifetime/1" and group["group"] == i
                and group["plan"] == presented["plan"] and group["phases"] == plan["phase_groups"][i]
                and group["prior"] == (lifetimes[i-1] if i else plan["installation"]["closed"])
                and group["end_ms"] <= plan["deadline_ms"], "R7 native lifetime differs from the closed plan")
    if count == 100:
        outer, whole, release = keep([(capture/name, digest) for name, digest in zip(
            ("outer-owned.json", "fixture-closure.json", "control-mutex-released.json"), presented["terminal"], strict=True)])
        fixture, mutex = base.read(whole["path"]), base.read(release["path"])
        retired = base.read(outer["path"])
        require(settled_capture(retired) and retired["argv"].count("--resume-r6") == 1
                and str(capture) in retired["argv"] and plan_sha in retired["argv"]
                and fixture["schema"] == FIXTURE and fixture["plan"] == presented["plan"]
                and fixture["ordered_questions"] == count and fixture["lifetimes"] == lifetimes
                and fixture["publication"] == groups[0]["publication"] == groups[1]["publication"]
                and mutex["acquired"] == acquired and mutex["outer"] == outer,
                "R7 whole capture or mutex release differs")

    _, runner = keep([(RAW/"r7_chronological_inputs.py", presented["source_sha256"]),
                      (RAW/"run_r7_compiler_batch.py", native["source_sha256"])])
    owner = runpy.run_path(runner["path"])
    compiler_build = Path(native["build"]["path"])
    require(compiler_build == owner["BUILD"], "foreign R7 compiler build")
    linked(native["build"], compiler_build)
    linked(native["build_log"], compiler_build.with_suffix(".log"))
    linked(native["build_owner"], compiler_build.parent/"owned-image/execution.json")
    image = Path(native["image"]["path"])
    _, log_pin = keep([(image, native["image"]["sha256"]), (compiler_root/"native.log", native["log_sha256"])])
    require(0 < log_pin["bytes"] <= 1 << 20, "unbounded R7 compiler stdout")
    owner["emitted_image"](native["build"], native["build_log"], native["build_owner"], image)
    settings, expected = owner["projection_inputs"](wrapper)
    job = native["job"]
    assigned = job["suspended_assignment"]
    require(owner["parent"].settled(job) and assigned["image"] == native["image"]
            and assigned["pid"] == job["owned_leader_pid"] and assigned["execution_user_sid"] == base.SID
            and native["summary"] == expected and native["command"] == [
                str(image), "--exact", owner["SELECTOR"], "--ignored", "--nocapture", "--test-threads=1"],
            "R7 native compiler Job, image or complete census differs")
    log = Path(log_pin["path"]).read_text(encoding="utf-8").splitlines()
    require(all(sum(line.endswith(text) for line in log) == 1 for text in expected.values())
            and any("1 passed; 0 failed;" in line for line in log), "R7 compiler stdout differs from the recorded result")

    rows, phases = validate_population(base.read(models["compact"]["path"], 128 << 20), ids, count, plan_sha)
    phase_paths = [projection_root/f"phase-{i:02}-independent-inputs.json" for i in range(len(phases))]
    require(all(p["path"] == str(path) for p, path in zip(phases, phase_paths, strict=True)), "foreign R7 phase-report path")
    keep([(path, pin["sha256"]) for path, pin in zip(phase_paths, phases, strict=True)])
    for i, path in enumerate(phase_paths):
        group = i // 7
        validate_phase(base.read(str(path)), i, plan_sha, lifetimes[group], groups[group], rows[i*8:(i+1)*8])
    current = base.read(str(phase_paths[phase]))
    life_root = capture/f"lifetime-{phase//7:02}"
    phase_owner = base.read(linked(current["phase_owner"], life_root/f"phase-{phase:02}.owned.json")["path"])
    require(phase_owner["phase"] == phase and phase_owner["transcript"] == current["source_transcript"]
            and settled_capture(phase_owner), "R7 native phase did not close on the original inputs")
    linked(current["source_transcript"], life_root/f"phase-{phase:02}.jsonl")
    selected = rows[phase*8:(phase+1)*8]
    native_files = []
    for row in selected:
        for item in row["captures"]:
            name = Path(item["path"]).name
            path = life_root/"fixture/context-cal-admissions"/name
            require(re.fullmatch(r"record-[0-9]{4}\.json", name) and item["path"] == str(path),
                    "foreign R7 observer file")
            native_files.append((row, item, path))
    keep([(path, item["sha256"]) for _, item, path in native_files])
    for row, item, path in native_files:
        pin = linked(item, path)
        validate_capture(base.read(pin["path"], 8 << 20), row, groups[phase//7]["publication"],
                         active["profiles"]["arbiter-focus-qwen3-reranker-06b"], active["pipeline"])
    settings = {key: value for key, value in settings.items() if "LARGE" not in key}
    return list(refs.values()), selected, settings | {"AIRK_CUDA_CAL_PHASE": str(phase)}
