# SPDX-License-Identifier: Apache-2.0
"""Count provisional ContextBench repository clusters from the pinned blind archive.

Run in the checked Windows human shell with isolated Python and the provisioned
PyArrow. Supply --release, --selection, and --blind-history as absolute paths.
The single JSON stdout result is aggregate metadata. No request texts, gold
contexts, patches, menus, inferred labels, or row/cluster identities are emitted.
"""

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
from urllib.parse import urlsplit


RELEASE = ("2f56535bdc73eb8a68bf4ebb49789d8e9cd4f219ea60df6290b85278aee61ca8", 26_102_607)
SELECTION_SHA = "2c271a67417e6ce16efb109df2b2a4903f8554b448e1d448d222008e79a0bb7a"
BLIND_HISTORY_SHA = "879b615fe50a27cd41e6538a35a73a64acc1c0a2765ac440d1f7cbe069badb7c"
COLUMNS = ("instance_id", "original_inst_id", "repo", "repo_url", "base_commit")
PART = r"[a-zA-Z0-9][a-zA-Z0-9._-]*"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def regular(path):
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file()
            and not any(part.is_symlink() or part.is_junction() for part in (path, *path.parents)),
            "canonical absolute regular ContextBench input path required")
    return path


def json_pin(path, expected):
    with regular(path).open("rb") as source:
        raw = source.read((1 << 20) + 1)
    require(0 < len(raw) <= 1 << 20 and hashlib.sha256(raw).hexdigest() == expected,
            "bounded pinned ContextBench metadata JSON differs")
    return json.loads(raw), {"path": str(path), "bytes": len(raw), "sha256": expected}


def archive_state(source):
    stat = os.fstat(source.fileno())
    return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns


def projected_rows(path, parquet):
    with regular(path).open("rb") as source:
        before = archive_state(source)
        require(before[2] == RELEASE[1] and hashlib.file_digest(source, "sha256").hexdigest() == RELEASE[0],
                "exact quarantined ContextBench archive required")
        source.seek(0)
        reader = parquet.ParquetFile(source)
        require(reader.metadata.num_rows == 1136
                and all(reader.schema_arrow.names.count(column) == 1 for column in COLUMNS),
                "exact ContextBench metadata column projection unavailable")
        table = reader.read(columns=list(COLUMNS), use_threads=False)
        require(tuple(table.column_names) == COLUMNS and table.num_rows == 1136,
                "ContextBench metadata materialization differs from the fixed projection")
        rows = table.to_pylist()
        require(archive_state(source) == before, "ContextBench archive changed while reading metadata")
    return rows


def github_slug(value):
    if not isinstance(value, str) or not value:
        return None
    try:
        url = urlsplit(value)
    except ValueError:
        return None
    if url.scheme not in ("http", "https") or url.netloc.lower() not in ("github.com", "www.github.com"):
        return None
    path = url.path.removeprefix("/").removesuffix("/").removesuffix(".git")
    if url.query or url.fragment or not re.fullmatch(f"{PART}/{PART}", path):
        return None
    return path.lower()


def repository(label, url):
    link = github_slug(url)
    if not isinstance(label, str) or not label:
        return None, "empty_repository_label"
    if re.fullmatch(f"{PART}/{PART}", label):
        normalized = label.lower()
        if link is None:
            return None, "qualified_label_without_supported_url"
        if normalized != link:
            return None, "qualified_label_url_conflict"
        return normalized, "matching_qualified_label_and_url"
    if re.fullmatch(PART, label):
        if link and label.lower() == link.split("/")[1]:
            return link, "bare_label_resolved_by_url"
        return None, "bare_label_without_matching_url_leaf"
    return None, "unsupported_repository_label"


def credible_repository_claims(row):
    label = row["repo"]
    claims = {github_slug(row["repo_url"])}
    if isinstance(label, str) and re.fullmatch(f"{PART}/{PART}", label):
        claims.add(label.lower())
    return claims - {None}


def clustered(rows):
    clusters = Counter(repo for repo, _, _ in rows if repo is not None)
    return {
        "rows": sum(clusters.values()),
        "literal_repository_clusters": len(clusters),
        "largest_cluster_rows": max(clusters.values(), default=0),
        "cluster_size_histogram": {str(size): count for size, count in sorted(Counter(clusters.values()).items())},
    }


def census(rows, selected):
    require(all(set(row) == set(COLUMNS)
                and isinstance(row["instance_id"], str) and bool(row["instance_id"])
                and isinstance(row["original_inst_id"], str) and bool(row["original_inst_id"])
                for row in rows), "missing ContextBench metadata identity")
    row_counts = Counter(row["original_inst_id"] for row in rows)
    require(len(selected) == 450 and all(row_counts[key] == 1 for key in selected),
            "original TRAIN membership is absent or ambiguous")
    cohort = {True: [], False: []}
    statuses = {True: Counter(), False: Counter()}
    unresolved_claims = {True: set(), False: set()}
    claims_by_issue = {}
    for row in rows:
        train = row["original_inst_id"] in selected
        repo, status = repository(row["repo"], row["repo_url"])
        raw_commit = row["base_commit"]
        commit = raw_commit.lower() if isinstance(raw_commit, str) and re.fullmatch("[0-9a-fA-F]{40}", raw_commit) else None
        issue = row["original_inst_id"]
        claims = credible_repository_claims(row)
        claims_by_issue.setdefault(issue, set()).update(claims)
        if repo is None:
            unresolved_claims[train].update(claims)
        statuses[train][status] += 1
        cohort[train].append((repo, commit, issue))
    training, complement = cohort[True], cohort[False]
    require(len(training) == 450 and len(complement) == 686, "frozen ContextBench split counts differ")
    train_repos = {repo for repo, _, _ in training if repo is not None}
    train_claims = train_repos | unresolved_claims[True]
    train_leaves = {repo.split("/")[1] for repo in train_claims}
    train_commits = {commit for _, commit, _ in training if commit is not None}
    repos_by_leaf, repos_by_commit = {}, {}
    for repo, commit, _ in training + complement:
        if repo is not None:
            repos_by_leaf.setdefault(repo.split("/")[1], set()).add(repo)
            if commit is not None:
                repos_by_commit.setdefault(commit, set()).add(repo)
    train_instances = {row["instance_id"] for row in rows if row["original_inst_id"] in selected}
    same_instance = {
        row["original_inst_id"] for row in rows
        if row["original_inst_id"] not in selected and row["instance_id"] in train_instances
    }
    instance_counts = Counter(row["instance_id"] for row in rows)
    nonunique_instances = {
        row["original_inst_id"] for row in rows if instance_counts[row["instance_id"]] > 1
    }
    shared = [row for row in complement if row[0] in train_repos]
    literal_disjoint = [row for row in complement if row[0] is not None and row[0] not in train_repos]
    provisional = []
    signals = Counter()
    for repo, commit, issue in complement:
        shared_leaf = repo is not None and len(repos_by_leaf[repo.split("/")[1]]) > 1
        shared_commit = commit is not None and len(repos_by_commit.get(commit, ())) > 1
        if shared_leaf:
            signals["same_leaf_under_multiple_owners"] += 1
        if shared_commit:
            signals["same_base_commit_under_multiple_repository_names"] += 1
        if repo is None:
            signals["unresolved_repository_identity"] += 1
            if claims_by_issue[issue] & train_claims:
                signals["unresolved_complement_claim_touches_train"] += 1
        if repo is not None and repo not in train_repos and repo in unresolved_claims[True]:
            signals["resolved_complement_touches_unresolved_train_claim"] += 1
        if repo in train_repos:
            signals["same_literal_train_repository"] += 1
        if repo is not None and repo not in train_repos and repo.split("/")[1] in train_leaves:
            signals["different_owner_same_train_repository_leaf"] += 1
        if commit is None:
            signals["missing_or_non_full_hex_base_commit"] += 1
        if commit in train_commits:
            signals["same_full_train_base_commit"] += 1
        if issue in same_instance or issue in train_instances:
            signals["instance_id_collision_with_train"] += 1
        if issue in nonunique_instances or row_counts[issue] != 1:
            signals["nonunique_instance_or_original_id"] += 1
        if (repo is not None and repo not in train_claims and repo.split("/")[1] not in train_leaves
                and not shared_leaf
                and commit is not None and commit not in train_commits and not shared_commit
                and issue not in same_instance and issue not in train_instances
                and issue not in nonunique_instances and row_counts[issue] == 1):
            provisional.append((repo, commit, issue))

    return {
        "archive_rows": len(rows),
        "original_train_rows": len(training),
        "blind_complement_rows": len(complement),
        "repository_identity_status": {"train": dict(statuses[True]), "complement": dict(statuses[False])},
        "train_with_repository_identity": clustered(training),
        "complement_on_literal_train_repositories": clustered(shared),
        "complement_on_other_literal_repositories": clustered(literal_disjoint),
        "complement_without_repository_identity": len(complement) - len(literal_disjoint) - len(shared),
        "distinct_slug_claims_on_unresolved_train_rows": len(unresolved_claims[True]),
        "distinct_slug_claims_on_unresolved_complement_rows": len(unresolved_claims[False]),
        "overlapping_metadata_exposure_signals": {
            key: signals[key] for key in (
                "unresolved_repository_identity",
                "unresolved_complement_claim_touches_train",
                "resolved_complement_touches_unresolved_train_claim",
                "same_literal_train_repository",
                "different_owner_same_train_repository_leaf",
                "same_leaf_under_multiple_owners",
                "missing_or_non_full_hex_base_commit",
                "same_base_commit_under_multiple_repository_names",
                "same_full_train_base_commit",
                "instance_id_collision_with_train",
                "nonunique_instance_or_original_id",
            )
        },
        "provisional_after_literal_train_exposure_fence": clustered(provisional),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--selection", required=True, type=Path)
    parser.add_argument("--blind-history", required=True, type=Path)
    args = parser.parse_args()
    selection, selection_pin = json_pin(args.selection, SELECTION_SHA)
    history, history_pin = json_pin(args.blind_history, BLIND_HISTORY_SHA)
    archive_pin = {"path": str(args.release), "bytes": RELEASE[1], "sha256": RELEASE[0]}
    ids = selection["selected_ids"]
    require(isinstance(ids, list) and len(ids) == 450 and all(isinstance(value, str) and value for value in ids)
            and selection["source_opaque_sha256"] == RELEASE[0]
            and selection["physical_release_rows"] == 1136
            and selection["original_assigned_TRAIN_ids"] == 450
            and selection["missing_ids"] == [] and selection["all_other_rows_quarantined"] == 686
            and selection["no_nonTRAIN_question_or_context_column_materialized_in_Python"] is True,
            "reviewed ContextBench TRAIN ID selection differs")
    previously_pinned = history["physical_quarantined_release_pin"]
    require(all(previously_pinned[key] == value for key, value in archive_pin.items())
            and history["present_original_TRAIN_ids"] == 450 and history["other_rows_quarantined_count"] == 686
            and history["no_nonTRAIN_question_or_context_output_use_scoring_training_mining"] is True,
            "original ContextBench blind exposure receipt differs")

    import pyarrow
    import pyarrow.parquet as parquet

    counts = census(projected_rows(args.release, parquet), set(ids))
    print(json.dumps({
        "schema": "airk.private.contextbench-blind-metadata-eligibility-census/1",
        "archive": archive_pin,
        "train_selection": selection_pin,
        "blind_history": history_pin,
        "projected_columns": list(COLUMNS),
        "pyarrow_version": pyarrow.__version__,
        "counts": counts,
        "scope": {
            "publisher_test_partition_identified": False,
            "held_out_ids_reserved": False,
            "fork_alias_clone_or_cross_dataset_exposure_closed": False,
            "gold_context_source_spans_menus_or_statistical_power_reviewed": False,
            "qualification_or_admission": False,
        },
    }, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
