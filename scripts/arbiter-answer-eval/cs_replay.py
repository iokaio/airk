# SPDX-License-Identifier: Apache-2.0
"""Replay a baseline's recorded first Answer calls under a change set, and compare coverage (plan 5.2 step 4).

For each completed session of ``--series`` whose result is one of the
``--configurations`` (for example ``noModel``, ``focus-cpu``, ``compact-cpu``),
the first recorded ``airk_answer`` call is re-issued against the live handle.
It uses the arguments the wrapper sent, the agents' environment and the same
typed-retry rule. The script then compares:

* first-call coverage of essential notes (coverage rule 2), against the
  recorded first reply scored under rule 2;
* the evidence digest (same or changed);
* the call time.

The live stages must match ``--stages`` (focus/context profile names, as
``fixture.py stage`` names them). Calls run with bounded parallelism. Output:
``<out>/<label>/cs-replay.json`` and the raw replies.

``python -I -B cs_replay.py --series baseline-bookbuilder-r2 --configurations noModel --label all-noModel --out <R>/change-set-1/replays/bookbuilder``
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import sys
from pathlib import Path
from statistics import mean

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import evidence  # noqa: E402
import replay  # noqa: E402
import settled  # noqa: E402


def recorded_first(session_dir: Path) -> dict | None:
    for row in common.read_jsonl(session_dir / "wrapper" / "turns.jsonl"):
        if row.get("turn") and row.get("tool") == "airk_answer":
            return row
    return None


def one(scenario: dict, session_dir: Path, handle: common.Handle, out: Path, timeout_s: float,
        answer_defaults: dict | None = None) -> dict:
    first = recorded_first(session_dir)
    if first is None:
        return {"session_dir": str(session_dir), "scenario_id": scenario["id"], "skipped": "no recorded Answer call"}
    recorded_body = common.raw_body(Path(first["result_path"]).read_bytes()) if first.get("ok") else None
    recorded_cov = coverage.compute(scenario, [dict(first, turn=1)]) if recorded_body is not None else None
    args = replay.apply_defaults("airk_answer", first.get("sent_arguments") or first["arguments"], answer_defaults)
    row = replay.issue(handle, scenario["repository"], "airk_answer", args, out, "first-answer", timeout_s)
    row["turn"] = 1
    new_cov = coverage.compute(scenario, [row])
    return {"scenario_id": scenario["id"], "session_dir": str(session_dir), "ok": row["ok"], "error": row["error"],
            "elapsed_ms": row["elapsed_ms"], "recorded_elapsed_ms": first.get("elapsed_ms"),
            "bytes": row["bytes"], "recorded_bytes": first.get("bytes"), "typed_retries": row.get("typed_retries"),
            "digest": row["digest"],
            "recorded_digest": evidence.evidence_digest(recorded_body) if recorded_body is not None else None,
            "essential_covered": new_cov["essential_covered"], "essential_total": new_cov["essential_total"],
            "recorded_essential_covered": recorded_cov["essential_covered"] if recorded_cov else None,
            "covered_any": new_cov["covered_any"],
            "recorded_covered_any": recorded_cov["covered_any"] if recorded_cov else None}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--series", required=True)
    parser.add_argument("--configurations", required=True, help="result names, e.g. noModel,focus-cpu")
    parser.add_argument("--label", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--stages", default="{}", help='JSON {"focus": profile, "context": profile}')
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--answer-defaults", help="JSON merged into each replayed call (e.g. a change set's budget)")
    parser.add_argument("--call-timeout", type=float, default=300.0)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    args = parser.parse_args(argv)
    handle = common.load_handle(common.resolve_handle(args.handle, common.DEFAULT_R / "fixture"))
    wanted = {"focus": None, "context": None, **json.loads(args.stages)}
    live = handle.stages()
    common.require(live == wanted, f"live stages {live} differ from --stages {wanted}")
    scenarios = {s["id"]: s for s in map(common.load_scenario, common.scenario_files(args.scenarios_dir))}
    names = set(args.configurations.split(","))
    repos = set(args.repos.split(","))
    jobs = []
    for record_path in sorted((common.DEFAULT_R / "sessions" / args.series).glob("*/*/session.json")):
        record = common.read_json(record_path)
        name = record_path.parent.name
        if name in names and record.get("status") == "complete" and record.get("repo") in repos:
            jobs.append((scenarios[record["scenario_id"]], record_path.parent,
                         args.out / args.label / f"{record_path.parent.parent.name}" / name))
    # A result the pass reused (identical evidence to an earlier session) has no session folder of its own;
    # its recorded first reply is the source session's, so the source session stands in for it.
    reuse_path = common.DEFAULT_R / "passes" / args.series / "reuse.json"
    reuse = common.read_json(reuse_path) if reuse_path.is_file() else {}
    have = {(job[0]["id"], job[1].name) for job in jobs}
    for key, source in sorted(reuse.items()):
        scenario_dir, name = key.split("/", 1)
        scenario = next((s for s in scenarios.values() if f"{s['repository']}-{s['slot']}" == scenario_dir), None)
        if name not in names or scenario is None or scenario["repository"] not in repos or \
                (scenario["id"], name) in have or source.get("series") != args.series:
            continue
        jobs.append((scenario, Path(source["session_dir"]), args.out / args.label / scenario_dir / name))
    out_dir = args.out / args.label
    start = {r: settled.snapshot(handle, r) for r in sorted(repos)}
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        defaults = json.loads(args.answer_defaults) if args.answer_defaults else None
        rows = list(pool.map(lambda j: {**one(j[0], j[1], handle, j[2], args.call_timeout, defaults),
                                        "result": j[1].name}, jobs))
    end = {r: settled.snapshot(handle, r) for r in sorted(repos)}
    ok = [r for r in rows if r.get("ok")]
    report = {"schema": "airk.private.answer-eval-cs-replay/1", "generated": common.now_iso(), "label": args.label,
              "series": args.series, "configurations": sorted(names), "stages": live, "handle": str(handle.path),
              "lifetime": handle.path.parent.name, "readiness": handle.readiness(),
              "tokens": {r: settled.compare(start[r], end[r]) for r in start},
              "calls": len(rows), "ok": len(ok),
              "essential_covered": sum(r["essential_covered"] for r in ok),
              "recorded_essential_covered": sum(r["recorded_essential_covered"] or 0 for r in ok),
              "essential_total": sum(r["essential_total"] for r in ok),
              "same_evidence": sum(r["digest"] == r["recorded_digest"] for r in ok),
              "elapsed_ms_mean": round(mean(r["elapsed_ms"] for r in ok)) if ok else None,
              "recorded_elapsed_ms_mean": round(mean(r["recorded_elapsed_ms"] or 0 for r in ok)) if ok else None,
              "rows": rows}
    common.write_json(out_dir / "cs-replay.json", report)
    print(json.dumps({k: v for k, v in report.items() if k not in ("rows", "readiness")}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
