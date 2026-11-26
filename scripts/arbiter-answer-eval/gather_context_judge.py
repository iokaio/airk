# SPDX-License-Identifier: Apache-2.0
"""Blind-score closed Gather Context diagnostic actors with the existing rubric."""
from __future__ import annotations

import argparse
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
    spec = {"schema": "airk.private.gather-context-diagnostic-judge/1", "actor_spec_sha256": actor_sha,
            "judge_model": MODEL, "effort": EFFORT, "rubric_sha256": common.sha256_bytes(judge.JUDGE_SYSTEM.encode()),
            "schema_sha256": common.sha256_bytes(common.canonical_json(schema)), "judges": ("j1", "j2"),
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
    for case, arm, scenario, packet, measured, record in packets:
        decisions = []
        for trial in spec["judges"]:
            directory = args.run_root / f"{packet['result']}-{trial}"
            audit("Prepare", directory)
            audit("Before", directory)
            started = time.monotonic()
            try:
                result, stream, stderr, observed = codex_isolation_probe.run_app_server(
                    run_session.codex_command(codex, directory), run_session.codex_child_env(codex, directory),
                    directory / "work", model=MODEL, effort=EFFORT, instruction=judge.packet_prompt(packet),
                    tool_catalog=[], clean_home=True, output_schema=schema, base_instructions=judge.JUDGE_SYSTEM,
                    timeout=900, require_chatgpt=True,
                )
            finally:
                audit("After", directory)
            elapsed = int((time.monotonic() - started) * 1000)
            verdict, _, problems = judge.validate_codex_judge(result, observed, MODEL, scenario)
            paths = {"packet.json": common.canonical_json(packet), "stream.jsonl": stream, "stderr.txt": stderr}
            receipt = {"judgment_spec_sha256": frozen_sha, "trial": trial, "actor": provenance, "observed": observed,
                       "verdict": verdict, "problems": problems, "elapsed_ms": elapsed, "exit_code": result,
                       "files": {name: common.sha256_bytes(content) for name, content in paths.items()}}
            for name, content in paths.items():
                write_new(directory / name, content)
            write_new(directory / "judge.json", common.canonical_json(receipt) + b"\n")
            audit("After", directory)
            require(not problems, f"diagnostic judge {packet['result']}/{trial} did not settle")
            decisions.append(grade.grade_one(measured, verdict, "complete"))
        resolved = grade.adjudicate(*decisions, None)
        result = {"id": case, "arm": arm, "result": packet["result"], "adjudication": resolved,
                  "evidence_turn": measured["all_essential_turn"], "coverage": measured,
                  "tool_calls": record["tool_calls"], "same_case_independent_judges": decisions}
        results.append(result)
        print(f"{case} {arm} {resolved['source']} grade {resolved['grade']} · "
              f"{measured['essential_covered']}/{measured['essential_total']} spans", flush=True)
    audit("Summarize", args.run_root)
    result = {"spec_sha256": frozen_sha, "actor_spec_sha256": actor_sha, "results": results}
    write_new(args.run_root / "diagnostic-results.json", common.canonical_json(result) + b"\n")
    audit("Summarized", args.run_root)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign-root", "actor-root", "run-root", "pwsh"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    for name in ("actor-spec-sha256", "disposition-sha256", "expected-sid"):
        parser.add_argument(f"--{name}", required=True)
    try:
        score(parser.parse_args())
    except (common.HarnessError, PreparationError, OSError, ValueError, TypeError, KeyError,
            subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        print(f"Gather Context diagnostic judging stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
