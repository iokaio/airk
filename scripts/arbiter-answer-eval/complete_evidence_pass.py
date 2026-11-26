# SPDX-License-Identifier: Apache-2.0
"""Generate a private complete-evidence spec and authenticate its gate disposition."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import grade  # noqa: E402
import judge  # noqa: E402
import prompts_freeze  # noqa: E402
import run_session  # noqa: E402

SCHEMA = "airk.private.answer-eval-ceiling-disposition/1"
DIAGNOSTIC = "diagnostic-sol"


def treatments(manifest: dict) -> dict:
    return {
        key: run_session.CompleteEvidenceInput(
            Path(pin["manifest"]), pin["manifest_sha256"], Path(pin["source_root"]), pin["publication_token"],
        )
        for key, pin in manifest["complete_evidence"].items()
    }


def diagnostic_verdict(row: dict, scenario: dict, session: dict, scoring: list[dict], path: Path,
                       provenance: dict) -> tuple[dict, dict]:
    directory = Path(row["session_dir"])
    key = row["key"]
    record = common.read_json(path)
    common.require(record.get("schema") == judge.VERDICT_SCHEMA and record.get("key") == key
                   and record.get("judge") == DIAGNOSTIC and record.get("status") == "valid"
                   and record.get("provider") == "codex" and record.get("codex") == provenance
                   and record.get("model_requested") == "gpt-6.2-sol"
                   and record.get("effort_requested") == "medium"
                   and Path(record.get("session_dir", "")).resolve() == directory.resolve()
                   and not record.get("problems"), f"{key}: invalid Sol diagnostic receipt")
    packet_path, output_path = path.with_name("packet.json"), path.with_name("output.json")
    grade.require_file_digest(packet_path, record.get("packet_sha256"), key)
    grade.require_file_digest(output_path, record.get("output_sha256"), key)
    grade.require_file_digest(path.with_name("app-server.jsonl"), record.get("app_server_sha256"), key)
    common.require(common.read_json(packet_path) == judge.build_packet(scenario, directory, key),
                   f"{key}: Sol diagnostic scored a different packet")
    output = common.read_json(output_path)
    verdict, usage, problems = judge.validate_codex_judge(record.get("exit_code"), output, "gpt-6.2-sol", scenario)
    thread = record.get("session_id")
    common.require(not problems and verdict == record.get("verdict") and usage == record.get("usage")
                   and thread == output.get("thread_id")
                   and thread not in [session["session_id"], *(verdict["session_id"] for verdict in scoring)]
                   and record.get("codex_turn_id") == output.get("turn_id"),
                   f"{key}: diagnostic is not a distinct valid Sol verdict")
    return verdict, record


def summarize(cases: list[dict]) -> dict:
    roster = prompts_freeze.scenario_roster()
    identities = {case["scenario_id"] for case in cases}
    common.require(len(cases) == len(roster) and identities == roster, "invalid ceiling case roster")
    threads = [thread for case in cases for thread in case.get("threads", [])]
    crossed = {thread for thread in threads if threads.count(thread) > 1}
    cases = [{**case, "status": "invalid", "reason": "actor or judge thread reused across cases",
              "correct_finish": False} if crossed.intersection(case.get("threads", [])) else case for case in cases]
    eligible = all(case["status"] == "eligible" for case in cases)
    counts = {
        repo: sum(case["status"] == "eligible" and case["correct_finish"]
                  for case in cases if case["repo"] == repo)
        for repo in common.REPOS
    }
    total = sum(counts.values())
    passed = eligible and total >= 12 and min(counts.values()) >= 5
    return {"status": "passed" if passed else "below-threshold" if eligible else "incomplete",
            "correct_finishes": total, "correct_finishes_by_repo": counts, "cases": cases}


def score_case(run, row: dict, scenario: dict, adjudications: dict) -> dict:
    key = row["key"]
    directory = Path(row["session_dir"])
    case = {"scenario_id": scenario["id"], "repo": scenario["repository"], "key": key,
            "session": str(directory), "status": "missing", "correct_finish": False}
    receipt_path = directory / "session.json"
    if not receipt_path.is_file():
        return {**case, "reason": "missing actor receipt"}
    try:
        session, _ = run.require_terminal_session(row, scenario, None)
        common.require(session["status"] == "complete", f"actor status {session['status']}")
        treatment_receipt = run.require_ceiling_treatment(scenario, directory, session)
        case["first_dispatch"] = treatment_receipt
        recorded = coverage.for_session(scenario, directory, write=False)
        common.require(recorded.get("all_essential_turn") == 1, "dispatched reply misses an essential span")
        entry = run.spec["results"][0]
        common.require(type(session.get("elapsed_ms")) is int
                       and 0 <= session["elapsed_ms"] <= int(float(entry.get("session_timeout", 1800)) * 1000)
                       and recorded["counted_turns"] <= common.TURN_CAP, "actor exceeds the frozen cap")
        paths = [Path(path) for path in row["judges"]]
        diagnostic = run.pass_dir / "judging" / key / DIAGNOSTIC / "verdict.json"
        case["judges"] = [str(path) for path in [*paths, diagnostic]]
        if not all(path.is_file() for path in [*paths, diagnostic]):
            return {**case, "reason": "missing scoring or diagnostic judge receipt"}
        scoring = grade.codex_verdicts(row, scenario, session, run.codex_identity)
        scorer_receipts = [common.read_json(path) for path in paths]
        probe, diagnostic_receipt = diagnostic_verdict(row, scenario, session, scorer_receipts, diagnostic, run.codex_identity)
        agreement = [grade.grade_one(recorded, probe, "complete")["grade"] ==
                     grade.grade_one(recorded, verdict, "complete")["grade"] for verdict in scoring]
        verdicts = [grade.grade_one(recorded, verdict, "complete") for verdict in scoring]
        override = adjudications.get(key)
        if override is not None:
            common.require(abs(verdicts[0]["grade"] - verdicts[1]["grade"]) > 1
                           and type(override.get("grade")) is int and 0 <= override["grade"] <= 4
                           and type(override.get("correct_finish")) is bool
                           and (override["grade"] == 4) == override["correct_finish"]
                           and isinstance(override.get("reason"), str) and override["reason"].strip(),
                           "invalid ceiling adjudication")
        resolved = grade.adjudicate(*verdicts, override)
        case.update({
            "scoring_grades": [result["grade"] for result in verdicts],
            "adjudication": resolved,
            "diagnostic_grade_agreement": agreement,
            "threads": [session["session_id"], *(receipt["session_id"] for receipt in scorer_receipts),
                        diagnostic_receipt["session_id"]],
            "receipt_hashes": {"session": common.sha256_file(receipt_path),
                               **{path.parent.name: common.sha256_file(path) for path in [*paths, diagnostic]}},
        })
        if resolved["source"] in ("pending", "single-judge"):
            return {**case, "status": "unresolved", "reason": "scoring judge disagreement"}
        return {**case, "status": "eligible", "correct_finish": resolved["correct_finish"] is True}
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        return {**case, "status": "invalid", "reason": str(error)}


def disposition(run, spec_path: Path) -> dict:
    common.require(run.ceiling and spec_path.is_file(), "authenticated ceiling spec required")
    cases = {scenario["id"]: scenario for scenario in run.scenarios}
    rows = run.index(write=False)["results"]
    common.require(len(rows) == len(cases) and all(row["sample"] == 1 for row in rows),
                   "complete-evidence pass must have one sample per case")
    adjudications = run.adjudications()
    common.require(set(adjudications) <= {row["key"] for row in rows}, "unknown ceiling adjudication")
    scores = [score_case(run, row, cases[row["scenario_id"]], adjudications) for row in rows]
    summary = summarize(scores)
    settled_log = run.pass_dir / "settled.jsonl"
    observations = common.read_jsonl(settled_log) if settled_log.is_file() else []
    observed = {(row.get("repo"), row.get("phase"), row.get("publication_token"))
                for row in observations if row.get("settled")}
    expected = {(repo, phase, token) for repo, token in run.ceiling_publications.items()
                for phase in ("start", "end")}
    if (run.pass_dir / "INVALID.json").exists() or not expected <= observed:
        summary.update(status="incomplete", settlement="missing or invalid publication receipts")
    if settled_log.is_file():
        summary["settled"] = prompts_freeze.pinned_input(str(settled_log))
    return {"schema": SCHEMA, "series": run.series, "r_root": str(run.r_root.resolve()),
            "spec": prompts_freeze.pinned_input(str(spec_path)), "prompts_freeze": run.prompts,
            "scenario_freeze": run.freeze, **summary}


def require_passing_disposition(pin: dict, normal_manifest: dict) -> dict:
    from run_pass import Pass

    common.require(isinstance(pin, dict) and set(pin) == {"path", "sha256"}
                   and all(isinstance(value, str) and value for value in pin.values()),
                   "normal baseline needs a ceiling disposition pin")
    path = Path(prompts_freeze.pinned_input(pin["path"], pin["sha256"])["path"])
    saved = common.read_json(path)
    common.require(saved.get("schema") == SCHEMA and saved.get("status") == "passed",
                   "complete-evidence experiment requires review before normal evaluation")
    spec_path = Path(prompts_freeze.pinned_input(**saved["spec"])["path"])
    spec = common.read_json(spec_path)
    common.require(spec.get("purpose") == prompts_freeze.CEILING_PURPOSE,
                   "disposition does not belong to a ceiling pass")
    run = Pass(spec, Path(saved["r_root"]), read_only=True)
    ceiling_manifest = common.read_json(Path(spec["prompts_freeze"]))
    matching = ("scenario_freeze", "provider", "runner", "executable", "model_catalog", "tool_catalogs",
                "agent_prompts", "judge_prompt", "judge_schema", "judge_model", "judge_effort",
                "scoring_judges", "answer_budget", "turn_cap", "files")
    common.require(all(ceiling_manifest.get(key) == normal_manifest.get(key) for key in matching),
                   "normal baseline differs from the authenticated ceiling inputs")
    common.require(disposition(run, spec_path) == saved, "ceiling disposition differs from its retained receipts")
    return run.ceiling_publications


def main(argv: list[str] | None = None) -> int:
    from make_loop_spec import PROFILES, stage_a_spec

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--r-root", type=Path, required=True)
    parser.add_argument("--series", required=True)
    parser.add_argument("--codex-freeze", type=Path, required=True)
    parser.add_argument("--codex-home", type=Path, required=True)
    parser.add_argument("--handle", type=Path, required=True)
    args = parser.parse_args(argv)
    root = args.r_root.resolve()
    target = root / "passes" / args.series
    common.require(target.parent == root / "passes" and not target.exists(), "ceiling needs a fresh pass identity")
    manifest = common.read_json(args.codex_freeze)
    common.require(manifest.get("purpose") == prompts_freeze.CEILING_PURPOSE, "ceiling requires its own freeze")
    spec = stage_a_spec(args.series, root, PROFILES, list(common.REPOS), {}, "",
                        freeze_path=args.codex_freeze, codex_home=args.codex_home, handle=args.handle, stage="baseline")
    spec["stage"] = "complete-evidence"
    spec["results"] = [{**entry, "path": "answer"} for entry in spec["results"] if entry.get("arm") == "C"]
    common.write_json(target / "spec.json", spec)
    print(json.dumps({"spec": str(target / "spec.json")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())