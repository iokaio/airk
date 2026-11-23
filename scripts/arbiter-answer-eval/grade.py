# SPDX-License-Identifier: Apache-2.0
"""Rubric A grades, the case table, the substantial-win and configuration rules,
headroom and the per-step model comparisons (plan 4.1, 4.3, 4.4).

Input: a pass index (``pass-index.json`` written by run_pass) listing every
result with its session, coverage and judge verdicts, plus an optional
``adjudications.json`` from the primary agent. Output: ``results.csv`` with
the fixed columns below and ``summary.json``.

Harness conventions the plan leaves open (stated so they can be overruled):

* A brief "missing" a fact counts like "wrong": the fact was not stated correctly.
* Grade 1 "one essential fact short": exactly one essential note is a deficit
  (uncovered, or not stated correctly), whatever the turn count.
* Adjudicated grade: equal judge grades stand; grades one apart take the lower
  (and that judge's verdict); grades more than one apart wait for the primary
  agent's ``adjudications.json`` entry and the row is marked pending.
* "Worse" also covers Answer reaching no correct finish while the better of
  Parse and Search does (the plan's "Answer fails").
* "Not worse on average in a kind": the configuration's mean adjudicated grade
  in that kind is not below the mean grade of the better of Parse and Search.
* A close call (plan 3.5) is a pair where both finish and the turn difference
  (better-of turns minus Answer turns) is -1, 0, 1 or 2: one turn either way
  would cross the win or the worse boundary.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import sys
from pathlib import Path
from statistics import mean

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

COLUMNS = ["series", "repo", "slot", "kind", "configuration", "device", "session_id", "reused_from",
           "judge1_grade", "judge2_grade", "adjudicated_grade", "correct_finish", "turns_to_finish",
           "first_call_yield", "essential_covered", "essential_total", "bytes", "elapsed_ms",
           "rubric_b_j1", "rubric_b_j2", "cost_usd"]
ANSWER_CONFIGS = ("noModel", "focus", "compact", "large", "answer", "arbiter")
BASELINES = ("parse", "search")


def is_answer(configuration: str, path: str | None = None) -> bool:
    """An Answer result: a named Answer configuration, or any configuration on an Answer path (the gate's
    precision arms, e.g. ``compactFp32`` on path ``compact``)."""
    return configuration in ANSWER_CONFIGS or (path is not None and path in common.ANSWER_PATHS)


# ---------------------------------------------------------------- Rubric A


def grade_one(coverage: dict | None, verdict: dict | None, status: str) -> dict:
    """One judge's Rubric A grade for one result."""
    if status != "complete" or coverage is None or verdict is None:
        return {"grade": 0, "correct_finish": False, "deficits": None, "reason": f"session {status}"}
    essential = coverage["essential"]
    uncovered = set(coverage["essential_uncovered"])
    stated = {row["id"]: row["verdict"] for row in verdict.get("notes") or [] if isinstance(row, dict)}
    not_correct = {n for n in essential if stated.get(n) != "correct"}
    deficits = sorted(uncovered | not_correct)
    if verdict.get("misleading_followed") is True:
        return {"grade": 0, "correct_finish": False, "deficits": deficits, "reason": "misleading evidence followed"}
    if not deficits:
        turn = coverage["all_essential_turn"]
        grade = 4 if turn == 1 else 3 if turn == 2 else 2 if turn in (3, 4) else 1
        return {"grade": grade, "correct_finish": True, "deficits": [], "reason": f"correct finish at turn {turn}"}
    if len(deficits) == 1:
        return {"grade": 1, "correct_finish": False, "deficits": deficits, "reason": "one essential fact short"}
    return {"grade": 0, "correct_finish": False, "deficits": deficits, "reason": "more than one essential fact short"}


def rubric_b(verdict: dict | None) -> float | None:
    if not verdict:
        return None
    rubric = verdict.get("rubric_b") or {}
    scores = [rubric[k]["score"] for k in ("find", "choice", "trace", "useful")
              if isinstance(rubric.get(k), dict) and isinstance(rubric[k].get("score"), int)]
    return round(mean(scores), 3) if len(scores) == 4 else None


def adjudicate(g1: dict | None, g2: dict | None, override: dict | None) -> dict:
    if override is not None:
        return {"grade": override["grade"], "correct_finish": bool(override.get("correct_finish", False)),
                "source": "adjudicated", "reason": override.get("reason")}
    present = [g for g in (g1, g2) if g is not None]
    if not present:
        return {"grade": None, "correct_finish": None, "source": "pending", "reason": "no valid verdict"}
    if len(present) == 1:
        return {**present[0], "source": "single-judge"}
    if g1["grade"] == g2["grade"]:
        both = g1["correct_finish"] and g2["correct_finish"]
        return {"grade": g1["grade"], "correct_finish": both, "source": "agree", "reason": g1["reason"]}
    if abs(g1["grade"] - g2["grade"]) == 1:
        lower = g1 if g1["grade"] < g2["grade"] else g2
        return {**lower, "source": "lower-of-adjacent"}
    return {"grade": None, "correct_finish": None, "source": "pending",
            "reason": f"judges differ by {abs(g1['grade'] - g2['grade'])} grades"}


# ---------------------------------------------------------------- comparison


def finish_turns(row: dict) -> int | None:
    return row["turns_to_finish"] if row.get("correct_finish") is True else None


def better_of(rows: list[dict]) -> dict | None:
    """The better of Parse and Search (primary agent's tie rule, 22 Nov 2026; one rule everywhere).

    1. A correct finish at fewer turns.
    2. If neither finishes: more essential notes covered over the session.
    3. Then fewer session turns.
    4. Then Search.
    """
    rows = [r for r in rows if r is not None]
    if not rows:
        return None
    finished = [r for r in rows if finish_turns(r) is not None]
    if finished:
        return min(finished, key=lambda r: (finish_turns(r), r.get("configuration") != "search"))
    return min(rows, key=lambda r: (-(r.get("essential_covered") or 0), r.get("session_turns") or 0,
                                    r.get("configuration") != "search"))


def case(answer: dict, best: dict | None) -> dict:
    """The case table of plan 4.1 and the substantial-win rule of 4.3."""
    a = finish_turns(answer)
    b = finish_turns(best) if best is not None else None
    if answer.get("adjudicated_grade") is None or (best is not None and best.get("adjudicated_grade") is None):
        return {"case": "pending", "substantial": False, "worse": False, "close_call": False, "diff": None}
    if a is not None and b is None:
        return {"case": "substantial-win (only Answer finishes)", "substantial": True, "worse": False,
                "close_call": False, "diff": None}
    if a is None and b is not None:
        return {"case": "worse (no correct finish for Answer)", "substantial": False, "worse": True,
                "close_call": False, "diff": None}
    if a is None and b is None:
        return {"case": "no win (neither finishes)", "substantial": False, "worse": False, "close_call": False,
                "diff": None}
    diff = b - a
    if diff >= 2:
        label, substantial, worse = "substantial-win", True, False
    elif diff == 1:
        label, substantial, worse = "better (one turn sooner)", False, False
    elif diff == 0:
        label, substantial, worse = "tie", False, False
    else:
        label, substantial, worse = "worse (finishes later)", False, True
    return {"case": label, "substantial": substantial, "worse": worse, "close_call": diff in (-1, 0, 1, 2),
            "diff": diff}


def combine_samples(cases: list[dict]) -> dict:
    """Plan 3.5: a second session's verdict stands if both agree; otherwise undecided, not a win."""
    if len(cases) <= 1:
        return cases[0] if cases else {"case": "pending", "substantial": False, "worse": False}
    labels = {(c["substantial"], c["worse"], c["case"].split(" ")[0]) for c in cases}
    if len(labels) == 1:
        return {**cases[0], "samples": len(cases)}
    return {"case": "undecided (samples differ)", "substantial": False, "worse": False, "close_call": True,
            "diff": None, "samples": len(cases)}


def configuration_rule(per_scenario: list[dict]) -> dict:
    """Plan 4.3: >= 12 of 20 substantial wins, <= 2 worse, >= 5 wins per repository, no kind worse on average.

    The repositories are the ones the pass ran: munarium and bookbuilder in the loop series, airk and
    bookbuilder in the one-turn history. A pass without rows counts the default pair."""
    wins = [s for s in per_scenario if s["case"]["substantial"]]
    worse = [s for s in per_scenario if s["case"]["worse"]]
    present = {s["repo"] for s in per_scenario}
    repos = ([repo for repo in common.KNOWN_REPOS if repo in present] + sorted(present - set(common.KNOWN_REPOS))
             or list(common.REPOS))
    by_repo = {repo: sum(1 for s in wins if s["repo"] == repo) for repo in repos}
    kinds = {}
    for kind in sorted({s["kind"] for s in per_scenario}):
        rows = [s for s in per_scenario if s["kind"] == kind and s["answer_grade"] is not None
                and s["best_grade"] is not None]
        if rows:
            kinds[kind] = {"answer_mean_grade": round(mean(r["answer_grade"] for r in rows), 3),
                           "best_mean_grade": round(mean(r["best_grade"] for r in rows), 3)}
            kinds[kind]["worse_on_average"] = kinds[kind]["answer_mean_grade"] < kinds[kind]["best_mean_grade"]
    pending = [s for s in per_scenario if s["case"]["case"] == "pending"]
    met = (len(wins) >= 12 and len(worse) <= 2 and all(v >= 5 for v in by_repo.values())
           and not any(k["worse_on_average"] for k in kinds.values()))
    return {"scenarios": len(per_scenario), "substantial_wins": len(wins), "worse": len(worse),
            "wins_by_repo": by_repo, "kinds": kinds, "pending": len(pending),
            "substantially_improves": met and not pending, "met_ignoring_pending": met}


def headroom(best_rows: dict) -> dict:
    """Scenarios where a substantial win is possible: the better of Parse and Search needs >= 3 calls or never finishes."""
    room = [key for key, row in best_rows.items() if row is None or finish_turns(row) is None or finish_turns(row) >= 3]
    return {"count": len(room), "scenarios": sorted(room), "of": len(best_rows), "rule_reachable": len(room) >= 12}


def step_comparison(a_rows: dict, b_rows: dict) -> dict:
    """What configuration A adds over B on shared scenario rows (plan 4.4)."""
    shared = sorted(set(a_rows) & set(b_rows))
    a_fin = {k for k in shared if finish_turns(a_rows[k]) is not None}
    b_fin = {k for k in shared if finish_turns(b_rows[k]) is not None}
    both = sorted(a_fin & b_fin)
    elapsed = [(a_rows[k].get("elapsed_ms") or 0) - (b_rows[k].get("elapsed_ms") or 0) for k in shared]
    return {"rows": len(shared), "correct_finishes_a": len(a_fin), "correct_finishes_b": len(b_fin),
            "finishes_added": len(a_fin) - len(b_fin), "gained": sorted(a_fin - b_fin), "lost": sorted(b_fin - a_fin),
            "turns_saved": sum(finish_turns(b_rows[k]) - finish_turns(a_rows[k]) for k in both),
            "turns_saved_rows": len(both),
            "time_added_ms_mean": round(mean(elapsed), 1) if elapsed else None,
            "time_added_ms_total": sum(elapsed)}


# ---------------------------------------------------------------- rows


def build_rows(index: dict, adjudications: dict) -> list[dict]:
    rows = []
    by_key = {r["key"]: r for r in index["results"]}
    for entry in index["results"]:
        source = by_key.get(entry.get("reused_from")) if entry.get("reused_from") else entry
        source = source or entry
        record_path = Path(source["session_dir"]) / "session.json" if source.get("session_dir") else None
        if record_path is None or not record_path.is_file():
            continue  # not run yet (e.g. an arm that runs after this grading): not a result
        session = common.read_json(record_path)
        coverage = common.read_json(source["coverage"]) if source.get("coverage") and Path(source["coverage"]).is_file() else None
        judged = []
        for path in source.get("judges", []):
            record = common.read_json(path) if Path(path).is_file() else None
            judged.append(record["verdict"] if record and record.get("status") == "valid" else None)
        while len(judged) < 2:
            judged.append(None)
        status = session.get("status", "missing")
        g = [grade_one(coverage, v, status) if (v is not None or status != "complete") else None for v in judged[:2]]
        adjudicated = adjudicate(g[0], g[1], adjudications.get(source["key"]))
        turns = coverage["all_essential_turn"] if coverage and adjudicated.get("correct_finish") else None
        rows.append({
            "key": entry["key"], "series": index["series"], "repo": entry["repo"], "slot": entry["slot"],
            "kind": entry["kind"], "configuration": entry["configuration"], "device": entry["device"],
            "path": entry.get("path"), "arm": entry.get("arm"),
            "sample": entry.get("sample", 1), "session_id": session.get("session_id"),
            "reused_from": entry.get("reused_from") or "",
            "judge1_grade": None if g[0] is None else g[0]["grade"],
            "judge2_grade": None if g[1] is None else g[1]["grade"],
            "adjudicated_grade": adjudicated["grade"], "adjudication": adjudicated["source"],
            "correct_finish": adjudicated["correct_finish"], "turns_to_finish": turns,
            "first_call_yield": coverage["first_call_yield"] if coverage else None,
            "essential_covered": coverage["essential_covered"] if coverage else None,
            "session_turns": coverage["counted_turns"] if coverage else None,
            "essential_total": coverage["essential_total"] if coverage else None,
            "bytes": coverage["total_bytes"] if coverage else None,
            "elapsed_ms": coverage["total_elapsed_ms"] if coverage else None,
            "rubric_b_j1": rubric_b(judged[0]), "rubric_b_j2": rubric_b(judged[1]),
            "cost_usd": session.get("cost_usd") if not entry.get("reused_from") else 0.0,
            "judge_cost_usd": 0.0 if entry.get("reused_from") else sum(
                (common.read_json(p).get("cost_usd") or 0) for p in source.get("judges", []) if Path(p).is_file()),
            "status": status,
        })
    return rows


def csv_text(rows: list[dict]) -> str:
    buffer = io.StringIO()
    writer = csv.DictWriter(buffer, fieldnames=COLUMNS, extrasaction="ignore", lineterminator="\n")
    writer.writeheader()
    for row in rows:
        writer.writerow({k: ("" if row.get(k) is None else row.get(k)) for k in COLUMNS})
    return buffer.getvalue()


def summarize(rows: list[dict], series: str) -> dict:
    scen = lambda r: f"{r['repo']}-{r['slot']}"  # noqa: E731
    first = [r for r in rows if r.get("sample", 1) == 1]
    results: dict[tuple, dict[str, dict]] = {}
    for r in first:
        results.setdefault((r["configuration"], r["device"]), {})[scen(r)] = r
    samples: dict[tuple, dict[str, list[dict]]] = {}
    for r in rows:
        samples.setdefault((r["configuration"], r["device"]), {}).setdefault(scen(r), []).append(r)
    baselines = {c: rows_ for (c, _d), rows_ in results.items() if c in BASELINES}
    scenarios = sorted({scen(r) for r in first})
    best = {s: better_of([baselines.get(c, {}).get(s) for c in BASELINES]) for s in scenarios}
    meta = {scen(r): (r["repo"], r["kind"]) for r in first}
    configs = {}
    for (configuration, device), by_scen in sorted(results.items()):
        if not is_answer(configuration, next(iter(by_scen.values())).get("path")):
            continue
        per = []
        for s, row in sorted(by_scen.items()):
            cases = []
            for k, a_sample in enumerate(samples[(configuration, device)][s]):
                best_k = better_of([_sample(samples, c, s, k) for c in BASELINES]) if baselines else None
                cases.append(case(a_sample, best_k))
            c = combine_samples(cases)
            per.append({"scenario": s, "repo": meta[s][0], "kind": meta[s][1], "case": c,
                        "answer_grade": row["adjudicated_grade"],
                        "best_grade": best[s]["adjudicated_grade"] if best.get(s) else None,
                        "best_path": best[s]["configuration"] if best.get(s) else None})
        rule = configuration_rule(per)
        b_values = [v for r in by_scen.values() for v in (r["rubric_b_j1"], r["rubric_b_j2"]) if v is not None]
        best_b = [v for s in by_scen if best.get(s) for v in (best[s]["rubric_b_j1"], best[s]["rubric_b_j2"])
                  if v is not None]
        configs[f"{configuration}-{device}"] = {
            "configuration": configuration, "device": device, "rule": rule, "scenarios": per,
            "close_calls": [p["scenario"] for p in per if p["case"].get("close_call") and p["case"].get("samples", 1) == 1],
            "rubric_b_mean": round(mean(b_values), 3) if b_values else None,
            "rubric_b_best_mean": round(mean(best_b), 3) if best_b else None,
            "correct_finishes": sum(1 for r in by_scen.values() if r["correct_finish"] is True)}
    steps = []

    def rows_of(configuration, device):
        return results.get((configuration, device))

    no_model = next((rows_of("noModel", d) for d in ("none", "cpu", "gpu") if rows_of("noModel", d)), None)
    best_rows = {s: r for s, r in best.items() if r is not None}
    if no_model:
        steps.append({"step": "noModel vs better of Parse and Search", **step_comparison(no_model, best_rows)})
    for device in ("cpu", "gpu"):
        for a, b, b_rows in (("focus", "noModel", no_model), ("compact", "focus", rows_of("focus", device)),
                             ("large", "compact", rows_of("compact", device))):
            a_rows = rows_of(a, device)
            if a_rows and b_rows:
                steps.append({"step": f"{a} vs {b}", "device": device, **step_comparison(a_rows, b_rows)})
    return {"schema": "airk.private.answer-eval-summary/1", "series": series, "generated": common.now_iso(),
            "results": len(rows), "headroom": headroom({s: best.get(s) for s in scenarios}),
            "configurations": configs, "model_steps": steps,
            "pending_adjudication": [r["key"] for r in rows if r["adjudication"] == "pending"],
            "reused": [{"key": r["key"], "reused_from": r["reused_from"]} for r in rows if r["reused_from"]],
            "cost_usd": {"agents": round(sum(r["cost_usd"] or 0 for r in rows), 4),
                         "judges": round(sum(r["judge_cost_usd"] or 0 for r in rows), 4)}}


def rubric_a_winner(a: dict | None, b: dict | None) -> str:
    """Who wins Rubric A on one scenario between two arms: the higher adjudicated grade; at equal grades with
    both finishing, fewer turns to the finish; otherwise a tie. ``pending`` while a grade is missing.
    (Harness convention for the gates of plan 12.7.1 and 12.7.2, stated so it can be overruled.)"""
    if a is None or b is None or a.get("adjudicated_grade") is None or b.get("adjudicated_grade") is None:
        return "pending"
    if a["adjudicated_grade"] != b["adjudicated_grade"]:
        return "a" if a["adjudicated_grade"] > b["adjudicated_grade"] else "b"
    ta, tb = finish_turns(a), finish_turns(b)
    if ta is not None and tb is not None and ta != tb:
        return "a" if ta < tb else "b"
    return "tie"


def arm_close_call(a: dict | None, b: dict | None) -> bool:
    """A gate close call (plan 12.7.1 test 3, as in 3.5): one turn or one fact would change the winner. Both
    finish within one turn of each other, or one finishes and the other is one essential fact short."""
    if a is None or b is None:
        return False
    ta, tb = finish_turns(a), finish_turns(b)
    if ta is not None and tb is not None:
        return abs(ta - tb) <= 1
    if (ta is None) != (tb is None):
        other = b if ta is not None else a
        return other.get("adjudicated_grade") == 1
    return False


def arm_compare(a_rows: dict, b_rows: dict) -> dict:
    """Gate test 3 between arm A and arm B (``{scenario: row}``, first samples): correct finishes, turns to a
    correct finish, Rubric A wins and losses for B against A, Rubric B means and the close calls."""
    shared = sorted(set(a_rows) | set(b_rows))
    per = []
    for scenario in shared:
        a, b = a_rows.get(scenario), b_rows.get(scenario)
        per.append({"scenario": scenario, "repo": (a or b)["repo"], "kind": (a or b)["kind"],
                    "a_grade": None if a is None else a.get("adjudicated_grade"),
                    "b_grade": None if b is None else b.get("adjudicated_grade"),
                    "a_turns": None if a is None else finish_turns(a), "b_turns": None if b is None else finish_turns(b),
                    "a_finish": bool(a and a.get("correct_finish") is True),
                    "b_finish": bool(b and b.get("correct_finish") is True),
                    "winner": rubric_a_winner(a, b), "close_call": arm_close_call(a, b)})

    def b_mean(rows):
        values = [v for r in rows.values() for v in (r.get("rubric_b_j1"), r.get("rubric_b_j2")) if v is not None]
        return round(mean(values), 3) if values else None

    return {"scenarios": per, "rows": len(per),
            "a_correct_finishes": sum(1 for p in per if p["a_finish"]),
            "b_correct_finishes": sum(1 for p in per if p["b_finish"]),
            "finishes_added": sum(1 for p in per if p["b_finish"]) - sum(1 for p in per if p["a_finish"]),
            "b_wins": sum(1 for p in per if p["winner"] == "b"), "b_losses": sum(1 for p in per if p["winner"] == "a"),
            "ties": sum(1 for p in per if p["winner"] == "tie"), "pending": sum(1 for p in per if p["winner"] == "pending"),
            "a_rubric_b_mean": b_mean(a_rows), "b_rubric_b_mean": b_mean(b_rows),
            "close_calls": [p["scenario"] for p in per if p["close_call"]]}


def _sample(samples, configuration, scenario, k):
    for (c, _d), by_scen in samples.items():
        if c == configuration and scenario in by_scen:
            rows = by_scen[scenario]
            return rows[k] if k < len(rows) else rows[0]
    return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pass-dir", type=Path, required=True)
    parser.add_argument("--adjudications", type=Path, help="default: <pass-dir>/adjudications.json if present")
    args = parser.parse_args(argv)
    index = common.read_json(args.pass_dir / "pass-index.json")
    adj_path = args.adjudications or args.pass_dir / "adjudications.json"
    adjudications = common.read_json(adj_path) if adj_path.is_file() else {}
    rows = build_rows(index, adjudications)
    common.write_text(args.pass_dir / "results.csv", csv_text(rows))
    summary = summarize(rows, index["series"])
    common.write_json(args.pass_dir / "summary.json", summary)
    print(json.dumps({"results": len(rows), "pending": len(summary["pending_adjudication"]),
                      "headroom": summary["headroom"]["count"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
