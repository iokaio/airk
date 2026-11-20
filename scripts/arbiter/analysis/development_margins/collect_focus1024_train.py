# SPDX-License-Identifier: Apache-2.0
"""Summarize the full frozen TRAIN cohort, including absences and unqualified runs."""
import hashlib
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
inputs = runpy.run_path(str(HERE / "capture_inputs.py"))
focus = runpy.run_path(str(HERE / "focus1024.py"))
ROSTER_SHA = "586f4d2d22c2cc4daef0c6b71bef5035f8165f9f8bde546a093d08d474049d67"
require = inputs["require"]


def observed(path):
    require(path.resolve(strict=True) == path and path.is_file(), "canonical regular receipt required")
    raw = path.read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def native_control_name(path):
    # The Rust fixture records canonical extended-length DOS paths; the CLI
    # command receives the ordinary drive-qualified spelling under this root.
    return Path(str(path).removeprefix("\\\\?\\"))


def verified_control(run, owned_sha, capture):
    owner = inputs["load"](run / "owned.json", owned_sha)
    proof = owner.get("shared_installation")
    if proof is None:
        require(run.name not in {"compact-development-TRAIN_Q564-focus1024-run-r2",
                                 "compact-development-TRAIN_Q564-focus1024-run-r3",
                                 "compact-development-TRAIN_Q598-focus1024-run-r1"},
                "expected shared installation receipt missing")
        return run / "fixture/control"
    campaign = inputs["CAMPAIGN"]
    fixed = campaign / "cal100-focus1024-capture-r3/fixture/control"
    plan_file = campaign / "cal100-focus1024-capture-r4/lifetime-plan.json"
    plan_sha = "9259b4be9e340ac8e050060e904f53acffee27e3807eca6145a9a8f3be3e299d"
    require(run.parent == campaign and proof["control_root"] == str(fixed) and proof["anchor_plan_sha256"] == plan_sha,
            "shared control or installation anchor differs")
    plan = inputs["load"](plan_file, plan_sha)["installation"]
    def child(name, relative):
        seal = proof[name]
        path = run / relative
        require(seal["path"] == str(path), "foreign shared-control evidence")
        return inputs["load"](path, seal["sha256"])
    ready = child("ready", "fixture/ready.json")
    closed = child("closed", "fixture/closed.json")
    saved = child("saved", "fixture/restart-owner.json")
    restart = child("restart_request", "restart-request.json")
    status = child("status", "commands/reuse-final-status.stdout")
    command = child("answer_command", "commands/answer.command.json")
    require(all(native_control_name(p) == fixed for p in (ready["control_root"], closed["control_root"], saved["control"]))
            and ready["health"]["instance"] == closed["instance"] == capture["fixture_instance"]
            and closed["shutdown_completed"] and closed["service_lease_reacquired"]
            and command["exit_code"] == 0 and command["argv"][3:5] == ["--service-runtime-dir", str(fixed)]
            and all(status[k]["running"] == status[k]["queued"] == 0 for k in ("focus", "context")),
            "native shared control, case incarnation, command or settlement differs")
    for key in ("ready", "closed"):
        require(restart[key] == {f: plan[key][f] for f in ("path", "sha256")}, "restart anchor changed")
    original = inputs["load"](Path(restart["ready"]["path"]), restart["ready"]["sha256"])
    require(native_control_name(original["control_root"]) == fixed and original["profiles"] == ready["profiles"]
            and original["inputs_sha256"] == ready["inputs_sha256"]
            and original["health"]["executable_sha256"] == ready["health"]["executable_sha256"]
            and original["health"]["instance"] != ready["health"]["instance"],
            "native restart changed original control, executable or profile closure")
    prior = proof["previous"]
    path = Path(prior["path"])
    r4 = campaign / "cal100-focus1024-capture-r4/outer-owned.json"
    r5 = campaign / "cal100-focus1024-capture-r5/control-mutex-released.json"
    required_prior = {"TRAIN_Q564": (r4, r5),
                      "TRAIN_Q598": (campaign / "compact-development-TRAIN_Q564-focus1024-run-r2/owned.json",)}
    search_run = run.name in ("compact-development-TRAIN_Q598-focus1024-run-r2","compact-development-TRAIN_Q598-focus1024-run-r3") and capture["case_id"] == "TRAIN_Q598"
    if search_run:
        latest = campaign / "cal100-search-mode-timing-r1"
        if run.name.endswith("-r3"):
            retired = campaign/"compact-development-TRAIN_Q598-focus1024-run-r2/owned.json"
            require(path==retired and prior["sha256"]=="f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771", "Q598 physical predecessor differs")
            retired_owner=inputs["load"](path,prior["sha256"])
            before=child("control_before","control-before.json")
            require(proof["control_before"]["sha256"]==retired_owner["shared_installation"]["control_after"]["sha256"] and
                    before["control_root"]==str(fixed) and retired_owner["exit_code"]==1 and retired_owner["owned_descendants_settled"],
                    "Q598 r3 lost previous stopped custody")
            prior=retired_owner["shared_installation"]["previous"]
            path=Path(prior["path"])
        required_prior["TRAIN_Q598"] = (latest / "outer-owned.json",)
        require(prior["sha256"] == "056fe3e1c3103a60c1b2e88303ef491e89f059fbfbba539ae6a377768e0156fe", "foreign Search predecessor")
    elif capture["case_id"] == "TRAIN_Q598":
        require(prior["sha256"] == "cae29573bd4d5669b0bf35eb1d963c6c29135954bd31278fb0e85b82a657a642",
                "Q598 requires the original independently closed D Q564 receipt")
    require(path.parent.parent == campaign and path in required_prior.get(capture["case_id"], (path,)), "foreign predecessor path")
    if path == r5:
        receipt = proof["grouped_custody"]
        require(receipt["path"] == str(campaign / "cal100-focus1024-capture-r5-custody.json"), "foreign grouped custody")
        source = HERE / "grouped_reuse.py"
        inputs["pin"](source, "8588e26901dc869a955949083aa0580abca727e735a22166a9ecd81c4dbea667")
        prior_status, _ = runpy.run_path(str(source))["verify"](campaign, prior["sha256"], receipt["sha256"], plan)
    else:
        require(path.name in ("owned.json", "outer-owned.json") and "grouped_custody" not in proof,
                "foreign ungrouped predecessor")
        predecessor = inputs["load"](path, prior["sha256"])
        require(predecessor["owned_descendants_settled"] and predecessor["owned_job_final_active_processes"] == 0
                and predecessor["owned_job_final_member_pids"] == [], "previous owner did not settle")
        if search_run:
            require(predecessor["exit_code"] == 0 and not predecessor["timed_out"], "Search owner did not settle")
            prior_status = {"path":str(path.parent/"commands/stage-after.stdout"), "sha256":"30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"}
            warm = child("startup_warm", "startup-warm-before-setup.json")
            rows = warm["completions"]
            require(warm["instance"] == ready["health"]["instance"] and warm["configuration_revision"] == 13 and
                    len(rows) == 2 and {r["stage"] for r in rows} == {"focus","context"} and
                    all(r["observer_id"] == ready["warm_observer_id"] and r["accepted_configure_revision"] == 0 and
                        r["profile_manifest"] == saved["configuration"][r["stage"]]["selected_profile"] and r["outcome"] == "ready" for r in rows),
                    "original rehydration differs")
        elif path == r4:
            prior_status = plan["status"]
        else:
            prior_status = predecessor["shared_installation"]["status"]
            require(path.name == "owned.json" and prior_status["path"] == str(path.parent / "commands/reuse-final-status.stdout"),
                    "foreign predecessor idle status")
    require(restart["status"] == {f: prior_status[f] for f in ("path", "sha256")}
            and inputs["load"](Path(prior_status["path"]), prior_status["sha256"])["configuration"] == saved["configuration"],
            "saved native configuration differs from the sealed predecessor")
    return fixed


def prior_q541(run):
    pins = {
        "owned.json": "56ed43d6bf290c8fdc26f6f9f0ae4bb0c378fc291f22d2f8531a8f3181306f9c",
        "outer.log": "98e8c7ccd352343a660180bd5351f18b65812871cf8b8f51b21515bbe04334b5",
        "fixture/closed.json": "738282e001f33b12e78ec060bebd74df8191d2c3e698662fa209f248d6ec3a77",
        "fixture/composed-observer.closed.json": "16326c02f9f6a2ed9a322a4ddb6e8c88b6374ff7f85d63552058b3b5996d8471",
        "fixture/composed-observer.jsonl": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    }
    for name, sha in pins.items():
        inputs["pin"](run / name, sha)
    owner = json.loads((run / "owned.json").read_bytes())
    closed = json.loads((run / "fixture/closed.json").read_bytes())
    journal = json.loads((run / "fixture/composed-observer.closed.json").read_bytes())
    require(owner["exit_code"] == 1 and owner["owned_descendants_settled"] and not owner["owned_job_forced_termination"]
            and not owner["timed_out"] and owner["owned_job_final_active_processes"] == 0 and owner["owned_job_final_member_pids"] == []
            and closed["shutdown_completed"] and closed["service_lease_reacquired"] and journal["complete"] and not journal["error"]
            and all(journal[k] == 0 for k in ("records", "bytes", "last_sequence", "dropped", "writer_failed"))
            and not any((run / "commands").glob("answer*")) and not any((run / "fixture/context-cal-admissions").iterdir())
            and not (run / "capture.private.json").exists(), "Q541 predecessor cannot be authenticated as pre-Answer failure")
    return {"attempt": 1, "state": "source_pin_failure_before_answer", "run_directory": str(run), "proofs": pins,
            "elapsed_seconds": owner["elapsed_seconds"], "answer_submitted": False, "exit_code": owner["exit_code"]}


def collect(row):
    case = row["case"]
    original_run = inputs["CAMPAIGN"] / ("compact-development-" + case + "-focus1024-run-r1")
    prior = [prior_q541(original_run)] if case == "TRAIN_Q541" else []
    attempt = 2 if prior else 1
    q564 = original_run.parent / "compact-development-TRAIN_Q564-focus1024-run-r2"
    if case == "TRAIN_Q564" and q564.exists():
        require(not q564.with_name("compact-development-TRAIN_Q564-focus1024-run-r3").exists(), "ambiguous Q564 fresh runs")
        inputs["pin"](q564 / "owned.json","cae29573bd4d5669b0bf35eb1d963c6c29135954bd31278fb0e85b82a657a642")
        pins = {"owned.json": "7863042a003d877937fc929cc615da2da876d1b9171e2ba9eb6e6e9a416cce55",
                "outer.log": "d3f9595679e43fd98e35c737629223e9c1cc33ffec302f276b9ba70de0355fb9"}
        for name, seal in pins.items():
            inputs["pin"](original_run / name, seal)
        previous = json.loads((original_run / "owned.json").read_bytes())
        require(previous["exit_code"] == 1 and previous["owned_descendants_settled"]
                and not any((original_run / "commands").glob("answer*")), "Q564 original setup evidence differs")
        prior = [{"attempt": 1, "state": "pre_answer_setup_failed", "answer_submitted": False,
                  "elapsed_seconds": previous["elapsed_seconds"], "run_directory": str(original_run), "proofs": pins},
                 {"attempt": 2, "state": "historical_other_volume_not_reopened", "elapsed_seconds": None}]
        attempt = 3
    if case == "TRAIN_Q598" and (original_run.parent/"compact-development-TRAIN_Q598-focus1024-run-r2").is_dir():
        pin = "6866fe743068d17a73d36022ac1fe82f85bbcea5467b301da2a12dfd89447cd8"
        failed = inputs["load"](original_run/"owned.json",pin)
        require(failed["exit_code"] == 1 and failed["owned_descendants_settled"] and not any((original_run/"commands").glob("answer*")), "Q598 first attempt differs")
        prior = [{"attempt":1,"state":"pre_answer_setup_failed","answer_submitted":False,"elapsed_seconds":failed["elapsed_seconds"],
                  "run_directory":str(original_run),"proofs":{"owned.json":pin}}]
        attempt = 2
        if (original_run.parent/"compact-development-TRAIN_Q598-focus1024-run-r3").is_dir():
            r2=original_run.parent/"compact-development-TRAIN_Q598-focus1024-run-r2"
            seal="f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771"
            failed=inputs["load"](r2/"owned.json",seal)
            require(failed["exit_code"]==1 and failed["owned_descendants_settled"] and not any((r2/"commands").glob("answer*")), "Q598 publication failure differs")
            prior.append({"attempt":2,"state":"pre_answer_readiness_publication_failed","answer_submitted":False,"elapsed_seconds":failed["elapsed_seconds"],
                          "run_directory":str(r2),"proofs":{"owned.json":seal}})
            attempt = 3
    run = q564 if case == "TRAIN_Q564" and attempt == 3 else inputs["CAMPAIGN"] / f"compact-development-{case}-focus1024-run-r{attempt}"
    out = {"case": case, "original_descriptor_sha256": row["descriptor_sha256"], "attempt": attempt,
           "run_directory": str(run), "prior_attempts": prior}
    if not (run / "owned.json").is_file():
        return dict(out, state="started_unsettled" if run.exists() else "unobserved")
    owner, out["owned_sha256"] = observed(run / "owned.json")
    out["elapsed_seconds"] = owner["elapsed_seconds"]
    times = [owner["elapsed_seconds"], *(item["elapsed_seconds"] for item in prior)]
    out["total_attempt_elapsed_seconds"] = None if None in times else sum(times)
    if None in times:
        out["observed_attempt_elapsed_seconds"] = sum(item for item in times if item is not None)
    require(owner["focus_width"] == 1024, "unexpected focus width")
    if (owner["exit_code"] != 0 or owner["timed_out"] or owner["owned_job_forced_termination"]
            or not owner["owned_descendants_settled"] or not owner["source_seals_unchanged"]
            or owner["owned_job_final_active_processes"] != 0 or owner["owned_job_final_member_pids"] != []
            or not (run / "capture.private.json").is_file()):
        return dict(out, state="unqualified_capture", exit_code=owner["exit_code"])
    original = inputs["load"](inputs["INPUTS"] / case / "pilot.private.json", row["descriptor_sha256"])
    expected = inputs["digest"](inputs["encode"](focus["bind"](original, row["descriptor_sha256"])))
    capture, out["capture_sha256"] = observed(run / "capture.private.json")
    out["capture_path"] = str(run / "capture.private.json")
    require(capture["case_id"] == case and capture["input_sha256"] == owner["capture_descriptor_sha256"] == expected
            and capture["ordered_source_documents"] == 50 and capture["judged"] is False, "case provenance differs")
    control = verified_control(run, out["owned_sha256"], capture)
    if control != run / "fixture/control":
        out["verified_control_root"] = str(control)
    out.update(active_descriptor_sha256=expected, publication=capture["publication"])
    admissions = []
    for item in capture["native_observations"]:
        require(Path(item["file"]).name == item["file"], "foreign admission path")
        record = inputs["load"](run / "fixture/context-cal-admissions" / item["file"], item["sha256"])
        require(record["actual_Focus_profile"] == focus["FOCUS"] and not record["Context_NN_dispatch"]
                and record["request"]["remaining_input_tokens"] == item["original_remaining_tokens"], "admission identity differs")
        admissions.append(dict(item, remaining_ns=record["remaining_ns_at_observation"], ordinal=record["request"]["ordinal"]))
    if admissions:
        require("zero_disposition" not in capture, "both positive and zero admissions")
        return dict(out, state="admitted", admissions=admissions)
    zero = capture["zero_disposition"]
    require(zero["kind"] == "observed_budget_stop_without_context_admission" and zero["native_admissions"] == 0
            and zero["compiler_rows"] == 0, "unexpected zero disposition")
    return dict(out, state="budget_stop_zero", zero_disposition=zero)


if __name__ == "__main__":
    roster = inputs["load"](inputs["INPUTS"] / "roster.private.json", ROSTER_SHA)
    require(len(roster) == len({row["case"] for row in roster}) == 15, "frozen TRAIN membership differs")
    rows = [collect(row) for row in roster]
    counts = {state: sum(row["state"] == state for row in rows) for state in sorted({row["state"] for row in rows})}
    print(json.dumps({"schema": "airk.private.techqa-train-focus1024-capture-census/1", "roster_sha256": ROSTER_SHA,
                      "active_focus_manifest": focus["FOCUS"], "counts": counts, "rows": rows,
                      "context_model_scored": False, "task_quality_measured": False}, sort_keys=True))
