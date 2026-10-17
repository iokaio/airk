# SPDX-License-Identifier: Apache-2.0
"""Authenticate and download exact main CI artifacts before release promotion."""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import zipfile

REPOSITORY = "iokaio/airk"
TARGETS = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin")


def api(path, paginate=False):
    command = ["gh", "api", f"repos/{REPOSITORY}/{path}"]
    if paginate:
        command += ["--paginate", "--slurp"]
    return json.loads(subprocess.check_output(command))


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


def select_artifacts(artifacts, commit, run_id, attempt, kinds=("native", "vsix")):
    suffix = f"{commit}-{run_id}" + (f"-{attempt}" if attempt is not None else "")
    expected = {f"airk-{kind}-{target}-{suffix}"
                for target in TARGETS for kind in kinds}
    selected = {}
    for artifact in artifacts:
        name = artifact.get("name")
        if name not in expected:
            continue
        origin = artifact.get("workflow_run", {})
        if (name in selected or artifact.get("expired") is not False
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", ""))
                or origin.get("id") != int(run_id) or origin.get("head_sha") != commit
                or origin.get("head_branch") != "main"):
            raise ValueError("Duplicate, expired or unbound candidate artifact")
        selected[name] = dict(id=positive(artifact["id"]), digest=artifact["digest"])
    if set(selected) != expected:
        raise ValueError("Candidate lacks the complete artifact platform set for this attempt")
    return selected


def attest(run_id, commit, allow_manual=False):
    run = api(f"actions/runs/{positive(run_id)}")
    verify_run(run, commit, "ci.yml", ("push", "workflow_dispatch") if allow_manual else ("push",))
    workflow = api("actions/workflows/ci.yml")
    if run.get("workflow_id") != workflow.get("id"):
        raise ValueError("Producer workflow identity differs")
    attempt = positive(run["run_attempt"])
    jobs = [job for page in api(f"actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100", True) for job in page["jobs"]]
    names = {"Required checks", "container", "quick", "plan", "extension"}
    names.update(f"Native / {target}" for target in TARGETS)
    names.update(f"Contracts / {os_name}" for os_name in ("ubuntu-24.04", "windows-2025", "macos-15", "macos-15-intel"))
    for name in names:
        matches = [job for job in jobs if job["name"] == name]
        if len(matches) != 1 or matches[0].get("conclusion") != "success":
            raise ValueError(f"Producer required job did not succeed exactly once: {name}")
    artifacts = [a for page in api(f"actions/runs/{run_id}/artifacts?per_page=100", True) for a in page["artifacts"]]
    return dict(schema="airk-ci-producer/1", repository=REPOSITORY, workflow_id=workflow["id"],
                commit=commit, run_id=str(run_id), attempt=str(attempt), event=run["event"],
                artifacts=select_artifacts(artifacts, commit, run_id, attempt))


def resolve(commit, run_id=None):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected full source commit")
    if run_id:
        return attest(positive(run_id), commit, allow_manual=True)
    pages = api(f"actions/workflows/ci.yml/runs?head_sha={commit}&event=push&status=success&per_page=100", True)
    runs = sorted((r for page in pages for r in page["workflow_runs"]), key=lambda r: r["id"], reverse=True)
    for run in runs:
        try:
            return attest(run["id"], commit)
        except ValueError as error:
            print(f"Candidate {run['id']} unavailable: {error}")
    raise ValueError("No eligible main CI candidate. Run CI manually on this exact main commit, then supply candidate_run_id; no implicit rebuild is performed.")


def verify_record(record, commit):
    if record.get("commit") != commit or record.get("schema") != "airk-ci-producer/1":
        raise ValueError("Producer record belongs to another commit/schema")
    current = attest(record["run_id"], commit, allow_manual=record.get("event") == "workflow_dispatch")
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
        with archive.open("wb") as output:
            subprocess.run(["gh", "api", f"repos/{REPOSITORY}/actions/artifacts/{positive(artifact['id'])}/zip"], stdout=output, check=True)
        with archive.open("rb") as stream:
            actual = "sha256:" + hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != artifact["digest"]:
            raise ValueError("Downloaded GitHub artifact digest differs from authenticated metadata")
        unpack_artifact(archive, destination)


def release_bundle(commit, run_id, attempt, destination):
    """A failed publish may reuse a successful assembly from this run's earlier attempt."""
    run_id, attempt = positive(run_id), positive(attempt)
    run = api(f"actions/runs/{run_id}")
    # The current attempt is running, so authenticate its origin separately.
    origin = {**run, "status": "completed"}
    verify_run(origin, commit, "github-release.yml", ("workflow_dispatch",), success=False)
    artifacts = [a for page in api(f"actions/runs/{run_id}/artifacts?per_page=100", True) for a in page["artifacts"]]
    prefix = f"airk-release-{commit}-{run_id}-"
    matches = [a for a in artifacts if a["name"].startswith(prefix)
               and a["name"][len(prefix):].isdigit() and 0 < int(a["name"][len(prefix):]) <= attempt]
    if not matches:
        raise ValueError("No retained release bundle; rerun assembly explicitly")
    chosen_attempt = max(int(a["name"][len(prefix):]) for a in matches)
    chosen = [a for a in matches if a["name"] == prefix + str(chosen_attempt)]
    if len(chosen) != 1 or chosen[0].get("expired") is not False:
        raise ValueError("Ambiguous or expired release bundle")
    jobs = [j for page in api(f"actions/runs/{run_id}/attempts/{chosen_attempt}/jobs?per_page=100", True) for j in page["jobs"]]
    assembly = [j for j in jobs if j["name"] == "assemble"]
    if len(assembly) != 1 or assembly[0].get("conclusion") != "success":
        raise ValueError("Bundle assembly did not succeed")
    download(chosen[0], destination)


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
    verify_record(record["producer"], commit)
    assets = {asset["name"]: asset.get("digest") for asset in release["assets"]}
    if (len(assets) != len(release["assets"]) or len(record["assets"]) != 16
            or set(assets) != set(record["assets"]) | {"release.json", "release-notes.md", "SHA256SUMS"}
            or any(assets.get(name) != "sha256:" + digest for name, digest in record["assets"].items())):
        raise ValueError("Published asset set differs from verified release provenance")
    print(f"Verified published tag {tag} from CI {record['producer']['run_id']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("resolve", "download", "bundle", "tag", "rehearsal", "rehearsal-download"))
    parser.add_argument("--record", type=Path, default=Path("artifacts/producer.json"))
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--run-id")
    args = parser.parse_args()
    commit = os.environ["GITHUB_SHA"]
    if args.command == "rehearsal":
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
        record = resolve(commit, os.environ.get("AIRK_CANDIDATE_RUN_ID"))
        args.record.parent.mkdir(parents=True, exist_ok=True)
        args.record.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(f"Verified producer {record['run_id']}, attempt {record['attempt']}, exact commit {commit}")
    elif args.command == "bundle":
        release_bundle(commit, os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"], args.output)
    else:
        record = json.loads(args.record.read_text(encoding="utf-8"))
        verify_record(record, commit)
        for name, artifact in record["artifacts"].items():
            if name.startswith("airk-native-") and (not args.target or name.startswith(f"airk-native-{args.target}-")):
                download(artifact, args.output if args.target else args.output / name)


if __name__ == "__main__":
    main()
