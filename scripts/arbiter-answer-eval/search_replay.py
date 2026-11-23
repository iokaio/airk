# SPDX-License-Identifier: Apache-2.0
"""Replay the recorded Search-path query sequences without a session, to separate a search switch from session variance.

For each named series and each scenario, the script takes the Search result's first session
(``R/sessions/<series>/<scenario>/search``) and re-issues every counted ``airk_search`` call
with the arguments the wrapper sent, in the recorded order, against the live lifetime. It then
computes coverage (rule 2 by default) over the replayed results only. The recorded coverage of
the same calls (from the session's own result files) is computed alongside for reference.

The lifetime's search switch (``handle.json`` ``search``: ``search_v2`` or unset) is recorded
as the arm. Run it once on a lifetime of each switch over the same publication, then
``--combine`` the two outputs into one table per scenario: series x arm.

``python -I -B search_replay.py --series baseline,cs2-airk --repos airk --out <R>/change-set-2/search-replay/<arm>``
``python -I -B search_replay.py --combine <dir-a> <dir-b> --out <file>``
"""

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
import evidence  # noqa: E402
import replay  # noqa: E402

SCHEMA = "airk.private.answer-eval-search-replay/1"


def search_turns(session_dir: Path) -> list[dict]:
    return [t for t in common.read_jsonl(session_dir / "wrapper" / "turns.jsonl")
            if t.get("turn") and t.get("tool") == "airk_search" and not t.get("rejected")]


def replay_one(session_dir: Path, scenario: dict, handle, out: Path, rule: int, timeout_s: float,
               backend=None) -> dict:
    turns = search_turns(session_dir)
    rows, recorded = [], []
    for number, turn in enumerate(turns, start=1):
        args = turn.get("sent_arguments") or turn["arguments"]
        row = replay.issue(handle, scenario["repository"], "airk_search", args, out, f"call-{number:02d}", timeout_s,
                           backend)
        row["turn"] = number
        rows.append(row)
        recorded.append({"turn": number, "tool": "airk_search", "ok": bool(turn.get("ok")),
                         "result_path": turn.get("result_path"), "bytes": turn.get("bytes"),
                         "elapsed_ms": turn.get("elapsed_ms")})
    live = coverage.compute(scenario, rows, rule=rule)
    then = coverage.compute(scenario, recorded, rule=rule)
    return {"scenario_id": scenario["id"], "session_dir": str(session_dir), "calls": len(rows),
            "ok": sum(1 for r in rows if r["ok"]), "essential_total": live["essential_total"],
            "essential_covered": live["essential_covered"], "recorded_essential_covered": then["essential_covered"],
            "essential_uncovered": live["essential_uncovered"], "recorded_essential_uncovered": then["essential_uncovered"],
            "covered": live["covered_any"], "recorded_covered": then["covered_any"],
            "elapsed_ms_mean": int(sum(r["elapsed_ms"] or 0 for r in rows) / len(rows)) if rows else 0,
            "rows": rows}


def run(args) -> int:
    handle = common.load_handle(common.resolve_handle(args.handle, common.DEFAULT_R / "fixture"))
    raw = common.read_json(handle.path)
    arm = raw.get("search") or "default"
    repos = args.repos.split(",")
    scenarios = {s["id"]: s for s in map(common.load_scenario, common.scenario_files(args.scenarios_dir, repos,
                                                                                    list(common.SLOTS)))}
    report = {"schema": SCHEMA, "generated": common.now_iso(), "arm": arm, "lifetime": raw.get("lifetime"),
              "image": (raw.get("image") or {}).get("label"), "handle_sha256": handle.sha256, "rule": args.rule,
              "tokens_start": {}, "tokens_end": {}, "series": {}}
    import settled  # noqa: PLC0415 - only the live mode needs it
    for repo in repos:
        report["tokens_start"][repo] = settled.snapshot(handle, repo).get("publication_token")
    for series in args.series.split(","):
        entries = []
        for sid, scenario in sorted(scenarios.items()):
            session_dir = args.r_root / "sessions" / series / sid / "search"
            if not (session_dir / "wrapper" / "turns.jsonl").is_file():
                continue
            entries.append(replay_one(session_dir, scenario, handle, args.out / series / sid, args.rule,
                                      args.call_timeout))
            e = entries[-1]
            print(f"{arm} {series} {sid}: {e['essential_covered']}/{e['essential_total']} "
                  f"(recorded {e['recorded_essential_covered']}), {e['calls']} calls", flush=True)
        report["series"][series] = entries
    for repo in repos:
        report["tokens_end"][repo] = settled.snapshot(handle, repo).get("publication_token")
    report["tokens_unchanged"] = all(report["tokens_start"][r] and report["tokens_start"][r] == report["tokens_end"][r]
                                     for r in repos)
    common.write_json(args.out / "search-replay.json", report)
    return 0 if report["tokens_unchanged"] else 2


def combine(dirs: list[Path]) -> dict:
    reports = [common.read_json(d / "search-replay.json") for d in dirs]
    tokens = {json.dumps(r["tokens_start"], sort_keys=True) for r in reports}
    table: dict[str, dict] = {}
    for report in reports:
        for series, entries in report["series"].items():
            for e in entries:
                row = table.setdefault(e["scenario_id"], {"essential_total": e["essential_total"]})
                row[f"{series}@{report['arm']}"] = e["essential_covered"]
                row[f"{series}@recorded"] = e["recorded_essential_covered"]
    cells = sorted({k for row in table.values() for k in row if k != "essential_total"})
    totals = {c: sum(row.get(c, 0) for row in table.values()) for c in cells}
    return {"schema": SCHEMA + "+combined", "generated": common.now_iso(), "arms": [r["arm"] for r in reports],
            "lifetimes": [r["lifetime"] for r in reports], "same_publication": len(tokens) == 1,
            "tokens": reports[0]["tokens_start"], "rule": reports[0]["rule"], "scenarios": table, "totals": totals}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--series", default="baseline,cs2-airk")
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    parser.add_argument("--rule", type=int, default=evidence.COVERAGE_RULE)
    parser.add_argument("--call-timeout", type=float, default=300.0)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--combine", nargs=2, type=Path, metavar="DIR")
    args = parser.parse_args(argv)
    if args.combine:
        result = combine(args.combine)
        common.write_json(args.out, result)
        cells = sorted({k for row in result["scenarios"].values() for k in row if k != "essential_total"})
        print("| scenario | of | " + " | ".join(cells) + " |")
        for sid, row in sorted(result["scenarios"].items()):
            print(f"| {sid} | {row['essential_total']} | " + " | ".join(str(row.get(c, "-")) for c in cells) + " |")
        print("| total | | " + " | ".join(str(result["totals"].get(c, "-")) for c in cells) + " |")
        print(json.dumps({"same_publication": result["same_publication"], "arms": result["arms"]}))
        return 0
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
