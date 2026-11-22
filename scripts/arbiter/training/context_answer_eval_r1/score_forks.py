# SPDX-License-Identifier: Apache-2.0
"""Score the oracle fork collector's rows and apply the Context trainer go/no-go (design sections 9-10).

Each branch's delivered packet is scored with the harness coverage rule (``evidence.py``, rule 2,
text spans only, as ``coverage.compute`` does) against the scenario's essential notes. Per decision
point (a call and an ordinal) the report gives the queue-order coverage, the best forced coverage,
the signed best-forced gain and the oracle gain ``max(0, best - queue)`` (the queue order is what an
abstaining chooser gets). Per ordinal, over every call whose queue branch is valid: the oracle
leverage (share of calls with an oracle gain of at least one essential note) and the mean oracle
gain. A call with no Context request at that ordinal counts as a gain of 0.

``--compact-choices`` (JSON ``{call id: {"choice": <hex>} | {"abstained": true}}``) or
``--compact-sessions`` (a pass series; the first Answer reply's ordinal-1 assessment) adds the share
of the oracle gain that provisional Compact captured. The go/no-go thresholds are the design's.

``python -I -B score_forks.py --rows rows.jsonl --out score.json [--compact-sessions <R>/sessions/<series> --compact-configuration compact]``
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from statistics import mean

HERE = Path(__file__).resolve().parent
HARNESS = HERE.parents[2] / "arbiter-answer-eval"
if str(HARNESS) not in sys.path:
    sys.path.insert(0, str(HARNESS))

import common  # noqa: E402
import evidence  # noqa: E402

SCORE_SCHEMA = "airk.private.context-forks-score/1"
ROWS_SCHEMA = "airk.arbiter.context-forks.rows/1"
HEX = re.compile(r"^[0-9a-f]{64}$")

# Design section 10 (context-trainer.md b0dc0d0d...cd76d), adopted by the primary on 22 Nov 2026.
NO_GO_LEVERAGE = 0.10
NO_GO_GAIN = 0.20
NO_ROOM_CAPTURE = 0.70
GO_LEVERAGE = 0.20
GO_GAIN = 0.50
GO_CAPTURE = 0.40


def read_rows(path: Path) -> tuple[dict, list[dict], dict | None]:
    start, calls, complete = None, [], None
    for row in common.read_jsonl(path):
        if row.get("event") == "start":
            common.require(row.get("schema") == ROWS_SCHEMA, f"{path}: foreign rows schema {row.get('schema')}")
            start = row
        elif row.get("event") == "call":
            calls.append(row)
        elif row.get("event") == "complete":
            complete = row
    common.require(start is not None, f"{path}: no start row")
    return start, calls, complete


def essential_covered(packet, scenario: dict) -> list[str]:
    essential = {n["id"] for n in scenario["notes"] if n["mark"] == "essential"}
    spans = evidence.extract(packet) if packet is not None else []
    return [n for n in evidence.covered_notes(spans, scenario["notes"]) if n in essential]


def branch_score(branch: dict, scenario: dict) -> dict:
    valid = bool(branch.get("valid")) and not branch.get("reply_is_error")
    covered = essential_covered(branch.get("packet"), scenario) if valid else []
    stop = ((branch.get("native") or {}).get("stop"))
    return {"branch": branch.get("branch"), "valid": valid, "covered": covered, "count": len(covered),
            "stop": stop, "elapsed_ms": branch.get("elapsed_ms"),
            "read_slot": branch.get("read_slot"),
            "choice": (branch.get("forced") or {}).get("choice"),
            "same_roster_as_queue": branch.get("same_roster_as_queue"),
            "error": branch.get("error") or branch.get("transport_failure")}


def request_at(branch: dict, ordinal: int) -> dict | None:
    return next((r for r in branch.get("requests") or [] if r.get("ordinal") == ordinal), None)


def decision_point(call: dict, scenario: dict, ordinal: int) -> dict:
    queue = branch_score(call["queue"], scenario)
    request = request_at(call["queue"], ordinal)
    forks = [branch_score(f, scenario) for f in call.get("forks") or []
             if (f.get("forced") or {}).get("ordinal") == ordinal]
    usable = [f for f in forks if f["valid"] and f["same_roster_as_queue"] is not False]
    offered = sum(1 for card in (request or {}).get("cards") or [] if card.get("action") == "read")
    point = {"call": call["id"], "scenario_id": call.get("scenario_id"), "ordinal": ordinal,
             "queue_valid": queue["valid"], "has_request": request is not None, "offered_reads": offered,
             "forks_valid": len(usable), "forks_invalid": len(forks) - len(usable),
             "queue_coverage": queue["count"], "queue_covered": queue["covered"],
             "best_forced_coverage": None, "best_forced_gain": None, "oracle_gain": 0,
             "best_slots": [], "gained_notes": [], "worse_forks": 0, "forks": forks}
    if usable and queue["valid"]:
        best = max(f["count"] for f in usable)
        point["best_forced_coverage"] = best
        point["best_forced_gain"] = best - queue["count"]
        point["oracle_gain"] = max(0, best - queue["count"])
        point["best_slots"] = [f["read_slot"] for f in usable if f["count"] == best]
        point["worse_forks"] = sum(1 for f in usable if f["count"] < queue["count"])
        if point["oracle_gain"] > 0:
            gained: set[str] = set()
            for f in usable:
                if f["count"] == best:
                    gained.update(n for n in f["covered"] if n not in queue["covered"])
            point["gained_notes"] = sorted(gained)
    return point


def card_choice(result: dict) -> str | None:
    """The read choice of a selected card in a recorded assessment (any serde shape of the action)."""
    stack = [result.get("card", {}).get("action") if isinstance(result.get("card"), dict) else None]
    while stack:
        node = stack.pop()
        if isinstance(node, dict):
            if isinstance(node.get("choice"), str) and HEX.match(node["choice"]):
                return node["choice"]
            stack.extend(node.values())
    return None


def compact_from_sessions(series: Path, configuration: str) -> dict:
    """Ordinal-1 Compact decisions of a pass's first Answer replies, keyed by scenario id."""
    choices = {}
    for record_path in sorted(Path(series).glob("*/*/session.json")):
        record = common.read_json(record_path)
        if record.get("configuration", record.get("path")) != configuration or record.get("status") != "complete":
            continue
        first = next((t for t in common.read_jsonl(record_path.parent / "wrapper" / "turns.jsonl")
                      if t.get("turn") and t.get("tool") == "airk_answer"), None)
        if not first or not first.get("ok") or not first.get("result_path"):
            continue
        body = common.raw_body(Path(first["result_path"]).read_bytes())
        history = body.get("assessment_history") if isinstance(body, dict) else None
        if not isinstance(history, list):
            choices[record["scenario_id"]] = {"unknown": "no assessment_history in the reply"}
            continue
        event = next((e for e in history if isinstance(e, dict) and e.get("ordinal") == 1), None)
        result = (event or {}).get("result") or {}
        if event is None or result.get("status") in ("abstained", "unavailable"):
            choices[record["scenario_id"]] = {"abstained": True}
        else:
            choice = card_choice(result)
            choices[record["scenario_id"]] = {"choice": choice} if choice else {"terminal": True}
    return choices


def compact_capture(points: list[dict], choices: dict) -> dict:
    matched, unmatched = [], []
    for point in points:
        decision = choices.get(point["call"]) or choices.get(point["scenario_id"])
        if decision is None or not point["queue_valid"]:
            continue
        if decision.get("abstained") or decision.get("terminal") or not point["has_request"]:
            matched.append((point, point["queue_coverage"]))
        elif decision.get("choice"):
            fork = next((f for f in point["forks"] if f["valid"] and f["choice"] == decision["choice"]), None)
            if fork is None:
                unmatched.append(point["call"])
            else:
                matched.append((point, fork["count"]))
        else:
            unmatched.append(point["call"])
    oracle = sum(p["oracle_gain"] for p, _ in matched)
    delta = sum(c - p["queue_coverage"] for p, c in matched)
    return {"matched": len(matched), "unmatched": unmatched,
            "compact_gain_total": delta, "oracle_gain_total": oracle,
            "compact_mean_gain": (delta / len(matched)) if matched else None,
            "capture": (delta / oracle) if oracle > 0 else None}


def summarize(points: list[dict]) -> dict:
    counted = [p for p in points if p["queue_valid"]]
    gains = [p["oracle_gain"] for p in counted]
    signed = [p["best_forced_gain"] for p in counted if p["best_forced_gain"] is not None]
    return {"calls": len(points), "calls_counted": len(counted),
            "calls_with_request": sum(1 for p in counted if p["has_request"]),
            "calls_with_forks": sum(1 for p in counted if p["forks_valid"]),
            "invalid_forks": sum(p["forks_invalid"] for p in points),
            "mean_queue_coverage": mean(p["queue_coverage"] for p in counted) if counted else None,
            "mean_best_forced_coverage": mean(p["best_forced_coverage"] for p in counted
                                              if p["best_forced_coverage"] is not None) if signed else None,
            "mean_oracle_gain": mean(gains) if gains else None,
            "mean_best_forced_gain": mean(signed) if signed else None,
            "leverage": (sum(1 for g in gains if g >= 1) / len(gains)) if gains else None,
            "calls_with_a_worse_fork": sum(1 for p in counted if p["worse_forks"])}


def verdict(summary: dict, capture: dict | None) -> dict:
    leverage, gain = summary["leverage"], summary["mean_oracle_gain"]
    share = (capture or {}).get("capture")
    compact_mean = (capture or {}).get("compact_mean_gain")
    if leverage is None or gain is None:
        return {"decision": "undetermined", "reason": "no counted calls"}
    if leverage < NO_GO_LEVERAGE or gain < NO_GO_GAIN:
        return {"decision": "no-go: fix the roster, not the weights",
                "reason": f"oracle leverage {leverage:.3f} (< {NO_GO_LEVERAGE}) or mean oracle gain {gain:.3f} "
                          f"(< {NO_GO_GAIN}): Context option B or a gathering change, not training"}
    if share is not None and share >= NO_ROOM_CAPTURE:
        return {"decision": "no-go: no room",
                "reason": f"provisional Compact already captures {share:.3f} of the oracle gain (>= {NO_ROOM_CAPTURE})"}
    if leverage >= GO_LEVERAGE and gain >= GO_GAIN:
        if share is None and compact_mean is None:
            return {"decision": "undetermined: leverage clears the go bar, Compact capture not measured",
                    "reason": "supply --compact-sessions or --compact-choices"}
        if (share is not None and share < GO_CAPTURE) or (compact_mean is not None and compact_mean < 0):
            return {"decision": "go: build and train Compact, pending the non-win attribution",
                    "reason": "leverage, gain and capture clear the bars; the design also requires at least two "
                              "non-winning scenarios caused by 'the model chose badly' or 'retrieved and not "
                              "followed' where the oracle fork covers the note (pass analysis; candidates below)"}
    return {"decision": "between: build the collector and labels only; training waits",
            "reason": f"leverage {leverage:.3f}, mean oracle gain {gain:.3f}, capture {share}"}


def score(rows_path: Path, scenarios_dir: Path, compact: dict | None = None) -> dict:
    start, calls, complete = read_rows(rows_path)
    scenarios = {}
    for repo in common.REPOS:
        for path in common.scenario_files(scenarios_dir, [repo]):
            scenario = common.load_scenario(path)
            scenarios[scenario["id"]] = scenario
    ordinals = sorted(set(start.get("fork_ordinals") or [1]))
    by_ordinal, missing = {}, []
    for ordinal in ordinals:
        points = []
        for call in calls:
            scenario = scenarios.get(call.get("scenario_id"))
            if scenario is None:
                missing.append(call.get("scenario_id"))
                continue
            points.append(decision_point(call, scenario, ordinal))
        summary = summarize(points)
        capture = compact_capture(points, compact) if (compact is not None and ordinal == 1) else None
        entry = {"summary": summary, "points": points}
        if ordinal == 1:
            entry["compact"] = capture
            entry["verdict"] = verdict(summary, capture)
            entry["non_win_candidates"] = [{"call": p["call"], "notes": p["gained_notes"], "slots": p["best_slots"]}
                                           for p in points if p["gained_notes"]]
        by_ordinal[str(ordinal)] = entry
    return {"schema": SCORE_SCHEMA, "generated": common.now_iso(), "rows": str(rows_path),
            "rows_sha256": common.sha256_file(rows_path), "input_digest": start.get("input_digest"),
            "layers": start.get("layers"), "complete": complete is not None,
            "coverage_rule": evidence.COVERAGE_RULE, "count_fact_rows": False,
            "thresholds": {"no_go_leverage": NO_GO_LEVERAGE, "no_go_gain": NO_GO_GAIN,
                           "no_room_capture": NO_ROOM_CAPTURE, "go_leverage": GO_LEVERAGE,
                           "go_gain": GO_GAIN, "go_capture": GO_CAPTURE},
            "unknown_scenarios": sorted(set(filter(None, missing))), "ordinals": by_ordinal}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--rows", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--compact-choices", type=Path)
    source.add_argument("--compact-sessions", type=Path)
    parser.add_argument("--compact-configuration", default="compact")
    args = parser.parse_args(argv)
    compact = None
    if args.compact_choices:
        compact = common.read_json(args.compact_choices)
    elif args.compact_sessions:
        compact = compact_from_sessions(args.compact_sessions, args.compact_configuration)
    report = score(args.rows, args.scenarios_dir, compact)
    common.write_json(args.out, report)
    first = report["ordinals"].get("1") or next(iter(report["ordinals"].values()))
    print(json.dumps({"summary": first["summary"], "compact": first.get("compact"),
                      "verdict": first.get("verdict"), "complete": report["complete"]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
