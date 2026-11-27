# SPDX-License-Identifier: Apache-2.0
"""Model-dose step: a reranker chooses the passages of a Gather Context brief.

Evaluation tooling for the model-dose experiment (plan
``docs/arbiter/retrieval-strategy-combined.md``, "How much local
inference"). The 4B model is not in the runtime, so its choice is applied
here, between the service and the acting agent, and measured by what the
agent then spends. Timing here says nothing about the product.

Input: the brief the service returned in its default mode (schema
``airk.gather-context.brief/1``), the same call's full reply, and a scoring
function. Output: the same brief with ``passages`` and ``omitted`` replaced.
Everything else in the brief is left as the service wrote it.

- Candidates of a need are the text rows of the records the full reply
  says were used for it (``record_uses``), in the kernel's order. A row
  trimmed of an overlap does not hold its lines whole and is not a
  candidate, the same rule the brief itself follows.
- ``dose`` is the number of (need, passage) judgments spent on one call,
  dealt one candidate a need at a time in the kernel's order.
- A need's judged candidates go first, best score first; its unjudged
  ones follow in the kernel's order. Passages are then taken in rounds,
  each need's best before any need's second, inside ``PASSAGE_BYTES``; a
  passage that does not fit is skipped and the next is tried; a passage
  two needs share is sent once.

It is not a prototype of the loop: it changes no retrieval and no reply
field other than the two named.
"""

from __future__ import annotations

import json

BRIEF_SCHEMA = "airk.gather-context.brief/1"
FULL_SCHEMA = "airk.arbiter.context/3"
PASSAGE_BYTES = 16_000
"""The brief's own passage allowance (``BRIEF_PASSAGE_BYTES`` in ``airk-mcp`` ``response::brief``)."""
MAX_BRIEF_BYTES = 24_000
OMITTED_LIMIT = 48
INSTRUCTION = ("Given one need of a software development task, judge whether the Document is one of the few source "
               "passages a developer must read to meet that need, not merely a passage on the same topic.")
"""The judge instruction the loop prototype measured with."""


def need_numbers(full: dict) -> dict[tuple[int, int], int]:
    """The brief numbers sub-needs from 1 in part order; ``record_uses`` names them by part and id."""
    numbers, count = {}, 0
    for part in full.get("parts") or []:
        for sub in part.get("sub_needs") or []:
            count += 1
            numbers[(part.get("id"), sub.get("id"))] = count
    return numbers


def passages_of(record: dict) -> list[dict]:
    """The whole-text rows of one record's span projection."""
    found = []
    for row in (record.get("canonical_result") or {}).get("spans") or []:
        if not isinstance(row, dict) or row.get("overlaps") or not isinstance(row.get("text"), str) or not row["text"]:
            continue
        start, end = row.get("start_line"), row.get("end_line", row.get("start_line"))
        if isinstance(row.get("path"), str) and isinstance(start, int) and isinstance(end, int):
            found.append({"at": f"{row['path']}:{start}-{end}", "text": row["text"]})
    return found


def candidates(full: dict) -> dict[int, list[dict]]:
    """Per brief need number, its candidate passages in the kernel's order, each location once."""
    numbers = need_numbers(full)
    records = {record.get("id"): record for record in full.get("records") or []}
    pool: dict[int, list[dict]] = {}
    for use in full.get("record_uses") or []:
        number = numbers.get((use.get("part"), use.get("need")))
        record = records.get(use.get("record"))
        if number is None or record is None:
            continue
        mine = pool.setdefault(number, [])
        for passage in passages_of(record):
            if all(passage["at"] != known["at"] for known in mine):
                mine.append(passage)
    return pool


def deal(pool: dict[int, list[dict]], dose: int) -> list[tuple[int, int]]:
    """The (need, candidate index) pairs to judge: one candidate a need at a time, at most ``dose``."""
    pairs, depth = [], 0
    while len(pairs) < dose and any(depth < len(items) for items in pool.values()):
        for number in sorted(pool):
            if depth < len(pool[number]) and len(pairs) < dose:
                pairs.append((number, depth))
        depth += 1
    return pairs


def choose(brief: dict, full: dict, score, dose: int) -> tuple[dict, dict]:
    """The brief with the model's choice of passages, and what was spent making it."""
    if brief.get("schema") != BRIEF_SCHEMA or full.get("schema") != FULL_SCHEMA or dose <= 0:
        return brief, {"applied": False, "judgments": 0}
    texts = {need.get("need"): need.get("text") or "" for need in brief.get("needs") or []}
    pool = {number: items for number, items in candidates(full).items() if number in texts}
    pairs = deal(pool, dose)
    scores = score(INSTRUCTION, [(texts[number], pool[number][index]["text"]) for number, index in pairs]) if pairs else []
    judged = {pair: value for pair, value in zip(pairs, scores)}
    order = {number: sorted(range(len(items)), key=lambda index: (
        (0, -judged[(number, index)]) if (number, index) in judged else (1, index)))
        for number, items in pool.items()}
    chosen: list[dict] = []
    spent, depth = 0, 0
    while any(depth < len(indices) for indices in order.values()):
        for number in sorted(order):
            if depth >= len(order[number]):
                continue
            passage = pool[number][order[number][depth]]
            known = next((item for item in chosen if item["at"] == passage["at"]), None)
            if known is not None:
                if number not in known["needs"]:
                    known["needs"].append(number)
                continue
            size = len(passage["text"].encode("utf-8"))
            if spent + size > PASSAGE_BYTES:
                continue
            chosen.append({"at": passage["at"], "needs": [number], "text": passage["text"]})
            spent += size
        depth += 1
    out = dict(brief)
    carried = {item["at"] for item in chosen}
    listed = [row.get("at") for need in brief.get("needs") or [] for row in need.get("where") or []]
    listed += [item.get("at") for item in brief.get("passages") or []] + list(brief.get("omitted") or [])
    omitted = []
    for at in listed:
        if isinstance(at, str) and at not in carried and at not in omitted:
            omitted.append(at)
    out["passages"] = chosen
    more = int(brief.get("omitted_more") or 0) + max(0, len(omitted) - OMITTED_LIMIT)
    out.pop("omitted", None)
    out.pop("omitted_more", None)
    if omitted:
        out["omitted"] = omitted[:OMITTED_LIMIT]
    if more:
        out["omitted_more"] = more
    dropped = 0
    while out["passages"] and len(json.dumps(out, ensure_ascii=False, separators=(",", ":")).encode("utf-8")) > MAX_BRIEF_BYTES:
        gone = out["passages"].pop()
        out.setdefault("omitted", []).append(gone["at"])
        dropped += 1
    return out, {"applied": True, "judgments": len(pairs), "candidates": sum(len(items) for items in pool.values()),
                 "passages": len(out["passages"]), "passage_bytes": sum(len(item["text"].encode("utf-8")) for item in out["passages"]),
                 "dropped_for_ceiling": dropped,
                 "kept_from_service": len(carried & {item.get("at") for item in brief.get("passages") or []})}
