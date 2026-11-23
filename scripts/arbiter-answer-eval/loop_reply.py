# SPDX-License-Identifier: Apache-2.0
"""Answer /3 (the local multi-round loop) replies, read for the evaluation.

Shapes are those of the Core contract, ``crates/airk-core/src/arbiter/CONTRACT.md``
section "Answer loop wire contract version 3", the generated schema
``packages/contracts-gen/arbiter.schema.json`` (``ContextResponseV3``) and the
Rust-emitted cases ``fixtures/split/arbiter/answer_loop_cases.json``. Nothing
here validates a reply the way the kernel does (``validate_with_log``); the
harness reads what it measures and reports what it cannot read.

* A reply is ``airk.arbiter.context/3`` with ``parts`` (sub-needs with their
  coverage and cited record ids), ``records`` (``LoopEvidence``: id,
  operation, arguments, role, canonical_result; no scope), ``decisions``
  (compact ``DecisionRecord``), ``decision_log`` (the custody log's summary),
  ``degradations``, ``partial`` and ``ticket``.
* A ticket reply (queued at admission) carries no parts, records or
  decisions. The caller attaches to it with ``continuation`` = the ticket id.
* A decision's key is (stage, part, need, round). Its choice is the set of
  chosen cards (symbol, class). Two decisions are a like-for-like comparison
  only when their ``roster_digest`` (the presented cards) is equal.

``check_arm`` states what a gate arm (12.7.1, 12.7.2, 12.7.3) requires of a
reply: Focus only (no Context model decides anything), or one Context model
on CUDA at one precision taking every model decision by top score with no
abstention.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import evidence  # noqa: E402

RESPONSE_SCHEMA = "airk.arbiter.context/3"
REQUEST_SCHEMA = "airk.arbiter.context.request/3"
PACKET_SCHEMA = "airk.arbiter.packet/2"
# The request and its default budget are common.ANSWER_SCHEMA and common.ANSWER_BUDGET_DEFAULT.
REPLY_FRAME_BYTES = 131072
CARD_STAGES = ("split", "plan", "coverage", "follow_up")
COVERAGE_KINDS = ("covered", "partial", "missing")
# ``arbiter`` is the Context model value since step LH (change-log entry 62). The LG series replies name the
# values before it (``context_compact``, ``context_large``) and stay checkable.
CONTEXT_MODELS = ("arbiter", "context_compact", "context_large")
PRECISIONS = ("float16", "float32")


def is_v3(body: Any) -> bool:
    return isinstance(body, dict) and body.get("schema") == RESPONSE_SCHEMA


def is_ticket(body: Any) -> bool:
    return is_v3(body) and isinstance(body.get("ticket"), dict)


def ticket_wait(body: Any) -> tuple[bool, int, str | None]:
    """(True, retry_after_ms, ticket id) for a queued ticket reply; the caller re-asks with continuation."""
    if not is_ticket(body):
        return False, 0, None
    ticket = body["ticket"]
    return True, int(ticket.get("retry_after_ms") or 0), ticket.get("id")


def _int(value: Any) -> int | None:
    return value if isinstance(value, int) and not isinstance(value, bool) else None


# ---------------------------------------------------------------- decisions


def chosen(decision: dict) -> tuple:
    cards = decision.get("chosen") or []
    return tuple(sorted((str(c.get("symbol")), str(c.get("class"))) for c in cards if isinstance(c, dict)))


def decision_key(decision: dict) -> tuple:
    return (decision.get("stage"), _int(decision.get("part")), _int(decision.get("need")), _int(decision.get("round")))


def decisions(body: Any) -> list[dict]:
    """The reply's included decisions, normalized, in ordinal order."""
    if not is_v3(body):
        return []
    out = []
    for raw in body.get("decisions") or []:
        if not isinstance(raw, dict):
            continue
        source = raw.get("source") if isinstance(raw.get("source"), dict) else {}
        out.append({"ordinal": _int(raw.get("ordinal")), "key": decision_key(raw), "stage": raw.get("stage"),
                    "part": _int(raw.get("part")), "need": _int(raw.get("need")), "round": _int(raw.get("round")),
                    "chosen": chosen(raw), "roster": raw.get("roster_digest"), "source": source.get("kind"),
                    "mode": source.get("mode"), "fallback": source.get("fallback"), "rule": source.get("rule"),
                    "reason": source.get("reason"), "model": raw.get("model"), "device": raw.get("device"),
                    "precision": raw.get("precision"), "top": raw.get("top"), "runner_up": raw.get("runner_up"),
                    "margin": raw.get("margin"), "applicability": raw.get("applicability"),
                    "work_unknown": bool(raw.get("work_unknown"))})
    return sorted(out, key=lambda d: d["ordinal"] or 0)


# How a decision's source counts in the gate report (a slower model may be refused or shed more often):
# "model" took the decision; "fell_back" means the model was asked and the deterministic order stood in
# (sources ``abstained`` and ``unavailable``); "not_dispatched" means a model decision was due and the call
# was never sent (source ``not_dispatched``: token budget, deadline or no model for that call), counted apart
# from both; "no_model" is a decision the loop takes without a model (source ``deterministic``). The three
# fallback sources carry their fallback rule and a typed reason.
SOURCE_CLASSES = {"model": "model", "abstained": "fell_back", "unavailable": "fell_back",
                  "not_dispatched": "not_dispatched", "deterministic": "no_model"}
SOURCE_CLASS_ORDER = ("model", "fell_back", "not_dispatched", "no_model", "other")
REASONED_CLASSES = ("fell_back", "not_dispatched")


def source_class(kind: Any) -> str:
    return SOURCE_CLASSES.get(str(kind), "other")


def fallback_reasons(body: Any) -> dict[str, dict[str, int]]:
    """``{stage: {"<source>/<reason>": count}}`` for the reply's included decisions that fell back or were
    not dispatched.

    The typed reason (``unavailable``, ``deadline``, ``token_budget``, ``abstained``, ...) is per decision, so
    it is known only for the decisions the reply includes; the decision log's counts (``summary``'s
    ``decision_counts``) cover every logged decision, by source only."""
    out: dict[str, dict[str, int]] = {}
    for decision in decisions(body):
        if source_class(decision["source"]) in REASONED_CLASSES:
            stage = out.setdefault(str(decision["stage"]), {})
            reason = f"{decision['source']}/{decision['reason']}"
            stage[reason] = stage.get(reason, 0) + 1
    return out


# Sources of a decision that was due to a Context model, whoever took it in the end.
MODEL_DUE_SOURCES = ("model", "abstained", "unavailable", "not_dispatched")
# Whether a Context model took part in a reply. The reply sets the ``context_absent`` degradation both when no
# Context model was present and when one was present but every call was not dispatched, so the degradation
# alone cannot tell a Focus-only reply from a model that was never reached. The decision counts by source
# can: a decision that was due to a model names it, also when it was not dispatched.
PRESENCE_STATES = ("model_took_decisions", "model_present_no_call_dispatched", "model_present_none_taken",
                   "no_model")


def log_counts(body: Any) -> list[dict]:
    """The decision log's count rows ``{stage, source, count[, model]}``: every logged decision."""
    log = body.get("decision_log") if isinstance(body, dict) and isinstance(body.get("decision_log"), dict) else {}
    return [{"stage": str(row.get("stage")), "source": str(row.get("source")), "count": int(row.get("count") or 0),
             "model": row.get("model")} for row in log.get("counts") or [] if isinstance(row, dict)]


def context_presence(body: Any) -> dict | None:
    """Whether the Context model took part, per stage and overall. None for a ticket or a non-/3 reply.

    Counted from the decision log's counts (every logged decision); a reply whose log carries no counts is
    counted from the decisions it includes, and says so in ``from``. ``state`` is one of ``PRESENCE_STATES``:
    the model took at least one decision; a model was due and no call was dispatched; a model was due and took
    none for other reasons (it was unavailable, at compile time or later, or abstained every time); or no
    decision was due to a model (Focus only). ``stages_without_model`` lists the stages where decisions were
    due to a model and it took none."""
    if not is_v3(body) or is_ticket(body):
        return None
    rows, origin = log_counts(body), "decision_log.counts"
    if not rows:
        rows = [{"stage": str(d["stage"]), "source": str(d["source"]), "count": 1, "model": d["model"]}
                for d in decisions(body)]
        origin = "included_decisions"
    stages: dict[str, dict[str, int]] = {}
    models = set()
    for row in rows:
        if row["source"] not in MODEL_DUE_SOURCES and not row["model"]:
            continue
        if row["model"]:
            models.add(str(row["model"]))
        if row["source"] in MODEL_DUE_SOURCES:
            stage = stages.setdefault(row["stage"], {source: 0 for source in MODEL_DUE_SOURCES})
            stage[row["source"]] += row["count"]
    total = {source: sum(stage[source] for stage in stages.values()) for source in MODEL_DUE_SOURCES}
    due = sum(total.values())
    if total["model"]:
        state = "model_took_decisions"
    elif not due:
        state = "no_model"
    elif total["not_dispatched"] == due:
        state = "model_present_no_call_dispatched"
    else:
        state = "model_present_none_taken"
    return {"state": state, "due": due, "by_source": total, "models": sorted(models),
            "stages": {stage: stages[stage] for stage in sorted(stages)},
            "stages_without_model": sorted(s for s, c in stages.items() if sum(c.values()) and not c["model"]),
            "from": origin}


def _due_text(counts: dict) -> str:
    return (f"not dispatched {counts['not_dispatched']}, unavailable {counts['unavailable']}, "
            f"abstained {counts['abstained']}")


def current_decisions(body: Any) -> dict[tuple, dict]:
    """The current decision per (stage, part, need, round): applicability current, else the latest ordinal."""
    found: dict[tuple, dict] = {}
    for decision in decisions(body):
        previous = found.get(decision["key"])
        if previous is None or decision["applicability"] == "current" or previous["applicability"] != "current":
            found[decision["key"]] = decision
    return found


# ---------------------------------------------------------------- parts, sub-needs, records


def sub_needs(body: Any) -> list[dict]:
    if not is_v3(body):
        return []
    out = []
    for part in body.get("parts") or []:
        if not isinstance(part, dict):
            continue
        for need in part.get("sub_needs") or []:
            if not isinstance(need, dict):
                continue
            out.append({"part": _int(part.get("id")), "need": _int(need.get("id")), "kind": need.get("kind"),
                        "text": need.get("text"), "coverage": need.get("coverage"),
                        "evidence": [e for e in need.get("evidence") or [] if isinstance(e, str)],
                        "evidence_omitted": _int(need.get("evidence_omitted")) or 0,
                        "unrun_reads": [dict(u) for u in need.get("unrun_reads") or [] if isinstance(u, dict)],
                        "_raw": need})
    return out


def records(body: Any) -> list[dict]:
    if not is_v3(body):
        return []
    return [r for r in body.get("records") or [] if isinstance(r, dict)]


def record_spans(body: Any) -> dict[str, list[evidence.Span]]:
    """Spans per record id, from each record's canonical result (arguments are not evidence)."""
    return {str(r.get("id")): evidence.extract(r.get("canonical_result")) for r in records(body)}


def sub_need_spans(body: Any) -> dict[tuple, list[evidence.Span]]:
    """Spans per (part, need): its cited records' spans plus any compact spans the sub-need carries itself
    (changes entry 16 lets L3 send loop evidence as compact spans per sub-need)."""
    by_record = record_spans(body)
    out = {}
    for need in sub_needs(body):
        spans: list[evidence.Span] = []
        for record_id in need["evidence"]:
            spans.extend(by_record.get(record_id, []))
        own = {k: v for k, v in need["_raw"].items() if k not in ("evidence", "unrun_reads", "text")}
        spans.extend(evidence.extract(own))
        out[(need["part"], need["need"])] = spans
    return out


FAILURE_CODE = re.compile(r'\\*"error\\*"\s*:\s*\{\\*"code\\*"\s*:\s*\\*"([A-Za-z0-9_]+)')


def _failure_code(failure: Any) -> str:
    """The error code of a failed read's recorded envelope (``canonical_result.failure``: the tool result's
    JSON text), or ``unknown``. The service keeps only the first 512 characters of that text (s5 shakedown),
    so a cut envelope is read by its leading ``{"error":{"code":...`` member."""
    try:
        envelope = json.loads(failure) if isinstance(failure, str) else failure
        text = envelope["content"][0]["text"] if isinstance(envelope, dict) and "content" in envelope else None
        body = json.loads(text) if isinstance(text, str) else envelope
        code = body.get("error", {}).get("code") if isinstance(body, dict) else None
        if code:
            return str(code)
    except (ValueError, KeyError, IndexError, TypeError, AttributeError):
        pass
    found = FAILURE_CODE.search(failure) if isinstance(failure, str) else None
    return found.group(1) if found else "unknown"


def read_failures(body: Any) -> dict[str, int]:
    """Records whose read failed, counted by ``operation:code`` (s5 shakedown, 23 November 2026: every search
    read of a reply failed ``read_failed`` in the service and the reply still looked like an answer)."""
    out: dict[str, int] = {}
    for record in records(body):
        result = record.get("canonical_result")
        if isinstance(result, dict) and result.get("failure") is not None:
            key = f"{record.get('operation') or 'unknown'}:{_failure_code(result['failure'])}"
            out[key] = out.get(key, 0) + 1
    return dict(sorted(out.items()))


def record_uses(body: Any) -> list[dict]:
    if not is_v3(body):
        return []
    return [u for u in body.get("record_uses") or [] if isinstance(u, dict)]


def degradation_kinds(body: Any) -> list[str]:
    if not is_v3(body):
        return []
    return sorted({str(d.get("kind")) for d in body.get("degradations") or [] if isinstance(d, dict)})


def packing(body: Any) -> dict | None:
    """How the reply was packed, for the question whether Answer needs a compression step: what Union cut,
    merged or left out, as far as a reply shows it. The reply does not state its trimming level, so the
    observable facts are counted: span rows and how many are ``truncated``; hits that are not rows
    (``hits_total`` larger than the row count: uncited hits dropped from trimming level 1 on); rows whose text
    another row carries (``union.repeats_merged``) and rows trimmed of an overlap (``union.overlaps_trimmed``);
    and the records left out (``omitted_evidence``)."""
    if not is_v3(body) or is_ticket(body):
        return None
    rows = truncated = hits = 0
    for record in records(body):
        result = record.get("canonical_result")
        if evidence.is_loop_projection(result):
            spans = [row for row in result["spans"] if isinstance(row, dict)]
            rows += len(spans)
            truncated += sum(1 for row in spans if row.get("truncated") is True)
            hits += _int(result.get("hits_total")) or 0
    union = body.get("union") if isinstance(body.get("union"), dict) else {}
    return {"records": len(records(body)), "span_rows": rows, "rows_truncated": truncated, "hits_total": hits,
            "hits_not_rows": max(0, hits - rows), "repeats_merged": _int(union.get("repeats_merged")) or 0,
            "overlaps_trimmed": _int(union.get("overlaps_trimmed")) or 0,
            "omitted_evidence": len(body.get("omitted_evidence") or [])}


def summary(body: Any) -> dict | None:
    """What a v3 reply says about its own run: parts and stops, sub-needs, partial, ticket, decisions, work."""
    if not is_v3(body):
        return None
    needs = sub_needs(body)
    parts = [p for p in body.get("parts") or [] if isinstance(p, dict)]
    log = body.get("decision_log") if isinstance(body.get("decision_log"), dict) else {}
    work = body.get("work") if isinstance(body.get("work"), dict) else {}
    partial = body.get("partial") if isinstance(body.get("partial"), dict) else None
    counts: dict[str, dict[str, int]] = {}
    for row in log.get("counts") or []:
        if isinstance(row, dict):
            stage = counts.setdefault(str(row.get("stage")), {})
            stage[str(row.get("source"))] = stage.get(str(row.get("source")), 0) + int(row.get("count") or 0)
    return {
        "ticket": is_ticket(body), "partial": None if partial is None else {
            "stage": partial.get("stage"), "reason": partial.get("reason"), "part": partial.get("part"),
            "continuation": partial.get("continuation")},
        "parts": len(parts), "stops": [p.get("stop") for p in parts],
        "shortened_parts": sum(1 for p in parts if p.get("shortened")),
        "rounds_per_part": [len(p.get("rounds") or []) for p in parts],
        "sub_needs": {kind: sum(1 for n in needs if n["coverage"] == kind) for kind in COVERAGE_KINDS},
        "sub_need_kinds": sorted({str(n["kind"]) for n in needs}),
        "unrun_reads": sum(len(n["unrun_reads"]) for n in needs),
        "records": len(records(body)), "omitted_evidence": len(body.get("omitted_evidence") or []),
        "decisions_total": log.get("total"), "decisions_included": log.get("included"),
        "decisions_omitted": log.get("omitted"), "decision_counts": counts,
        "fallback_reasons": fallback_reasons(body), "context_presence": context_presence(body),
        "packing": packing(body),
        "card_calls": log.get("card_calls"), "focus_calls": log.get("focus_calls"),
        "focus_pairs": log.get("focus_pairs"), "model_timings": log.get("timings") or {},
        "work_unknown": log.get("work_unknown"), "log_digest": log.get("digest"),
        "degradations": degradation_kinds(body), "read_failures": read_failures(body),
        "work": {k: work.get(k) for k in ("calls", "fanout", "rounds", "input_tokens", "candidates", "bytes",
                                          "compile_us", "inference_us", "policy_us", "queue_us")},
        "elapsed_ms": body.get("elapsed_ms"), "union_dedup": (body.get("union") or {}).get("dedup_count"),
        "conflicts": len((body.get("union") or {}).get("conflicts") or []),
    }


# ---------------------------------------------------------------- arms


def arm_requirement(arm: dict) -> dict:
    """Normalize a spec's ``arm_check``: ``{"context": null}`` (arm F) or
    ``{"context": "arbiter"|"context_compact"|"context_large", "device": "cuda", "precision": "float16"|"float32",
    "mode": "top_score"}`` (arms C and L, and the precision arms)."""
    context = arm.get("context")
    if context is not None and context not in CONTEXT_MODELS:
        raise ValueError(f"unknown Context model {context!r}")
    precision = arm.get("precision")
    if precision is not None and precision not in PRECISIONS:
        raise ValueError(f"unknown precision {precision!r}")
    # ``focus``: True when the arm stages Focus (it must take part), False when the arm runs without Focus (arm A,
    # Arbiter only: Focus must be absent), absent when the arm does not say (older specs; not checked).
    focus = arm.get("focus")
    if focus not in (True, False, None):
        raise ValueError(f"the arm's focus requirement is true, false or absent, not {focus!r}")
    return {"context": context, "device": arm.get("device", "cuda"), "precision": precision,
            "mode": arm.get("mode", "top_score"), "focus": focus}


# A read the service failed to run (not a scope refusal or an empty result): its evidence is missing for a
# reason that has nothing to do with the arm, so the reply cannot stand for the arm.
SERVICE_READ_FAILURES = ("read_failed",)


def stage_problems(body: Any, want: dict) -> list[str]:
    """Problems common to every arm: Focus staged and taking no part, and reads the service failed to run."""
    problems = []
    log = body.get("decision_log") if isinstance(body.get("decision_log"), dict) else {}
    if want["focus"] is True:
        calls = _int(log.get("focus_calls")) or 0
        if "focus_absent" in degradation_kinds(body) or calls == 0:
            problems.append(f"Focus is staged and took no part ({calls} Focus calls"
                            + (", focus_absent" if "focus_absent" in degradation_kinds(body) else "") + ")")
    elif want["focus"] is False:
        # Arbiter only (arm A): Focus is switched off, so no Focus call, no Focus profile in the model work, and
        # the reply discloses focus_absent whenever it holds evidence Focus would have ranked.
        calls = _int(log.get("focus_calls")) or 0
        work = body.get("model_work") if isinstance(body.get("model_work"), dict) else {}
        if calls or _int(work.get("focus_calls")):
            problems.append(f"Focus is off in this arm and {calls or work.get('focus_calls')} Focus calls ran")
        if work.get("focus_profile") is not None:
            problems.append(f"Focus is off in this arm and the reply names Focus profile {work.get('focus_profile')}")
        if records(body) and "focus_absent" not in degradation_kinds(body):
            problems.append("Focus is off in this arm and the reply does not disclose focus_absent")
    # Any failed record refuses the reply (coordinator, 23 November 2026): a service failure (``read_failed``) and
    # a read the service refused as invalid (``invalid_arguments``, such as a range across document locations,
    # which the loop should never send) both leave the arm's evidence short. They are named apart.
    failures = read_failures(body)
    service = {k: n for k, n in failures.items() if k.split(":", 1)[1] in SERVICE_READ_FAILURES}
    other = {k: n for k, n in failures.items() if k not in service}
    if service:
        problems.append(f"{sum(service.values())} reads failed in the service ("
                        + ", ".join(f"{k} x{n}" for k, n in service.items()) + ")")
    if other:
        problems.append(f"{sum(other.values())} reads were refused or failed otherwise ("
                        + ", ".join(f"{k} x{n}" for k, n in other.items()) + ")")
    return problems


def check_arm(body: Any, arm: dict) -> list[str]:
    """Problems that show a reply was not produced by the arm. A ticket reply shows nothing either way."""
    if not is_v3(body):
        return ["not an Answer /3 reply"]
    if is_ticket(body):
        return []
    want = arm_requirement(arm)
    problems = stage_problems(body, want)
    kinds = degradation_kinds(body)
    ds = decisions(body)
    presence = context_presence(body)
    if want["context"] is None:
        scored = [d for d in ds if d["source"] in MODEL_DUE_SOURCES or d["model"]]
        if scored:
            problems.append(f"{len(scored)} decisions name a Context model in a Focus-only arm")
        # A model that was present and never reached also reports context_absent: it is not Focus only.
        if presence["state"] != "no_model" or presence["models"]:
            problems.append(f"the decision log counts {presence['due']} decisions due to a Context model "
                            f"({', '.join(presence['models']) or 'unnamed'}) in a Focus-only arm: "
                            f"{presence['state']}")
        if ds and "context_absent" not in kinds:
            problems.append("no context_absent degradation in a Focus-only arm")
        return problems
    abstained = [d for d in ds if d["source"] == "abstained"]
    if abstained or "context_abstained" in kinds:
        problems.append(f"{len(abstained)} abstained decisions (the gate mode takes every decision by top score)")
    if "context_absent" in kinds:
        if presence["state"] == "model_present_no_call_dispatched":
            problems.append(f"context_absent: the Context model was present and no call was dispatched "
                            f"({presence['due']} decisions due)")
        elif presence["state"] == "model_present_none_taken":
            problems.append(f"context_absent: the Context model was present and took no decision "
                            f"({_due_text(presence['by_source'])})")
        else:
            problems.append("context_absent: no Context model took part")
    others = [m for m in presence["models"] if m != want["context"]]
    if others:
        problems.append(f"the decision log counts decisions of {', '.join(others)}, not {want['context']}")
    for kind in ("model", "device"):
        if kind in kinds:
            problems.append(f"{kind} degradation: the arm's model or device was not kept")
    for d in ds:
        if d["source"] != "model":
            continue
        if d["model"] != want["context"]:
            problems.append(f"decision {d['ordinal']} scored by {d['model']}, not {want['context']}")
        if d["mode"] != want["mode"]:
            problems.append(f"decision {d['ordinal']} mode {d['mode']}, not {want['mode']}")
        if d["device"] != want["device"]:
            problems.append(f"decision {d['ordinal']} on {d['device']}, not {want['device']}")
        if want["precision"] is not None and d["precision"] != want["precision"]:
            problems.append(f"decision {d['ordinal']} at {d['precision']}, not {want['precision']}")
    # The arm's model must have taken decisions. Every logged decision counts (the log's counts by source), so
    # a reply that merely omits its model decisions is not refused, and one whose model took none is.
    if presence["state"] in ("model_present_no_call_dispatched", "model_present_none_taken"):
        problems.append(f"the arm's model took none of the {presence['due']} decisions due to it "
                        f"({_due_text(presence['by_source'])})")
    elif presence["state"] == "no_model" and (ds or log_counts(body)):
        problems.append("no model-scored decision: no decision was due to a Context model")
    return problems


def arm_flags(body: Any, arm: dict) -> list[str]:
    """Stages of a Context arm's reply where decisions were due to the model and it took none, while it took
    decisions in another stage. The reply is still the arm's (``check_arm`` passes on this account), but the
    stage ran on the deterministic order: the scenario is flagged so the gate reads it with that in mind."""
    if not is_v3(body) or is_ticket(body) or arm_requirement(arm)["context"] is None:
        return []
    presence = context_presence(body)
    if presence["state"] != "model_took_decisions":
        return []
    return [f"stage {stage}: the model took none of the {sum(presence['stages'][stage].values())} decisions due "
            f"({_due_text(presence['stages'][stage])})" for stage in presence["stages_without_model"]]
