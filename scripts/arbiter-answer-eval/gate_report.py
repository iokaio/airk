# SPDX-License-Identifier: Apache-2.0
"""The gate report of plan 12.7 (LG): Large against Compact, Focus only against Focus with Compact, precision.

Input: the manifest ``make_loop_spec.py gate`` writes
(``<R>/passes/<series>/gate-report-inputs.json``), naming each arm's
``arm-replays.json``, the agreement, outcomes, precision and performance
documents of ``gate.py`` and the switch receipts. Output: one Markdown report
for ``docs/arbiter/answer-evaluation/`` written for the user's personal review,
and a JSON twin with every figure.

Sections: the decisions asked of the user with the computed bars; for each
gate the bar, the agreement tables, first-reply evidence, judged outcomes and
a per-scenario side-by-side (each arm's evidence by location, the judges'
grades and reasons); the precision check with the escalation rule; performance,
concurrency and switches; arm identity; and the receipts (path and SHA-256).

Bars:

* 12.7.1, the user's to set, as proposed in the plan: Large is significantly
  better when (a) it has at least 3 more correct finishes than Compact, or (b)
  it wins Rubric A on at least 8 scenarios and loses at most 3; and in both
  cases its first-reply essential coverage is no lower than Compact's on
  either repository and its median end-to-end time is at most twice
  Compact's. If neither outcome condition is met, the report recommends
  dropping Large.
* 12.7.2: the decision rule is deferred (plan); the report proposes the same
  bar with Compact in Large's place and Focus only in Compact's.
* 12.7.3: an FP32 arm is escalated to judged sessions when more than 5% of its
  stage's decisions flip, or the essential notes covered differ on any
  scenario (``gate.precision``).

No source text is copied: evidence appears as locations (path and lines) and
note ids only. Judges' reasons on bookbuilder scenarios have quoted and code
text removed (``redact``), because reports quote none of that repository's
source.

``python -I -B gate_report.py --inputs <R>/passes/<series>/gate-report-inputs.json --out docs/arbiter/answer-evaluation/<NN>-gate.md``
"""

from __future__ import annotations

import argparse
import re
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import evidence  # noqa: E402
import gate  # noqa: E402
import loop_reply  # noqa: E402

REPORT_SCHEMA = "airk.private.answer-eval-gate-report/1"
PRIVATE_REPOS = ("bookbuilder",)
MAX_REASON = 240
MAX_RECORDS = 8
MAX_LOCATIONS = 6

_QUOTED = re.compile(r"```.*?```|`[^`]*`|\"[^\"]*\"|“[^”]*”|'[^'\n]{2,}'", re.S)


def redact(text: str | None) -> str:
    """A reason with quoted and code text removed, whitespace collapsed and the length capped."""
    if not text:
        return ""
    cleaned = _QUOTED.sub("[quoted text omitted]", str(text))
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned if len(cleaned) <= MAX_REASON else cleaned[:MAX_REASON - 1].rstrip() + "…"


def reason_for(repo: str, text: str | None) -> str:
    return redact(text) if repo in PRIVATE_REPOS else re.sub(r"\s+", " ", str(text or "")).strip()[:2 * MAX_REASON]


def cell(value) -> str:
    if value is None:
        return "–"
    if isinstance(value, float):
        return f"{value:.3g}"
    if isinstance(value, (list, tuple)):
        return ", ".join(str(v) for v in value) if value else "none"
    return str(value).replace("|", "\\|")


def table(headers: list[str], rows: list[list]) -> list[str]:
    out = ["| " + " | ".join(headers) + " |", "| " + " | ".join("---" for _ in headers) + " |"]
    out += ["| " + " | ".join(cell(v) for v in row) + " |" for row in rows]
    return out + [""]


# ---------------------------------------------------------------- bars


def repo_essential(doc: dict) -> dict[str, int]:
    out: dict[str, int] = {}
    for row in doc["rows"]:
        out[row["repo"]] = out.get(row["repo"], 0) + len(row["first_reply"]["essential_covered"])
    return out


def model_bar(outcome: dict, doc_a: dict, doc_b: dict) -> dict:
    """Plan 12.7.1's proposed bar, B (the larger configuration) against A."""
    finishes = outcome["finishes_added"] >= 3
    rubric = outcome["b_wins"] >= 8 and outcome["b_losses"] <= 3
    ea, eb = repo_essential(doc_a), repo_essential(doc_b)
    coverage = {repo: eb.get(repo, 0) >= ea.get(repo, 0) for repo in sorted(set(ea) | set(eb))}
    ta, tb = gate.arm_performance(doc_a)["elapsed_ms_median"], gate.arm_performance(doc_b)["elapsed_ms_median"]
    time_ok = ta is not None and tb is not None and tb <= 2 * ta
    outcome_met = finishes or rubric
    met = outcome_met and all(coverage.values()) and time_ok
    return {"finishes_condition": finishes, "rubric_a_condition": rubric, "outcome_condition": outcome_met,
            "coverage_condition": coverage, "time_condition": time_ok, "met": met,
            "finishes_added": outcome["finishes_added"], "b_wins": outcome["b_wins"], "b_losses": outcome["b_losses"],
            "essential_a": ea, "essential_b": eb, "median_ms_a": ta, "median_ms_b": tb,
            "pending": outcome.get("pending", 0), "undecided": outcome.get("undecided", 0)}


def large_recommendation(bar: dict) -> str:
    if bar["pending"]:
        return (f"Not yet decidable: {bar['pending']} scenarios await adjudication. The bar is computed on the "
                "decided scenarios only.")
    if not bar["outcome_condition"]:
        return "Large is not significantly better than Compact under the proposed bar: I recommend dropping Large."
    if not bar["met"]:
        failed = [k for k in ("coverage_condition", "time_condition")
                  if not (all(bar[k].values()) if isinstance(bar[k], dict) else bar[k])]
        return ("Large meets an outcome condition but not the " + " and ".join(f.split("_")[0] for f in failed)
                + " condition, so it is not significantly better under the proposed bar: I recommend dropping Large.")
    return "Large is significantly better than Compact under the proposed bar: I recommend continuing Large."


def compact_recommendation(bar: dict) -> str:
    if bar["pending"]:
        return f"Not yet decidable: {bar['pending']} scenarios await adjudication."
    if bar["met"]:
        return ("Compact is significantly better than Focus only under the proposed bar: I recommend keeping "
                "Compact as the loop's Context stage.")
    return ("Compact is not significantly better than Focus only under the proposed bar: I recommend shipping the "
            "loop with Focus only, unless the user sets a different bar.")


# ---------------------------------------------------------------- side by side


def located_evidence(row: dict) -> list[str]:
    """The evidence one reply returned, by location only: each record's operation and its first places."""
    if not row.get("ok") or not row.get("result_path") or not Path(row["result_path"]).is_file():
        return []
    body = common.raw_body(Path(row["result_path"]).read_bytes())
    out = []
    for record in loop_reply.records(body)[:MAX_RECORDS]:
        places = []
        for span in evidence.extract(record.get("canonical_result")):
            if span.start is None:
                continue
            place = f"{span.path}:{span.start}-{span.end}"
            if place not in places:
                places.append(place)
            if len(places) == MAX_LOCATIONS:
                break
        out.append(f"{record.get('operation')} {', '.join(places) if places else '(no located hits)'}")
    more = len(loop_reply.records(body)) - MAX_RECORDS
    if more > 0:
        out.append(f"and {more} more records")
    return out


def _judging_lines(repo: str, judging: list[dict]) -> list[str]:
    lines = []
    for record in judging or []:
        if not record.get("valid"):
            lines.append(f"  - {record.get('judge')}: no valid verdict")
            continue
        notes = "; ".join(f"{n['id']} {n['verdict']}: {reason_for(repo, n.get('reason'))}" for n in record["notes"])
        lines.append(f"  - {record['judge']}: {notes}")
        if record.get("misleading_followed"):
            lines.append(f"    misleading evidence followed: {reason_for(repo, record.get('misleading_reason'))}")
    return lines


def side_by_side(label_a: str, label_b: str, doc_a: dict, doc_b: dict, outcome: dict | None) -> list[str]:
    by_a = {r["scenario_id"]: r for r in doc_a["rows"]}
    by_b = {r["scenario_id"]: r for r in doc_b["rows"]}
    judged = {p["scenario"]: p for p in (outcome or {}).get("scenarios", [])}
    out = []
    for sid in sorted(set(by_a) | set(by_b)):
        out.append(f"#### {sid}")
        out.append("")
        rows = []
        for label, doc_row, side in ((label_a, by_a.get(sid), "a"), (label_b, by_b.get(sid), "b")):
            if doc_row is None:
                rows.append([label, "not run"] + [None] * 6)
                continue
            fr = doc_row["first_reply"]
            subs = fr.get("sub_needs") or {}
            j = judged.get(sid, {})
            rows.append([label, f"{len(fr['essential_covered'])}/{fr['essential_total']} ({cell(fr['essential_covered'])})",
                         len(fr["supporting_covered"]), doc_row.get("bytes"),
                         f"{subs.get('covered', 0)}/{subs.get('partial', 0)}/{subs.get('missing', 0)}",
                         fr.get("partial") or "none", j.get(f"{side}_grade"), j.get(f"{side}_turns")])
        out += table(["Arm", "Essential", "Supporting", "Bytes", "Sub-needs c/p/m", "Partial", "Grade", "Turns"], rows)
        if sid in judged:
            out.append(f"Rubric A winner: **{judged[sid]['winner']}**"
                       + (" (close call)" if judged[sid].get("close_call") else ""))
            out.append("")
        for label, doc_row, side in ((label_a, by_a.get(sid), "a"), (label_b, by_b.get(sid), "b")):
            if doc_row is None:
                continue
            out.append(f"- {label} evidence: " + ("; ".join(located_evidence(doc_row)) or "none"))
            out += _judging_lines(doc_row["repo"], judged.get(sid, {}).get(f"{side}_judging"))
        out.append("")
    return out


# ---------------------------------------------------------------- sections


def agreement_section(doc: dict) -> list[str]:
    rows = [[stage, c.get("agree", 0), c.get("differ", 0), c.get("diverged", 0), c.get("only_a", 0) + c.get("only_b", 0),
             c.get("agreement_share"), c.get("favoured_a", 0), c.get("favoured_b", 0), c.get("favoured_neither", 0)]
            for stage, c in sorted(doc["by_stage"].items())]
    out = table(["Stage", "Agree", "Differ", "Diverged", "Unpaired", "Agreement", f"Notes favour {doc['a']['arm']}",
                 f"Notes favour {doc['b']['arm']}", "Neither"], rows)
    views = doc["views"]
    out.append(f"Of {doc['scenarios']} scenarios: the same split on {views['same_split']}, the same sub-needs on "
               f"{views['same_sub_needs']}, the same coverage verdicts on {views['same_coverage_verdicts']}, the "
               f"same follow-ups on {views['same_follow_ups']} and identical evidence on {views['same_evidence']}. "
               f"Scenario-level coverage favours {doc['a']['arm']} on {doc['scenario_favoured']['a']}, "
               f"{doc['b']['arm']} on {doc['scenario_favoured']['b']}, neither on {doc['scenario_favoured']['neither']}.")
    out.append("")
    return out


def evidence_section(docs: dict[str, dict]) -> list[str]:
    rows = []
    for arm, doc in docs.items():
        for repo in sorted({r["repo"] for r in doc["rows"]}):
            rs = [r for r in doc["rows"] if r["repo"] == repo]
            rows.append([arm, repo, sum(len(r["first_reply"]["essential_covered"]) for r in rs),
                         sum(r["first_reply"]["essential_total"] for r in rs),
                         sum(len(r["first_reply"]["supporting_covered"]) for r in rs),
                         statistics.median([r["bytes"] for r in rs if r.get("bytes") is not None] or [0]),
                         sum((r["first_reply"].get("sub_needs") or {}).get("covered", 0) for r in rs),
                         sum((r["first_reply"].get("sub_needs") or {}).get("missing", 0) for r in rs)])
    return table(["Arm", "Repo", "Essential covered", "Essential total", "Supporting covered", "Median bytes",
                  "Sub-needs covered", "Sub-needs missing"], rows)


def outcome_section(outcome: dict, a: str, b: str) -> list[str]:
    rows = [[a, outcome["a_correct_finishes"], outcome["a_rubric_b_mean"]],
            [b, outcome["b_correct_finishes"], outcome["b_rubric_b_mean"]]]
    out = table(["Arm", "Correct finishes", "Rubric B mean"], rows)
    out.append(f"Rubric A: {b} wins {outcome['b_wins']}, loses {outcome['b_losses']}, ties {outcome['ties']}; "
               f"undecided {outcome.get('undecided', 0)}, pending {outcome.get('pending', 0)}. Close calls: "
               f"{cell(outcome.get('close_calls'))}.")
    out.append("")
    return out


def bar_section(bar: dict, a: str, b: str) -> list[str]:
    rows = [[f"{b} has at least 3 more correct finishes", bar["finishes_condition"], bar["finishes_added"]],
            [f"{b} wins Rubric A on at least 8 and loses at most 3", bar["rubric_a_condition"],
             f"{bar['b_wins']} won, {bar['b_losses']} lost"]]
    for repo, ok in bar["coverage_condition"].items():
        rows.append([f"{b}'s essential coverage no lower on {repo}", ok,
                     f"{bar['essential_b'].get(repo, 0)} against {bar['essential_a'].get(repo, 0)}"])
    rows.append([f"{b}'s median time at most twice {a}'s", bar["time_condition"],
                 f"{cell(bar['median_ms_b'])} ms against {cell(bar['median_ms_a'])} ms"])
    rows.append(["Bar met", bar["met"], ""])
    return table(["Condition", "Met", "Figures"], rows)


def precision_section(docs: list[dict]) -> list[str]:
    rows = []
    for d in docs:
        p16, p32 = d["fp16"]["performance"], d["fp32"]["performance"]
        rows.append([d["stage"], f"{d['fp32']['arm']} against {d['fp16']['arm']}", d["decisions"], d["flips"],
                     f"{d['flip_share']:.1%}", len(d["essential_differs"]), d["favoured"]["fp32"], d["favoured"]["fp16"],
                     p32["model_ms_per_call_median"], p16["model_ms_per_call_median"],
                     cell(p32["peak_used_mib"]), cell(p16["peak_used_mib"]), d["escalate"]])
    out = table(["Stage", "Arms", "Decisions", "Flips", "Share", "Scenarios with essential differences",
                 "Notes favour FP32", "Notes favour FP16", "Model ms/call FP32", "Model ms/call FP16",
                 "Peak MiB FP32", "Peak MiB FP16", "Escalate"], rows)
    for d in docs:
        if d["escalate"]:
            out.append(f"- {d['stage']} ({d['fp32']['arm']}): escalated to judged sessions: "
                       + "; ".join(d["escalation_reasons"]) + ".")
    if docs and not any(d["escalate"] for d in docs):
        out.append("- No stage is sensitive to precision, so the Large gate already answers Compact FP32 against "
                   "Large FP16 (plan 12.7.3).")
    out.append("")
    return out


def performance_section(perf: dict) -> list[str]:
    rows = [[name, a["answers"], a["ok"], a["elapsed_ms_median"], a["elapsed_ms_p90"], a["elapsed_ms_max"],
             a["model_ms_per_call_median"], cell(a["rounds"]), cell(a["partials"]) if a["partials"] else "none",
             a["tickets"], cell(a["peak_used_mib"])] for name, a in perf["arms"].items()]
    out = table(["Arm", "Answers", "OK", "Median ms", "p90 ms", "Max ms", "Model ms/call", "Rounds used",
                 "Partials", "Tickets", "Peak MiB"], rows)
    conc = [[name, r["level"], r["answers"], r["wall_s"], r["answers_per_minute"], r["elapsed_ms_median"],
             r["tickets"], cell(r["partials"]) if r["partials"] else "none", cell(r["peak_used_mib"])]
            for name, runs in perf.get("concurrency", {}).items() for r in runs]
    if conc:
        out += table(["Arm", "Concurrent", "Answers", "Wall s", "Answers/min", "Median ms", "Tickets", "Partials",
                      "Peak MiB"], conc)
    switches = [[cell(s.get("from")), cell(s.get("to")), s.get("release_s"), s.get("warm_s"), s.get("total_s")]
                for s in perf.get("switches", [])]
    if switches:
        out += table(["From", "To", "Release s", "Warm s", "Total s"], switches)
    out.append("Per-stage model time is not in the reply; the decision log (service custody) holds it. Peak memory "
               "is device-wide.")
    out.append("")
    return out


def identity_section(docs: dict[str, dict]) -> list[str]:
    rows = []
    for arm, doc in docs.items():
        problems = sorted({p for r in doc["rows"] for p in r.get("arm_problems") or []})
        rows.append([arm, doc["configuration"], doc.get("arm_ok"), "; ".join(problems[:4]) if problems else "none"])
    return table(["Arm", "Configuration", "Identity met", "Problems"], rows)


# ---------------------------------------------------------------- build


def _maybe(path) -> dict | None:
    return common.read_json(Path(path)) if path and Path(path).is_file() else None


def receipts(paths: list[Path]) -> list[list]:
    return [[str(p), common.sha256_file(p)] for p in paths if p.is_file()]


def build(inputs: dict) -> tuple[str, dict]:
    arm_docs = {}
    for arm, path in inputs["arms"].items():
        target = Path(path) / "arm-replays.json" if Path(path).is_dir() else Path(path)
        if target.is_file():
            arm_docs[arm] = gate.load_arm(target)
    agreements = {k: _maybe(v) for k, v in inputs.get("agreement", {}).items()}
    outcomes = {k: _maybe(v) for k, v in inputs.get("outcomes", {}).items()}
    precisions = [d for d in (_maybe(p) for p in inputs.get("precision", [])) if d]
    perf = _maybe(inputs.get("performance"))
    figures = {"schema": REPORT_SCHEMA, "series": inputs["series"], "generated": common.now_iso(), "bars": {}}
    lines = [f"# Gate report: {inputs['series']}", "",
             "Model gates of plan section 12.7, on CUDA only, for the user's personal review: Large against "
             "Compact (12.7.1), Focus only against Focus with Compact (12.7.2), and the replay-only precision "
             "check (12.7.3). Evidence is shown by location only; no source text is quoted.", ""]
    summary: list[str] = ["## Decisions for the user", ""]
    body: list[str] = []
    for pair, (a, b), title, recommend in ((("C-L"), ("C", "L"), "12.7.1 Large on CUDA against Compact on CUDA",
                                            large_recommendation),
                                           (("F-C"), ("F", "C"), "12.7.2 Focus only against Focus with Compact",
                                            compact_recommendation)):
        body += [f"## {title}", ""]
        if a not in arm_docs or b not in arm_docs:
            body += [f"Not run: the replays of arm {a} or {b} are missing.", ""]
            summary.append(f"- {title}: not run.")
            continue
        outcome = outcomes.get(pair)
        if outcome is not None:
            bar = model_bar(outcome, arm_docs[a], arm_docs[b])
            figures["bars"][pair] = bar
            proposed = " (proposed bar; the decision rule is the user's)" if pair == "F-C" else " (proposed bar)"
            body += [f"### The bar{proposed}", ""] + bar_section(bar, a, b)
            summary.append(f"- {title}: {recommend(bar)}")
            body += ["### Test 3: judged sessions", ""] + outcome_section(outcome, a, b)
        else:
            summary.append(f"- {title}: judged sessions not graded yet.")
        if agreements.get(pair):
            body += ["### Test 1: decision agreement", ""] + agreement_section(agreements[pair])
        body += ["### Test 2: first-reply evidence", ""] + evidence_section({a: arm_docs[a], b: arm_docs[b]})
        body += ["### Per-scenario side by side", ""] + side_by_side(a, b, arm_docs[a], arm_docs[b], outcome)
    body += ["## 12.7.3 Precision check: FP32 against FP16, per stage", ""]
    body += precision_section(precisions) if precisions else ["Not run.", ""]
    figures["precision"] = [{k: d[k] for k in ("stage", "decisions", "flips", "flip_share", "essential_differs",
                                               "escalate", "escalation_reasons")} | {"fp32": d["fp32"]["arm"],
                                                                                    "fp16": d["fp16"]["arm"]}
                            for d in precisions]
    summary.append("- 12.7.3 precision: " + ("; ".join(
        f"{d['stage']} {d['fp32']['arm']}: {'escalate' if d['escalate'] else 'no real difference'}" for d in precisions)
        or "not run") + ". Any precision finding goes to the user for the decisions after L7.")
    body += ["## Test 4: performance", ""]
    body += performance_section(perf) if perf else ["Not run.", ""]
    body += ["## Arm identity", ""] + identity_section(arm_docs)
    paths = [Path(p) / "arm-replays.json" if Path(p).is_dir() else Path(p) for p in inputs["arms"].values()]
    paths += [Path(p) for p in (*inputs.get("agreement", {}).values(), *inputs.get("outcomes", {}).values(),
                                *inputs.get("precision", []))]
    if inputs.get("performance"):
        paths.append(Path(inputs["performance"]))
    if inputs.get("switches") and Path(inputs["switches"]).is_dir():
        paths += sorted(Path(inputs["switches"]).glob("*.json"))
    body += ["## Receipts", ""] + table(["File", "SHA-256"], receipts(paths))
    figures["receipts"] = receipts(paths)
    return "\n".join(lines + summary + [""] + body).rstrip() + "\n", figures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True, help="the Markdown report")
    parser.add_argument("--figures", type=Path, help="default: <inputs folder>/gate-report.json")
    args = parser.parse_args(argv)
    text, figures = build(common.read_json(args.inputs))
    common.write_text(args.out, text)
    common.write_json(args.figures or args.inputs.parent / "gate-report.json", figures)
    print(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
