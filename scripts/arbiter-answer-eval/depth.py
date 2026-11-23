# SPDX-License-Identifier: Apache-2.0
"""No-agent depth measurement (plan step E4): where the search pool holds each essential note.

For each scenario, one ``airk_search`` on the instruction verbatim with the
largest pool the tool allows (``limit`` 100, the catalog's maximum, and
``max_bytes`` at its maximum, 65,536), with saved-source verification off. The
byte budget can return fewer than 100 hits; ``hits_returned`` records how many. For each essential note it records the
1-based rank of the first hit that covers it by line overlap (the ``rule2``
column: rules 2 and 3 agree on a hit, because they differ only for a path
without a line span, which a search hit never is), the first rank under rule 1
(any line of overlap), and the first rank of any hit on the note's path. Tool calls use the agents' environment (the
product's default response mode). The settled-index guard runs before and
after, and the publication token must not change.

``python -I -B depth.py --handle H --out <R>/depth/<name> [--repos munarium,bookbuilder] [--limit 100]``
writes ``depth.json`` and the raw results. If the requested limit is above the
catalog's maximum, the maximum is used and both are recorded. The search
cannot page (it has no cursor), so the pool is capped there.

``answer_attribution`` is the same question asked of one Answer /3 reply: for
each essential note, the first record (1-based, in reply order) that covers
it, that record's operation and its uses (part, sub-need, round, cause), and
the sub-needs that cite it.
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
import loop_reply  # noqa: E402
import settled  # noqa: E402

DEPTH_SCHEMA = "airk.private.answer-eval-depth/1"


def ranks(hits_spans: list[list[evidence.Span]], note: dict) -> dict:
    """First 1-based ranks at which a hit covers the note (rule 2, rule 1) or lies on its path."""
    first = {"rule2": None, "rule1": None, "same_path": None}
    paths = [note["path"], *[a["path"] for a in note.get("alternatives") or []]]
    for rank, spans in enumerate(hits_spans, start=1):
        if first["rule2"] is None and evidence.covers(spans, note, rule=2):
            first["rule2"] = rank
        if first["rule1"] is None and evidence.covers(spans, note, rule=1):
            first["rule1"] = rank
        if first["same_path"] is None and any(evidence.path_matches(s.path, p) for s in spans for p in paths):
            first["same_path"] = rank
    return first


def measure(scenario: dict, body: dict) -> dict:
    hits = body.get("hits") or []
    hits_spans = [evidence.extract(h) for h in hits]
    notes = []
    for note in scenario["notes"]:
        if note["mark"] != "essential":
            continue
        notes.append({"id": note["id"], "path": note["path"], "lines": note.get("lines"), **ranks(hits_spans, note)})
    found = [n["rule2"] for n in notes if n["rule2"] is not None]
    return {"scenario_id": scenario["id"], "hits_returned": len(hits), "truncated": bool(body.get("truncated")),
            "notes": notes,
            "essential_in_pool": len(found), "essential_total": len(notes),
            "deepest_rank": max(found) if found else None}


def answer_attribution(scenario: dict, body) -> dict:
    """Where an Answer /3 reply holds each essential note (the current coverage rule, ``evidence.COVERAGE_RULE``):
    record rank, operation, uses, citing needs."""
    recs = loop_reply.records(body)
    spans = loop_reply.record_spans(body)
    uses: dict[str, list[dict]] = {}
    for use in loop_reply.record_uses(body):
        uses.setdefault(str(use.get("record")), []).append(
            {k: use.get(k) for k in ("part", "need", "round", "cause")})
    citing: dict[str, list[list]] = {}
    for need in loop_reply.sub_needs(body):
        for record_id in need["evidence"]:
            citing.setdefault(record_id, []).append([need["part"], need["need"]])
    need_spans = loop_reply.sub_need_spans(body)
    notes = []
    for note in scenario["notes"]:
        if note["mark"] != "essential":
            continue
        row = {"id": note["id"], "record_rank": None, "record": None, "operation": None, "uses": [],
               "cited_by": [], "sub_needs_covering": sorted([list(k) for k, v in need_spans.items()
                                                             if evidence.covers(v, note)])}
        for rank, record in enumerate(recs, start=1):
            record_id = str(record.get("id"))
            if evidence.covers(spans.get(record_id, []), note):
                row.update(record_rank=rank, record=record_id, operation=record.get("operation"),
                           uses=uses.get(record_id, []), cited_by=citing.get(record_id, []))
                break
        notes.append(row)
    return {"scenario_id": scenario["id"], "records": len(recs), "notes": notes,
            "essential_in_reply": sum(1 for n in notes if n["record_rank"] is not None),
            "essential_total": len(notes)}


def depth_query(instruction: str, limit: int, max_bytes: int) -> dict:
    """The depth search. Saved-source verification is off: with 100 hits its metadata does not fit the
    65,536-byte reply (``verification_reply_budget``, found on the bookbuilder baseline), and the depth
    measurement needs the ranked pool, not a source check."""
    return {"query": instruction, "limit": limit, "max_bytes": max_bytes, "verification": "off"}


def budgets(first: int) -> list[int]:
    """Hit-text budgets tried in order. The whole reply is capped at 65,536 bytes (``scope_reply_budget``), and
    its metadata comes on top of the hit text, so a 100-hit reply can need a smaller text budget. Only that
    typed refusal steps down; the budget used is recorded per row."""
    return [first] + [b for b in (49152, 32768, 16384) if b < first]


def error_code(outcome: common.CallOutcome) -> str | None:
    body = common.envelope_body(outcome.envelope) if outcome.envelope else None
    error = body.get("error") if isinstance(body, dict) else None
    return error.get("code") if isinstance(error, dict) else None


def catalog_limit_max(catalog: dict) -> int | None:
    schema = (catalog.get("airk_search") or {}).get("inputSchema") or {}
    return ((schema.get("properties") or {}).get("limit") or {}).get("maximum")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", required=True, help="a handle.json, or @newest")
    parser.add_argument("--catalog", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    parser.add_argument("--freeze", type=Path, default=common.DEFAULT_R / "scenario-freeze.json")
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--slots", default=",".join(common.SLOTS))
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--max-bytes", type=int, default=65536)
    parser.add_argument("--call-timeout", type=float, default=300.0)
    parser.add_argument("--pilot", action="store_true", help="record an unsettled index instead of refusing")
    args = parser.parse_args(argv)
    handle_path = common.resolve_handle(args.handle, common.DEFAULT_R / "fixture")
    handle = common.load_handle(handle_path, accept_queryable=args.pilot)
    catalog = common.load_catalog(args.catalog or handle_path.resolve().parent.parent / "tools-catalog.json")
    maximum = catalog_limit_max(catalog)
    limit = min(args.limit, maximum) if maximum else args.limit
    frozen = common.read_json(args.freeze)["files"] if args.freeze and args.freeze.is_file() else None
    scenarios = []
    for path in common.scenario_files(args.scenarios_dir, args.repos.split(","), args.slots.split(",")):
        scenario = common.load_scenario(path)
        if frozen is not None:
            rel = path.resolve().relative_to(args.freeze.resolve().parent).as_posix()
            common.require(frozen.get(rel) == scenario["_sha256"], f"scenario {rel} differs from the freeze")
        scenarios.append(scenario)
    repos = sorted({s["repository"] for s in scenarios})
    start = {repo: settled.snapshot(handle, repo) for repo in repos}
    unsettled = {r: s["problems"] for r, s in start.items() if not s["settled"]}
    common.require(args.pilot or not unsettled, f"index not settled: {unsettled}")
    rows = []
    for scenario in scenarios:
        attempts = []
        for max_bytes in budgets(args.max_bytes):
            query = depth_query(scenario["instruction"], limit, max_bytes)
            outcome = common.call_tool(handle, "airk_search", scenario["repository"], query, args.call_timeout,
                                       strip_env=common.AGENT_STRIPPED_ENV)
            attempts.append({"max_bytes": max_bytes, "ok": outcome.ok, "code": error_code(outcome)})
            if outcome.ok or error_code(outcome) != "scope_reply_budget":
                break
        raw = args.out / f"{scenario['id']}.stdout.json"
        common.write_bytes(raw, outcome.stdout)
        body = common.envelope_body(outcome.envelope) if outcome.ok else None
        row = measure(scenario, body) if isinstance(body, dict) else {"scenario_id": scenario["id"], "notes": []}
        row.update(ok=outcome.ok, error=outcome.error, elapsed_ms=outcome.elapsed_ms, result_path=str(raw),
                   max_bytes_used=attempts[-1]["max_bytes"], budget_attempts=attempts,
                   result_sha256=common.sha256_bytes(outcome.stdout), arguments={**query, "query": "<instruction>"})
        rows.append(row)
        print(json.dumps({k: row.get(k) for k in ("scenario_id", "ok", "hits_returned", "essential_in_pool",
                                                   "essential_total", "deepest_rank", "elapsed_ms")}))
    end = {repo: settled.snapshot(handle, repo) for repo in repos}
    changes = [settled.compare(start[r], end[r]) for r in repos]
    report = {"schema": DEPTH_SCHEMA, "generated": common.now_iso(), "handle": str(handle_path),
              "handle_sha256": handle.sha256, "coverage_rule": evidence.COVERAGE_RULE,
              "limit_requested": args.limit, "limit_used": limit, "limit_maximum": maximum,
              "max_bytes": args.max_bytes, "paging": "none (airk_search has no cursor)",
              "settled_start": start, "settled_end": end, "token_unchanged": all(c["unchanged"] for c in changes),
              "rows": rows}
    common.write_json(args.out / "depth.json", report)
    return 0 if report["token_unchanged"] else 1


if __name__ == "__main__":
    sys.exit(main())
