# SPDX-License-Identifier: Apache-2.0
"""Admission for the private two-repository Focus training inputs."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import runpy


REPOSITORIES = {"bookbuilder", "munarium"}
POLICY = Path(__file__).resolve().parent.parent / "answer_eval_pairs" / "l7_policy.py"
REPLAY_FILES = {
    "census": "f2-native-census-actual-plan-r1/execution/native-census.json",
    "roster": "f2-existing-pairs-closure-r1/all-original1955-roster.json",
    "techqa": "f2-existing-replay-candidates-r1/techqa-software-document-fixed45.json",
}


def verified_bytes(path: Path, expected: str, pins: dict) -> bytes:
    if not isinstance(expected, str) or re.fullmatch("[0-9a-f]{64}", expected) is None:
        raise ValueError("expected a lowercase SHA-256 digest")
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"expected a regular pinned input: {path.name}")
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError(f"input SHA-256 differs: {path.name}")
    pins[str(path.resolve())] = expected
    return data


def assignments(values, expected_names, name):
    mapped = {}
    for item in values:
        key, sep, value = item.partition("=")
        if not sep or key in mapped or not value:
            raise ValueError(f"{name} requires unique name=value entries")
        mapped[key] = value
    if mapped.keys() != expected_names:
        raise ValueError(f"{name} requires exactly {', '.join(sorted(expected_names))}")
    return mapped


def protected_matchers(path, expected, pins):
    policy = runpy.run_path(str(POLICY))
    protected = policy["load_protected_paths"](path, expected)
    normalized_path = policy["normalized_path"]
    safe_relative_path = policy["safe_relative_path"]
    bookbuilder = policy["bookbuilder_denies"]()
    denied_by_fixture = policy["denied_by_fixture"]
    pins[str(path.resolve())] = expected
    for source in (POLICY, policy["FIXTURE"]):
        pins[str(source)] = hashlib.sha256(source.read_bytes()).hexdigest()

    def matches(repo, source):
        return (not safe_relative_path(source) or normalized_path(source) in protected[repo]
                or (repo == "bookbuilder" and denied_by_fixture(source, bookbuilder)))

    return ({repo: (lambda source, repo=repo: matches(repo, source)) for repo in REPOSITORIES},
            policy["deny_policy_sha256"](bookbuilder))


def check_pairs(repo, pairs, blocked):
    """Fail closed if an admitted pair would import a protected instruction or span."""
    if not pairs:
        raise ValueError(f"{repo}: corpus contains no pairs")
    for pair in pairs:
        spans = pair.get("evidence")
        source = pair.get("source")
        if (pair.get("repo") != repo or not isinstance(spans, list) or not spans
                or not isinstance(pair.get("commit"), str) or re.fullmatch("[0-9a-f]{40}", pair["commit"]) is None
                or pair.get("split") not in ("train", "heldout")):
            raise ValueError(f"{repo}: pair identity or evidence is invalid")
        places = spans + ([source] if source is not None else [])
        if any(blocked(place.get("path")) for place in places):
            raise ValueError(f"{repo}: corpus includes a protected source or evidence path")

def admit_local(pairs_dir, repo_specs, stats_sha, policy_path, policy_sha, pins):
    repositories = assignments(repo_specs, REPOSITORIES, "--repo")
    blocked, deny_sha = protected_matchers(policy_path, policy_sha, pins)
    stats_path = pairs_dir / "stats.json"
    stats = json.loads(verified_bytes(stats_path, stats_sha, pins))
    mine = stats.get("mine") or {}
    commits = mine.get("repos") or {}
    pair_sha = stats.get("pair_sha256") or {}
    if (stats.get("schema") != "airk.private.answer-eval-pairs/1" or mine.get("mode") != "l7"
            or mine.get("protected_paths_sha256") != policy_sha or commits.keys() != REPOSITORIES
            or pair_sha.keys() != REPOSITORIES):
        raise ValueError("mined cohort does not bind the L7 protected-path inventory and both pair files")
    if mine.get("bookbuilder_deny_policy_sha256") != deny_sha:
        raise ValueError("mined cohort differs from the current Bookbuilder deny policy")
    pairs_by_repo = {}
    for repo, entry in commits.items():
        for field in ("head", "snapshot_commit"):
            if re.fullmatch("[0-9a-f]{40}", entry.get(field, "")) is None:
                raise ValueError(f"{repo}: corpus needs exact Git commit identities")
        raw = verified_bytes(pairs_dir / f"{repo}.jsonl", pair_sha[repo], pins)
        pairs = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line.strip()]
        check_pairs(repo, pairs, blocked[repo])
        pairs_by_repo[repo] = pairs
    return repositories, stats, blocked, pairs_by_repo


def admit_replay(execution_root, specs, pins):
    expected = assignments(specs, REPLAY_FILES.keys(), "--replay-sha256")
    return {name: verified_bytes(execution_root / path, expected[name], pins) for name, path in REPLAY_FILES.items()}
