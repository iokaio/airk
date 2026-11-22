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
CONTEXT_MODELS = ("context_compact", "context_large")
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


def record_uses(body: Any) -> list[dict]:
    if not is_v3(body):
        return []
    return [u for u in body.get("record_uses") or [] if isinstance(u, dict)]


def degradation_kinds(body: Any) -> list[str]:
    if not is_v3(body):
        return []
    return sorted({str(d.get("kind")) for d in body.get("degradations") or [] if isinstance(d, dict)})


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
        "card_calls": log.get("card_calls"), "focus_calls": log.get("focus_calls"),
        "focus_pairs": log.get("focus_pairs"), "model_timings": log.get("timings") or {},
        "work_unknown": log.get("work_unknown"), "log_digest": log.get("digest"),
        "degradations": degradation_kinds(body),
        "work": {k: work.get(k) for k in ("calls", "fanout", "rounds", "input_tokens", "candidates", "bytes",
                                          "compile_us", "inference_us", "policy_us", "queue_us")},
        "elapsed_ms": body.get("elapsed_ms"), "union_dedup": (body.get("union") or {}).get("dedup_count"),
        "conflicts": len((body.get("union") or {}).get("conflicts") or []),
    }


# ---------------------------------------------------------------- arms


def arm_requirement(arm: dict) -> dict:
    """Normalize a spec's ``arm_check``: ``{"context": null}`` (arm F) or
    ``{"context": "context_compact"|"context_large", "device": "cuda", "precision": "float16"|"float32",
    "mode": "top_score"}`` (arms C and L, and the precision arms)."""
    context = arm.get("context")
    if context is not None and context not in CONTEXT_MODELS:
        raise ValueError(f"unknown Context model {context!r}")
    precision = arm.get("precision")
    if precision is not None and precision not in PRECISIONS:
        raise ValueError(f"unknown precision {precision!r}")
    return {"context": context, "device": arm.get("device", "cuda"), "precision": precision,
            "mode": arm.get("mode", "top_score")}


def check_arm(body: Any, arm: dict) -> list[str]:
    """Problems that show a reply was not produced by the arm. A ticket reply shows nothing either way."""
    if not is_v3(body):
        return ["not an Answer /3 reply"]
    if is_ticket(body):
        return []
    want = arm_requirement(arm)
    problems = []
    kinds = degradation_kinds(body)
    ds = decisions(body)
    if want["context"] is None:
        scored = [d for d in ds if d["source"] in ("model", "abstained", "unavailable") or d["model"]]
        if scored:
            problems.append(f"{len(scored)} decisions name a Context model in a Focus-only arm")
        if ds and "context_absent" not in kinds:
            problems.append("no context_absent degradation in a Focus-only arm")
        return problems
    abstained = [d for d in ds if d["source"] == "abstained"]
    if abstained or "context_abstained" in kinds:
        problems.append(f"{len(abstained)} abstained decisions (the gate mode takes every decision by top score)")
    if "context_absent" in kinds:
        problems.append("context_absent: no Context model took part")
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
    if ds and not any(d["source"] == "model" for d in ds):
        problems.append("no model-scored decision among the included decisions")
    return problems
