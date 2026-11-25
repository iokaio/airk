# SPDX-License-Identifier: Apache-2.0
"""Measure public Search depth for the initial queries retained in Answer /3."""

from __future__ import annotations

import argparse
import math
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import depth
import evidence
import loop_reply
import settled


INITIAL_CAUSES = {"part_search", "need_search", "shared_read"}
SEARCH_FIELDS = {"query", "limit", "max_bytes", "full_chunks"}
SCHEMA = "airk.private.answer-initial-search-depth/1"


def under(path: Path, root: Path) -> Path:
    resolved = path.resolve(strict=True)
    common.require(resolved.is_relative_to(root.resolve(strict=True)), "receipt path escapes its selected scope")
    return resolved


def verified_first(pass_dir: Path, row: dict, scenario: dict, digests: dict, source: str) -> tuple[dict, dict]:
    key = row["key"]
    entry = digests.get(key) or {}
    expected = entry.get("digest")
    common.require(entry.get("ok") is True and isinstance(expected, str) and not entry.get("arm_problems"),
                   f"{key}: canonical digest unavailable")
    if source == "canonical":
        raw_path = under(pass_dir / "digests" / key / "canonical.stdout.json", pass_dir / "digests")
        raw = raw_path.read_bytes()
        body = common.raw_body(raw)
        common.require(isinstance(body, dict) and evidence.evidence_digest(body) == expected,
                       f"{key}: canonical evidence digest differs")
    else:
        directory = under(Path(row["session_dir"]), pass_dir.parent.parent / "sessions")
        session = common.read_json(directory / "session.json")
        common.require(session.get("scenario_id") == row["scenario_id"] and session.get("sample") == 1
                       and session.get("scenario_sha256") == scenario["_sha256"] and session.get("repo") == row["repo"]
                       and session.get("slot") == row["slot"] and session.get("path") in common.ANSWER_PATHS,
                       f"{key}: first session identity differs")
        turns = common.read_jsonl(directory / "wrapper" / "turns.jsonl")
        first = [item for item in turns if item.get("turn") == 1 and item.get("rejected") is False]
        common.require(len(first) == 1 and first[0].get("tool") == "airk_answer" and first[0].get("ok") is True
                       and (first[0].get("first_call") or {}).get("rule") == "canonical"
                       and first[0].get("sent_arguments", {}).get(common.PRIMARY_ARG["airk_answer"]) == scenario["instruction"],
                       f"{key}: no successful canonical first tool turn")
        raw_path = under(Path(first[0]["result_path"]), directory / "wrapper")
        raw = raw_path.read_bytes()
        common.require(common.sha256_bytes(raw) == first[0].get("result_sha256"), f"{key}: first tool receipt hash differs")
        body = common.raw_body(raw)
    common.require(loop_reply.is_v3(body) and not loop_reply.is_ticket(body), f"{key}: expected an evidence-bearing Answer /3")
    return body, {"source": source, "raw_sha256": common.sha256_bytes(raw),
                  "evidence_digest": evidence.evidence_digest(body), "pinned_canonical_digest": expected}


def initial_searches(body: dict) -> list[tuple[dict, list[dict]]]:
    uses = {}
    for use in loop_reply.record_uses(body):
        if use.get("round") == 1 and use.get("cause") in INITIAL_CAUSES:
            uses.setdefault(use["record"], []).append({key: use.get(key) for key in ("part", "need", "round", "cause")})
    rows = []
    for record in loop_reply.records(body):
        if record.get("operation") != "search" or record.get("id") not in uses:
            continue
        args = record.get("arguments")
        common.require(isinstance(args, dict) and set(args) == SEARCH_FIELDS and isinstance(args["query"], str)
                       and bool(args["query"]) and type(args["limit"]) is int and args["limit"] > 0
                       and type(args["max_bytes"]) is int and args["max_bytes"] > 0
                       and type(args["full_chunks"]) is bool, "unexpected retained initial Search arguments")
        rows.append((record, uses[record["id"]]))
    return rows


def measure_notes(scenario: dict, projection: dict, narrow: dict | None, deep: dict | None) -> list[dict]:
    spans = evidence.extract(projection)
    results = []
    for note in scenario["notes"]:
        if note["mark"] != "essential":
            continue
        delivered = evidence.covers(spans, note)
        shallow = depth.ranks([evidence.extract(hit) for hit in narrow["hits"]], note) if narrow is not None else None
        wide = depth.ranks([evidence.extract(hit) for hit in deep["hits"]], note) if deep is not None else None
        shallow_hit = shallow is not None and shallow["rule2"] is not None
        wide_hit = wide is not None and wide["rule2"] is not None
        if delivered:
            placement = "answer_projection"
        elif shallow_hit:
            placement = "matched_search_only"
        elif wide_hit:
            placement = "widened_search_only" if shallow is not None else "widened_search_matched_unobserved"
        else:
            placement = "absent_from_observed_searches" if shallow is not None and wide is not None else "search_unobserved"
        results.append({"note": note["id"], "projection": delivered, "matched": shallow, "widened": wide,
                        "placement": placement})
    return results


class SearchReplay:
    def __init__(self, handle, tokens, *, call_limit, timeout, wall_time, call=None):
        self.handle, self.tokens, self.call = handle, tokens, call or common.call_tool
        self.call_limit, self.timeout = call_limit, timeout
        self.deadline = time.monotonic() + wall_time
        self.cache = {}
        self.attempts = {}

    def one(self, repo, arguments):
        key = common.sha256_bytes(common.canonical_json([repo, self.tokens[repo], arguments]))
        if key not in self.cache:
            remaining = min(self.timeout, self.deadline - time.monotonic())
            common.require(len(self.attempts) < self.call_limit and remaining > 0, "diagnostic call budget exhausted")
            result = self.call(self.handle, "airk_search", repo, arguments, remaining,
                               strip_env=common.AGENT_STRIPPED_ENV)
            body = common.envelope_body(result.envelope) if result.ok else None
            self.attempts[key] = {"repo": repo, "arguments": arguments, "stdout_sha256": common.sha256_bytes(result.stdout),
                                  "ok": result.ok, "error_code": depth.error_code(result), "elapsed_ms": result.elapsed_ms}
            if result.ok:
                common.require(isinstance(body, dict) and isinstance(body.get("hits"), list)
                               and (body.get("publication") or {}).get("token") == self.tokens[repo]
                               and (body.get("execution") or {}).get("served_mode") == "hybrid"
                               and body.get("focus") is None,
                               "Search reply publication, hybrid mode, Focus state, or shape differs")
            self.cache[key] = body
        return key, self.cache[key]

    def pair(self, repo, record, limit, max_bytes):
        matched = {**record["arguments"], "verification": "off", "response_mode": "full"}
        common.require(matched["limit"] <= limit and matched["max_bytes"] <= max_bytes,
                       "live Search caps cannot widen retained arguments")
        shallow_key, shallow = self.one(repo, matched)
        deep_keys = []
        for bound in depth.budgets(max_bytes):
            if bound < matched["max_bytes"]:
                continue
            deep_key, deep = self.one(repo, {**matched, "limit": limit, "max_bytes": bound})
            deep_keys.append(deep_key)
            if deep is not None or self.attempts[deep_key]["error_code"] != "scope_reply_budget":
                return shallow_key, shallow, deep_keys, deep
        return shallow_key, shallow, deep_keys, None


def run(args):
    common.require(args.call_limit > 0 and all(math.isfinite(x) and x > 0 for x in (args.timeout, args.wall_time)),
                   "invalid diagnostic call bounds")
    pass_dir = args.pass_dir.resolve(strict=True)
    pass_index_path = pass_dir / "pass-index.json"
    digests_path = pass_dir / "digests.json"
    index = common.read_json(pass_index_path)
    digests = common.read_json(digests_path)
    common.require(index.get("schema") == "airk.private.answer-eval-pass-index/1", "unknown pass index")
    freeze = index["freeze"]
    freeze_path = Path(freeze["path"]).resolve(strict=True)
    common.require(common.sha256_file(freeze_path) == freeze["sha256"], "scenario freeze hash differs")
    frozen = common.read_json(freeze_path)["files"]
    scenarios_dir = args.scenarios_dir or freeze_path.parent / "scenarios"
    out = args.out.resolve(strict=True)
    common.require(out.is_dir() and not any(out.iterdir()), "use a fresh empty private output directory")
    selections = []
    for row in index["results"]:
        repo, slot = row["repo"], row["slot"]
        arm = row.get("arm") or row["configuration"]
        if (row["sample"] != 1 or row["path"] not in common.ANSWER_PATHS or repo not in args.repo
                or slot not in args.slot or arm not in args.arm):
            continue
        scenario_path = under(scenarios_dir / repo / f"{slot}.json", freeze_path.parent)
        scenario = common.load_scenario(scenario_path)
        common.require(scenario["id"] == row["scenario_id"] and
                       frozen.get(scenario_path.relative_to(freeze_path.parent).as_posix()) == scenario["_sha256"],
                       "selected scenario does not match its freeze")
        body, first = verified_first(pass_dir, row, scenario, digests, args.source)
        common.require(initial_searches(body), "selected Answer has no retained initial Search arguments")
        selections.append((row, arm, scenario, body, first))
    common.require(selections, "no matching sample-one Answer rows")
    handle = common.load_handle(args.handle)
    common.require(handle.sha256 == args.handle_sha256, "live handle pin differs")
    catalog_sha = common.sha256_file(args.catalog)
    common.require(catalog_sha == args.catalog_sha256, "live Search catalog pin differs")
    catalog = common.load_catalog(args.catalog)
    cap = depth.catalog_limit_max(catalog)
    maximum = (catalog["airk_search"]["inputSchema"]["properties"].get("max_bytes") or {}).get("maximum")
    common.require(type(cap) is int and type(maximum) is int, "live Search count/byte caps unavailable")
    common.require(handle.live_stages(timeout_s=args.timeout) == {"focus": None, "context": None},
                   "live Search control must have Focus and Arbiter off")
    repos = sorted({row["repo"] for row, *_ in selections})
    start = {repo: settled.snapshot(handle, repo, args.timeout) for repo in repos}
    common.require(all(row["settled"] for row in start.values()), "a selected Search control index is unsettled")
    tokens = {repo: status["publication_token"] for repo, status in start.items()}
    for row, _arm, _scenario, body, _first in selections:
        scope = body.get("scope") or {}
        common.require(scope.get("index_id") == handle.index_id(row["repo"])
                       and (scope.get("publication") or {}).get("token") == tokens[row["repo"]],
                       "retained Answer and live Search index/publication differ")

    engine = SearchReplay(handle, tokens, call_limit=args.call_limit, timeout=args.timeout, wall_time=args.wall_time)
    rows = []
    records_private = {}
    try:
        for row, arm, scenario, body, first in selections:
            for record, uses in initial_searches(body):
                shallow_key, shallow, deep_keys, deep = engine.pair(row["repo"], record, cap, maximum)
                project = record.get("canonical_result") or {}
                common.require(evidence.is_loop_projection(project), "initial Search record has no /3 projection")
                record_id = record["id"]
                records_private[f"{row['key']}:{record_id}"] = {"arguments": record["arguments"],
                    "matched_attempt": shallow_key, "widened_attempts": deep_keys}
                rows.append({"repo": row["repo"], "slot": row["slot"], "arm": arm, "scenario": scenario["id"],
                             "record": record_id, "uses": uses, "first_reply": first,
                             "native_hits_total_in_projection": project.get("hits_total"),
                             "projected_spans": len(project["spans"]),
                             "matched_hits_returned": None if shallow is None else len(shallow["hits"]),
                             "widened_hits_returned": None if deep is None else len(deep["hits"]),
                             "matched_truncated": None if shallow is None else shallow.get("truncated"),
                             "widened_truncated": None if deep is None else deep.get("truncated"),
                             "matched_attempt": shallow_key, "widened_attempts": deep_keys,
                             "notes": measure_notes(scenario, project, shallow, deep)})
    finally:
        common.write_json(out / "queries.private.json", {"schema": SCHEMA, "records": records_private,
                                                        "attempts": engine.attempts})
        end = {repo: settled.snapshot(handle, repo, args.timeout) for repo in repos}
        common.require(all(end[repo]["settled"] and settled.compare(start[repo], end[repo])["unchanged"] for repo in repos)
                       and handle.live_stages(timeout_s=args.timeout) == {"focus": None, "context": None},
                       "Search control publication, readiness, or stage changed")
    report = {"schema": SCHEMA, "pass_series": index["series"], "source": args.source,
              "input_sha256": {"pass_index": common.sha256_file(pass_index_path), "canonical_digests": common.sha256_file(digests_path),
                              "scenario_freeze": freeze["sha256"], "handle": handle.sha256, "catalog": catalog_sha},
              "publications": tokens, "unique_calls": len(engine.attempts), "coverage_rule": evidence.COVERAGE_RULE,
              "limits": {"widened_count": cap, "widened_max_bytes": maximum, "paging": False},
              "scope": "Retained first-round Answer search arguments only; public Search results with stages off. No internal pre-fusion roster or original Answer ranking is observed.",
              "rows": rows}
    common.write_json(out / "initial-search-depth.json", report)
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pass-dir", required=True, type=Path)
    parser.add_argument("--source", choices=("canonical", "actual"), default="canonical")
    parser.add_argument("--handle", required=True, type=Path)
    parser.add_argument("--handle-sha256", required=True)
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--catalog-sha256", required=True)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--scenarios-dir", type=Path)
    parser.add_argument("--repo", action="append", required=True, choices=common.REPOS)
    parser.add_argument("--slot", action="append", required=True, choices=common.SLOTS)
    parser.add_argument("--arm", action="append", required=True)
    parser.add_argument("--call-limit", type=int, default=80)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--wall-time", type=float, default=900)
    return run(parser.parse_args(argv))


if __name__ == "__main__":
    sys.exit(main())
