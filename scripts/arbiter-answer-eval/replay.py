# SPDX-License-Identifier: Apache-2.0
"""Replay Answer calls against a live handle and measure coverage (plan 5.2, steps 4-5).

Two modes:

* ``--sessions <dir>``: for every session under ``<dir>`` (a series folder of
  ``R/sessions``), re-run its recorded first ``airk_answer`` call (with
  ``--all-calls``, every recorded ``airk_answer`` call) with the arguments the
  wrapper sent, optionally overridden by ``--answer-defaults``. Coverage of
  the scenario notes is computed over the replayed calls, and the first
  reply's evidence digest is compared with the recorded one.
* ``--canonical``: for every scenario, issue the canonical first call of a path
  (the instruction verbatim as question/query/text, the wrapper's forced
  arguments) and record coverage and digest. run_pass uses this for the
  session-reuse rule (identical evidence in identical order => reuse) and for
  the CPU/GPU device-pair check.

Writes ``replay.json`` and raw results under ``--out``.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import evidence  # noqa: E402

REPLAY_SCHEMA = "airk.private.answer-eval-replay/1"


def canonical_first_call(path: str, instruction: str, answer_defaults: dict | None = None) -> tuple[str, dict]:
    tool = common.PATH_TOOLS[path][0]
    args = {common.PRIMARY_ARG[tool]: instruction}
    if tool == "airk_answer":
        budget = dict(common.ANSWER_BUDGET_DEFAULT)
        extra = dict(answer_defaults or {})
        budget.update(extra.pop("budget", {}) or {})
        args.update({"schema": common.ANSWER_SCHEMA, "budget": budget, **extra})
    elif tool == "airk_parse":
        args.update({"mode": "execute", "schema": common.PARSE_SCHEMA})
    return tool, args


def apply_defaults(tool: str, sent: dict, answer_defaults: dict | None) -> dict:
    if tool != "airk_answer" or not answer_defaults:
        return dict(sent)
    args = dict(sent)
    extra = dict(answer_defaults)
    budget = dict(args.get("budget") or {})
    budget.update(extra.pop("budget", {}) or {})
    args.update({"budget": budget, **extra})
    return args


def issue(handle: common.Handle, repo: str, tool: str, args: dict, out: Path, stem: str,
          timeout_s: float, backend=None) -> dict:
    # Same environment as an agent's tool call (decision 2): the product's default response mode.
    call = backend or (lambda t, a: common.call_tool(handle, t, repo, a, timeout_s, strip_env=common.AGENT_STRIPPED_ENV))
    outcome = call(tool, args)
    retries = 0
    while retries < 8 and common.typed_retry(outcome.envelope)[0]:  # the same typed-retry rule as the wrapper
        time.sleep(min(max(common.typed_retry(outcome.envelope)[1], 25) * (2 ** retries), 2000) / 1000)
        retries += 1
        outcome = call(tool, args)
    out.mkdir(parents=True, exist_ok=True)
    raw = out / f"{stem}.stdout.json"
    common.write_bytes(raw, outcome.stdout)
    body = common.raw_body(outcome.stdout) if outcome.ok else None
    return {"tool": tool, "arguments": args, "ok": outcome.ok, "error": outcome.error, "elapsed_ms": outcome.elapsed_ms,
            "bytes": len(common.envelope_text(outcome.envelope).encode("utf-8")) if outcome.envelope else 0,
            "result_path": str(raw), "result_sha256": common.sha256_bytes(outcome.stdout),
            "digest": evidence.evidence_digest(body) if body is not None else None,
            "response_mode": common.effective_response_mode(handle, args, common.AGENT_STRIPPED_ENV),
            "typed_retries": retries}


def replay_session(session_dir: Path, scenario: dict, handle: common.Handle, out: Path, *, all_calls: bool,
                   answer_defaults: dict | None, timeout_s: float, backend=None) -> dict:
    turns = [t for t in common.read_jsonl(Path(session_dir) / "wrapper" / "turns.jsonl")
             if t.get("turn") and t.get("tool") == "airk_answer"]
    if not all_calls:
        turns = turns[:1]
    rows = []
    for number, turn in enumerate(turns, start=1):
        args = apply_defaults("airk_answer", turn.get("sent_arguments") or turn["arguments"], answer_defaults)
        row = issue(handle, scenario["repository"], "airk_answer", args, out, f"call-{number:02d}", timeout_s, backend)
        row.update(turn=number, recorded_turn=turn["turn"])
        recorded = Path(turn["result_path"]) if turn.get("result_path") else None
        if recorded and recorded.is_file():
            body = common.raw_body(recorded.read_bytes())
            row["recorded_digest"] = evidence.evidence_digest(body) if body is not None else None
            row["same_evidence"] = row["recorded_digest"] == row["digest"]
        rows.append(row)
    cov = coverage.compute(scenario, rows)
    return {"session_dir": str(session_dir), "scenario_id": scenario["id"], "calls": rows, "coverage": cov,
            "first_digest": rows[0]["digest"] if rows else None}


def replay_canonical(scenario: dict, path: str, handle: common.Handle, out: Path, *, answer_defaults: dict | None,
                     timeout_s: float, backend=None) -> dict:
    tool, args = canonical_first_call(path, scenario["instruction"], answer_defaults)
    row = issue(handle, scenario["repository"], tool, args, out, "canonical", timeout_s, backend)
    row["turn"] = 1
    cov = coverage.compute(scenario, [row])
    return {"scenario_id": scenario["id"], "path": path, "call": row, "coverage": cov, "first_digest": row["digest"]}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--sessions", type=Path, help="series folder under R/sessions")
    mode.add_argument("--canonical", metavar="PATH", choices=sorted(common.PATH_TOOLS))
    parser.add_argument("--all-calls", action="store_true")
    parser.add_argument("--answer-defaults")
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--slots", default=",".join(common.SLOTS))
    parser.add_argument("--call-timeout", type=float, default=300.0)
    parser.add_argument("--accept-queryable", action="store_true", help="pilot only (see common.load_handle)")
    args = parser.parse_args(argv)
    handle = common.load_handle(args.handle, accept_queryable=args.accept_queryable)
    defaults = json.loads(args.answer_defaults) if args.answer_defaults else None
    repos, slots = args.repos.split(","), args.slots.split(",")
    scenarios = {s["id"]: s for s in map(common.load_scenario, common.scenario_files(args.scenarios_dir, repos, slots))}
    entries = []
    if args.canonical:
        for sid, scenario in sorted(scenarios.items()):
            entries.append(replay_canonical(scenario, args.canonical, handle, args.out / sid,
                                            answer_defaults=defaults, timeout_s=args.call_timeout))
    else:
        for record_path in sorted(args.sessions.glob("*/*/session.json")):
            record = common.read_json(record_path)
            scenario = scenarios.get(record.get("scenario_id"))
            if scenario is None or record.get("path") not in common.ANSWER_PATHS or record.get("status") != "complete":
                continue
            name = f"{record_path.parent.parent.name}/{record_path.parent.name}"
            entries.append(replay_session(record_path.parent, scenario, handle, args.out / name,
                                          all_calls=args.all_calls, answer_defaults=defaults,
                                          timeout_s=args.call_timeout))
    report = {"schema": REPLAY_SCHEMA, "generated": common.now_iso(), "handle": str(args.handle),
              "handle_readiness": handle.readiness(),
              "handle_sha256": handle.sha256, "answer_defaults": defaults, "mode": "canonical" if args.canonical else "sessions",
              "entries": entries}
    common.write_json(args.out / "replay.json", report)
    for entry in entries:
        cov = entry["coverage"]
        print(f"{entry['scenario_id']}: yield {cov['first_call_yield']:.2f}, essential "
              f"{cov['essential_covered']}/{cov['essential_total']}, digest {(entry['first_digest'] or '-')[:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
