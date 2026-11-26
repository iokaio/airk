# SPDX-License-Identifier: Apache-2.0
"""Run fresh Windows Sol diagnostics using ordinary tools or authenticated archived evidence."""
from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys

import common
from gather_context_actor import ARCHIVE_TOOLS, EFFORT, MODEL, run_actor
from gather_context_export import verify_export
from gather_context_replay import (archived_presentation, campaign_pin, freeze_reference,
                                   load_pinned, prepare_spec, read_ceiling, require, source_root, write_new)
from gather_context_sources import TOOLS
from prepare_complete_evidence import PreparationError
from run_session import CodexRunConfig, SYSTEM_PROMPT

ARMS = ("ordinary", "original", "grouped")


def diagnostic(args) -> dict:
    campaign = args.campaign_root.resolve(strict=True)
    run_root = args.run_root.resolve(strict=True)
    require(run_root == args.run_root and run_root.is_relative_to(campaign)
            and run_root != campaign and run_root.is_dir(), "run directory is outside the assigned campaign")
    root = Path(__file__).resolve().parents[2]
    disposition = {"path": "passes/07-windows-complete-evidence-r1/ceiling-disposition.json",
                   "sha256": args.disposition_sha256}
    spec = prepare_spec(campaign, root, disposition)
    history = load_pinned(campaign, disposition, "ceiling disposition")
    historical = load_pinned(campaign, campaign_pin(campaign, **history["spec"]), "archived spec")
    freeze = load_pinned(campaign, spec["scenario_freeze"], "scenario freeze")
    repositories, sessions, archives = read_ceiling(campaign, spec, freeze)
    spec_sha = common.sha256_bytes(common.canonical_json(spec) + b"\n")
    reference = freeze_reference(campaign, root, spec, spec_sha)
    cases = {case["id"]: case for case in reference["cases"]}
    selected = args.case or list(cases)
    require(set(selected) <= set(cases) and len(selected) == len(set(selected)), "unknown or repeated case")
    arms = args.arm or list(ARMS)
    require(len(arms) == len(set(arms)), "repeated arm")
    config = historical["codex_config"]
    exports = {}
    if "ordinary" in arms:
        for repo in sorted({cases[case]["repo"] for case in selected}):
            pin = repositories[repo]
            receipt = load_pinned(campaign, pin["export_receipt"], f"{repo} export receipt")
            export = source_root(campaign, repo, pin)
            exports[repo] = verify_export(export, receipt, git=args.git)
    evidence_limit = historical["answer_defaults"]["budget"]["max_bytes"]
    timeout = historical["results"][0]["session_timeout"]
    diagnostics = {"schema": "airk.private.gather-context-diagnostic/1", "source_spec_sha256": spec_sha,
                   "run_root": str(run_root), "cases": selected, "arms": arms, "exports": exports,
                   "model": MODEL, "effort": EFFORT, "time_limit_seconds": timeout,
                   "reply_evidence_bytes": evidence_limit, "tool_call_limit": common.TURN_CAP,
                   "ordinary_tool_offer": TOOLS, "archive_tool_offer": ARCHIVE_TOOLS,
                   "shared_prompt_sha256": common.sha256_bytes(SYSTEM_PROMPT.encode("utf-8")),
                   "workers": 1, "historical_config": config,
                   "controller_files": {Path(name).name: common.sha256_file(Path(__file__).with_name(name))
                                        for name in ("gather_context_diagnostic.py", "gather_context_actor.py",
                                                     "gather_context_custody.ps1", "gather_context_replay.py",
                                                     "gather_context_sources.py", "gather_context_presentation.py",
                                                     "gather_context_export.py", "codex_isolation_probe.py",
                                                     "run_session.py")}}
    frozen = common.canonical_json(diagnostics) + b"\n"
    frozen_sha = common.sha256_bytes(frozen)
    codex = CodexRunConfig(Path(config["executable"]), config["executable_sha256"],
                           Path(config["model_catalog"]), config["model_catalog_sha256"],
                           Path(config["isolated_home"]), frozen_sha)
    checked_script = Path(__file__).with_name("gather_context_custody.ps1")

    def audit(action: str, directory: Path, home: Path) -> None:
        command = [str(args.pwsh), "-NoProfile", "-File", str(checked_script),
                   "-Action", action, "-Campaign", str(campaign), "-Directory", str(directory),
                   "-IsolatedHome", str(home), "-ExpectedSid", args.expected_sid]
        subprocess.run(command, stdin=subprocess.DEVNULL, check=True, timeout=60)

    audit("Freeze", run_root, codex.isolated_home)
    write_new(run_root / "diagnostic-spec.json", frozen)
    audit("Frozen", run_root, codex.isolated_home)
    def run_case(case_id: str) -> list[dict]:
        case = cases[case_id]
        repo, slot = case["repo"], case["slot"]
        scenario = load_pinned(campaign, {"path": f"scenarios/{repo}/{slot}.json",
                               "sha256": case["scenario_sha256"]}, "development scenario")
        pair = None
        if any(arm != "ordinary" for arm in arms):
            pair = archived_presentation(campaign, case, repositories[repo], sessions[case_id], archives[case_id])
        closed = []
        for arm in arms:
            directory = run_root / f"{case_id}-{arm}"
            audit("Prepare", directory, codex.isolated_home)
            actor_inputs = {}
            if arm == "ordinary":
                actor_inputs["source_root"] = source_root(campaign, repo, repositories[repo])
            else:
                text = pair.original if arm == "original" else pair.grouped
                reply = {"content": [{"type": "text", "text": text.decode("utf-8")}]}
                actor_inputs.update(archived_reply=reply,
                                    archived_reply_sha256=common.sha256_bytes(common.canonical_json(reply)))
            began = common.now_iso()
            print(f"{began} {case_id} {arm} started", flush=True)
            result = run_actor(config=codex, instruction=scenario["instruction"], directory=directory,
                               timeout_s=timeout, arm="ordinary" if arm == "ordinary" else "archived",
                               reply_byte_limit=evidence_limit,
                               audit_workspace=lambda phase, path, home: audit(phase.capitalize(), path, home),
                               write_receipt=write_new, **actor_inputs)
            audit("After", directory, codex.isolated_home)
            row = {"case": case_id, "arm": arm, "status": result["status"], "tool_calls": result["tool_calls"],
                   "text_bytes": result["evidence_text_bytes"], "elapsed_ms": result["elapsed_ms"],
                   "session_sha256": common.sha256_file(directory / "session.json")}
            print(f"{case_id} {arm} {row['status']} · {row['tool_calls']} calls · {row['elapsed_ms']} ms", flush=True)
            closed.append(row)
            cap_exceeded = (result["status"] == "discarded"
                            and result["problems"] == ["Actor exceeded the tool call cap"])
            require(result["status"] in ("complete", "timeout") or cap_exceeded,
                    f"diagnostic actor {case_id}/{arm} stopped; audit its receipt")
        return closed

    outcomes = [row for case in selected for row in run_case(case)]
    result = {"spec_sha256": frozen_sha, "outcomes": outcomes}
    require(len(result["outcomes"]) == len(selected) * len(arms), "incomplete diagnostic actor roster")
    audit("Summarize", run_root, codex.isolated_home)
    write_new(run_root / "diagnostic-results.json", common.canonical_json(result) + b"\n")
    audit("Summarized", run_root, codex.isolated_home)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--campaign-root", required=True, type=Path)
    parser.add_argument("--run-root", required=True, type=Path)
    parser.add_argument("--disposition-sha256", required=True)
    parser.add_argument("--expected-sid", required=True)
    parser.add_argument("--pwsh", required=True, type=Path)
    parser.add_argument("--git", required=True, type=Path)
    parser.add_argument("--case", action="append")
    parser.add_argument("--arm", choices=ARMS, action="append")
    try:
        result = diagnostic(parser.parse_args())
        print(f"Closed {len(result['outcomes'])} diagnostic actor sessions")
    except (common.HarnessError, PreparationError, OSError, ValueError, TypeError, KeyError,
            subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        print(f"Gather Context diagnostic stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
