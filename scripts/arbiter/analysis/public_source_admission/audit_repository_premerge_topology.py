# SPDX-License-Identifier: Apache-2.0
"""Audit the 35 frozen post-issue bases using public commit graph metadata only."""

import argparse
from collections import Counter, deque
import hashlib
import json
from pathlib import Path
import runpy
import sys
import time


HELPER = "screen_repository_confirmation_upstreams.py"
HELPER_SHA = "c8c5a96c858819e8218d43edc383c2e78441be616aa100554f2e44f11edb00ad"
PINS = {
    "pinned-upstream-screen.private.json": "dcef091a79fc092e5422b39c62c47c197810a5263c19d1d4a66c08362f3987c4",
    "replacement-screen-16-r1.private.json": "96c939368ad6ac79cfc91a1f309f9887aa6ef22e1be82033f4a91a5774d3f8aa",
    "replacement-screen-16-r2.private.json": "0b6fdcde3a79efddd4b64b8dc2a1989de9564f01635c163ff077b11d6d415e42",
}
COMMIT = "oid committedDate tree { oid } parents(first:8) { totalCount nodes { oid } }"


def later_bases(helper, directory):
    receipts = [json.loads(helper["pinned_bytes"](directory / name, pin, 1 << 20))
                for name, pin in PINS.items()]
    original, first, second = receipts
    initial = [r for r in original["rows"] if r["base_object_resolved"] and r["postmodel_issue_and_ordered_merge"]
               and r["base_tied_to_pull_ref_or_first_merge_parent"] and not r["known_contextbench_overlap"]
               and r["license_at_base"].get("permissive_classifier") is True]
    replacements = [r for r in first["rows"] + second["rows"] if r["screen_failures"] == []]
    selected = initial + replacements[:11]
    later = [r for r in selected if r["base_date_no_later_than_original_issue"] is False]
    if (len(initial), len(replacements), len({r["instance_id"] for r in selected}), len(later)) != (39, 20, 50, 35):
        raise ValueError("frozen screening cohort differs")
    return later


def metadata_fields(row):
    return (f'databaseId visibility frozen:object(oid:"{row["base_commit"]}") {{ ... on Commit {{ {COMMIT} }} }} '
            f'issue(number:{int(row["issue_number"])}) {{ number createdAt }} '
            f'pull:pullRequest(number:{int(row["pull_number"])}) {{ number merged createdAt mergedAt '
            f'baseRefOid headRefOid mergeCommit {{ {COMMIT} }} commits(first:50) {{ totalCount '
            f'nodes {{ commit {{ {COMMIT} }} }} }} }}')


def parent_path(start, target, commits):
    """Return a path of observed Git parent edges, requiring strict ancestry."""
    if not start or start == target:
        return []
    queue = deque([(start, [start])])
    visited = {start}
    while queue:
        oid, path = queue.popleft()
        for parent in ((commits.get(oid) or {}).get("parents") or {}).get("nodes") or []:
            ancestor = parent["oid"]
            if ancestor == target:
                return path + [ancestor]
            if ancestor not in visited:
                visited.add(ancestor)
                queue.append((ancestor, path + [ancestor]))
    return []


def audit(row, data, parse):
    result = {"instance_id": row["instance_id"], "repository": row["canonical_repo"],
              "base_oid": row["base_commit"], "issue_number": row["issue_number"],
              "pull_number": int(row["pull_number"]), "observation": data}
    data = data or {}
    pull, issue, base = (data.get(k) or {} for k in ("pull", "issue", "frozen"))
    merge = pull.get("mergeCommit") or {}
    base_oid = row["base_commit"]
    if (data.get("databaseId") != row["lineage"][0]["id"] or data.get("visibility") != "PUBLIC"
            or issue.get("number") != row["issue_number"] or pull.get("number") != int(row["pull_number"])
            or pull.get("merged") is not True or base.get("oid") != base_oid or not merge.get("oid")):
        return dict(result, status="unresolved_identity")
    timestamps = [(parse(item.get(live)), parse(row.get(frozen))) for item, live, frozen in (
        (issue, "createdAt", "original_issue_created_at"), (base, "committedDate", "base_commit_date"),
        (pull, "createdAt", "pull_created_at"), (pull, "mergedAt", "pull_merged_at"))]
    if any(live is None or live != frozen for live, frozen in timestamps):
        return dict(result, status="chronology_differs_from_sealed_metadata")
    issue_time, base_time, _, merged_time = (live for live, _ in timestamps)
    if not issue_time < base_time <= merged_time:
        return dict(result, status="base_time_outside_issue_to_merge_interval")
    roster = pull.get("commits") or {}
    payloads = [node["commit"] for node in roster.get("nodes") or [] if node and node.get("commit")]
    graph = {c["oid"]: c for c in [*payloads, base, merge]}
    merge_path = parent_path(merge["oid"], base_oid, graph)
    head = pull.get("headRefOid")
    head_path = parent_path(head, base_oid, graph)
    base_tree = (base.get("tree") or {}).get("oid")
    merge_tree = (merge.get("tree") or {}).get("oid")
    result.update(merge_parent_path=merge_path, pull_head_parent_path=head_path,
                  pull_reports_frozen_base=pull.get("baseRefOid") == base_oid,
                  frozen_base_listed_in_pull=any(c["oid"] == base_oid for c in payloads),
                  pull_commit_list_complete=roster.get("totalCount") == len(payloads))
    if not merge_path:
        status = "strict_merge_ancestry_unproved"
    elif not base_tree or not merge_tree:
        status = "tree_identity_unavailable"
    elif base_tree == merge_tree:
        status = "same_tree_as_merge"
    else:
        status = "strict_premerge_ancestor_with_different_tree"
    return dict(result, status=status)


HISTORY_INPUT = "69dd8e32d8d2a6a034f1db870617eb52942abfbab19daf8095dc951f9144ad65"


def extend_history(helper, directory, gh):
    prior = json.loads(helper["pinned_bytes"](directory / "premerge-topology-r1.private.json", HISTORY_INPUT, 4 << 20))
    pending = {r["instance_id"]: r for r in prior["rows"] if r["status"] == "strict_merge_ancestry_unproved"}
    if len(pending) != 12:
        raise ValueError("frozen unresolved cohort differs")
    graphs = {key: {} for key in pending}
    cursors, pages, batches, paths = {}, {key: [] for key in pending}, [], {}
    active = dict(pending)
    # Preserve every fetched edge and stop each case once its path is proved.
    # Depth is a metadata request budget. A missed ancestor remains unresolved.
    for _ in range(5):
        requests = []
        for key, row in active.items():
            merge = row["observation"]["pull"]["mergeCommit"]["oid"]
            after = ",after:" + json.dumps(cursors[key]) if key in cursors else ""
            fields = (f'databaseId visibility commit:object(oid:"{merge}") {{ ... on Commit {{ oid '
                      f'history(first:100{after}) {{ pageInfo {{ hasNextPage endCursor }} '
                      'nodes { oid parents(first:8) { nodes { oid } } } } } }')
            requests.append((key, row["repository"], fields))
        if not requests:
            break
        responses, audit_batches = helper["graph_batches"](gh, requests, 2)
        batches.extend(audit_batches)
        remaining = {}
        for key, row in active.items():
            data = responses.get(key) or {}
            pages[key].append(data)
            merge = row["observation"]["pull"]["mergeCommit"]["oid"]
            commit = data.get("commit") or {}
            if (data.get("visibility") != "PUBLIC" or data.get("databaseId") != row["observation"]["databaseId"]
                    or commit.get("oid") != merge):
                continue
            history = commit.get("history") or {}
            for node in history.get("nodes") or []:
                if node:
                    graphs[key][node["oid"]] = node
            path = parent_path(merge, row["base_oid"], graphs[key])
            if path:
                paths[key] = path
                continue
            page = history.get("pageInfo") or {}
            cursor = page.get("endCursor")
            if page.get("hasNextPage") is True and cursor and cursor != cursors.get(key):
                cursors[key] = cursor
                remaining[key] = row
        active = remaining
    rows = []
    for key, row in pending.items():
        path = paths.get(key, [])
        obs = row["observation"]
        base_tree = (obs["frozen"].get("tree") or {}).get("oid")
        merge_tree = (obs["pull"]["mergeCommit"].get("tree") or {}).get("oid")
        status = "strict_merge_ancestry_unproved"
        if path:
            if not base_tree or not merge_tree:
                status = "tree_identity_unavailable"
            elif base_tree == merge_tree:
                status = "same_tree_as_merge"
            else:
                status = "strict_premerge_ancestor_with_different_tree"
        rows.append({"instance_id": key, "pages": pages[key], "merge_parent_path": path, "status": status})
    return {"schema": "airk.private.repository-premerge-extended-history/1", "input_sha256": HISTORY_INPUT,
            "batches": batches, "rows": rows, "counts": Counter(r["status"] for r in rows), "admitted": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--gh", required=True, type=Path)
    parser.add_argument("--extend-history", action="store_true")
    args = parser.parse_args()
    if not (sys.flags.isolated and sys.dont_write_bytecode and sys.flags.no_site):
        parser.error("run with isolated Python -I -B -S")
    started = time.monotonic()
    path = Path(__file__).with_name(HELPER)
    if hashlib.sha256(path.read_bytes()).hexdigest() != HELPER_SHA:
        raise ValueError("upstream helper changed")
    helper = runpy.run_path(str(path))
    helper["pinned_bytes"](args.gh, helper["GH_SHA"], 64 << 20)
    if args.extend_history:
        result = extend_history(helper, args.directory, args.gh)
        result.update(collector_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      helper_sha256=HELPER_SHA, github_cli_sha256=helper["GH_SHA"],
                      elapsed_seconds=time.monotonic() - started)
        print(json.dumps(result, sort_keys=True, allow_nan=False))
        return
    cases = later_bases(helper, args.directory)
    requests = [(r["instance_id"], r["canonical_repo"], metadata_fields(r)) for r in cases]
    values, batches = helper["graph_batches"](args.gh, requests, 3)
    results = [audit(r, values[r["instance_id"]], helper["parse_date"]) for r in cases]
    print(json.dumps({"schema": "airk.private.repository-premerge-topology/1", "inputs": PINS,
                      "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      "helper_sha256": HELPER_SHA, "github_cli_sha256": helper["GH_SHA"],
                      "batches": batches, "rows": results,
                      "counts": Counter(r["status"] for r in results), "admitted": 0,
                      "elapsed_seconds": time.monotonic() - started}, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
