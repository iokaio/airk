# SPDX-License-Identifier: Apache-2.0
"""Blind-score closed Gather Context diagnostic actors with the existing rubric."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import json
from pathlib import Path
import subprocess
import sys
import time

import codex_isolation_probe
import common
import coverage
import grade
import judge
import run_session
from gather_context_diagnostic import ARMS
from gather_context_replay import load_pinned, require, write_new
from prepare_complete_evidence import PreparationError

MODEL = "gpt-6.2-astra"
EFFORT = "medium"

MAX_JOBS = 4
JUDGE_TIMEOUT_S = 900


@dataclass(frozen=True)
class Trial:
    directory: Path
    name: str
    scenario: dict
    packet: dict
    measured: dict


@dataclass(frozen=True)
class Observation:
    code: int | None
    stream: bytes
    stderr: bytes
    status: dict
    elapsed_ms: int
    error: str | None


def observe_trial(trial: Trial, config: run_session.CodexRunConfig, schema: dict) -> Observation:
    started = time.monotonic()
    error = None
    try:
        code, stream, stderr, observed = codex_isolation_probe.run_app_server(
            run_session.codex_command(config, trial.directory), run_session.codex_child_env(config, trial.directory),
            trial.directory / "work", model=MODEL, effort=EFFORT, instruction=judge.packet_prompt(trial.packet),
            tool_catalog=[], clean_home=True, output_schema=schema, base_instructions=judge.JUDGE_SYSTEM,
            timeout=JUDGE_TIMEOUT_S, require_chatgpt=True,
        )
    except Exception as failure:
        # Preserve sibling receipts even when one child fails outside the driver's
        # normal terminal result. The driver owns Popen teardown before returning.
        error = type(failure).__name__
        code, stream, stderr, observed = 2, b"", b"", {}
    return Observation(code, stream, stderr, observed, int((time.monotonic() - started) * 1000), error)


def audit_completed_cohort(trials: list[Trial], audit) -> None:
    failures = []
    for trial in trials:
        try:
            audit("After", trial.directory)
        except (common.HarnessError, PreparationError, OSError, subprocess.SubprocessError) as error:
            failures.append((trial.directory.name, error))
    if failures:
        names = ", ".join(name for name, _ in failures)
        raise common.HarnessError(f"Judge output custody failed for {names}") from failures[0][1]


def collect_cohort(trials: list[Trial], *, audit, observe) -> list[Observation]:
    """Join the full cohort before inspecting any child output or auditing the shared home."""
    require(1 <= len(trials) <= MAX_JOBS, "invalid judge cohort width")
    require(len({trial.directory for trial in trials}) == len(trials), "judge directories are not distinct")
    for trial in trials:
        audit("Prepare", trial.directory)
        audit("Before", trial.directory)
    try:
        if len(trials) == 1:
            observations = [observe(trials[0])]
        else:
            with ThreadPoolExecutor(max_workers=len(trials)) as pool:
                observations = list(pool.map(observe, trials))
    finally:
        # Leaving the pool joins all submitted work, including a failed worker.
        audit_completed_cohort(trials, audit)
    return observations


def finish_cohort(trials: list[Trial], observations: list[Observation], *, audit, writer,
                  frozen_sha: str, provenance: dict) -> list[dict]:
    """Retain every audited result before accepting grades or stopping on a failed verdict."""
    eligibility = []
    for trial, observation in zip(trials, observations, strict=True):
        verdict, _, problems = judge.validate_codex_judge(observation.code, observation.status, MODEL, trial.scenario)
        if observation.error:
            problems.append(f"judge driver raised {observation.error}")
        elif type(observation.status.get("process_exit")) is not int:
            problems.append("judge app-server process exit was not observed")
        paths = {"packet.json": common.canonical_json(trial.packet), "stream.jsonl": observation.stream,
                 "stderr.txt": observation.stderr}
        receipt = {
            "judgment_spec_sha256": frozen_sha, "trial": trial.name, "actor": provenance,
            "observed": observation.status, "verdict": verdict, "problems": problems,
            "elapsed_ms": observation.elapsed_ms, "exit_code": observation.code,
            "files": {name: common.sha256_bytes(content) for name, content in paths.items()},
        }
        for name, content in paths.items():
            writer(trial.directory / name, content)
        writer(trial.directory / "judge.json", common.canonical_json(receipt) + b"\n")
        eligibility.append((trial, verdict, problems))
    audit_completed_cohort(trials, audit)
    for trial, _, problems in eligibility:
        require(not problems, f"diagnostic judge {trial.packet['result']}/{trial.name} did not settle")
    return [grade.grade_one(trial.measured, verdict, "complete") for trial, verdict, _ in eligibility]


def read_actor(directory: Path, source_spec_sha: str, scenario: dict) -> tuple[dict, dict, dict]:
    raw = (directory / "session.json").read_bytes()
    record = json.loads(raw)
    require(record["status"] == "complete" and record["actor"]["freeze_sha256"] == source_spec_sha,
            "actor did not settle under the frozen diagnostics spec")
    for name, digest in record["files"].items():
        require(Path(name).name == name and common.sha256_file(directory / name) == digest,
                "actor artifact changed after settlement")
    instruction = (directory / "instruction.txt").read_bytes().decode("utf-8")
    require(instruction == scenario["instruction"], "diagnostic actor instruction differs")
    rows = common.read_jsonl(directory / "turns.jsonl")
    require([row["ordinal"] for row in rows] == list(range(1, record["tool_calls"] + 1)),
            "actor call roster differs")
    turns = []
    bodies = {}
    display_calls = []
    first = {"present": False, "text": "", "bytes": 0, "truncated_for_packet": False}
    for row in rows:
        digest = record["files"][row["reply_file"]]
        require(row["reply_sha256"] == digest, "actor reply pin differs from its manifest")
        envelope = common.read_json(directory / row["reply_file"])
        text = common.envelope_text(envelope)
        require(row["reply_text_bytes"] == len(text.encode("utf-8")), "actor reply size differs")
        primary = "gather_context"
        name = "primary_tool" if row["tool"] == primary else row["tool"]
        if row["ordinal"] == 1:
            first = {"present": row["success"], "bytes": len(text.encode("utf-8")),
                     "truncated_for_packet": len(text) > judge.FIRST_REPLY_CAP,
                     "text": judge.neutralize(text[:judge.FIRST_REPLY_CAP], primary)}
        if row["success"]:
            body = json.loads(text)
            if name == "source_read":
                body = {"hits": [{"citation": {key: item[key] for key in ("path", "start_line", "end_line")},
                                  "text": item["text"]} for item in body["passages"]]}
            bodies[row["ordinal"]] = body
        visible = {"turn": row["ordinal"], "tool": name, "arguments": row["arguments"],
                   "ok": row["success"], "bytes": row["reply_text_bytes"]}
        display_calls.append(visible)
        turns.append({**visible, "elapsed_ms": row["elapsed_ms"]})
    measured = coverage.compute(scenario, turns, body_loader=lambda row: bodies[row["turn"]])
    brief = (directory / "brief.md").read_bytes().decode("utf-8")
    require(brief == record["observed"]["final"], "actor brief differs from observed final")
    notes = [{"id": item["id"], "mark": item["mark"],
              "location": f"{item['path']}:{item['lines'][0]}-{item['lines'][1]}"
              if item.get("lines") else item["path"],
              "symbol": item.get("symbol"), "fact": item.get("fact")} for item in scenario["notes"]]
    packet = {"schema": judge.PACKET_SCHEMA, "result": judge.result_label(common.sha256_bytes(raw)),
              "instruction": instruction, "notes": notes, "brief": judge.neutralize(brief, "gather_context"),
              "calls": display_calls, "first_reply": first, "session_status": "complete"}
    return packet, measured, record


def score(args) -> dict:
    require(type(args.jobs) is int and 1 <= args.jobs <= MAX_JOBS, "judge jobs must be between 1 and 4")
    campaign = args.campaign_root.resolve(strict=True)
    roots = (args.actor_root, args.run_root)
    require(all(path == path.resolve(strict=True) and path.is_relative_to(campaign) and path.is_dir()
                for path in roots) and args.actor_root != args.run_root, "diagnostic roots differ or escaped campaign")
    raw = (args.actor_root / "diagnostic-spec.json").read_bytes()
    actor_sha = common.sha256_bytes(raw)
    require(actor_sha == args.actor_spec_sha256, "diagnostic actor spec changed")
    actor_spec = json.loads(raw)
    require(actor_spec["schema"] == "airk.private.gather-context-diagnostic/1"
            and Path(actor_spec["run_root"]) == args.actor_root, "incorrect diagnostic actor root")
    disposition = load_pinned(campaign, {
        "path": "passes/07-windows-complete-evidence-r1/ceiling-disposition.json",
        "sha256": args.disposition_sha256,
    }, "ceiling disposition")
    frozen = disposition["scenario_freeze"]
    relative = Path(frozen["path"]).relative_to(campaign).as_posix()
    freeze = load_pinned(campaign, {"path": relative, "sha256": frozen["sha256"]}, "scenario freeze")
    require(set(actor_spec["arms"]) <= set(ARMS) and all(case.split("-", 1)[0] in common.REPOS
            and case.split("-", 1)[1] in common.SLOTS for case in actor_spec["cases"]), "invalid diagnostic roster")
    jobs = [(case, arm) for case in actor_spec["cases"] for arm in actor_spec["arms"]]
    packets = []
    statuses = {}
    summary = args.actor_root / "diagnostic-results.json"
    if summary.is_file():
        observed = common.read_json(summary)
        require(observed["spec_sha256"] == actor_sha
                and {(row["case"], row["arm"]) for row in observed["outcomes"]} == set(jobs)
                and len(observed["outcomes"]) == len(jobs), "diagnostic actor result roster differs")
        statuses = {(row["case"], row["arm"]): row for row in observed["outcomes"]}
    results = []
    for case, arm in jobs:
        repo, slot = case.split("-", 1)
        path = f"scenarios/{repo}/{slot}.json"
        digest = freeze["files"][path]
        scenario = load_pinned(campaign, {"path": path, "sha256": digest}, "development case")
        scenario["_sha256"] = digest
        require((scenario["id"], scenario["set"]) == (case, "development"), "different diagnostic case")
        directory = args.actor_root / f"{case}-{arm}"
        if statuses:
            status = statuses[case, arm]
            require(common.sha256_file(directory / "session.json") == status["session_sha256"],
                    "actor session changed after the diagnostic summary")
            if status["status"] != "complete":
                require(status["status"] in ("timeout", "discarded"), "invalid terminal actor disposition")
                results.append({"id": case, "arm": arm, "tool_calls": status["tool_calls"],
                                "adjudication": {"grade": 0, "correct_finish": False,
                                                 "source": status["status"], "reason": "actor did not finish"}})
                continue
        packet, measured, record = read_actor(directory, actor_sha, scenario)
        packets.append((case, arm, scenario, packet, measured, record))

    config = actor_spec["historical_config"]
    schema = judge.codex_verdict_schema()
    driver = Path(__file__).with_name("gather_context_custody.ps1")
    custody_sha = common.sha256_file(driver)
    require(custody_sha == actor_spec["controller_files"]["gather_context_custody.ps1"],
            "judge custody helper differs from the frozen actor run")
    spec = {"schema": "airk.private.gather-context-diagnostic-judge/1", "actor_spec_sha256": actor_sha,
            "judge_model": MODEL, "effort": EFFORT, "rubric_sha256": common.sha256_bytes(judge.JUDGE_SYSTEM.encode()),
            "schema_sha256": common.sha256_bytes(common.canonical_json(schema)), "judges": ("j1", "j2"),
            "judges_per_cohort": args.jobs, "judge_timeout_s": JUDGE_TIMEOUT_S, "custody_sha256": custody_sha,
            "controller_sha256": common.sha256_file(Path(__file__)),
            "packets": {packet["result"]: common.sha256_bytes(common.canonical_json(packet))
                        for _, _, _, packet, _, _ in packets}}
    data = common.canonical_json(spec) + b"\n"
    frozen_sha = common.sha256_bytes(data)
    codex = run_session.CodexRunConfig(Path(config["executable"]), config["executable_sha256"],
                                      Path(config["model_catalog"]), config["model_catalog_sha256"],
                                      Path(config["isolated_home"]), frozen_sha)
    provenance = run_session.codex_provenance(codex)

    def audit(action, directory):
        subprocess.run([str(args.pwsh), "-NoProfile", "-File", str(driver), "-Action", action,
                        "-Campaign", str(campaign), "-Directory", str(directory), "-IsolatedHome",
                        str(codex.isolated_home), "-ExpectedSid", args.expected_sid],
                       check=True, stdin=subprocess.DEVNULL, timeout=60)

    audit("Freeze", args.run_root)
    write_new(args.run_root / "diagnostic-spec.json", data)
    audit("Frozen", args.run_root)

    trials = [
        Trial(args.run_root / f"{packet['result']}-{name}", name, scenario, packet, measured)
        for _, _, scenario, packet, measured, _ in packets
        for name in spec["judges"]
    ]
    decisions_by_packet = {}
    for offset in range(0, len(trials), args.jobs):
        cohort = trials[offset:offset + args.jobs]
        print(f"Judge cohort {offset // args.jobs + 1} started · {len(cohort)} sessions", flush=True)
        observations = collect_cohort(
            cohort, audit=audit, observe=lambda trial: observe_trial(trial, codex, schema),
        )
        decisions = finish_cohort(
            cohort, observations, audit=audit, writer=write_new, frozen_sha=frozen_sha, provenance=provenance,
        )
        for trial, decision in zip(cohort, decisions, strict=True):
            decisions_by_packet.setdefault(trial.packet["result"], []).append(decision)

    for case, arm, _, packet, measured, record in packets:
        decisions = decisions_by_packet[packet["result"]]
        resolved = grade.adjudicate(*decisions, None)
        result = {"id": case, "arm": arm, "result": packet["result"], "adjudication": resolved,
                  "evidence_turn": measured["all_essential_turn"], "coverage": measured,
                  "tool_calls": record["tool_calls"], "same_case_independent_judges": decisions}
        results.append(result)
        print(f"{case} {arm} {resolved['source']} grade {resolved['grade']} · "
              f"{measured['essential_covered']}/{measured['essential_total']} spans", flush=True)
    audit("Summarize", args.run_root)
    result = {"spec_sha256": frozen_sha, "actor_spec_sha256": actor_sha, "results": results,
              "proof_scope": "Hosted Gather Context diagnostics and blinded grading",
              "settlement_observation": "app-server exit and checked output custody after each joined cohort"}
    write_new(args.run_root / "diagnostic-results.json", common.canonical_json(result) + b"\n")
    audit("Summarized", args.run_root)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign-root", "actor-root", "run-root", "pwsh"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    for name in ("actor-spec-sha256", "disposition-sha256", "expected-sid"):
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--jobs", type=int, choices=range(1, MAX_JOBS + 1), default=1,
                        help="maximum judges in each independently audited cohort (default: 1)")
    try:
        score(parser.parse_args())
    except (common.HarnessError, PreparationError, OSError, ValueError, TypeError, KeyError,
            subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        print(f"Gather Context diagnostic judging stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
