# SPDX-License-Identifier: Apache-2.0
"""Replay pinned Windows CS1 follow-up Search and indexed-read calls."""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import depth
import evidence
import search_focus_ab as search
import settled


TOOLS = {"airk_search", "airk_read_file"}
PROFILES = {"none", "ranked_keywords", "semantic_subqueries", "fusion_diversity", "adaptive_preserve"}


def recorded_rows(cohort, index):
    search.roster(cohort["rows"])
    sessions = {(item["repo"], item["slot"]): item for item in index["results"]
                if item["path"] == "search" and item["sample"] == 1 and item["device"] == "none"}
    common.require(len(sessions) == len(cohort["rows"]), "recorded Search session roster differs")
    results = []
    roster = Counter()
    for row in cohort["rows"]:
        entry = sessions[row["repo"], row["slot"]]
        root = Path(entry["session_dir"])
        source = row["source_sha256"]
        search.pinned(root / "session.json", source["session"])
        turns = search.pinned(root / "wrapper" / "turns.jsonl", source["turn_log"])
        calls = []
        for line in turns.splitlines():
            turn = json.loads(line)
            tool = turn.get("tool")
            if not turn.get("turn") or turn.get("rejected") or tool not in TOOLS or turn.get("first_call"):
                continue
            arguments = turn["sent_arguments"]
            common.require(isinstance(arguments, dict), "missing original sent arguments")
            common.require(list(turn["env_stripped"]) == list(common.AGENT_STRIPPED_ENV),
                           "recorded Search transport arguments used different environment exclusions")
            raw = search.pinned(Path(turn["result_path"]), turn["result_sha256"])
            envelope = json.loads(raw)
            spans = evidence.extract(common.envelope_body(envelope)) if turn["ok"] else []
            notes = evidence.covered_notes(spans, row["essential"], rule=2)
            calls.append({"ordinal": turn["turn"], "tool": tool, "arguments": arguments,
                          "response_mode": turn["response_mode"], "recorded_ok": bool(turn["ok"]),
                          "recorded_covered": notes, "recorded_result_sha256": turn["result_sha256"]})
            roster[tool] += 1
        common.require(calls and [call["ordinal"] for call in calls] ==
                       sorted({call["ordinal"] for call in calls}), "follow-up ordinal order differs")
        results.append((row, calls))
    common.require(roster == Counter({"airk_search": 56, "airk_read_file": 68}), "recorded follow-up call count differs")
    return results


def collect(args):
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    index = json.loads(search.pinned(args.index, args.index_sha256))
    workload = recorded_rows(cohort, index)
    if args.target:
        requested = set(args.target)
        selected = set()
        filtered = []
        for row, calls in workload:
            prefix = f"{row['repo']}/{row['slot']}/"
            kept = [call for call in calls if prefix + str(call["ordinal"]) in requested]
            if kept:
                filtered.append((row, kept))
                selected.update(prefix + str(call["ordinal"]) for call in kept)
        common.require(selected == requested, "one or more follow-up targets were not in the frozen roster")
        workload = filtered
    handle = common.load_handle(args.handle)
    raw = json.loads(search.pinned(args.handle, args.handle_sha256))
    common.require(handle.sha256 == args.handle_sha256 and handle.state == "ready", "follow-up service is not pinned Ready")
    identities = search.live_identity(handle, raw, sorted({row["repo"] for row in cohort["rows"]}))
    common.require(identities["search_profile"] == args.profile, "unexpected follow-up Search profile")
    pre = search.stage(handle, None, args.timeout)
    indexes = identities["indexes"]
    control = json.loads(search.pinned(args.settled_control, args.settled_control_sha256))
    common.require(control["state"] == "closed" and control["final_stage_idle"] is True,
                   "follow-up replay requires a pinned closed control")
    for repo, observed in indexes.items():
        expected = control["indexes"][observed["physical"]]
        common.require(all(observed[key] == expected[key] for key in
                           ("index_id", "binding", "source_commit", "pipeline_digest", "publication_token")),
                       f"{repo}: live follow-up scope differs from the selected settled control")
    start = {repo: settled.snapshot(handle, repo, args.timeout) for repo in indexes}
    common.require(all(start[repo]["settled"] and start[repo]["publication_token"] == record["publication_token"]
                       for repo, record in indexes.items()), "starting publication is not settled")
    rows = []
    for request, calls in workload:
        repo = request["repo"]
        measurements = []
        for call in calls:
            arguments = call["arguments"]
            common.require(common.effective_response_mode(handle, arguments, common.AGENT_STRIPPED_ENV)
                           == call["response_mode"], "follow-up response mode differs")
            result = common.call_tool(handle, call["tool"], repo, arguments, args.timeout,
                                      strip_env=common.AGENT_STRIPPED_ENV)
            body = common.envelope_body(result.envelope) if result.ok else None
            token = (body.get("publication") or {}).get("token") if isinstance(body, dict) else None
            common.require(token is None or token == indexes[repo]["publication_token"], "follow-up publication changed")
            measurements.append({
                "ordinal": call["ordinal"], "tool": call["tool"], "request_sha256": search.digest(arguments),
                "response_mode": call["response_mode"], "recorded_ok": call["recorded_ok"],
                "recorded_covered": call["recorded_covered"], "recorded_result_sha256": call["recorded_result_sha256"],
                "ok": result.ok, "error_code": None if result.ok else depth.error_code(result),
                "covered": evidence.covered_notes(evidence.extract(body), request["essential"], rule=2) if result.ok else [],
                "result_sha256": common.sha256_bytes(result.stdout), "elapsed_ms": result.elapsed_ms,
            })
        rows.append({"repo": repo, "slot": request["slot"], "calls": measurements})
    end = {repo: settled.snapshot(handle, repo, args.timeout) for repo in indexes}
    common.require(common.sha256_file(handle.path) == args.handle_sha256
                   and search.stage(handle, None, args.timeout)["revision"] == pre["revision"]
                   and all(end[repo]["settled"] and settled.compare(start[repo], end[repo])["unchanged"]
                           for repo in indexes), "follow-up fixture drifted")
    return {"schema": "airk.private.windows-followup-replay/1", "identity": identities, "rows": rows,
            "cohort_sha256": args.cohort_sha256, "index_sha256": args.index_sha256,
            "settled_control_sha256": args.settled_control_sha256, "targets": sorted(args.target or []),
            "scope": "Historical follow-up calls only, in original order. Rule 2 coverage is over measured replies."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cohort", "index", "handle", "settled-control"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--profile", choices=sorted(PROFILES), required=True)
    parser.add_argument("--target", action="append", help="one recorded repo/slot/original-turn; repeatable")
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    search.write_new(args.output, collect(args))


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
