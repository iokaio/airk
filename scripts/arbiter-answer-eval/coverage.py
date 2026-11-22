# SPDX-License-Identifier: Apache-2.0
"""Coverage of a scenario's reference notes over one session's turn log (formats.md).

For each counted turn: the notes its result covers, the notes newly covered,
and the cumulative essential coverage. Summaries: first-call yield (share of
essential notes covered by turn 1), the turn at which every essential note is
covered (or null), total bytes returned to the agent and total tool time.

``python -I -B coverage.py --scenario S.json --session-dir DIR`` writes
``DIR/coverage.json``.
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
import evidence  # noqa: E402

COVERAGE_SCHEMA = "airk.private.answer-eval-coverage/1"


def compute(scenario: dict, turns: list[dict], count_fact_rows: bool = False,
            body_loader=None, rule: int = evidence.COVERAGE_RULE) -> dict:
    """Coverage over counted turns. ``body_loader(row)`` returns the parsed result body."""
    notes = scenario["notes"]
    essential = [n["id"] for n in notes if n["mark"] == "essential"]
    loader = body_loader or (lambda row: common.raw_body(Path(row["result_path"]).read_bytes())
                             if row.get("result_path") and Path(row["result_path"]).is_file() else None)
    counted = sorted((t for t in turns if t.get("turn") and not t.get("rejected")), key=lambda t: t["turn"])
    seen: set[str] = set()
    per_turn, all_turn = [], None
    total_bytes = total_ms = 0
    for row in counted:
        body = loader(row) if row.get("ok") else None
        spans = evidence.extract(body) if body is not None else []
        covered = evidence.covered_notes(spans, notes, count_fact_rows, rule)
        new = [n for n in covered if n not in seen]
        seen.update(covered)
        total_bytes += int(row.get("bytes") or 0)
        total_ms += int(row.get("elapsed_ms") or 0)
        essential_so_far = [n for n in essential if n in seen]
        per_turn.append({"turn": row["turn"], "tool": row.get("tool"), "ok": bool(row.get("ok")),
                         "bytes": row.get("bytes"), "elapsed_ms": row.get("elapsed_ms"),
                         "covered": covered, "new": new, "essential_cumulative": essential_so_far})
        if all_turn is None and len(essential_so_far) == len(essential):
            all_turn = row["turn"]
    first = per_turn[0]["covered"] if per_turn else []
    first_yield = (sum(1 for n in essential if n in first) / len(essential)) if essential else 0.0
    return {"schema": COVERAGE_SCHEMA, "scenario_id": scenario.get("id"), "scenario_sha256": scenario.get("_sha256"),
            "count_fact_rows": count_fact_rows, "coverage_rule": rule, "essential": essential,
            "helpful": [n["id"] for n in notes if n["mark"] != "essential"],
            "turns": per_turn, "counted_turns": len(per_turn),
            "first_call_yield": round(first_yield, 4),
            "essential_covered": sum(1 for n in essential if n in seen), "essential_total": len(essential),
            "essential_uncovered": [n for n in essential if n not in seen],
            "all_essential_turn": all_turn, "covered_any": sorted(seen),
            "total_bytes": total_bytes, "total_elapsed_ms": total_ms}


def for_session(scenario: dict, directory: Path, count_fact_rows: bool = False, write: bool = True,
                rule: int = evidence.COVERAGE_RULE, name: str = "coverage.json") -> dict:
    directory = Path(directory)
    result = compute(scenario, common.read_jsonl(directory / "wrapper" / "turns.jsonl"), count_fact_rows,
                     rule=rule)
    result["session_dir"] = str(directory)
    if write:
        common.write_json(directory / name, result)
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenario", type=Path, required=True)
    parser.add_argument("--session-dir", type=Path, required=True)
    parser.add_argument("--rule", type=int, choices=[1, 2], default=evidence.COVERAGE_RULE)
    parser.add_argument("--name", default="coverage.json", help="output file name in the session folder")
    parser.add_argument("--count-fact-rows", action="store_true",
                        help="let outline/symbol rows (a name at a line range) cover notes by overlap")
    args = parser.parse_args(argv)
    result = for_session(common.load_scenario(args.scenario), args.session_dir, args.count_fact_rows,
                         rule=args.rule, name=args.name)
    print(json.dumps({k: result[k] for k in ("first_call_yield", "essential_covered", "essential_total",
                                             "all_essential_turn", "total_bytes", "total_elapsed_ms")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
