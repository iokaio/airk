# SPDX-License-Identifier: Apache-2.0
"""Own one isolated held-out TechQA pre-Context-dispatch capture on Windows."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
INPUT = CAMPAIGN / "techqa-predispatch-pilot-inputs-r1"
DESCRIPTOR = "d1bc5ae7f75c9a0574eb8c4bba53dcbfa7001927e58c476fb6f4bdd8ed9aadab"
LARGE = "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"
LIMIT = 1800
BUDGET = 120
NATIVE = ROOT / "scripts/arbiter/evaluation/run_compact_answer.py"
NATIVE_SHA = "9625bb14ca85eaf5057b3b272d92eb99589fa13e47ea1eeef56d35b0fcb09496"
OWNER_SHA = "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path, expected=None):
    require(path.resolve(strict=True) == path and path.is_file() and not path.is_symlink() and not path.is_junction(),
            "ordinary canonical input required")
    with path.open("rb") as stream:
        sha = hashlib.file_digest(stream, "sha256").hexdigest()
    require(expected is None or sha == expected, "source pin changed: " + path.name)
    return sha


def read(path):
    return json.loads(path.read_bytes())


def validate_inputs():
    pin(INPUT / "pilot.private.json", DESCRIPTOR)
    pilot = read(INPUT / "pilot.private.json")
    for name, expected in pilot["private_files"].items():
        pin(INPUT / name, expected)
    for asset in pilot["runtime_inputs"].values():
        pin(Path(asset["path"]), asset["sha256"])
    tree = Path(pilot["source_root"])
    root = tree.parent
    pin(root / "manifest.private.json", pilot["full_menu_manifest_sha256"])
    docs = read(root / "manifest.private.json")["documents"]
    expected = {key + ".txt": docs[key] for key in pilot["publisher_order"]}
    require(len(expected) == 50 and {p.name for p in tree.iterdir()} == expected.keys(), "pilot source closure differs")
    for name, entry in expected.items():
        pin(tree / name, entry["sha256"])
    require(read(INPUT / "source-scopes.private.json") == {"source_roots": [str(tree)]}, "foreign source scope")
    for part in pilot["minilm_assets"]:
        pin(Path(pilot["runtime_inputs"]["minilm"]["path"]).parent / part["name"], part["sha256"])
    return pilot


def confirmed_census(record, answer, request, binding):
    native = record["request"]
    require(record["schema"] == "airk.private-selected-Context-native-admission-observation/1"
            and record["Context_NN_dispatch"] is False and record["Focus_configuration"] == "ON"
            and record["actual_Context_profile"]["manifest"] == binding["context"]
            and record["actual_Focus_profile"] == binding["focus"] and record["MiniLM_pipeline"] == binding["pipeline"],
            "observer identity or non-dispatch changed")
    require(native["question"] == answer["question"] == request["question"]
            and native["read_attempt"]["request_id"] == answer["request_id"]
            and native["scope"] == answer["assessment"]["scope"] and native["scope"]["publication"] == binding["publication"],
            "captured request or scope differs from canonical Answer")
    require(answer["scope_observation"] == binding["scope_observation"]
            and answer["read_scope"] == {"source": "named", "index_ids": [binding["index_id"]]}, "delivered scope differs")
    matches = [event for event in answer["assessment_history"] if event["ordinal"] == native["ordinal"]]
    require(len(matches) == 1 and matches[0]["basis"] == native["roster"]["basis"], "ambiguous assessment association")
    advice = matches[0]["result"]["advice"]
    require(advice["input_digest"] == record["planned_input_digest"] and advice["observed_input_tokens"] == 0
            and advice["work"]["input_tokens"] == 0 and advice["outcome"]["status"] == "unavailable",
            "Context dispatch or compiler digest differs")
    allowance = native["remaining_input_tokens"]
    require(native["parent_budget"] == request["budget"] and 0 <= allowance <= 8192
            and record["known_Focus_input_tokens"] + allowance <= 8192, "original token budget changed")
    census = record["compiler_census"]
    require(len(census) == 2 and {entry["profile"] for entry in census} == {binding["context"], LARGE},
            "both independently identified Context compilers required")
    stats = []
    for entry in census:
        require(entry["NN_invocations"] == 0, "unexpected compiler dispatch")
        if "compiler_refusal" in entry:
            require(entry["fits_remaining_allowance"] is False and "compiled" not in entry,
                    "inconsistent compiler refusal")
            stats.append({"profile": entry["profile"], "refusal": entry["compiler_refusal"]})
            continue
        tokens = entry["input_tokens"]
        require(tokens == len(entry["compiled"]["input_ids"]) > 0
                and entry["fits_remaining_allowance"] is (tokens <= allowance), "invalid prompt census")
        stats.append({"profile": entry["profile"], "input_tokens": tokens,
                      "fits_remaining_allowance": entry["fits_remaining_allowance"],
                      "compiled_input_digest": entry["compiled"]["input_digest"]})
    return {"offered_choices": len(native["choices"]), "roster_cards": len(native["roster"]["cards"]),
            "original_remaining_tokens": allowance, "compiler_census": stats,
            "verified_probe_requests": len(record["verified_probe_requests"]),
            "actual_focus_forward_observed": record["actual_Focus_forward_observed"], "context_forwards": 0}


def wait_stage(native, work, process, ready, stage, manifest, revision, deadline):
    def completed():
        journal = work / "warm-completions.jsonl"
        require(journal.stat().st_size <= 8 << 20, "warm completion journal exceeded budget")
        with journal.open("rb") as stream:
            for line in stream:
                if not line.endswith(b"\n"):
                    return None
                row = json.loads(line)
                if row["stage"] == stage and row["accepted_configure_revision"] == revision:
                    require(row["profile_manifest"] == manifest and row["observer_id"] == ready["warm_observer_id"]
                            and row["outcome"] == "ready", "native warm failed or changed identity")
                    return row
        return None
    return native["wait_observed"](work, completed, process, min(deadline, time.monotonic() + BUDGET),
                                   stage + " warm completion", file_changes=0x18)


def execute(native, owner, output):
    pilot = validate_inputs()
    assets = pilot["runtime_inputs"]
    service, cli_image = (Path(assets[k]["path"]) for k in ("service", "cli"))
    work, logs, index = output / "fixture", output / "commands", output / "index"
    logs.mkdir()
    index.mkdir()
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith("AIRK_")}
    env.update(pilot["required_environment"])
    env.update(AIRK_ARBITER_EVALUATION_INPUTS=assets["inputs"]["path"],
               AIRK_ARBITER_EVALUATION_INPUTS_SHA256=assets["inputs"]["sha256"], AIRK_ARBITER_EVALUATION_WORK=str(work),
               AIRK_ARBITER_EVALUATION_SECONDS=str(LIMIT), AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT="throughput64gib4threads",
               AIRK_MCP_RESPONSE_MODE="full")
    deadline = time.monotonic() + LIMIT

    def invoke(label, *args):
        return native["run_logged"]([cli_image, "--output", "json", "--service-runtime-dir", work / "control", *args],
                                    logs, label, env, deadline)[0]

    def cli(label, *args):
        return read(invoke(label, *args))

    with (output / "service.log").open("xb") as log:
        with subprocess.Popen([service, "--ignored", "--exact", pilot["native_selector"], "--test-threads", "1"],
                              cwd=ROOT, env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                              creationflags=subprocess.CREATE_NO_WINDOW) as process:
            try:
                native["wait_observed"](output, lambda: True if (work / "ready.json").is_file() else None,
                                        process, deadline, "pilot observer readiness")
                ready = read(work / "ready.json")
                admissions = work / "context-cal-admissions"
                require(ready["health"]["process_id"] == process.pid and ready["health"]["executable_sha256"] == assets["service"]["sha256"]
                        and ready["inputs_sha256"] == assets["inputs"]["sha256"] and admissions.is_dir()
                        and not any(admissions.iterdir()) and (work / "control").samefile(ready["control_root"]),
                        "fresh pre-dispatch pilot fixture identity differs")
                setup = invoke("setup", "--root", pilot["source_root"], "--data-dir", index, "library", "setup",
                               "--name", "TechQA held-out pilot", "--watch", "off", "--apply")
                with setup.open("rb") as stream:
                    draft, applied = map(json.loads, stream)
                require("draft" in draft, "native setup draft missing")
                project = applied["receipt"]["request"]["registration"]["binding"]
                index_id = applied["receipt"]["index_id"]
                drafts = output / "semantic-drafts"
                native["run_logged"](native["ignored"](service, "emit_private_selected_vector_configuration"), logs,
                                     "emit-minilm", env | {"AIRK_SELECTED_MINILM_DIRECTORY": str(Path(assets["minilm"]["path"]).parent),
                                                          "AIRK_SELECTED_VECTOR_DRAFT_OUTPUT": str(drafts)}, deadline)
                job = cli("install-minilm", "--service-project", project, "configure", "--file", drafts / "minilm-install.json")["job"]
                native["wait_job"](cli, job["id"], "configure")
                job = cli("index", "--service-project", project, "index")["job"]
                native["wait_job"](cli, job["id"], "index")
                status = native["wait_semantic"](cli, index_id)
                require(status["files"] == 50 and status["read_scope"] == {"source": "named", "index_ids": [index_id]}
                        and status["scope_observation"]["selection"]["owner"] == "agent", "pilot native source count/scope differs")
                publication = status["publication"]
                search = native["tool_body"](cli("pipeline", "tools", "call", "airk_search", "--index", index_id,
                                               "--args", json.dumps({"query": "source", "limit": 1, "max_bytes": 4096,
                                                                    "full_chunks": True, "response_mode": "full", "include_status": True})))
                require(search["semantic_readiness"]["pipeline_digest"] == pilot["required_minilm_pipeline"]
                        and search["publication"] == publication and search["execution"]["incomplete"] is False
                        and search["execution"]["served_mode"] in ("hybrid", "vector"), "pilot MiniLM publication differs")
                selected = {"focus": pilot["cpu_focus_manifest"], "context": pilot["cpu_context_bootstrap_manifest"]}
                for stage, manifest in selected.items():
                    prefix = "enable-" + stage
                    stage_env = env | {"AIRK_ARBITER_EVALUATION_SCOPES": str(INPUT / "source-scopes.private.json"),
                                       "AIRK_ARBITER_EVALUATION_SCOPES_SHA256": pilot["private_files"]["source-scopes.private.json"],
                                       "AIRK_ARBITER_EVALUATION_PROFILE": manifest, "AIRK_ARBITER_EVALUATION_STAGE": stage,
                                       "AIRK_ARBITER_EVALUATION_ENABLED": "true", "AIRK_ARBITER_EVALUATION_CONTROL_PREFIX": prefix}
                    native["run_logged"](native["ignored"](service, "emit_private_arbiter_configuration"),
                                         logs, "emit-" + stage, stage_env, deadline)
                    configuration = cli("configure-" + stage, "arbiter", "configure", "--legacy",
                                        "--file", work / (prefix + ".configure.json"))
                    wait_stage(native, work, process, ready, stage, manifest, configuration["configuration"]["revision"], deadline)
                state = cli("ready-stages", "arbiter", "status-legacy")
                require(all(state[k]["availability"] == "ready" and state[k]["active_profile"] == value
                            and state[k]["selected_profile"] == value for k, value in selected.items()), "pilot stages not both ready")
                request = read(INPUT / "request.private.json")
                emitted = read(work / "context-request.json")
                require(emitted | {"question": request["question"]} == request, "held-out request differs from native template")
                require(not any(admissions.iterdir()), "unexpected admission before original request")
                current = native["tool_body"](cli("request-scope", "tools", "call", "airk_status", "--index", index_id,
                                                  "--args", '{"response_mode":"full","include_status":true}'))
                require(current["publication"] == publication and current["scope_observation"] == status["scope_observation"],
                        "held-out source scope or publication changed")
                answer = native["tool_body"](cli("answer", "tools", "call", "airk_answer", "--index", index_id,
                                                 "--args-file", INPUT / "request.private.json"))
                files = sorted(admissions.glob("record-*.json"))
                require(files and len(files) == len(answer["assessment_history"]), "incomplete delivered admission census")
                binding = dict(selected, pipeline=pilot["required_minilm_pipeline"], publication=publication,
                               scope_observation=current["scope_observation"], index_id=index_id)
                records = []
                for file in files:
                    observed = read(file)
                    records.append({"file": file.name, "sha256": pin(file),
                                    **confirmed_census(observed, answer, request, binding)})
            finally:
                if process.poll() is None and (work / "ready.json").is_file():
                    invoke("shutdown", "service", "shutdown")
            require(process.wait(timeout=max(0, min(BUDGET, deadline - time.monotonic()))) == 0, "native fixture did not settle")
    closed = read(work / "closed.json")
    require(closed["instance"] == ready["health"]["instance"] and closed["shutdown_completed"]
            and closed["service_lease_reacquired"], "private lease was not naturally released")
    summary = read(work / "composed-observer.closed.json")
    require(summary["complete"] and summary["dropped"] == summary["writer_failed"] == 0, "incomplete execution journal")
    journal = work / "composed-observer.jsonl"
    require(journal.stat().st_size == summary["bytes"] <= 64 << 20, "execution log size differs")
    with journal.open("rb") as stream:
        dispatches = [json.loads(line) for line in stream]
    require(len(dispatches) == summary["records"] and not any(row.get("task") == "airk.context_action.v1"
            and row["kind"] in ("dispatch_admitted", "score_returned") for row in dispatches),
            "unexpected actual Context dispatch")
    require(validate_inputs() == pilot, "pilot sources changed across the original attempt")
    native["write_json"](output / "capture.private.json", {
        "schema": "airk.private.techqa-predispatch-capture/1", "case_id": pilot["question_id"],
        "input_sha256": DESCRIPTOR, "publication": publication, "fixture_instance": closed["instance"],
        "ordered_source_documents": len(pilot["publisher_order"]), "native_observations": records, "judged": False,
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    pin(NATIVE, NATIVE_SHA)
    pin(ROOT / "scripts/provision-static-onnx-runtime.py", OWNER_SHA)
    native = runpy.run_path(str(NATIVE))
    owner = native["owner_module"]()
    if args.check_only:
        require(args.output is None and not args.worker, "check-only takes no execution target")
        pilot = validate_inputs()
        print(json.dumps({"verified": DESCRIPTOR, "case": pilot["question_id"], "runtime_started": False}))
        return
    output = owner.plain(args.output)
    require(output.parent == CAMPAIGN and re.fullmatch(r"techqa-predispatch-pilot-run-r[1-9][0-9]*", output.name),
            "new private pilot target required")
    handler = Path(__file__).resolve()
    if args.worker:
        require(os.environ.get("AIRK_TECHQA_PILOT_OWNER") == str(output), "original owner required")
        with native["capped_worker_job"](output):
            execute(native, owner, output)
        return
    require(not output.exists(), "fresh original pilot target required")
    output.mkdir()
    before = pin(handler)
    result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", handler, "--output", output, "--worker"],
                                          ROOT, output / "outer.log", LIMIT,
                                          os.environ | {"AIRK_TECHQA_PILOT_OWNER": str(output)})
    result["source_sha256"] = before
    result["source_seals_unchanged"] = pin(handler) == before and pin(NATIVE) == NATIVE_SHA
    native["write_json"](output / "owned.json", result)
    require(owner.successful(result) and result["source_seals_unchanged"] and result["owned_job_final_active_processes"] == 0
            and result["owned_job_final_member_pids"] == [] and (output / "capture.private.json").is_file(), "pilot did not settle")


if __name__ == "__main__":
    main()
