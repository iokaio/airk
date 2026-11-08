# SPDX-License-Identifier: Apache-2.0
"""Transfer an exact release pair between jobs; reuse only successful builds in this CI run."""
import argparse
from datetime import datetime
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


package = load("package-native")


def identity(target):
    commit = package.capture("git", "rev-parse", "HEAD")
    if commit != os.environ["GITHUB_SHA"] or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Transfer requires the exact CI checkout")
    for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"):
        if not re.fullmatch(r"[1-9][0-9]*", os.environ[key]):
            raise ValueError("Invalid CI run identity")
    return dict(commit=commit, tree=package.capture("git", "rev-parse", "HEAD^{tree}"),
                run_id=os.environ["GITHUB_RUN_ID"], target=target,
                qualification=os.environ.get("AIRK_QUALIFICATION", "false").lower() == "true")


def artifact_name(expected):
    return f"airk-runtime-{expected['target']}-{expected['commit']}-{expected['run_id']}"


def validate(runtime, target):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    source = package.runtime_builder().source_identity()
    files, executables = package.validate_runtime(runtime, version, target, source)
    build = package.bounded_json(runtime / "airk-build.json", 65_536)
    if build.get("profile") != "release" or "host: " + target not in build.get("rustc", "").splitlines():
        raise ValueError("Transfer is not a release pair for this native host")
    return files, executables


def seal(runtime, transfer, target):
    expected = identity(target)
    files, executables = validate(runtime, target)
    transfer.mkdir(parents=True, exist_ok=False)
    archive = transfer / "runtime.zip"
    package.write_archive(runtime, archive, files, executables)
    receipt = dict(schema="airk-ci-runtime/1", **expected, build_attempt=int(os.environ["GITHUB_RUN_ATTEMPT"]),
                   payload=files, archive_sha256=package.sha256(archive))
    (transfer / "transfer.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")


def payload_names(payload):
    if not isinstance(payload, dict) or not payload or len(payload) > package.MAX_PAYLOAD_FILES:
        raise ValueError("Transfer inventory exceeds budget")
    for name, digest in payload.items():
        if (not isinstance(name, str) or not name or "\\" in name or ":" in name
                or PurePosixPath(name).is_absolute() or any(part in ("", ".", "..") for part in name.split("/"))
                or not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest)):
            raise ValueError("Unsafe transfer inventory")


def restore(transfer, runtime, target):
    expected = identity(target)
    if set(path.name for path in transfer.iterdir()) != {"runtime.zip", "transfer.json"}:
        raise ValueError("Unexpected runtime transfer entries")
    receipt = package.bounded_json(transfer / "transfer.json", 524_288)
    if (receipt.get("schema") != "airk-ci-runtime/1" or any(receipt.get(key) != value for key, value in expected.items())
            or type(receipt.get("build_attempt")) is not int
            or not 1 <= receipt["build_attempt"] <= int(os.environ["GITHUB_RUN_ATTEMPT"])):
        raise ValueError("Runtime transfer belongs to another source, target, run or qualification")
    payload_names(receipt.get("payload"))
    archive = transfer / "runtime.zip"
    if archive.is_symlink() or package.sha256(archive) != receipt.get("archive_sha256"):
        raise ValueError("Runtime transfer archive digest differs")
    # Authenticate every entry before extraction or execution. The existing
    # packager enforces exact inventory, per-file hashes and executable modes.
    operating_system = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    builder = package.runtime_builder()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    executables = tuple(dict.fromkeys((*builder.runtime_executables(operating_system, version), builder.launcher_name(operating_system, version))))
    with zipfile.ZipFile(archive) as zipped:
        if sum(info.file_size for info in zipped.infolist()) > package.MAX_PAYLOAD_BYTES:
            raise ValueError("Transfer payload exceeds installer budget")
    package.verify_archive(archive, receipt["payload"], executables)
    if runtime.exists() or runtime.is_symlink():
        raise ValueError("Runtime destination must be fresh")
    runtime.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".ci-runtime-", dir=runtime.parent) as temporary:
        staging = Path(temporary) / "runtime"
        staging.mkdir()
        with zipfile.ZipFile(archive) as zipped:
            for name in receipt["payload"]:
                destination = staging / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                with zipped.open(name) as source, destination.open("xb") as output:
                    shutil.copyfileobj(source, output, length=1_048_576)
                destination.chmod(0o755 if name in executables else 0o644)
        files, _ = validate(staging, target)
        if files != receipt["payload"]:
            raise ValueError("Restored runtime differs from transfer inventory")
        staging.rename(runtime)
    return receipt


def resume(transfer, runtime, target):
    expected = identity(target)
    candidates = load("ci-candidates")
    artifacts = [a for a in candidates.run_artifacts(expected["run_id"]) if a.get("name") == artifact_name(expected)]
    if not artifacts or (len(artifacts) == 1 and artifacts[0].get("expired") is True):
        return False
    if len(artifacts) != 1:
        raise ValueError("Ambiguous runtime transfer")
    artifact = artifacts[0]
    if artifact.get("workflow_run", {}).get("id") != int(expected["run_id"]) or artifact.get("expired") is not False:
        raise ValueError("Runtime transfer origin differs")
    jobs = [job for page in candidates.api(f"actions/runs/{expected['run_id']}/jobs?filter=all&per_page=100", True)
            for job in page["jobs"]]
    matches = []
    failed_uploads = []
    for job in jobs:
        if job.get("name") != f"Native build / {target}" or job.get("conclusion") not in ("success", "failure", "cancelled"):
            continue
        if not 1 <= candidates.positive(job.get("run_attempt")) < int(os.environ["GITHUB_RUN_ATTEMPT"]):
            continue
        started, created, completed = (datetime.fromisoformat(value.replace("Z", "+00:00"))
                                      for value in (job["started_at"], artifact["created_at"], job["completed_at"]))
        if all(value.tzinfo is not None for value in (started, created, completed)) and started <= created <= completed:
            (matches if job["conclusion"] == "success" else failed_uploads).append(job)
    if not matches and len(failed_uploads) == 1:
        # Upload can precede a failed/cancelled post-job cache step. Never reuse
        # that build, but allow a new build to replace its unqualified handoff.
        print("Earlier runtime upload belongs to a failed build; rebuilding")
        return False
    if len(matches) != 1:
        raise ValueError("Runtime transfer is not bound to one successful earlier build job")
    candidates.download(dict(id=candidates.positive(artifact["id"]), digest=artifact.get("digest", "")), transfer)
    receipt = restore(transfer, runtime, target)
    if receipt["build_attempt"] != matches[0]["run_attempt"]:
        raise ValueError("Transfer receipt and successful build attempt differ")
    print(f"Reused successful native build attempt {receipt['build_attempt']}; validation still runs")
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("seal", "restore", "resume"))
    parser.add_argument("--target", choices=package.TARGETS, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--transfer", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "seal":
        seal(args.runtime_dir, args.transfer, args.target)
    elif args.command == "restore":
        restore(args.transfer, args.runtime_dir, args.target)
    else:
        restored = resume(args.transfer, args.runtime_dir, args.target)
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(f"restored={str(restored).lower()}\n")


if __name__ == "__main__":
    main()
