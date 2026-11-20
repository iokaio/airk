# SPDX-License-Identifier: Apache-2.0
"""Screen sealed repository review candidates using public GitHub metadata.

Run under the checked human account with an already authenticated GitHub CLI.
GraphQL requests select no issue text, messages, patches, source, or gold
fields. The license endpoint is projected to classifier and blob identity by
the CLI. All output goes to a fresh private receipt via the checked shell.
"""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROSTER_SHA = "28abd5497d6446adda3915b6f0f97a3374d40db28310762ae2de9c8891ae150e"
UPSTREAM_SHA = "d0b49948f596daca6d6cbad1d7fe66dff0a170cf8c6be456cf10372e42b3354c"
COLLECTOR_SHA = "03e7dd822ed1ea18d905a4cb0e940a80d7bb73955dd06899c78f13e73adf69cc"
GH_SHA = "df05089117a23889b3f6ea08d7644849afe7b6025647a09f33b45c71f1418f75"
REPO = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9_.-]*/[a-zA-Z0-9][a-zA-Z0-9_.-]*")
OID = re.compile(r"[a-f0-9]{40}")
LEGAL = re.compile(r"(?:LICEN[CS]E|COPYING|NOTICE|COPYRIGHT|UNLICENSE)(?:[.-][a-zA-Z0-9_.-]+)?", re.I)
IDENTITY = "nameWithOwner databaseId isPrivate visibility isFork mirrorUrl"
LINEAGE = IDENTITY + " parent { " + IDENTITY + " parent { " + IDENTITY + " } }"
TREE = "tree { oid entries { name oid type mode } }"
LICENSE_QUERY = "{path, sha, size, spdx: .license.spdx_id}"
PERMISSIVE = {"Apache-2.0", "MIT", "MIT-0", "BSD-2-Clause", "BSD-3-Clause", "ISC"}
MAX_RESPONSE = 4 << 20


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical_file(path):
    require(path.is_absolute() and path.is_file() and path.resolve(strict=True) == path
            and not any(part.is_symlink() or part.is_junction() for part in (path, *path.parents)),
            "canonical absolute metadata file required")
    return path


def pinned_bytes(path, expected, limit):
    with canonical_file(path).open("rb") as source:
        raw = source.read(limit + 1)
    require(0 < len(raw) <= limit and hashlib.sha256(raw).hexdigest() == expected,
            "reviewed input bytes differ")
    return raw


def load_collector():
    path = Path(__file__).with_name("collect_repository_confirmation_candidates.py")
    pinned_bytes(path, COLLECTOR_SHA, 1 << 20)
    spec = importlib.util.spec_from_file_location("reviewed_repository_candidate_collector", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def call_github(gh, arguments, query=None):
    env = dict(os.environ, GH_PROMPT_DISABLED="1", GH_NO_UPDATE_NOTIFIER="1",
               GH_NO_EXTENSION_UPDATE_NOTIFIER="1", GIT_TERMINAL_PROMPT="0")
    # Do not inherit debugging flags that can log authenticated requests.
    env.pop("GH_DEBUG", None)
    env.pop("DEBUG", None)
    try:
        completed = subprocess.run(
            [str(gh), "api", "--hostname", "github.com", *arguments],
            input=None if query is None else json.dumps({"query": query}).encode(),
            capture_output=True, timeout=20, check=False, env=env,
        )
    except subprocess.TimeoutExpired:
        return {}, "timeout"
    if not 0 < len(completed.stdout) <= MAX_RESPONSE or (query is None and completed.returncode):
        error = completed.stderr.decode(errors="replace")
        status = re.search(r"\bHTTP (\d{3})\b", error)
        return {}, f"http_{status[1]}" if status else "missing_bounded_response"
    try:
        value = json.loads(completed.stdout)
    except ValueError:
        return {}, "invalid_json"
    return (value, "ok") if isinstance(value, dict) else ({}, "invalid_json")


def repository_field(alias, repo, fields):
    require(REPO.fullmatch(repo), "unsupported repository slug")
    owner, name = repo.split("/")
    return f"{alias}: repository(owner:{json.dumps(owner)},name:{json.dumps(name)},followRenames:true) {{ {fields} }}"


def graph_batches(gh, requests, width):
    result, audit = {}, []
    for offset in range(0, len(requests), width):
        chunk = requests[offset:offset + width]
        fields = [repository_field(f"r{index}", repo, field) for index, (_, repo, field) in enumerate(chunk)]
        query = "query { rateLimit { remaining resetAt } " + " ".join(fields) + " }"
        response, status = call_github(gh, ["graphql", "--input", "-"], query)
        data = response.get("data")
        data = data if isinstance(data, dict) else {}
        errors = response.get("errors")
        audit.append({"status": status, "aliases": len(chunk), "errors": len(errors or []),
                      "rate_limit": data.get("rateLimit")})
        for index, (key, _, _) in enumerate(chunk):
            result[key] = data.get(f"r{index}")
    return result, audit


def lineage(row):
    chain = []
    for _ in range(3):
        if not isinstance(row, dict) or row.get("visibility") != "PUBLIC" or row.get("isPrivate") is not False:
            return chain, False
        slug, identity, fork = row.get("nameWithOwner"), row.get("databaseId"), row.get("isFork")
        if (not isinstance(slug, str) or not REPO.fullmatch(slug) or type(identity) is not int
                or identity <= 0 or type(fork) is not bool or any(node["id"] == identity for node in chain)):
            return chain, False
        chain.append({"repo": slug.lower(), "id": identity, "fork": fork,
                      "mirror_reported": bool(row.get("mirrorUrl"))})
        if not fork:
            return chain, True
        row = row.get("parent")
    return chain, False


def detail_fields(row):
    base = row["base_commit"]
    issue = int(row["issue_numbers"][0])
    pull = int(row["pull_number"])
    require(OID.fullmatch(base) and 0 < issue < 2**31 and 0 < pull < 2**31 and pull != issue,
            "unresolved candidate commit, issue or pull number")
    return " ".join((
        IDENTITY,
        "licenseInfo { spdxId }",
        f'base:object(oid:"{base}") {{ ... on Commit {{ oid committedDate {TREE} }} }}',
        f'issue(number:{issue}) {{ number createdAt authorAssociation author {{ __typename }} }}',
        f'pull:pullRequest(number:{pull}) {{ number createdAt mergedAt merged baseRefOid '
        'mergeCommit { oid parents(first:2) { nodes { oid } } } '
        'closingIssuesReferences(first:10) { nodes { number repository { nameWithOwner } } } }',
        f'defaultBranchRef {{ target {{ oid ... on Commit {{ {TREE} }} }} '
        f'compare(headRef:"{base}") {{ aheadBy behindBy baseTarget {{ oid }} headTarget {{ oid }} }} }}',
    ))


def legal_entries(commit):
    if not isinstance(commit, dict) or not isinstance(commit.get("tree"), dict):
        return []
    entries = commit["tree"].get("entries")
    if not isinstance(entries, list):
        return []
    return sorted(
        ({"path": row["name"], "oid": row["oid"]}
         for row in entries if isinstance(row.get("name"), str) and LEGAL.fullmatch(row["name"])
         and row.get("type") == "blob" and row.get("mode") in (33188, 33261)
         and OID.fullmatch(row.get("oid") or "")),
        key=lambda row: row["path"],
    )


def parse_date(value):
    try:
        parsed = datetime.fromisoformat(value)
        return parsed.astimezone(timezone.utc) if parsed.utcoffset() is not None else None
    except (TypeError, ValueError, OverflowError):
        return None


def observed_case(row, chain, resolved, previous, historical_ids, historical_names, earliest):
    canonical, identity = chain[0]["repo"], chain[0]["id"]
    public = (resolved is not None and resolved.get("visibility") == "PUBLIC"
              and resolved.get("isPrivate") is False and resolved.get("databaseId") == identity)
    observed = {
        "instance_id": row["instance_id"], "requested_repo": row["repo"], "canonical_repo": canonical,
        "lineage": chain,
        "identity_matches_prior_observation": previous.get("github_repository_id") == identity,
        "previous_redirect_resolved": previous.get("status") == "http_301" and canonical != row["repo"],
        "known_contextbench_overlap": any(node["id"] in historical_ids or node["repo"] in historical_names
                                          for node in chain),
        "base_commit": row["base_commit"],
        "issue_number": int(row["issue_numbers"][0]), "pull_number": int(row["pull_number"]),
    }
    if not public:
        return observed
    base, issue, pull = (resolved.get(name) or {} for name in ("base", "issue", "pull"))
    default = resolved.get("defaultBranchRef") or {}
    target = default.get("target") or {}
    comparison = default.get("compare") or {}
    merge = pull.get("mergeCommit") or {}
    parents = (merge.get("parents") or {}).get("nodes") or []
    issue_date, pull_date, merge_date, base_date = map(
        parse_date, (issue.get("createdAt"), pull.get("createdAt"), pull.get("mergedAt"), base.get("committedDate")),
    )
    fetched_base = base.get("oid") == row["base_commit"]
    direct_base = (pull.get("baseRefOid") == row["base_commit"]
                   or bool(parents and parents[0].get("oid") == row["base_commit"]))
    default_contains = ((comparison.get("headTarget") or {}).get("oid") == row["base_commit"]
                        and isinstance(target.get("oid"), str)
                        and (comparison.get("baseTarget") or {}).get("oid") == target["oid"]
                        and comparison.get("aheadBy") == 0)
    closed = (pull.get("closingIssuesReferences") or {}).get("nodes") or []
    observed.update({
        "base_object_resolved": fetched_base,
        "base_tied_to_pull_ref_or_first_merge_parent": bool(fetched_base and direct_base and pull.get("merged") is True),
        "base_ancestor_of_observed_default": bool(fetched_base and default_contains),
        "base_legal_blobs": legal_entries(base) if fetched_base else [],
        "default_legal_blobs": legal_entries(target),
        "default_head_oid": target.get("oid"),
        "current_spdx": (resolved.get("licenseInfo") or {}).get("spdxId"),
        "base_commit_date": base.get("committedDate"),
        "original_issue_created_at": issue.get("createdAt"),
        "original_issue_user_actor": (issue.get("author") or {}).get("__typename") == "User",
        "original_issue_author_association": issue.get("authorAssociation"),
        "pull_created_at": pull.get("createdAt"), "pull_merged_at": pull.get("mergedAt"),
        "original_issue_named_in_pull_closure": any(
            value and value.get("number") == observed["issue_number"]
            and ((value.get("repository") or {}).get("nameWithOwner") or "").lower() == canonical
            for value in closed),
        "postmodel_issue_and_ordered_merge": bool(
            issue.get("number") == observed["issue_number"] and pull.get("number") == observed["pull_number"]
            and issue_date and pull_date and merge_date and pull.get("merged") is True
            and earliest <= issue_date.date() and issue_date <= merge_date and pull_date <= merge_date),
        "base_date_no_later_than_original_issue": bool(base_date and issue_date and base_date <= issue_date),
    })
    return observed


def pinned_license(gh, observed):
    if observed.get("base_object_resolved") is not True:
        return {"status": "base_unresolved"}
    repo, commit = observed["canonical_repo"], observed["base_commit"]
    response, status = call_github(gh, ["--method", "GET", "--jq", LICENSE_QUERY,
                                      f"repos/{repo}/license?ref={commit}"])
    path, oid, spdx = response.get("path"), response.get("sha"), response.get("spdx")
    if status != "ok":
        return {"status": status}
    if (not isinstance(path, str) or not isinstance(oid, str)
            or {"path": path, "oid": oid} not in observed["base_legal_blobs"]):
        return {"status": "base_root_blob_not_matched"}
    return {"status": "exact_root_blob_matched", "path": path, "oid": oid, "bytes": response.get("size"),
            "spdx_classifier": spdx if isinstance(spdx, str) else None,
            "same_blob_as_observed_default": {"path": path, "oid": oid} in observed["default_legal_blobs"],
            "permissive_classifier": isinstance(spdx, str) and spdx in PERMISSIVE}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("roster", "previous", "gh"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode, "run in isolated Python without bytecode")
    pinned_bytes(args.gh, GH_SHA, 64 << 20)
    roster = json.loads(pinned_bytes(args.roster, ROSTER_SHA, 1 << 20))
    prior = json.loads(pinned_bytes(args.previous, UPSTREAM_SHA, 1 << 20))
    candidates = roster["rows"]
    previous = {row["requested_repo"]: row for row in prior["rows"]}
    require(len(candidates) == len(previous) == 50 and all(len(row["issue_numbers"]) == 1 for row in candidates)
            and set(previous) == {row["repo"] for row in candidates}, "exact 50 intake case identities differ")
    collector = load_collector()
    import pyarrow.parquet as parquet
    prior_names, _, fence = collector.exclusion_fence(
        collector.load_blind_owner(), parquet, Path(roster["exposure"]["full_release"]["path"]),
        Path(roster["exposure"]["train_selection"]["path"]), Path(roster["exposure"]["blind_exposure"]["path"]),
    )
    require(fence == roster["exposure"], "frozen ContextBench exclusion differs")
    names = sorted(prior_names | set(previous))
    identities, requests = graph_batches(args.gh, [(name, name, LINEAGE) for name in names], 16)
    historical = [lineage(identities.get(name)) for name in sorted(prior_names)]
    historic_ids = {node["id"] for chain, _ in historical for node in chain}
    historic_names = prior_names | {node["repo"] for chain, _ in historical for node in chain}
    active = [(row, *lineage(identities.get(row["repo"]))) for row in candidates]
    print(json.dumps({
        "phase": "public_repository_identities",
        "candidate_lineages": sum(ok for _, _, ok in active),
        "historical_lineages": sum(ok for _, ok in historical),
        "historical_claims": len(historical),
        "graph_batches": len(requests),
    }), file=sys.stderr, flush=True)
    lookup = [(row["repo"], chain[0]["repo"], detail_fields(row)) for row, chain, complete in active
              if chain and complete]
    details, detail_requests = graph_batches(args.gh, lookup, 6)
    print(json.dumps({
        "phase": "public_issue_and_commit_metadata",
        "resolved_repo_responses": sum(isinstance(value, dict) for value in details.values()),
        "graph_batches": len(detail_requests),
    }), file=sys.stderr, flush=True)
    observed = [
        observed_case(row, chain, details.get(row["repo"]), previous[row["repo"]],
                      historic_ids, historic_names, collector.MIN_DATE)
        if chain and complete else {"instance_id": row["instance_id"], "requested_repo": row["repo"],
                                    "lineage": chain, "status": "public_lineage_unresolved"}
        for row, chain, complete in active
    ]
    with ThreadPoolExecutor(max_workers=5) as executor:
        licenses = list(executor.map(lambda row: pinned_license(args.gh, row), observed))
    for row, license in zip(observed, licenses, strict=True):
        row["license_at_base"] = license
    summary_keys = (
        "identity_matches_prior_observation", "previous_redirect_resolved",
        "known_contextbench_overlap", "base_object_resolved",
        "base_tied_to_pull_ref_or_first_merge_parent", "base_ancestor_of_observed_default",
        "original_issue_user_actor", "original_issue_named_in_pull_closure", "postmodel_issue_and_ordered_merge",
        "base_date_no_later_than_original_issue",
    )
    print(json.dumps({
        "schema": "airk.private.repository-confirmation-pinned-upstream-screen/1",
        "roster_sha256": ROSTER_SHA, "previous_upstream_sha256": UPSTREAM_SHA,
        "historical_claims": len(prior_names), "historical_public_lineages_resolved": sum(ok for _, ok in historical),
        "candidate_public_lineages_resolved": sum(ok for _, _, ok in active),
        "distinct_candidate_canonical_ids": len({chain[0]["id"] for _, chain, ok in active if chain and ok}),
        "github_graphql_batches": requests + detail_requests,
        "github_license_requests": sum(value["status"] != "base_unresolved" for value in licenses),
        "observed_counts": {key: sum(row.get(key) is True for row in observed) for key in summary_keys},
        "license_status_counts": dict(Counter(value["status"] for value in licenses)),
        "exact_root_spdx_counts": dict(Counter(value.get("spdx_classifier") or "unknown"
                                              for value in licenses if value["status"] == "exact_root_blob_matched")),
        "admitted_cases": 0, "rows": observed,
    }, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
