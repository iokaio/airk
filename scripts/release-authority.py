# SPDX-License-Identifier: Apache-2.0
"""Campaign release nomination and read-only provenance/policy decisions.

A nominated SHA or producer record is never qualification by itself. Campaign
consumers must authenticate the exact run/workflow/jobs/artifacts through GitHub
before signing or publication. This module never mutates policies or publishes.
"""
from copy import deepcopy
import hashlib
import json
import re

REPOSITORY = "iokaio/airk"
CAMPAIGN_BRANCH = "feat/advanced-split"
WORKFLOWS = frozenset(("ci.yml", "github-release.yml", "extension-release.yml", "downloads.yml", "macos-signing.yml"))
ENVIRONMENTS = frozenset(("github-pages", "github-releases", "macos-signing", "native-signing", "vscode-marketplace"))
CAMPAIGN_JOBS = frozenset(("Required checks", "container", "quick", "plan", "extension",
    "Native / x86_64-unknown-linux-gnu", "Native / x86_64-pc-windows-msvc",
    "Native / aarch64-apple-darwin", "Native / x86_64-apple-darwin",
    "Contracts / ubuntu-24.04", "Contracts / windows-2025", "Contracts / macos-15", "Contracts / macos-15-intel"))


def commit_id(value):
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{40}", value) is None:
        raise ValueError("release_commit_invalid")
    return value


def positive(value):
    if isinstance(value, bool) or re.fullmatch(r"[1-9][0-9]*", str(value)) is None:
        raise ValueError("release_run_identity_invalid")
    return int(value)


def branch_identity(branch, commit, qualified_commit=None):
    commit_id(commit)
    if branch == "main" and qualified_commit is None:
        return branch
    if branch != CAMPAIGN_BRANCH or qualified_commit != commit:
        raise ValueError("release_branch_or_qualification_refused")
    return branch


def manual_context(environment, workflow):
    """Validate a nomination; the caller still needs live producer provenance."""
    ref = environment.get("GITHUB_REF", "")
    branch = ref.removeprefix("refs/heads/")
    if (workflow not in WORKFLOWS or ref != "refs/heads/" + branch
            or environment.get("GITHUB_REPOSITORY") != REPOSITORY
            or environment.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or environment.get("GITHUB_WORKFLOW_REF") != f"{REPOSITORY}/.github/workflows/{workflow}@{ref}"):
        raise ValueError("release_manual_context_refused")
    commit = commit_id(environment.get("GITHUB_SHA"))
    qualified = environment.get("AIRK_QUALIFIED_SHA") or None
    branch_identity(branch, commit, qualified)
    return {"branch": branch, "commit": commit, "campaign": branch == CAMPAIGN_BRANCH}


def record_branch(record):
    """Old main records stay byte-compatible; campaign records are versioned."""
    schema = record.get("schema")
    old = ("airk-ci-producer/1", "airk-ci-producer/2", "airk-signed-release-producer/1")
    if schema in old:
        if "source_branch" in record or "qualified_commit" in record:
            raise ValueError("release_legacy_record_changed")
        return "main"
    if schema not in ("airk-ci-producer/3", "airk-signed-release-producer/2"):
        raise ValueError("release_producer_schema_refused")
    if record.get("repository") != REPOSITORY or record.get("event") != "workflow_dispatch":
        raise ValueError("release_producer_origin_refused")
    if record.get("source_branch") != CAMPAIGN_BRANCH or record.get("qualified_commit") != record.get("commit"):
        raise ValueError("release_campaign_record_identity_refused")
    if schema == "airk-ci-producer/3" and "promotion" in record:
        raise ValueError("release_campaign_promotion_refused")
    return branch_identity(record.get("source_branch"), record.get("commit"), record.get("qualified_commit"))


def verify_remote_origin(api, run_id, commit, workflow, *, branch="main", qualified_commit=None,
                         events=("workflow_dispatch",)):
    """Read actual run/workflow provenance; a self-asserted record cannot admit."""
    branch_identity(branch, commit, qualified_commit)
    if workflow not in WORKFLOWS:
        raise ValueError("release_workflow_refused")
    run_id = positive(run_id)
    run = api(f"actions/runs/{run_id}")
    owner = api(f"actions/workflows/{workflow}")
    if (run.get("id") != run_id or run.get("workflow_id") != owner.get("id")
            or run.get("head_sha") != commit or run.get("head_branch") != branch
            or run.get("path") != f".github/workflows/{workflow}"
            or run.get("repository", {}).get("full_name") != REPOSITORY
            or run.get("head_repository", {}).get("full_name") != REPOSITORY
            or run.get("status") != "completed" or run.get("conclusion") != "success"
            or run.get("event") not in events
            or branch == CAMPAIGN_BRANCH and run.get("event") != "workflow_dispatch"):
        raise ValueError("release_live_origin_refused")
    positive(owner.get("id"))
    positive(run.get("run_attempt"))
    return run


def verify_campaign_ci(api, run_id, commit):
    """All four targets/contracts and container gates, with no PR promotion."""
    run = verify_remote_origin(api, run_id, commit, "ci.yml", branch=CAMPAIGN_BRANCH, qualified_commit=commit)
    pages = api(f"actions/runs/{positive(run_id)}/attempts/{positive(run['run_attempt'])}/jobs?per_page=100", True)
    jobs = [job for page in pages for job in page["jobs"]]
    for name in CAMPAIGN_JOBS:
        matches = [job for job in jobs if job.get("name") == name]
        if len(matches) != 1 or matches[0].get("conclusion") != "success":
            raise ValueError("release_campaign_job_refused: " + name)
    return run


def canonical(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def environment_change_plan(snapshot, commit):
    """Describe exact additions only; never adopt a policy for later removal."""
    commit_id(commit)
    if not isinstance(snapshot, list) or {row.get("name") for row in snapshot} != ENVIRONMENTS or len(snapshot) != len(ENVIRONMENTS):
        raise ValueError("release_environment_snapshot_incomplete")
    additions = []
    for row in snapshot:
        if row.get("deployment_branch_policy") != {"custom_branch_policies": True, "protected_branches": False}:
            raise ValueError("release_environment_policy_mode_refused")
        policies = row.get("branch_policies", [])
        ids = [positive(policy.get("id")) for policy in policies]
        if len(ids) != len(set(ids)) or not any(policy.get("name") == "main" and policy.get("type") == "branch" for policy in policies):
            raise ValueError("release_environment_main_policy_missing")
        if not any(policy.get("name") == CAMPAIGN_BRANCH and policy.get("type") == "branch" for policy in policies):
            additions.append({"environment": row["name"], "method": "POST",
                              "path": f"environments/{row['name']}/deployment-branch-policies",
                              "body": {"name": CAMPAIGN_BRANCH, "type": "branch"}})
    return {"schema": "airk.release-environment-change/1", "repository": REPOSITORY,
            "source_branch": CAMPAIGN_BRANCH, "qualified_commit": commit,
            "snapshot_sha256": hashlib.sha256(canonical(snapshot)).hexdigest(),
            "before": deepcopy(snapshot), "additions": additions,
            "restoration": "Delete only successfully created policy IDs after revalidating their exact environment/name/type; preserve all prior policies and protection rules."}


def environment_restoration_plan(plan, created, current):
    """Refuse changed custody; return deletes solely for this plan's creations."""
    if (plan.get("schema") != "airk.release-environment-change/1" or plan.get("repository") != REPOSITORY
            or plan.get("source_branch") != CAMPAIGN_BRANCH
            or hashlib.sha256(canonical(plan.get("before"))).hexdigest() != plan.get("snapshot_sha256")):
        raise ValueError("release_environment_plan_invalid")
    commit_id(plan.get("qualified_commit"))
    original = {row["name"]: row for row in plan["before"]}
    observed = {row["name"]: row for row in current}
    allowed = {row["environment"] for row in plan["additions"]}
    if len(observed) != len(current) or set(observed) != set(original):
        raise ValueError("release_environment_snapshot_incomplete")
    if len({row["environment"] for row in created}) != len(created):
        raise ValueError("release_environment_duplicate_creation")
    removals = []
    for row in created:
        name, policy_id = row["environment"], positive(row["policy_id"])
        if name not in allowed:
            raise ValueError("release_environment_unowned_policy")
        before, now = original[name], observed[name]
        if any(now.get(key) != before.get(key) for key in ("deployment_branch_policy", "protection_rules")):
            raise ValueError("release_environment_protection_changed")
        additions = [policy for policy in now["branch_policies"] if policy not in before["branch_policies"]]
        expected = {"id": policy_id, "name": CAMPAIGN_BRANCH, "type": "branch"}
        if additions != [expected] or any(policy not in now["branch_policies"] for policy in before["branch_policies"]):
            raise ValueError("release_environment_policy_custody_changed")
        removals.append({"environment": name, "method": "DELETE", "path": f"environments/{name}/deployment-branch-policies/{policy_id}"})
    return removals
