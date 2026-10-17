# SPDX-License-Identifier: Apache-2.0
"""Verify, assemble and publish numbered AIRK GitHub Releases."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


extension = module("extension_release", "verify-extension-release.py")
native = module("native_package", "package-native.py")
signer = module("native_signer", "sign-native.py")
producers = module("ci_candidates", "ci-candidates.py")
SIGNED_TARGETS = {"x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin"}
REPOSITORY = "iokaio/airk"
WORKFLOW = REPOSITORY + "/.github/workflows/github-release.yml@refs/heads/main"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity(version, build):
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version):
        raise ValueError("Expected a numeric checked-in package version")
    if not re.fullmatch(r"[1-9]\d*", str(build)):
        raise ValueError("Expected a positive workflow build number")
    return f"v{version}-build.{build}"


def context():
    if (os.environ.get("GITHUB_REPOSITORY") != REPOSITORY
            or os.environ.get("GITHUB_REF") != "refs/heads/main"
            or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or os.environ.get("GITHUB_WORKFLOW_REF") != WORKFLOW):
        raise ValueError("Release requires the manual AIRK main-branch workflow")
    commit = os.environ["GITHUB_SHA"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected full source commit")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if json.loads((ROOT / "packages/vscode/package.json").read_text(encoding="utf-8"))["version"] != version:
        raise ValueError("Native and extension source versions differ")
    tag = identity(version, os.environ["GITHUB_RUN_NUMBER"])
    for field in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"):
        if not re.fullmatch(r"[1-9]\d*", os.environ[field]):
            raise ValueError("Invalid workflow run identity")
    return commit, version, tag


def candidate(directory):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("Missing or linked candidate directory")
    receipt = extension.read_receipt(directory / "manifest.json")
    name = receipt["archive"]
    extension.package.safe_name(name)
    if "/" in name:
        raise ValueError("Candidate archive must be a filename")
    if set(p.name for p in directory.iterdir()) != {name, "manifest.json", "SHA256SUMS"}:
        raise ValueError("Unexpected candidate files")
    if any(p.is_symlink() or not p.is_file() for p in directory.iterdir()):
        raise ValueError("Linked or non-file candidate payload")
    archive = directory / name
    if digest(archive) != receipt["archive_sha256"]:
        raise ValueError("Candidate archive digest mismatch")
    expected = f'{digest(archive)}  {name}\n{digest(directory / "manifest.json")}  manifest.json\n'
    if (directory / "SHA256SUMS").read_text(encoding="utf-8") != expected:
        raise ValueError("Candidate checksums differ")
    return receipt, archive


def verify_native(directory, target, commit, version, signed):
    receipt, archive = candidate(directory)
    if (receipt.get("schema") != "airk-native-candidate/1"
            or receipt.get("target") != target or receipt.get("vsix_target") != native.TARGETS[target]
            or receipt.get("commit") != commit or receipt.get("version") != version
            or receipt.get("dirty_checkout") is not False
            or receipt.get("cargo_lock_sha256") != digest(ROOT / "Cargo.lock")
            or receipt.get("toolchain_file_sha256") != digest(ROOT / "rust-toolchain.toml")
            or receipt.get("signed_distribution") is not signed
            or archive.name != f"airk-{version}-{target}.zip"):
        raise ValueError("Native candidate provenance or signing state differs")
    executable = "airk.exe" if "windows" in target else "airk"
    required = {executable, "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}
    payload = receipt["payload"]
    if not required.issubset(payload):
        raise ValueError("Native legal files or executable missing")
    for name in payload:
        extension.package.safe_name(name)
        if name not in required and not name.startswith("notices/"):
            raise ValueError("Unexpected native payload path")
    with zipfile.ZipFile(archive) as zipped:
        if len(zipped.infolist()) > 15_000 or sum(p.file_size for p in zipped.infolist()) > 1_073_741_824:
            raise ValueError("Native archive exceeds verification budget")
        if any(stat.S_ISLNK(p.external_attr >> 16) or p.is_dir() for p in zipped.infolist()):
            raise ValueError("Native archive links/directories rejected")
    native.verify_archive(archive, payload, executable)
    signing = receipt.get("signing")
    if signed:
        if (not isinstance(signing, dict) or signing.get("schema") != "airk-native-signing/1"
                or signing.get("target") != target or signing.get("commit") != commit
                or signing.get("cargo_lock_sha256") != receipt["cargo_lock_sha256"]
                or signing.get("binary_sha256") != payload[executable]
                or not re.fullmatch(r"[0-9a-f]{64}", signing.get("source_sha256", ""))):
            raise ValueError("Signing receipt does not bind this build")
        verification = signing.get("verification", {})
        if verification.get("timestamp_present") is not True:
            raise ValueError("Trusted signing timestamp missing")
        if "apple" in target:
            if (signing.get("platform") != "Darwin" or signing.get("team_id") != "5PX7D99PXA"
                    or verification.get("trust") != "apple_developer_id"
                    or verification.get("team_id") != "5PX7D99PXA"
                    or verification.get("hardened_runtime") is not True
                    or (signing.get("notarization") or {}).get("status") != "Accepted"
                    or not (signing.get("notarization") or {}).get("id")):
                raise ValueError("Apple signature or accepted notarization missing")
        elif (signing.get("platform") != "Windows" or signing.get("azure") != signer.AZURE_SIGNING
              or verification.get("trust") != "authenticode_valid"
              or not re.fullmatch(r"[0-9a-fA-F]{40}", signing.get("identity", ""))
              or verification.get("signer_thumbprint") != signing["identity"]):
            raise ValueError("Azure Authenticode identity missing")
    elif signing is not None:
        raise ValueError("Unsigned native candidate contains signing metadata")
    return receipt, archive


def stage_signing(directory, target, commit, version):
    """Restore only the tested executable, never arbitrary artifact paths."""
    if target not in SIGNED_TARGETS:
        raise ValueError("Unsupported signing target")
    _, archive = verify_native(directory, target, commit, version, False)
    executable = "airk.exe" if "windows" in target else "airk"
    output = ROOT / "target" / target / "release"
    output.mkdir(parents=True, exist_ok=False)
    with zipfile.ZipFile(archive) as zipped:
        (output / executable).write_bytes(zipped.read(executable))
    (output / executable).chmod(0o755)


def assemble(directory, output, commit, version, tag, channel, run_id, attempt, producer=None):
    if tag != identity(version, tag.rsplit(".", 1)[-1]) or channel not in ("preview", "stable"):
        raise ValueError("Invalid release identity/channel")
    suffix = f"{commit}-{run_id}-{attempt}" if producer is None else f"{commit}-{run_id}"
    producer_suffix = suffix
    if producer is not None:
        producers.verify_record(producer, commit)
        producer_suffix = f"{commit}-{producer['run_id']}-{producer['attempt']}"
    selected = []
    extensions = []
    for target in native.TARGETS:
        prefix = "airk-signed" if target in SIGNED_TARGETS else "airk"
        native_dir = directory / f"{prefix}-native-{target}-{suffix}"
        vsix_dir = directory / f"{prefix}-vsix-{target}-{suffix}"
        receipt, archive = verify_native(native_dir, target, commit, version, target in SIGNED_TARGETS)
        vsix_receipt, vsix = candidate(vsix_dir)
        if (vsix_receipt["native_manifest_sha256"] != digest(native_dir / "manifest.json")
                or vsix_receipt["native_archive_sha256"] != receipt["archive_sha256"]):
            raise ValueError("VSIX and native download are not the same build")
        if target in SIGNED_TARGETS:
            original, _ = verify_native(directory / f"airk-native-{target}-{producer_suffix}", target, commit, version, False)
            executable = "airk.exe" if "windows" in target else "airk"
            if receipt["signing"]["source_sha256"] != original["payload"][executable]:
                raise ValueError("Signed binary did not originate in the tested candidate")
        elif producer is not None:
            original, _ = verify_native(directory / f"airk-native-{target}-{producer_suffix}", target, commit, version, False)
            if receipt != original:
                raise ValueError("Linux native package differs from the tested producer")
        extensions.append(vsix_dir / "manifest.json")
        selected.extend(((archive, archive.name), (vsix, vsix.name),
                         (native_dir / "manifest.json", f"airk-{version}-{target}.native-manifest.json"),
                         (vsix_dir / "manifest.json", f"airk-{version}-{native.TARGETS[target]}.vsix-manifest.json")))
    extension.verify(directory, commit, version, channel, receipts=extensions)
    output.mkdir(parents=True, exist_ok=False)
    for source, name in selected:
        shutil.copyfile(source, output / name)
    record = {"schema": "airk-github-release/1", "tag": tag, "package_version": version,
              "commit": commit, "channel": channel, "run_id": str(run_id), "attempt": str(attempt),
              "assets": {name: digest(output / name) for _, name in selected}}
    if producer is not None:
        record["producer"] = producer
    (output / "release.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    build_run = producer["run_id"] if producer else run_id
    build_attempt = producer["attempt"] if producer else attempt
    notes = (f"AIRK {tag} ({channel})\n\nPackage version: {version}. Source commit: {commit}.\n"
             f"Build and checks: https://github.com/{REPOSITORY}/actions/runs/{build_run}/attempts/{build_attempt}\n"
             f"Signing and assembly: https://github.com/{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}\n\n"
             "Downloads include native ZIPs and matching platform VSIX files. Windows uses Ioka LLC "
             "Azure Artifact Signing. Both macOS helpers use Developer ID signing and accepted Apple "
             "notarization; the standalone CLI has no stapled offline ticket. Linux is unsigned.\n\n"
             "Use the ZIP matching your OS/architecture, or install the matching VSIX from VS Code's "
             "Install from VSIX command. SHA256SUMS covers packages and build records. "
             "The build number identifies this release; app/extension package versions are unchanged.\n\n"
             "Installed-editor and oldest-OS qualification remain separate. Review the release checklist "
             "and dependency notices. This release does not publish to Visual Studio Marketplace.\n")
    (output / "release-notes.md").write_text(notes, encoding="utf-8")
    checksums = "".join(f"{digest(path)}  {path.name}\n" for path in sorted(output.iterdir()))
    (output / "SHA256SUMS").write_text(checksums, encoding="utf-8")
    return record


def github(path, *, missing=False, paginate=False):
    command = ["gh", "api", f"repos/{REPOSITORY}/{path}"]
    if paginate:
        command.extend(("--paginate", "--slurp"))
    result = subprocess.run(command,
                            capture_output=True, text=True, check=False)
    if result.returncode:
        if missing and "(HTTP 404)" in result.stderr:
            return None
        raise RuntimeError("GitHub API failed: " + result.stderr)
    return json.loads(result.stdout)


def release_id_for_tag(tag):
    # The by-tag endpoint returns published releases only. Include drafts and
    # every page, so an earlier partial publication cannot be overwritten.
    pages = github("releases?per_page=100", paginate=True)
    matches = [entry for page in pages for entry in page if entry.get("tag_name") == tag]
    if not matches:
        return None
    if len(matches) != 1 or type(matches[0].get("id")) is not int or matches[0]["id"] <= 0:
        raise ValueError("Ambiguous or invalid release identity; inspect retained drafts")
    return matches[0]["id"]


def require_unused_tag(tag):
    if (github(f"releases/tags/{tag}", missing=True) is not None
            or release_id_for_tag(tag) is not None
            or github(f"git/ref/tags/{tag}", missing=True) is not None):
        raise ValueError("Release/tag already exists; dispatch a new build rather than overwrite it")


def verify_bundle(directory, commit, version, tag, run_id, attempt):
    record = extension.read_receipt(directory / "release.json")
    if (record.get("schema") != "airk-github-release/1" or record.get("commit") != commit
            or record.get("tag") != tag or record.get("package_version") != version
            or record.get("run_id") != str(run_id) or record.get("attempt") != str(attempt)
            or record.get("channel") not in ("preview", "stable")):
        raise ValueError("Release bundle belongs to another run or commit")
    expected = set()
    for target, platform in native.TARGETS.items():
        expected.update((f"airk-{version}-{target}.zip", f"airk-{version}-{platform}.vsix",
                         f"airk-{version}-{target}.native-manifest.json",
                         f"airk-{version}-{platform}.vsix-manifest.json"))
    if set(record["assets"]) != expected:
        raise ValueError("Release bundle asset set differs")
    expected.update(("release.json", "release-notes.md", "SHA256SUMS"))
    if directory.is_symlink() or set(p.name for p in directory.iterdir()) != expected:
        raise ValueError("Unexpected release bundle files")
    if any(p.is_symlink() or not p.is_file() for p in directory.iterdir()):
        raise ValueError("Release bundle links rejected")
    for name, value in record["assets"].items():
        if digest(directory / name) != value:
            raise ValueError("Release bundle digest mismatch")
    paths = sorted(p for p in directory.iterdir() if p.name != "SHA256SUMS")
    if (directory / "SHA256SUMS").read_text(encoding="utf-8") != "".join(f"{digest(p)}  {p.name}\n" for p in paths):
        raise ValueError("Release bundle checksums differ")
    return record


def publish(directory, resume=False):
    commit, version, tag = context()
    attempt = os.environ["GITHUB_RUN_ATTEMPT"]
    if resume:
        attempt = extension.read_receipt(directory / "release.json")["attempt"]
        if not 0 < producers.positive(attempt) <= producers.positive(os.environ["GITHUB_RUN_ATTEMPT"]):
            raise ValueError("Release bundle comes from a future attempt")
    record = verify_bundle(directory, commit, version, tag, os.environ["GITHUB_RUN_ID"], attempt)
    assets = sorted(directory.iterdir())
    release_id = release_id_for_tag(tag) if resume else None
    expected = {p.name: "sha256:" + digest(p) for p in assets}
    if release_id is not None:
        uploaded = github(f"releases/{release_id}")
        actual = {p["name"]: p.get("digest") for p in uploaded["assets"]}
        if (uploaded.get("id") != release_id or uploaded.get("tag_name") != tag
                or uploaded.get("target_commitish") != commit
                or uploaded.get("prerelease") is not (record["channel"] == "preview")
                or len(uploaded["assets"]) != len(actual)
                or any(name not in expected or expected[name] != value for name, value in actual.items())):
            raise ValueError("Existing release identity/assets differ; draft retained for inspection")
        tag_ref = github(f"git/ref/tags/{tag}", missing=True)
        if tag_ref is not None and tag_ref.get("object", {}).get("sha") != commit:
            raise ValueError("Release tag points at another commit; draft retained for inspection")
        if uploaded.get("draft") is False:
            if actual != expected or tag_ref is None:
                raise ValueError("Published release is incomplete or has no matching tag")
            print("Verified release already published")
            return
        if uploaded.get("draft") is not True:
            raise ValueError("Invalid remote draft state")
        missing = [str(p) for p in assets if p.name not in actual]
        if missing:
            subprocess.run(["gh", "release", "upload", tag, "--repo", REPOSITORY, *missing], check=True)
    else:
        require_unused_tag(tag)
        create_draft(tag, commit, record["channel"], directory, assets)
        release_id = release_id_for_tag(tag)
    if release_id is None:
        raise ValueError("Uploaded draft not found; rerun the failed publish job after inspection")
    uploaded = github(f"releases/{release_id}")
    actual = {p["name"]: p.get("digest") for p in uploaded["assets"]}
    if (uploaded.get("id") != release_id or uploaded.get("tag_name") != tag
            or uploaded["draft"] is not True or uploaded["target_commitish"] != commit
            or uploaded["prerelease"] is not (record["channel"] == "preview")
            or len(uploaded["assets"]) != len(expected) or actual != expected):
        raise ValueError("Uploaded draft identity/assets differ; draft retained for inspection")
    tag_ref = github(f"git/ref/tags/{tag}", missing=True)
    if tag_ref is not None and tag_ref.get("object", {}).get("sha") != commit:
        raise ValueError("Release tag points at another commit; draft retained for inspection")
    subprocess.run(["gh", "api", f"repos/{REPOSITORY}/releases/{release_id}", "--method", "PATCH",
                    "-F", "draft=false", "-f", "make_latest=" + ("true" if record["channel"] == "stable" else "false")],
                   check=True)


def create_draft(tag, commit, channel, directory, assets):
    command = ["gh", "release", "create", tag, "--repo", REPOSITORY, "--draft",
               "--target", commit, "--title", f"AIRK {tag}", "--notes-file", str(directory / "release-notes.md")]
    if channel == "preview":
        command.append("--prerelease")
    subprocess.run(command + [str(p) for p in assets], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("preflight", "stage-signing", "assemble", "publish"))
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target", choices=native.TARGETS)
    parser.add_argument("--producer", type=Path)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--channel", choices=("preview", "stable"), default="stable")
    args = parser.parse_args()
    commit, version, tag = context()
    if args.command == "preflight":
        require_unused_tag(tag)
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(f"version={version}\ntag={tag}\n")
        print(f"Release {tag}, package version {version}, source {commit}")
    elif args.command == "stage-signing":
        stage_signing(args.directory, args.target, commit, version)
    elif args.command == "assemble":
        assemble(args.directory, args.output, commit, version, tag, args.channel,
                 os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"],
                 json.loads(args.producer.read_text(encoding="utf-8")) if args.producer else None)
        print(f"Verified release assets: {args.output}")
    else:
        publish(args.directory, resume=args.resume)


if __name__ == "__main__":
    main()
