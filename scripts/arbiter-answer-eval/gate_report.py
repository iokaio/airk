# SPDX-License-Identifier: Apache-2.0
"""The gate report of plan 12.7 (LG): Large against Compact, Focus only against Focus with Compact, precision.

Input: the manifest ``make_loop_spec.py gate`` writes
(``<R>/passes/<series>/gate-report-inputs.json``), naming each arm's
``arm-replays.json``, the agreement, outcomes, precision and performance
documents of ``gate.py`` and the switch receipts. Output: one Markdown report
for ``docs/arbiter/answer-evaluation/`` written for the user's personal review,
and a JSON twin with every figure.

Sections: what the bars indicate (recommendations; the user decides, plan
12.9) with the evidence limits (sample size, close calls, undecided and
pending scenarios, tests that did not run); for each
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
  Compact's. If neither outcome condition is met, the report states that
  the bar indicates dropping Large. Statements are neutral (what each bar
  indicates); the user decides.
* 12.7.2: the decision rule is deferred (plan); the report proposes the same
  bar with Compact in Large's place and Focus only in Compact's.
* 12.7.3: an FP32 arm is escalated to judged sessions when more than 5% of its
  stage's decisions flip, or the essential notes covered differ on any
  scenario (``gate.precision``).

No source text is copied: evidence appears as locations (path and lines) and
note ids only. Judges' reasons on bookbuilder and munarium scenarios have
quoted and code text removed (``redact``), because reports quote none of
those repositories' source (munarium is treated as private like bookbuilder
unless the primary agent says otherwise).

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
# Repositories whose source no report may quote: locations and note ids only, judges' reasons redacted.
PRIVATE_REPOS = ("bookbuilder", "munarium")
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


WITHHELD = "[reason withheld: private repository]"


def reason_for(repo: str, text: str | None) -> str:
    """A judge's reason as the report may show it. For a private repository the reason is withheld entirely: on
    23 November 2026 the bookbuilder check found that a reason with its quotes removed still paraphrases the
    project's content. The verdict per note id stays; the reasons stay in the private judging files."""
    if repo in PRIVATE_REPOS:
        return WITHHELD
    return re.sub(r"\s+", " ", str(text or "")).strip()[:2 * MAX_REASON]


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
        return "The 12.7.1 bar indicates dropping Large: no outcome condition was met."
    if not bar["met"]:
        failed = [k for k in ("coverage_condition", "time_condition")
                  if not (all(bar[k].values()) if isinstance(bar[k], dict) else bar[k])]
        return ("The 12.7.1 bar indicates dropping Large: an outcome condition was met, but not the "
                + " and ".join(f.split("_")[0] for f in failed) + " condition.")
    return "The 12.7.1 bar indicates continuing Large: an outcome condition and the coverage and time conditions were met."


def compact_recommendation(bar: dict) -> str:
    if bar["pending"]:
        return f"Not yet decidable: {bar['pending']} scenarios await adjudication."
    if bar["met"]:
        return ("The 12.7.2 bar indicates keeping Compact as the loop's Context stage: it met the proposed bar "
                "against Focus only.")
    return ("The 12.7.2 bar indicates shipping the loop with Focus only: Compact did not meet the proposed bar "
            "against Focus only.")


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


def parts_lines(perf: dict) -> list[str]:
    """How many parts each scenario's Answer split into, per arm. The default token pool funds two parts over
    three rounds; an Answer of more parts can lose card calls in a later round, disclosed as not dispatched
    for ``token_budget``, so the split decides whether the model was funded to the end."""
    out = []
    for name, arm in perf["arms"].items():
        by_scenario = arm.get("parts_by_scenario") or {}
        if not by_scenario:
            continue
        counts = ", ".join(f"{parts} parts: {n}" for parts, n in sorted((arm.get("parts") or {}).items()))
        many = sorted(sid for sid, parts in by_scenario.items() if parts and parts > 2)
        out.append(f"{name}: scenarios by parts of the Answer: {counts}."
                   + (f" More than two parts: {', '.join(many)}." if many else ""))
    if out:
        out.append("The default token pool funds two parts over three rounds. An Answer split into more parts "
                   "can lose card calls in a later round; those decisions are counted as not dispatched "
                   "(see Decisions by source).")
        out.append("")
    return out


def performance_section(perf: dict, concurrency_path: str | None = None) -> list[str]:
    rows = [[name, a["answers"], a["ok"], a["elapsed_ms_median"], a["elapsed_ms_p90"], a["elapsed_ms_max"],
             a["model_ms_per_call_median"], cell(a["rounds"]), cell(a["partials"]) if a["partials"] else "none",
             a["tickets"], cell(a["peak_used_mib"])] for name, a in perf["arms"].items()]
    out = table(["Arm", "Answers", "OK", "Median ms", "p90 ms", "Max ms", "Model ms/call", "Rounds used",
                 "Partials", "Tickets", "Peak MiB"], rows)
    out += parts_lines(perf)
    conc = [[name, r["level"], r["answers"], r["wall_s"], r["answers_per_minute"], r["elapsed_ms_median"],
             r["tickets"], cell(r["partials"]) if r["partials"] else "none", cell(r["peak_used_mib"])]
            for name, runs in perf.get("concurrency", {}).items() for r in runs]
    if conc:
        out += table(["Arm", "Concurrent", "Answers", "Wall s", "Answers/min", "Median ms", "Tickets", "Partials",
                      "Peak MiB"], conc)
        out += ["The concurrency test ran after every judged session, one arm at a time. Each concurrent call "
                "asked a different scenario's question"
                + (f"; path: {concurrency_path}." if concurrency_path else "."), ""]
    else:
        out += ["Concurrency (2 and 4 concurrent Answers): not run, so not passed.", ""]
    switches = [[cell(s.get("from")), cell(s.get("to")), s.get("release_s"), s.get("warm_s"), s.get("total_s")]
                for s in perf.get("switches", [])]
    if switches:
        out += table(["From", "To", "Release s", "Warm s", "Total s"], switches)
    out.append("Per-stage model time is not in the reply; the decision log (service custody) holds it. Peak memory "
               "is device-wide.")
    out.append("")
    return out


COVERAGE_RULES = {
    1: "any line of overlap; a path without a line span covers by a substring match of the symbol",
    2: "an overlap of at least min(5, the note's line count) lines; a path without a line span covers by a "
       "substring match of the symbol",
    3: "an overlap of at least min(5, the note's line count) lines; a path without a line span covers only "
       "when it names the note's symbol as a whole identifier, and never when the note has no symbol",
}


def coverage_rule_statement(arm_docs: dict) -> tuple[list[str], dict]:
    """Which coverage rule counted the reference notes, from each arm's ``arm-replays.json``. Arms scored
    under different rules are named: their coverage figures do not compare."""
    rules = {arm: doc.get("coverage_rule") for arm, doc in sorted(arm_docs.items())}
    distinct = sorted({str(r) for r in rules.values()})
    figures = {"arms": rules, "uniform": len(distinct) <= 1}
    if not rules:
        return ["Coverage rule: no arm replays were read.", ""], figures
    if len(distinct) == 1:
        rule = next(iter(rules.values()))
        text = COVERAGE_RULES.get(rule, "not a rule this report knows")
        return [f"Reference notes are counted under coverage rule {rule} in every arm: {text} "
                "(`evidence.COVERAGE_RULE`).", ""], figures
    listed = ", ".join(f"{arm} rule {rule}" for arm, rule in rules.items())
    return [f"The arms were scored under different coverage rules ({listed}), so their coverage figures do "
            "not compare. Replay the arms under one rule before reading the coverage conditions.", ""], figures


def decision_source_section(arm_docs: dict) -> tuple[list[str], dict]:
    """Per arm and stage: decisions the model took, decisions that fell back to the deterministic order after
    the model was asked, decisions never sent to the model, and decisions taken without a model
    (``gate.decision_sources``)."""
    figures = {arm: gate.decision_sources(doc) for arm, doc in sorted(arm_docs.items())}
    rows = []
    for arm, found in figures.items():
        for stage, counts in list(found["stages"].items()) + [("all stages", found["total"])]:
            reasons = found["fallback_reasons"].get(stage) if stage != "all stages" else None
            rows.append([arm, stage, counts["model"], counts["fell_back"], counts["not_dispatched"],
                         counts["no_model"], counts["other"],
                         ", ".join(f"{k} {v}" for k, v in reasons.items()) if reasons else "–"])
    out = table(["Arm", "Stage", "Model took", "Fell back after the model was asked", "Not dispatched", "No model",
                 "Other source", "Reasons (included decisions)"], rows) if rows else ["No arm reply carried "
                                                                                    "decision counts.", ""]
    out.append("Counts are every logged decision of the arm's first replies (`decision_log.counts`). A decision "
               "fell back when the model was asked and the deterministic order stood in: the model abstained or "
               "was unavailable. A decision was not dispatched when a model decision was due and the call was "
               "never sent: no token budget, no time or no model for that call. Both took the deterministic "
               "order, and neither is a decision the loop takes without a model. The reasons are the typed "
               "reasons of the decisions each reply includes, so they can cover fewer decisions than the counts. "
               "A slower model can be shed more often, so read this beside the outcome tables.")
    partial = [f"{arm}: {found['fell_back_with_reason']} of "
               f"{found['total']['fell_back'] + found['total']['not_dispatched']}"
               for arm, found in figures.items()
               if found["fell_back_with_reason"] != found["total"]["fell_back"] + found["total"]["not_dispatched"]]
    if partial:
        out.append("Fallen-back or not-dispatched decisions with a reason in the reply: " + "; ".join(partial)
                   + ".")
    missing = [arm for arm, found in figures.items() if found["replies_without_log_counts"]]
    if missing:
        out.append(f"Replies without decision counts in their log summary: {', '.join(missing)}.")
    out.append("")
    out += presence_section(figures)
    return out, figures


def presence_section(figures: dict) -> list[str]:
    """Per arm, the replies by whether the Context model took part. A reply reports ``context_absent`` both
    when no Context model was present and when one was present and no call reached it; the gate must not read
    the second as Focus only, so the two are counted apart (``loop_reply.context_presence``)."""
    if not figures:
        return []
    rows = [[arm, found["replies"], found["presence"]["model_took_decisions"],
             found["presence"]["model_present_no_call_dispatched"], found["presence"]["model_present_none_taken"],
             found["presence"]["no_model"]] for arm, found in figures.items()]
    out = table(["Arm", "Replies", "Model took decisions", "Model present, no call dispatched",
                 "Model present, took none (unavailable or abstained)", "No model (Focus only)"], rows)
    out.append("A reply counts as \"model present, no call dispatched\" when decisions were due to a Context "
               "model and every one was not dispatched. Such a reply carries the same `context_absent` "
               "degradation as a Focus-only reply and is not one: the arm had the model and did not reach it.")
    for arm, found in figures.items():
        for state, ids in sorted(found["presence_scenarios"].items()):
            out.append(f"{arm}, {state.replace('_', ' ')}: {', '.join(str(i) for i in ids)}.")
        for stage, ids in found["stages_without_model"].items():
            out.append(f"{arm}, stage {stage}: decisions were due to the model and it took none in "
                       f"{len(ids)} replies ({', '.join(str(i) for i in ids)}).")
    out.append("")
    return out


def packing_section(arm_docs: dict) -> tuple[list[str], dict]:
    """Reply size and what Union cut, per arm and repository, as distributions (``gate.reply_packing``)."""
    figures = {arm: gate.reply_packing(doc) for arm, doc in sorted(arm_docs.items())}

    def spread(value) -> str:
        return "–" if not value else f"{value['min']} / {value['median']} / {value['max']}"

    rows = []
    for arm, found in figures.items():
        for scope, group in [("all", found["all"]), *sorted(found["by_repo"].items())]:
            if group["replies"]:
                rows.append([arm, scope, group["replies"], spread(group["bytes"]), spread(group["span_rows"]),
                             f"{group['replies_with_truncated_rows']} ({spread(group['rows_truncated'])})",
                             f"{group['replies_with_hits_not_rows']} ({spread(group['hits_not_rows'])})",
                             spread(group["repeats_merged"]), spread(group["overlaps_trimmed"]),
                             f"{group['replies_with_omitted_evidence']} ({spread(group['omitted_evidence'])})",
                             spread(group["essential_per_kib"]), spread(group["helpful_per_kib"])])
    if not rows:
        return ["No arm reply carried packing facts.", ""], figures
    out = table(["Arm", "Repository", "Replies", "Bytes", "Span rows", "Replies with truncated rows (rows)",
                 "Replies with hits not shown (hits)", "Repeats merged", "Overlaps trimmed",
                 "Replies with omitted records (records)", "Essential notes per KiB", "Helpful notes per KiB"], rows)
    out += ["Each cell with three figures is minimum / median / maximum over the first replies. The reply does "
            "not state its trimming level, so the table shows what can be observed: rows marked truncated, hits "
            "that are not rows (uncited hits are dropped from the first trimming level on), rows whose text "
            "another row carries, rows trimmed of an overlap, and records left out of the frame. Per-reply "
            "figures are in the report's JSON (`reply_packing`). The agents' later reads against these rows are "
            "counted in the next section but one.", ""]
    return out, figures


def later_reads_section(pass_dir: str | None) -> tuple[list[str], dict]:
    """The agents' reads after an Answer reply, per arm: to a location the reply had delivered as a truncated row,
    to a row it delivered whole, or elsewhere (``coverage.later_reads`` in each judged session's coverage.json;
    the user's question whether Answer needs a compression step)."""
    index_path = Path(pass_dir) / "pass-index.json" if pass_dir else None
    if index_path is None or not index_path.is_file():
        return ["Not counted: the pass index is missing.", ""], {}
    keys = ("answer_replies", "reads_after", "to_truncated_row", "to_whole_row", "elsewhere", "no_location")
    figures: dict[str, dict] = {}
    for row in common.read_json(index_path).get("results") or []:
        path = Path(row.get("coverage") or "")
        found = (common.read_json(path).get("later_reads") if path.is_file() else None) or None
        arm = figures.setdefault(row.get("arm") or row.get("configuration"), {k: 0 for k in keys} | {
            "sessions": 0, "sessions_not_counted": 0})
        if found is None:
            arm["sessions_not_counted"] += 1
            continue
        arm["sessions"] += 1
        for key in keys:
            arm[key] += int(found.get(key) or 0)
    rows = [[arm, f["sessions"], f["reads_after"], f["to_truncated_row"], f["to_whole_row"], f["elsewhere"],
             f["no_location"], f["sessions_not_counted"]] for arm, f in sorted(figures.items())]
    out = table(["Arm", "Sessions", "Reads after an Answer", "To a truncated row", "To a whole row", "Elsewhere",
                 "No location", "Not counted"], rows)
    out += ["A read goes to a truncated row when one of its locations overlaps a row an earlier Answer reply in the "
            "session delivered with `truncated: true`; a read that a better cut would have saved is one of these.", ""]
    return out, figures


def read_failure_section(arm_docs: dict) -> tuple[list[str], dict]:
    """Records whose read failed, per arm and kind (``operation:code``), with the replies that had any. Read
    from each first reply's loop summary (``loop_reply.read_failures``); a reply from before that field shows
    as not counted."""
    figures: dict[str, dict] = {}
    rows = []
    for arm, doc in sorted(arm_docs.items()):
        kinds: dict[str, int] = {}
        replies = with_failures = uncounted = 0
        for row in doc.get("rows") or []:
            loop = row.get("loop")
            if not row.get("ok") or not isinstance(loop, dict):
                continue
            replies += 1
            found = loop.get("read_failures")
            if found is None:
                uncounted += 1
                continue
            with_failures += 1 if found else 0
            for kind, n in found.items():
                kinds[kind] = kinds.get(kind, 0) + int(n)
        figures[arm] = {"replies": replies, "replies_with_failures": with_failures, "not_counted": uncounted,
                        "by_kind": dict(sorted(kinds.items()))}
        service = {k: n for k, n in kinds.items() if k.endswith(":read_failed")}
        other = {k: n for k, n in kinds.items() if k not in service}
        rows.append([arm, replies, with_failures, uncounted,
                     ", ".join(f"{k} x{n}" for k, n in sorted(service.items())) or "none",
                     ", ".join(f"{k} x{n}" for k, n in sorted(other.items())) or "none"])
    if not rows:
        return ["No arm replies.", ""], figures
    out = table(["Arm", "Replies", "Replies with a failed read", "Not counted", "Service failures (read_failed)",
                 "Refused or other (for example invalid_arguments)"], rows)
    out += ["A failed read is a record whose `canonical_result` holds a `failure`, counted by operation and code. "
            "`read_failed` is a failure in the service; `invalid_arguments` is a read the service refused as "
            "invalid (for example a range across document locations). The arm check refuses a reply with either.", ""]
    return out, figures


def budget_statement(arm_docs: dict) -> tuple[list[str], dict]:
    """The exact budget the arms' Answers were sent, from each arm's ``arm-replays.json`` (``budgets_sent``).
    Arms sent different budgets are named: their figures do not compare."""
    sent = {arm: doc.get("budgets_sent") for arm, doc in sorted(arm_docs.items())}
    distinct = {common.canonical_json(row["budget"]): row["budget"] for rows in sent.values() for row in rows or []}
    unknown = [arm for arm, rows in sent.items() if not rows]
    figures = {"arms": sent, "uniform": len(distinct) == 1 and not unknown, "not_recorded": unknown}
    if not sent:
        return ["Budget sent: no arm replays were read.", ""], figures
    out = []
    if len(distinct) == 1:
        budget = next(iter(distinct.values()))
        out.append("Every Answer of " + (", ".join(a for a in sent if a not in unknown) or "no arm")
                   + " was sent this budget: " + ", ".join(f"{k} {v}" for k, v in sorted(budget.items())) + ".")
    elif distinct:
        out.append("The arms' Answers were sent different budgets, so their figures do not compare: "
                   + "; ".join(f"{arm}: " + " | ".join(", ".join(f"{k} {v}" for k, v in sorted(r['budget'].items()))
                                                       + f" ({r['calls']} calls)" for r in rows)
                               for arm, rows in sent.items() if rows) + ".")
    if unknown:
        out.append(f"The budget sent was not recorded for {', '.join(unknown)}.")
    return out + [""], figures


def evidence_limits(arm_docs: dict, outcomes: dict, precisions: list, perf: dict | None,
                    precision_note: str | None = None) -> list[str]:
    """How far the figures carry: sample size, close calls, undecided and pending scenarios, arms whose identity
    was not met, and tests that did not run. Stated so the user can weigh the recommendations."""
    out = []
    for pair, outcome in sorted(outcomes.items()):
        if not outcome:
            out.append(f"- {pair}: no judged sessions graded, so Rubric A and correct finishes are unknown.")
            continue
        seconds = sum(1 for s in outcome.get("scenarios", []) if "second_winner" in s)
        out.append(f"- {pair}: {outcome.get('rows', len(outcome.get('scenarios', [])))} scenarios, one judged session "
                   f"each, two judges; {len(outcome.get('close_calls') or [])} close calls, {seconds} with a second "
                   f"session, {outcome.get('undecided', 0)} undecided and {outcome.get('pending', 0)} pending "
                   "adjudication. A margin of one or two scenarios is within what a second sample can change.")
    bad = [arm for arm, doc in arm_docs.items() if not doc.get("arm_ok")]
    if bad:
        out.append(f"- Arm identity not met on some replies for {', '.join(bad)}: those arms' figures may not "
                   "describe the arm (see Arm identity).")
    starved = {arm: sum(1 for r in doc["rows"] if ((r.get("loop") or {}).get("context_presence") or {}).get(
        "state") in ("model_present_no_call_dispatched", "model_present_none_taken")) for arm, doc in arm_docs.items()}
    if any(starved.values()):
        out.append("- Replies in which the arm's Context model was present and took no decision: "
                   + ", ".join(f"{arm} {n}" for arm, n in sorted(starved.items()) if n)
                   + ". They ran on the deterministic order and are not Focus-only replies (see Decisions by "
                     "source).")
    flagged = {arm: len(doc.get("arm_flagged") or []) for arm, doc in arm_docs.items()}
    if any(flagged.values()):
        out.append("- Scenarios where one stage's model decisions were all shed: "
                   + ", ".join(f"{arm} {n}" for arm, n in sorted(flagged.items()) if n) + " (see Arm identity).")
    if len({common.canonical_json(row["budget"]) for doc in arm_docs.values()
            for row in doc.get("budgets_sent") or []}) > 1:
        out.append("- The arms' Answers were sent different budgets, so their figures do not compare.")
    missing = [arm for arm in ("F", "C", "L") if arm not in arm_docs]
    if missing:
        out.append(f"- Arms not run: {', '.join(missing)}.")
    if len({str(doc.get("coverage_rule")) for doc in arm_docs.values()}) > 1:
        out.append("- The arms were scored under different coverage rules, so their coverage figures do not "
                   "compare.")
    if not precisions:
        out.append("- The precision check (12.7.3) did not run" + (f": {precision_note}" if precision_note
                                                                     else "."))
    if not perf or not perf.get("concurrency"):
        out.append("- The concurrency test (12.7.1 test 4) did not run, so it is not passed.")
    out.append("- First-reply evidence and decisions come from one replay per scenario and arm; the loop is not "
               "repeated, so run-to-run variation is not measured. Peak memory is device-wide, with the arms' "
               "lifetimes side by side on one GPU.")
    out += [f"- {line}" for line in STANDING_LIMITS]
    return out + [""]


# Limits that hold for every LG report (coordinator, 23 November 2026), stated whatever the figures show.
STANDING_LIMITS = (
    "One judged session per scenario and arm (a second session only on close calls), so a single session can "
    "change a cell's outcome.",
    "The reference notes for munarium and bookbuilder were written and reviewed as the campaign folder describes; "
    "coverage counts only those notes.",
    "Known open items from the shakedown: record-level identities (which record keeps a repeated text, read "
    "sizes and digests, basis digests) differ run to run while the delivered evidence is equal; the source "
    "verification receipt takes 20 to 29 KB of most replies; a stage idle unload is prevented by the harness "
    "(idle unload off, stage checks before every Answer step and session), not by the product.",
    "Concurrency was measured on the unqueued path (no job table), with a different scenario's question in each "
    "concurrent call.",
)

FIRST_CALL_CONVENTION = (
    "**First call.** Every session's first call and every replay is the same request: `airk_answer` with the "
    "scenario's instruction as written, the default budget from the cases file, and the product's defaults for "
    "everything else (default response mode, default source verification). Arguments an agent added to its first "
    "call were recorded and not sent; from the second call on, the agent chose its own arguments (plan 3.5).")


def identity_section(docs: dict[str, dict]) -> list[str]:
    rows = []
    for arm, doc in docs.items():
        problems = sorted({p for r in doc["rows"] for p in r.get("arm_problems") or []})
        flagged = sorted(r["scenario_id"] for r in doc["rows"] if r.get("arm_flags"))
        rows.append([arm, doc["configuration"], doc.get("arm_ok"), "; ".join(problems[:4]) if problems else "none",
                     ", ".join(flagged) if flagged else "none"])
    return table(["Arm", "Configuration", "Identity met", "Problems", "Flagged scenarios"], rows) + [
        "A flagged scenario is the arm's own reply in which one stage's decisions were due to the model and it "
        "took none of them (not dispatched, unavailable or abstained), while it took decisions in another stage. "
        "Identity is not met when the model took no decision in the whole reply.", ""]


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
             "check (12.7.3). Evidence is shown by location only; no source text is quoted.", "",
             "The bars are applied as the plan proposes them. What follows are recommendations: the user decides "
             "(plan 12.9).", ""]
    rule_lines, figures["coverage_rule"] = coverage_rule_statement(arm_docs)
    lines += rule_lines
    budget_lines, figures["budgets_sent"] = budget_statement(arm_docs)
    lines += budget_lines
    lines += [FIRST_CALL_CONVENTION, ""]
    summary: list[str] = ["## What the bars indicate (recommendations; the user decides)", ""]
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
    precision_note = inputs.get("precision_note")
    body += precision_section(precisions) if precisions else [
        f"Not run. {precision_note}" if precision_note else "Not run.", ""]
    figures["precision"] = [{k: d[k] for k in ("stage", "decisions", "flips", "flip_share", "essential_differs",
                                               "escalate", "escalation_reasons")} | {"fp32": d["fp32"]["arm"],
                                                                                    "fp16": d["fp16"]["arm"]}
                            for d in precisions]
    summary.append("- 12.7.3 precision: " + ("; ".join(
        f"{d['stage']} {d['fp32']['arm']}: {'escalate' if d['escalate'] else 'no real difference'}" for d in precisions)
        or "not run") + ". Any precision finding goes to the user for the decisions after L7.")
    source_lines, figures["decision_sources"] = decision_source_section(arm_docs)
    body += ["## Decisions by source, per arm and stage", ""] + source_lines
    body += ["## Test 4: performance", ""]
    body += performance_section(perf, inputs.get("concurrency_path")) if perf else ["Not run.", ""]
    packing_lines, figures["reply_packing"] = packing_section(arm_docs)
    body += ["## Reply size and what was cut, per arm and repository", ""] + packing_lines
    failure_lines, figures["read_failures"] = read_failure_section(arm_docs)
    body += ["## Failed reads in the first replies", ""] + failure_lines
    later_lines, figures["later_reads"] = later_reads_section(inputs.get("pass_dir"))
    body += ["## Agents' later reads against the Answer reply's rows", ""] + later_lines
    body += ["## Arm identity", ""] + identity_section(arm_docs)
    summary += ["", "### Evidence limits", ""] + evidence_limits(arm_docs, outcomes, precisions, perf, inputs.get("precision_note"))[:-1]
    paths = [Path(p) / "arm-replays.json" if Path(p).is_dir() else Path(p) for p in inputs["arms"].values()]
    paths += [Path(p) for p in (*inputs.get("agreement", {}).values(), *inputs.get("outcomes", {}).values(),
                                *inputs.get("precision", []))]
    if inputs.get("performance"):
        paths.append(Path(inputs["performance"]))
    for key in ("switches", "lifetimes"):
        if inputs.get(key) and Path(inputs[key]).is_dir():
            paths += sorted(Path(inputs[key]).glob("*.json"))
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
