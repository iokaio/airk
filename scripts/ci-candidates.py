# SPDX-License-Identifier: Apache-2.0
"""Authenticate and download exact main CI artifacts before release promotion.

Split versions (split_receipts.SPLIT_VERSION and later) produce, per target, one
native service-pair candidate and, once per producer run, one universal UI-only
VSIX (`airk-vsix-universal-<source>-<run>-<attempt>`); earlier versions keep the
per-target native-plus-VSIX catalog. `version=None` selects that historical
catalog, so pre-split callers (the pinned 1.4.4 repair) keep their exact
behavior; the CLI passes the checked-in version. A universal VSIX never stands
in for a target's native evidence.
"""
import argparse
from datetime import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import tomllib
import zipfile

REPOSITORY = "iokaio/airk"
TARGETS = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin")
# PR CI gates and produces these; main push builds only the remaining target.
PR_TARGETS = TARGETS[:3]
MAIN_TARGETS = TARGETS[3:]
CONTRACT_HOSTS = ("ubuntu-24.04", "windows-2025", "macos-15", "macos-15-intel")


ROOT = Path(__file__).resolve().parent.parent
UNIVERSAL = "airk-vsix-universal-"


def _load(name, filename):
    specification = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(result)
    return result


reads = _load("github_read", "github-read.py")
# The C7 owner of the split threshold and the release catalog names.
contract = _load("airk_split_receipts", "split_receipts.py")


def checkout_version():
    """The checked-in package version; every candidate command runs on the exact commit's checkout."""
    return tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]


def split(version):
    return version is not None and contract.split_version(version)


def catalog_args(version):
    """Keyword arguments selecting `version`'s producer catalog; pre-split call shapes stay unchanged."""
    return dict(version=version) if split(version) else {}


def producer_kinds(version):
    """(per-target artifact kinds, one universal VSIX per producer run) of a version's catalog."""
    return (("native",), True) if split(version) else (("native", "vsix"), False)


def api(path, paginate=False):
    return reads.get(f"repos/{REPOSITORY}/{path}", paginate=paginate)


def positive(value):
    if not re.fullmatch(r"[1-9][0-9]*", str(value)):
        raise ValueError("Invalid run/artifact identity")
    return int(value)


def verify_run(run, commit, workflow, events, success=True):
    if (run.get("head_sha") != commit or run.get("head_branch") != "main"
            or run.get("event") not in events or run.get("path") != f".github/workflows/{workflow}"
            or run.get("repository", {}).get("full_name") != REPOSITORY
            or run.get("head_repository", {}).get("full_name") != REPOSITORY
            or run.get("status") != "completed"
            or (success and run.get("conclusion") != "success")):
        raise ValueError("Candidate is not a successful trusted main workflow at the exact commit")
    positive(run["id"])
    positive(run["run_attempt"])


def select_artifacts(artifacts, commit, run_id, attempt, kinds=("native", "vsix"), targets=TARGETS,
                     origin_sha=None, origin_branch="main", universal=False):
    suffix = f"{commit}-{run_id}" + (f"-{attempt}" if attempt is not None else "")
    expected = {f"airk-{kind}-{target}-{suffix}"
                for target in targets for kind in kinds}
    if universal:
        expected.add(UNIVERSAL + suffix)
    selected = {}
    for artifact in artifacts:
        name = artifact.get("name")
        if name not in expected:
            continue
        origin = artifact.get("workflow_run", {})
        if (name in selected or artifact.get("expired") is not False
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", ""))
                or origin.get("id") != int(run_id) or origin.get("head_sha") != (origin_sha or commit)
                or origin.get("head_branch") != origin_branch):
            raise ValueError("Duplicate, expired or unbound candidate artifact")
        selected[name] = dict(id=positive(artifact["id"]), digest=artifact["digest"])
    if set(selected) != expected:
        raise ValueError("Candidate lacks the complete artifact platform set for this attempt")
    return selected


def require_jobs(run_id, attempt, names):
    jobs = [job for page in api(f"actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100", True) for job in page["jobs"]]
    for name in names:
        matches = [job for job in jobs if job["name"] == name]
        if len(matches) != 1 or matches[0].get("conclusion") != "success":
            raise ValueError(f"Producer required job did not succeed exactly once: {name}")


def ci_workflow(run):
    workflow = api("actions/workflows/ci.yml")
    if run.get("workflow_id") != workflow.get("id"):
        raise ValueError("Producer workflow identity differs")
    return workflow["id"]


def run_artifacts(run_id):
    return [a for page in api(f"actions/runs/{run_id}/artifacts?per_page=100", True) for a in page["artifacts"]]


def promotion_name(commit, run_id, attempt):
    return f"airk-promotion-{commit}-{run_id}-{attempt}"


def attest_pull_request(run_id, head_sha, head_ref, *, version=None):
    """Authenticate a same-repository PR CI run that produced and gated the PR targets."""
    run = api(f"actions/runs/{positive(run_id)}")
    if (run.get("head_sha") != head_sha or run.get("head_branch") != head_ref or run.get("event") != "pull_request"
            or run.get("path") != ".github/workflows/ci.yml"
            or run.get("repository", {}).get("full_name") != REPOSITORY
            or run.get("head_repository", {}).get("full_name") != REPOSITORY
            or run.get("status") != "completed" or run.get("conclusion") != "success"):
        raise ValueError("PR producer is not a successful same-repository CI run at the final PR head")
    workflow_id = ci_workflow(run)
    attempt = positive(run["run_attempt"])
    names = {"Required checks", "container", "quick", "plan", "extension"}
    names.update(f"Native / {target}" for target in PR_TARGETS)
    names.update(f"Contracts / {os_name}" for os_name in CONTRACT_HOSTS)
    require_jobs(run_id, attempt, names)
    artifacts = run_artifacts(run_id)
    # The PR job checks out GitHub's merge ref; artifact names carry that commit.
    pattern = re.compile(rf"airk-native-{re.escape(PR_TARGETS[0])}-([0-9a-f]{{40}})-{run_id}-{attempt}")
    sources = {match.group(1) for a in artifacts if (match := pattern.fullmatch(a.get("name", "")))}
    if len(sources) != 1:
        raise ValueError("PR producer lacks one unambiguous merge-ref candidate")
    source = sources.pop()
    kinds, universal = producer_kinds(version)
    selected = select_artifacts(artifacts, source, run_id, attempt, kinds=kinds, targets=PR_TARGETS,
                                origin_sha=head_sha, origin_branch=head_ref, universal=universal)
    return dict(run_id=str(run_id), attempt=str(attempt), workflow_id=workflow_id, event="pull_request",
                head_sha=head_sha, head_ref=head_ref, source_commit=source, artifacts=selected)


def promote(commit, tree, scratch, *, version=None):
    """Select the merged PR's tested candidate; a mismatch leaves main to build everything."""
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not re.fullmatch(r"[0-9a-f]{40}", tree or ""):
        raise ValueError("Expected full main commit and tree")
    pulls = [p for p in api(f"commits/{commit}/pulls") if p.get("merged_at") and p.get("merge_commit_sha") == commit
             and p.get("base", {}).get("ref") == "main"]
    if len(pulls) != 1:
        raise ValueError("Main commit is not exactly one merged PR")
    pull = pulls[0]
    head = pull.get("head", {})
    if head.get("repo", {}).get("full_name") != REPOSITORY:
        raise ValueError("Forked PR artifacts are never promoted")
    head_sha, head_ref = head.get("sha", ""), head.get("ref", "")
    pages = api(f"actions/workflows/ci.yml/runs?head_sha={head_sha}&event=pull_request&status=success&per_page=100", True)
    for run in sorted((r for page in pages for r in page["workflow_runs"]), key=lambda r: r["id"], reverse=True):
        try:
            producer = attest_pull_request(run["id"], head_sha, head_ref, **catalog_args(version))
            for target in PR_TARGETS:
                name = f"airk-native-{target}-{producer['source_commit']}-{producer['run_id']}-{producer['attempt']}"
                destination = scratch / name
                download(producer["artifacts"][name], destination)
                receipt = json.loads((destination / "manifest.json").read_text(encoding="utf-8"))
                if (receipt.get("commit") != producer["source_commit"] or receipt.get("tree") != tree
                        or receipt.get("dirty_checkout") is not False or receipt.get("target") != target
                        or split(version) and receipt.get("version") != version):
                    raise ValueError("PR candidate source tree differs from main")
            if split(version):
                # The universal VSIX is promoted with the PR targets; its receipt binds the same tree.
                name = universal_artifact(producer)
                download(producer["artifacts"][name], scratch / name)
                receipt = json.loads((scratch / name / "manifest.json").read_text(encoding="utf-8"))
                if (receipt.get("commit") != producer["source_commit"] or receipt.get("tree") != tree
                        or receipt.get("target") != "universal" or receipt.get("version") != version):
                    raise ValueError("PR universal VSIX source tree differs from main")
            return dict(schema="airk-ci-promotion/1", repository=REPOSITORY, commit=commit, tree=tree,
                        pull_request=positive(pull["number"]), producer=producer)
        except ValueError as error:
            print(f"PR producer {run['id']} unavailable: {error}")
    raise ValueError("No eligible PR candidate; main builds every target")


def attest(run_id, commit, allow_manual=False, *, version=None):
    run = api(f"actions/runs/{positive(run_id)}")
    verify_run(run, commit, "ci.yml", ("push", "workflow_dispatch") if allow_manual else ("push",))
    workflow_id = ci_workflow(run)
    attempt = positive(run["run_attempt"])
    artifacts = run_artifacts(run_id)
    promotions = [a for a in artifacts if a.get("name") == promotion_name(commit, run_id, attempt)]
    base = dict(repository=REPOSITORY, workflow_id=workflow_id, commit=commit, run_id=str(run_id),
                attempt=str(attempt), event=run["event"])
    if not promotions and any(a.get("name", "").startswith(f"airk-promotion-{commit}-{run_id}-") for a in artifacts):
        raise ValueError("A partial rerun cannot reuse an earlier attempt's promotion; rerun all CI jobs")
    if not promotions:
        names = {"Required checks", "container", "quick", "plan", "extension"}
        names.update(f"Native / {target}" for target in TARGETS)
        names.update(f"Contracts / {os_name}" for os_name in CONTRACT_HOSTS)
        require_jobs(run_id, attempt, names)
        kinds, universal = producer_kinds(version)
        return dict(schema="airk-ci-producer/1", **base, artifacts=select_artifacts(
            artifacts, commit, run_id, attempt, kinds=kinds, universal=universal))
    if run["event"] != "push" or len(promotions) != 1 or promotions[0].get("expired") is not False:
        raise ValueError("Ambiguous, expired or manual promotion record")
    require_jobs(run_id, attempt, {"Required checks", "quick", "plan", *(f"Native / {t}" for t in MAIN_TARGETS)})
    record = promotions[0]
    with tempfile.TemporaryDirectory(prefix="airk-promotion-") as temporary:
        destination = Path(temporary) / "record"
        download(dict(id=record["id"], digest=record.get("digest", "")), destination)
        promotion = json.loads((destination / "promotion.json").read_text(encoding="utf-8"))
    producer = promotion.get("producer", {})
    if (promotion.get("schema") != "airk-ci-promotion/1" or promotion.get("repository") != REPOSITORY
            or promotion.get("commit") != commit or not re.fullmatch(r"[0-9a-f]{40}", str(promotion.get("tree", "")))):
        raise ValueError("Promotion record belongs to another commit/schema")
    # Recheck the PR producer's run, jobs and artifact identities now, not only at merge.
    if attest_pull_request(producer.get("run_id"), producer.get("head_sha"), producer.get("head_ref"),
                           **catalog_args(version)) != producer:
        raise ValueError("Promoted PR producer changed after promotion")
    # A promoted split main run builds only the remaining native target; the PR run's
    # universal VSIX is the release's tested extension.
    selected = {**producer["artifacts"],
                **select_artifacts(artifacts, commit, run_id, attempt, kinds=producer_kinds(version)[0],
                                   targets=MAIN_TARGETS)}
    return dict(schema="airk-ci-producer/2", **base, tree=promotion["tree"],
                promotion=dict(pull_request=promotion["pull_request"],
                               record=dict(id=positive(record["id"]), digest=record["digest"]),
                               producer={key: value for key, value in producer.items() if key != "artifacts"}),
                artifacts=selected)


def native_artifact(record, target):
    """The producer record names each authenticated original native artifact."""
    names = [name for name in record["artifacts"] if name.startswith(f"airk-native-{target}-")]
    if len(names) != 1:
        raise ValueError("Producer record lacks exactly one native artifact for the target")
    return names[0]


def universal_artifact(record):
    """The producer record names exactly one authenticated universal VSIX (split versions)."""
    names = [name for name in record["artifacts"] if name.startswith(UNIVERSAL)]
    if len(names) != 1:
        raise ValueError("Producer record lacks exactly one universal VSIX artifact")
    return names[0]


def resolve(commit, run_id=None, *, version=None):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected full source commit")
    if run_id:
        return attest(positive(run_id), commit, allow_manual=True, **catalog_args(version))
    pages = api(f"actions/workflows/ci.yml/runs?head_sha={commit}&event=push&status=success&per_page=100", True)
    runs = sorted((r for page in pages for r in page["workflow_runs"]), key=lambda r: r["id"], reverse=True)
    for run in runs:
        try:
            return attest(run["id"], commit, **catalog_args(version))
        except ValueError as error:
            print(f"Candidate {run['id']} unavailable: {error}")
    raise ValueError("No eligible main CI candidate. Run CI manually on this exact main commit, then supply candidate_run_id; no implicit rebuild is performed.")


def validate_record(record, commit, *, version=None):
    """Check a protected preflight record locally, without claiming live origin."""
    if (record.get("schema") not in ("airk-ci-producer/1", "airk-ci-producer/2")
            or record.get("repository") != REPOSITORY or record.get("commit") != commit
            or not re.fullmatch(r"[0-9a-f]{40}", commit)
            or record.get("event") not in ("push", "workflow_dispatch")):
        raise ValueError("Invalid pinned producer identity")
    for key in ("run_id", "attempt", "workflow_id"):
        positive(record.get(key))
    artifacts = record.get("artifacts", {})
    kinds, universal = producer_kinds(version)
    expected = set()

    def owner_of(target):
        if record["schema"] == "airk-ci-producer/2" and target in PR_TARGETS:
            owner = record.get("promotion", {}).get("producer", {})
            source = owner.get("source_commit", "")
            if not re.fullmatch(r"[0-9a-f]{40}", source):
                raise ValueError("Invalid promoted source commit")
            return owner, source
        return record, commit

    def require(owner, source, prefix):
        name = f"{prefix}{source}-{positive(owner.get('run_id'))}-{positive(owner.get('attempt'))}"
        expected.add(name)
        artifact = artifacts.get(name, {})
        positive(artifact.get("id"))
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", "")):
            raise ValueError("Invalid pinned artifact digest")

    for target in TARGETS:
        for kind in kinds:
            require(*owner_of(target), f"airk-{kind}-{target}-")
    if universal:
        # One universal VSIX, from the run that produced the PR targets (or every target).
        require(*owner_of(PR_TARGETS[0]), UNIVERSAL)
    count = len(TARGETS) * len(kinds) + universal
    if set(artifacts) != expected or len({a["id"] for a in artifacts.values()}) != count:
        raise ValueError(f"Pinned producer lacks exactly {count} unique artifacts")


def verify_record(record, commit, *, version=None):
    if record.get("commit") != commit or record.get("schema") not in ("airk-ci-producer/1", "airk-ci-producer/2"):
        raise ValueError("Producer record belongs to another commit/schema")
    current = attest(record["run_id"], commit, allow_manual=record.get("event") == "workflow_dispatch",
                     **catalog_args(version))
    if current != record:
        raise ValueError("Producer run attempt or artifact identities changed after preflight")


def attest_rehearsal(run_id, commit):
    """Only a complete, non-publishing main rehearsal may supply Marketplace VSIXs."""
    run_id = positive(run_id)
    run = api(f"actions/runs/{run_id}")
    verify_run(run, commit, "extension-release.yml", ("workflow_dispatch",))
    workflow = api("actions/workflows/extension-release.yml")
    if run.get("workflow_id") != workflow.get("id"):
        raise ValueError("Rehearsal workflow identity differs")
    attempt = positive(run["run_attempt"])
    jobs = [job for page in api(f"actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100", True) for job in page["jobs"]]
    required = {"preflight": "success", "publish": "skipped", "verify-identity": "skipped"}
    required.update({f"build ({target})": "success" for target in TARGETS})
    for name, conclusion in required.items():
        matches = [job for job in jobs if job["name"] == name]
        if len(matches) != 1 or matches[0].get("conclusion") != conclusion:
            raise ValueError(f"Rehearsal job has an unexpected result: {name}")
    artifacts = [a for page in api(f"actions/runs/{run_id}/artifacts?per_page=100", True) for a in page["artifacts"]]
    selected = select_artifacts(artifacts, commit, run_id, None, kinds=("vsix",))
    # Rehearsal artifact names survive retries. Bind each upload to the successful
    # build job in this attempt so retained files from older attempts cannot pass.
    for target in TARGETS:
        job = next(job for job in jobs if job["name"] == f"build ({target})")
        artifact = next(a for a in artifacts if a["name"] == f"airk-vsix-{target}-{commit}-{run_id}")
        try:
            started, created, completed = (datetime.fromisoformat(value.replace("Z", "+00:00"))
                                          for value in (job["started_at"], artifact["created_at"], job["completed_at"]))
            if any(value.tzinfo is None for value in (started, created, completed)) or not started <= created <= completed:
                raise ValueError("Upload outside successful job interval")
        except (KeyError, TypeError, AttributeError, ValueError) as error:
            raise ValueError("Rehearsal artifact is not bound to this attempt's build job") from error
    return dict(schema="airk-extension-rehearsal/1", repository=REPOSITORY, workflow_id=workflow["id"],
                commit=commit, run_id=str(run_id), attempt=str(attempt),
                artifacts=selected)


def download_rehearsal(record, commit, destination):
    if record.get("schema") != "airk-extension-rehearsal/1" or record.get("commit") != commit:
        raise ValueError("Rehearsal record belongs to another commit/schema")
    if attest_rehearsal(record["run_id"], commit) != record:
        raise ValueError("Rehearsal attempt or artifacts changed after preflight")
    if destination.exists():
        raise ValueError("Rehearsal destination must be fresh")
    for name, artifact in record["artifacts"].items():
        download(artifact, destination / name)
    if attest_rehearsal(record["run_id"], commit) != record:
        raise ValueError("Rehearsal changed during download")


def assembled_bundle(commit, run_id, maximum_attempt):
    """Select the retained successful assembly, not the latest workflow attempt.

    Failed publication/site-only retries leave the exact earlier signed bundle
    authoritative. A newer assembly supersedes that selection and invalidates
    any preflight pinned to older bytes; never silently fall back past it.
    """
    pattern = re.compile(rf"airk-release-{re.escape(commit)}-{run_id}-([1-9][0-9]*)")
    artifacts = [(artifact, int(match.group(1))) for artifact in run_artifacts(run_id)
                 if (match := pattern.fullmatch(artifact.get("name", "")))
                 and int(match.group(1)) <= maximum_attempt]
    if not artifacts:
        raise ValueError("Signed release lacks a retained exact-attempt bundle")
    attempt = max(attempt for _, attempt in artifacts)
    matches_artifact = [artifact for artifact, owner_attempt in artifacts if owner_attempt == attempt]
    if len(matches_artifact) != 1:
        raise ValueError("Signed release lacks one exact-attempt bundle")
    artifact = matches_artifact[0]
    origin = artifact.get("workflow_run", {})
    if (artifact.get("expired") is not False
            or not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", ""))
            or origin.get("id") != run_id or origin.get("head_sha") != commit
            or origin.get("head_branch") != "main"):
        raise ValueError("Signed release artifact is expired or has different provenance")
    jobs = [job for page in api(f"actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100", True)
            for job in page["jobs"]]
    matches = [job for job in jobs if job.get("name") == "assemble"]
    if len(matches) != 1 or matches[0].get("conclusion") != "success":
        raise ValueError("Signed release assembly did not succeed exactly once")
    try:
        started, created, completed = (datetime.fromisoformat(value.replace("Z", "+00:00"))
                                      for value in (matches[0]["started_at"], artifact["created_at"], matches[0]["completed_at"]))
        if any(value.tzinfo is None for value in (started, created, completed)) or not started <= created <= completed:
            raise ValueError("Upload outside assembly interval")
    except (KeyError, TypeError, AttributeError, ValueError) as error:
        raise ValueError("Signed release artifact is not bound to the successful assembly attempt") from error
    return artifact, attempt


def attest_signed_release(run_id, commit):
    """Pin the canonical signed assembly, never an unsigned Marketplace rebuild."""
    run_id = positive(run_id)
    run = api(f"actions/runs/{run_id}")
    verify_run(run, commit, "github-release.yml", ("workflow_dispatch",))
    workflow = api("actions/workflows/github-release.yml")
    if run.get("id") != run_id or run.get("workflow_id") != workflow.get("id"):
        raise ValueError("Signed release workflow identity differs")
    artifact, attempt = assembled_bundle(commit, run_id, positive(run["run_attempt"]))
    return dict(schema="airk-signed-release-producer/1", repository=REPOSITORY, workflow_id=workflow["id"],
                commit=commit, run_id=str(run_id), attempt=str(attempt), artifact_name=artifact["name"],
                artifact=dict(id=positive(artifact["id"]), digest=artifact["digest"]))


def download_signed_release(record, commit, destination, version, channel, *, tree=None, published_only=False):
    if (record.get("schema") != "airk-signed-release-producer/1" or record.get("commit") != commit
            or record.get("repository") != REPOSITORY):
        raise ValueError("Signed producer record belongs to another commit/schema")
    if destination.exists():
        raise ValueError("Signed release destination must be fresh")
    if attest_signed_release(record["run_id"], commit) != record:
        raise ValueError("Signed release attempt or artifact changed after preflight")
    with tempfile.TemporaryDirectory(prefix="airk-signed-release-") as temporary:
        bundle = Path(temporary) / "bundle"
        download(record["artifact"], bundle)
        if attest_signed_release(record["run_id"], commit) != record:
            raise ValueError("Signed release changed during download")
        spec = importlib.util.spec_from_file_location("signed_release", Path(__file__).with_name("github-release.py"))
        release = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(release)
        release.stage_marketplace_bundle(bundle, destination, commit, version, channel,
                                         record["run_id"], record["attempt"], tree=tree,
                                         published_only=published_only)


def unpack_artifact(archive, destination):
    if destination.exists():
        raise ValueError("Artifact destination must be fresh")
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        names = [entry.filename for entry in entries]
        if (len(names) != len(set(names)) or not names or len(names) > 30
                or sum(entry.file_size for entry in entries) > 2_147_483_648):
            raise ValueError("Artifact archive exceeds verification bounds or contains duplicates")
        for entry in entries:
            if (entry.is_dir() or stat.S_ISLNK(entry.external_attr >> 16)
                    or not re.fullmatch(r"[A-Za-z0-9_.-]+", entry.filename)
                    or entry.filename in (".", "..")):
                raise ValueError("Unsafe artifact path")
        destination.mkdir(parents=True)
        for entry in entries:
            with zipped.open(entry) as source, (destination / entry.filename).open("xb") as output:
                shutil.copyfileobj(source, output)


def download(artifact, destination):
    with tempfile.TemporaryDirectory(prefix="airk-artifact-") as temporary:
        archive = Path(temporary) / "artifact.zip"
        reads.get(f"repos/{REPOSITORY}/actions/artifacts/{positive(artifact['id'])}/zip", output=archive)
        with archive.open("rb") as stream:
            actual = "sha256:" + hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != artifact["digest"]:
            raise ValueError("Downloaded GitHub artifact digest differs from authenticated metadata")
        unpack_artifact(archive, destination)


def release_input_names(commit, run_id, *, version=None):
    """Same-run release inputs: signed (Linux: promoted) packages and a source receipt per target.

    Split versions have no per-target VSIX; the release run emits one channel-promoted
    universal VSIX (split_receipts.catalog_names `release_inputs`).
    """
    kinds, universal = producer_kinds(version)
    expected = {f"{UNIVERSAL}{commit}-{run_id}"} if universal else set()
    for target in TARGETS:
        prefix = "airk" if target == "x86_64-unknown-linux-gnu" else "airk-signed"
        expected.update(f"{prefix}-{kind}-{target}-{commit}-{run_id}" for kind in kinds)
        expected.add(f"airk-source-{target}-{commit}-{run_id}")
    return expected


def release_inputs(commit, run_id, destination, *, version=None):
    """Admit exact same-run signed outputs and small verified source receipts."""
    run_id = positive(run_id)
    run = api(f"actions/runs/{run_id}")
    verify_run({**run, "status": "completed"}, commit, "github-release.yml", ("workflow_dispatch",), success=False)
    expected = release_input_names(commit, run_id, **catalog_args(version))
    selected = {}
    for artifact in run_artifacts(run_id):
        name = artifact.get("name")
        if name not in expected:
            continue
        origin = artifact.get("workflow_run", {})
        if (name in selected or artifact.get("expired") is not False
                or origin.get("id") != run_id or origin.get("head_sha") != commit
                or origin.get("head_branch") != "main"
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", ""))):
            raise ValueError("Release inputs are duplicate, expired or from another origin")
        selected[name] = dict(id=positive(artifact["id"]), digest=artifact["digest"])
    if set(selected) != expected or len({row["id"] for row in selected.values()}) != len(expected):
        raise ValueError(f"Release inputs lack exactly {len(expected)} unique artifacts")
    destination.mkdir(parents=True, exist_ok=False)
    for name, artifact in selected.items():
        download(artifact, destination / name)
        print(f"Verified same-run release input: {name}")
        if name.startswith("airk-source-") and {p.name for p in (destination / name).iterdir()} != {"source.json"}:
            raise ValueError("Source receipt artifact contains unexpected files")


def release_bundle(commit, run_id, attempt, destination):
    """A failed publish may reuse a successful assembly from this run's earlier attempt."""
    run_id, attempt = positive(run_id), positive(attempt)
    run = api(f"actions/runs/{run_id}")
    # The current attempt is running, so authenticate its origin separately.
    origin = {**run, "status": "completed"}
    verify_run(origin, commit, "github-release.yml", ("workflow_dispatch",), success=False)
    artifact, _ = assembled_bundle(commit, run_id, attempt)
    download(artifact, destination)



def verify_tag(tag, commit):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:-build\.[1-9][0-9]*)?", tag):
        raise ValueError("Tag has no versioned release provenance; validate an explicit main CI candidate first")
    release = api(f"releases/tags/{tag}")
    ref = api(f"git/ref/tags/{tag}")
    if (release.get("draft") is not False or release.get("tag_name") != tag
            or ref.get("object", {}).get("sha") != commit):
        raise ValueError("Tag does not match a published release")
    receipts = [asset for asset in release["assets"] if asset["name"] == "release.json"]
    if len(receipts) != 1 or receipts[0]["size"] > 4_194_304:
        raise ValueError("Missing or oversized release provenance")
    data = subprocess.check_output(["gh", "api", f"repos/{REPOSITORY}/releases/assets/{positive(receipts[0]['id'])}",
                                    "-H", "Accept: application/octet-stream"])
    if "sha256:" + hashlib.sha256(data).hexdigest() != receipts[0].get("digest"):
        raise ValueError("Release provenance digest differs")
    record = json.loads(data)
    if record.get("commit") != commit or record.get("tag") != tag or record.get("schema") != "airk-github-release/1":
        raise ValueError("Release provenance belongs to another source/tag")
    version = record.get("package_version")
    if not isinstance(version, str) or tag.split("-", 1)[0] != f"v{version}":
        raise ValueError("Release provenance names another package version")
    verify_record(record["producer"], commit, **catalog_args(version))
    if split(version):
        # The reviewed split catalog: per-target pair packages (and installers) and one universal VSIX.
        catalog = contract.catalog_names(version, record.get("channel"), commit, record["producer"],
                                         dict(run_id=record.get("run_id")))[2]
        if set(record["assets"]) != catalog:
            raise ValueError("Release provenance differs from the split release catalog")
    count = len(catalog) if split(version) else 16
    assets = {asset["name"]: asset.get("digest") for asset in release["assets"]}
    if (len(assets) != len(release["assets"]) or len(record["assets"]) != count
            or set(assets) != set(record["assets"]) | {"release.json", "release-notes.md", "SHA256SUMS"}
            or any(assets.get(name) != "sha256:" + digest for name, digest in record["assets"].items())):
        raise ValueError("Published asset set differs from verified release provenance")
    print(f"Verified published tag {tag} from CI {record['producer']['run_id']}")


def download_names(record, kind, target=None, *, version=None):
    """The authenticated artifacts one download selects; selecting none is a failure."""
    if split(version) and kind == "vsix":
        if target:
            raise ValueError("Split versions have one universal VSIX; download it without --target")
        names = [universal_artifact(record)]
    else:
        names = [name for name in record["artifacts"] if name.startswith(f"airk-{kind}-")
                 and (not target or name.startswith(f"airk-{kind}-{target}-"))]
    if not names or target and len(names) != 1:
        raise ValueError("Producer record names no single matching artifact")
    return names


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("resolve", "download", "bundle", "tag", "rehearsal", "rehearsal-download", "promote", "release-inputs", "signed-release", "signed-release-download"))
    parser.add_argument("--record", type=Path, default=Path("artifacts/producer.json"))
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--kind", choices=("native", "vsix"), default="native")
    parser.add_argument("--pinned", action="store_true", help="Use a protected preflight record and verify downloaded digests locally")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--run-id")
    parser.add_argument("--version")
    parser.add_argument("--channel", choices=("stable", "preview"), default="stable")
    parser.add_argument("--published-only", action="store_true", help="Require exact matching published GitHub assets before Marketplace publication")
    args = parser.parse_args()
    commit = os.environ["GITHUB_SHA"]
    # Every command runs on the exact commit's checkout; its version selects the catalog.
    catalog = catalog_args(checkout_version())
    if args.command == "promote":
        if args.output is None:
            parser.error("promote requires --output")
        if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != commit:
            raise ValueError("Promotion requires the exact main checkout")
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], text=True).strip()
        with tempfile.TemporaryDirectory(prefix="airk-promote-") as temporary:
            record = promote(commit, tree, Path(temporary), **catalog)
        args.output.mkdir(parents=True, exist_ok=False)
        (args.output / "promotion.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(f"Promoted PR #{record['pull_request']} CI {record['producer']['run_id']} for main tree {tree}")
    elif args.command == "signed-release":
        record = attest_signed_release(args.run_id, commit)
        args.record.parent.mkdir(parents=True, exist_ok=True)
        args.record.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(f"Verified signed release {record['run_id']}, attempt {record['attempt']}, exact commit {commit}")
    elif args.command == "signed-release-download":
        if args.output is None or args.version is None:
            parser.error("signed-release-download requires --output and --version")
        if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != commit:
            raise ValueError("Signed release staging requires the exact main checkout")
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], text=True).strip()
        download_signed_release(json.loads(args.record.read_text(encoding="utf-8")), commit,
                                args.output, args.version, args.channel, tree=tree, published_only=args.published_only)
    elif args.command == "rehearsal":
        record = attest_rehearsal(args.run_id, commit)
        args.record.parent.mkdir(parents=True, exist_ok=True)
        args.record.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(f"Verified rehearsal {record['run_id']}, attempt {record['attempt']}, exact commit {commit}")
    elif args.command == "rehearsal-download":
        if args.output is None:
            parser.error("rehearsal-download requires --output")
        download_rehearsal(json.loads(args.record.read_text(encoding="utf-8")), commit, args.output)
    elif args.command == "tag":
        verify_tag(os.environ["GITHUB_REF_NAME"], commit)
    elif args.command == "resolve":
        record = resolve(commit, os.environ.get("AIRK_CANDIDATE_RUN_ID"), **catalog)
        args.record.parent.mkdir(parents=True, exist_ok=True)
        args.record.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(f"Verified producer {record['run_id']}, attempt {record['attempt']}, exact commit {commit}")
    elif args.command == "release-inputs":
        release_inputs(commit, os.environ["GITHUB_RUN_ID"], args.output, **catalog)
    elif args.command == "bundle":
        release_bundle(commit, os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"], args.output)
    else:
        record = json.loads(args.record.read_text(encoding="utf-8"))
        (validate_record if args.pinned else verify_record)(record, commit, **catalog)
        for name in download_names(record, args.kind, args.target, **catalog):
            download(record["artifacts"][name], args.output if args.target else args.output / name)


if __name__ == "__main__":
    main()
