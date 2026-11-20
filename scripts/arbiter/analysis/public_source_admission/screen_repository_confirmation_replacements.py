# SPDX-License-Identifier: Apache-2.0
"""Screen sixteen replacement MultiLang repositories from the authenticated release.

Run with the provisioned PyArrow interpreter and -IB, --directory pointing to
the original private roster directory, and --gh pointing to the checked CLI.
Retain stdout in a fresh private receipt. Only metadata fields are projected.
"""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

SCREEN_SHA = "c8c5a96c858819e8218d43edc383c2e78441be616aa100554f2e44f11edb00ad"
ORIGINAL_SCREEN_SHA = "dcef091a79fc092e5422b39c62c47c197810a5263c19d1d4a66c08362f3987c4"
FIRST_TRANCHE_SHA = "96c939368ad6ac79cfc91a1f309f9887aa6ef22e1be82033f4a91a5774d3f8aa"
TRANCHE = 16


def load_screen():
    path = Path(__file__).with_name("screen_repository_confirmation_upstreams.py")
    if hashlib.sha256(path.read_bytes()).hexdigest() != SCREEN_SHA:
        raise ValueError("upstream screener bytes differ")
    spec = importlib.util.spec_from_file_location("upstream_screen", path)
    screen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(screen)
    return screen


def candidates(collector, roster, directory, tranche):
    import pyarrow.parquet as parquet

    owner = collector.load_blind_owner()
    exposure = roster["exposure"]
    names, commits, fence = collector.exclusion_fence(
        owner, parquet, *(Path(exposure[key]["path"]) for key in ("full_release", "train_selection", "blind_exposure"))
    )
    collector.require(fence == exposure, "historical fence differs")
    leaves = {name.split("/")[1] for name in names}
    eligible = []
    seen = set()
    for split, size, sha in collector.SHARDS:
        rows, pin = collector.projected_shard(owner, parquet, directory, split, size, sha)
        collector.require(pin in roster["source"]["shards"], "shard provenance differs")
        for row in rows:
            instance = row["instance_id"]
            collector.require(instance not in seen, "duplicate publisher instance")
            seen.add(instance)
            item, _ = collector.eligible_row(owner, row, split, names, leaves, commits)
            if item:
                eligible.append(item)
    prior = roster["rows"]
    collector.require(len(seen) == 1077 and collector.select_candidates(eligible) == prior,
                      "original roster replay differs")
    excluded_names = {row["repo"] for row in prior}
    excluded_leaves = {name.split("/")[1] for name in excluded_names}
    excluded_bases = {row["base_commit"] for row in prior}
    remaining = [row for row in eligible if row["repo"] not in excluded_names
                 and row["repo"].split("/")[1] not in excluded_leaves and row["base_commit"] not in excluded_bases]
    ordered = collector.select_candidates(remaining)
    return ordered[(tranche - 1) * TRANCHE:tranche * TRANCHE], names


def family(chain):
    return {node["id"] for node in chain}


def reasons(row):
    failures = []
    for flag in ("complete_public_lineage", "postmodel_issue_and_ordered_merge",
                 "base_object_resolved", "base_tied_to_pull_ref_or_first_merge_parent"):
        if row.get(flag) is not True:
            failures.append(flag)
    for flag in ("known_contextbench_overlap", "original_roster_family_overlap", "within_tranche_family_overlap"):
        if row.get(flag) is not False:
            failures.append(flag)
    if row.get("previous_tranche_family_overlap"):
        failures.append("previous_tranche_family_overlap")
    license = row["license_at_base"]
    if license.get("status") != "exact_root_blob_matched" or license.get("permissive_classifier") is not True:
        failures.append("pinned_permissive_root_license")
    return failures


def run(directory, gh, tranche):
    screen = load_screen()
    screen.pinned_bytes(gh, screen.GH_SHA, 64 << 20)
    roster = json.loads(screen.pinned_bytes(directory / "review-candidates.private.json", screen.ROSTER_SHA, 1 << 20))
    original = json.loads(screen.pinned_bytes(directory / "pinned-upstream-screen.private.json",
                                              ORIGINAL_SCREEN_SHA, 1 << 20))
    collector = screen.load_collector()
    rows, names = candidates(collector, roster, directory, tranche)
    previous_family = set().union(*(family(row["lineage"]) for row in original["rows"]))
    earlier_family = set()
    if tranche == 2:
        first = json.loads(screen.pinned_bytes(directory / "replacement-screen-16-r1.private.json",
                                               FIRST_TRANCHE_SHA, 1 << 20))
        earlier_family.update(node_id for case in first["rows"] for node_id in family(case["lineage"]))
    requests = [(name, name, screen.LINEAGE) for name in sorted(names | {row["repo"] for row in rows})]
    identities, audit = screen.graph_batches(gh, requests, 16)
    historic = {name: screen.lineage(identities[name]) for name in sorted(names)}
    unresolved_names = [name for name, (_, ok) in historic.items() if not ok]
    history_ids = set().union(*(family(chain) for chain, _ in historic.values()))
    history_names = names | {node["repo"] for chain, _ in historic.values() for node in chain}
    active = [(row, *screen.lineage(identities[row["repo"]])) for row in rows]
    uses = Counter(node_id for _, chain, _ in active for node_id in family(chain))
    comparable = [(row["repo"], chain[0]["repo"], screen.detail_fields(row)) for row, chain, ok in active
                  if ok and len(row["issue_numbers"]) == 1]
    details, detail_audit = screen.graph_batches(gh, comparable, 6)
    observed = []
    for candidate, chain, ok in active:
        row = dict(candidate)
        row["lineage"] = chain
        row["complete_public_lineage"] = ok
        if ok and len(candidate["issue_numbers"]) == 1:
            facts = screen.observed_case(candidate, chain, details.get(candidate["repo"]), {},
                                         history_ids, history_names, collector.MIN_DATE)
            del facts["identity_matches_prior_observation"], facts["previous_redirect_resolved"]
            row.update(facts)
        row["original_roster_family_overlap"] = bool(family(chain) & previous_family)
        row["within_tranche_family_overlap"] = any(uses[value] > 1 for value in family(chain))
        if tranche == 2:
            row["previous_tranche_family_overlap"] = bool(family(chain) & earlier_family)
        observed.append(row)
    with ThreadPoolExecutor(max_workers=5) as executor:
        licenses = list(executor.map(lambda row: screen.pinned_license(gh, row), observed))
    for row, license in zip(observed, licenses, strict=True):
        row["license_at_base"] = license
        row["screen_failures"] = reasons(row)
    passed = [row for row in observed if not row["screen_failures"]]
    return {
        "schema": "airk.private.repository-confirmation-replacement-screen/1",
        "original_roster_sha256": screen.ROSTER_SHA, "original_screen_sha256": ORIGINAL_SCREEN_SHA,
        "upstream_screener_sha256": SCREEN_SHA, "collector_sha256": screen.COLLECTOR_SHA,
        "source": roster["source"], "exposure": roster["exposure"], "historical_claims": len(names),
        "unresolved_historical_claims": unresolved_names, "graphql_batches": audit + detail_audit,
        **({"previous_tranche_sha256": FIRST_TRANCHE_SHA, "tranche_number": 2} if tranche == 2 else {}),
        "review_tranche": len(observed), "scoped_passes": len(passed),
        "scoped_passes_base_no_later_than_issue": sum(row["base_date_no_later_than_original_issue"] for row in passed),
        "scoped_passes_later_base_user_actor": sum(not row["base_date_no_later_than_original_issue"]
                                                   and row["original_issue_user_actor"] for row in passed),
        "admitted_cases": 0, "rows": observed,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--gh", type=Path, required=True)
    parser.add_argument("--tranche", type=int, choices=(1, 2), default=1)
    args = parser.parse_args()
    if not sys.flags.isolated or not sys.dont_write_bytecode:
        parser.error("use isolated Python with bytecode disabled")
    print(json.dumps(run(args.directory, args.gh, args.tranche), sort_keys=True, allow_nan=False))
