# SPDX-License-Identifier: Apache-2.0
"""Select private repository confirmation review candidates without reading labels.

In the checked same-human shell, download the eight revision-pinned MultiLang
Parquet objects into a fresh private campaign directory, retaining their exact
publisher sizes and SHA-256 values below. Supply that directory and the three
existing ContextBench blind-fence inputs as absolute paths. Use isolated
Python with the provisioned PyArrow and retain stdout only in a private,
owner-audited roster file. This script opens no network connection.
"""

import argparse
from collections import Counter, defaultdict
from datetime import date, datetime
import hashlib
import importlib.util
import json
from pathlib import Path
import re


DATASET = "SWE-bench-Live/MultiLang"
REVISION = "eb09c2e5db6e4c5a95e01c903c8fd1d5da33c4f3"
COLUMNS = ("instance_id", "repo", "pull_number", "issue_numbers", "base_commit", "created_at")
MIN_DATE = date(2025, 8, 7)
RECENT = date(2026, 1, 1)
CANDIDATES = 50
BLIND_OWNER_SHA = "a2106ef61e28edb61dbf77e6d26a1a074dc851e8c40a893a3efa46e8f9203d55"
SHARDS = (
    ("c", 18_661_329,
     "a4c382c1416ee6e19964931998610f0bf1adc5132516ed034202675ad77541de"),
    ("cpp", 37_220_982,
     "8448db887817b63e4c0c284ca99de1ccda15023f48e5b2234a4084466e0768ae"),
    ("cs", 84_135_095,
     "29ffe16d0b2cd802e753262b8b0d7fe3f2bb1b489396da238146e79f37937c1f"),
    ("go", 47_688_780,
     "76d2b5dff0f3fac8303d30fa85495539e487d25974ad7c21cd21a545cb4756e2"),
    ("java", 90_384_179,
     "00387685808c71d21ada175335304b1c118859453afadd58ea21a00b0d568ee8"),
    ("js", 45_122_982,
     "bc6ec49ffaf9db97840d55eba6954fae8f5fb0fb071cf49e187f36ffadd55a7a"),
    ("rust", 57_661_358,
     "02ac78e2c51a84eb174ac393bb07b77478f1f96cf260af18835a711dd8074ebc"),
    ("ts", 31_013_117,
     "7e23783e27230c9cfab1035690035c25523043d6af635bc78da3fd2010c32714"),
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_blind_owner():
    path = Path(__file__).with_name("contextbench_blind_metadata_census.py")
    require(hashlib.sha256(path.read_bytes()).hexdigest() == BLIND_OWNER_SHA,
            "reviewed ContextBench blind census source differs")
    spec = importlib.util.spec_from_file_location("reviewed_contextbench_blind_owner", path)
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


def exclusion_fence(owner, parquet, release, selection_path, history_path):
    selection, selection_pin = owner.json_pin(selection_path, owner.SELECTION_SHA)
    history, history_pin = owner.json_pin(history_path, owner.BLIND_HISTORY_SHA)
    archive_pin = {"path": str(release), "bytes": owner.RELEASE[1], "sha256": owner.RELEASE[0]}
    previous = history["physical_quarantined_release_pin"]
    require(all(previous[key] == value for key, value in archive_pin.items())
            and history["present_original_TRAIN_ids"] == 450
            and history["other_rows_quarantined_count"] == 686
            and history["no_nonTRAIN_question_or_context_output_use_scoring_training_mining"] is True,
            "reviewed blind exposure receipt differs")
    rows = owner.projected_rows(release, parquet)
    ids = selection["selected_ids"]
    require(len(ids) == len(set(ids)) == selection["original_assigned_TRAIN_ids"] == 450
            and selection["source_opaque_sha256"] == owner.RELEASE[0]
            and selection["physical_release_rows"] == len(rows) == 1136
            and selection["all_other_rows_quarantined"] == 686
            and selection["missing_ids"] == []
            and selection["no_nonTRAIN_question_or_context_column_materialized_in_Python"] is True,
            "reviewed ContextBench TRAIN selection differs")
    owner.census(rows, set(ids))
    clusters = {repo for row in rows for repo in owner.credible_repository_claims(row)}
    commits = {row["base_commit"].lower() for row in rows
               if isinstance(row["base_commit"], str) and re.fullmatch(r"[0-9a-fA-F]{40}", row["base_commit"])}
    return clusters, commits, {
        "full_release": archive_pin, "train_selection": selection_pin, "blind_exposure": history_pin,
        "train_rows": 450, "full_release_rows": len(rows), "excluded_literal_repo_claims": len(clusters),
    }


def projected_shard(owner, parquet, directory, split, length, digest):
    name = f"{split}-00000-of-00001.parquet"
    path = owner.regular(directory / name)
    with path.open("rb") as source:
        before = owner.archive_state(source)
        require(before[2] == length and hashlib.file_digest(source, "sha256").hexdigest() == digest,
                f"{split} publisher shard differs")
        source.seek(0)
        reader = parquet.ParquetFile(source)
        require(all(reader.schema_arrow.names.count(key) == 1 for key in COLUMNS),
                f"{split} metadata column unavailable")
        table = reader.read(columns=list(COLUMNS), use_threads=False)
        require(tuple(table.column_names) == COLUMNS and table.num_rows == reader.metadata.num_rows,
                f"{split} metadata projection differs")
        rows = table.to_pylist()
        require(owner.archive_state(source) == before, f"{split} shard changed while reading metadata")
    return rows, {"split": split, "path": str(path), "bytes": length, "sha256": digest, "rows": len(rows),
                  "publisher_path": f"data/{name}"}


def eligible_row(owner, row, split, prior_repos, prior_leaves, prior_commits):
    repo_name, base, instance = row["repo"], row["base_commit"], row["instance_id"]
    repo = owner.github_slug(f"https://github.com/{repo_name}") if isinstance(repo_name, str) else None
    if (repo is None or not isinstance(base, str) or not re.fullmatch(r"[0-9a-f]{40}", base)
            or not isinstance(instance, str) or not re.fullmatch(r"[a-zA-Z0-9_.-]{1,300}", instance)):
        return None, "unresolved_identity"
    if repo in prior_repos or repo.split("/")[1] in prior_leaves or base in prior_commits:
        return None, "same_repo_leaf_or_commit_as_contextbench"
    try:
        created = datetime.fromisoformat(row["created_at"]).date()
    except (TypeError, ValueError):
        return None, "unresolved_publisher_date"
    if created < MIN_DATE:
        return None, "before_model_release_fence"
    pull, issues = row["pull_number"], row["issue_numbers"]
    # A PR-only row is not an independent issue-to-resolution provenance pair.
    if (not isinstance(pull, str) or not re.fullmatch(r"[1-9][0-9]{0,12}", pull)
            or not isinstance(issues, list) or not issues or len(issues) > 64
            or not all(isinstance(issue, str) and re.fullmatch(r"[1-9][0-9]{0,12}", issue) for issue in issues)
            or pull in issues):
        return None, "unresolved_issue_to_pull_provenance"
    return {
        "instance_id": instance,
        "repo": repo,
        "split": split,
        "base_commit": base,
        "publisher_created_at": row["created_at"],
        "issue_numbers": sorted(set(issues), key=int),
        "pull_number": pull,
        "base_commit_url": f"https://github.com/{repo}/commit/{base}",
    }, "eligible_metadata"


def select_candidates(eligible):
    # Take at most one literal repo or exact base commit. Interleave language
    # splits, preferring 2026 in each split so one large language cannot fill it.
    pending = defaultdict(list)
    for row in sorted(eligible, key=lambda row: (row["publisher_created_at"], row["instance_id"]), reverse=True):
        pending[row["split"]].append(row)
    retained, repos, leaves, commits = [], set(), set(), set()
    for recent_only in (True, False):
        for queue in zip_rounds(pending):
            for row in queue:
                recent = datetime.fromisoformat(row["publisher_created_at"]).date() >= RECENT
                leaf = row["repo"].split("/")[1]
                if (recent != recent_only or row["repo"] in repos or leaf in leaves
                        or row["base_commit"] in commits):
                    continue
                retained.append(row)
                repos.add(row["repo"])
                leaves.add(leaf)
                commits.add(row["base_commit"])
                if len(retained) == CANDIDATES:
                    return retained
    raise ValueError(f"fewer than {CANDIDATES} provisional distinct repos ({len(retained)})")


def zip_rounds(pending):
    splits = sorted(pending)
    for index in range(max((len(rows) for rows in pending.values()), default=0)):
        yield [pending[split][index] for split in splits if index < len(pending[split])]


def collect(args):
    import pyarrow.parquet as parquet

    owner = load_blind_owner()
    repos, commits, exposure = exclusion_fence(owner, parquet, args.release, args.selection, args.blind_history)
    prior_leaves = {repo.split("/")[1] for repo in repos}
    eligible, outcomes, shards = [], Counter(), []
    all_instances = set()
    for split, size, sha in SHARDS:
        rows, shard = projected_shard(owner, parquet, args.shards, split, size, sha)
        shards.append(shard)
        for row in rows:
            require(row["instance_id"] not in all_instances, "duplicate publisher instance ID")
            all_instances.add(row["instance_id"])
            item, reason = eligible_row(owner, row, split, repos, prior_leaves, commits)
            outcomes[reason] += 1
            if item is not None:
                eligible.append(item)
    require(len(all_instances) == 1077, "pinned MultiLang population size differs")
    selected = select_candidates(eligible)
    return {
        "schema": "airk.private.repository-confirmation-review-candidates/1",
        "source": {"dataset": DATASET, "revision": REVISION, "wrapper_license": "MIT",
                   "shards": shards, "projected_columns": list(COLUMNS), "minimum_publisher_date": str(MIN_DATE)},
        "exposure": exposure,
        "metadata_counts": dict(sorted(outcomes.items())),
        "eligible_literal_repos": len({row["repo"] for row in eligible}),
        "selected_split_counts": dict(sorted(Counter(row["split"] for row in selected).items())),
        "selected_publisher_date_2026_or_later": sum(
            datetime.fromisoformat(row["publisher_created_at"]).date() >= RECENT for row in selected),
        "distinct_literal_repos": len({row["repo"] for row in selected}),
        "distinct_repository_leaves": len({row["repo"].split("/")[1] for row in selected}),
        "status": "provisional_review_candidates",
        "admitted_cases": 0,
        "pending": ["fork and clone families", "later exposure", "upstream revision rights and reachability",
                    "blinded human source-span judgments", "complete menus and native visibility"],
        "rows": selected,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("shards", "release", "selection", "blind-history"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    require(sys_isolated(), "run with isolated Python and disabled bytecode")
    print(json.dumps(collect(args), sort_keys=True, allow_nan=False))


def sys_isolated():
    import sys

    return sys.flags.isolated and sys.dont_write_bytecode


if __name__ == "__main__":
    main()
