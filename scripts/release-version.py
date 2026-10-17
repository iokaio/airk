# SPDX-License-Identifier: Apache-2.0
"""Plan publication without reusing versions or changing tested binaries."""
import argparse
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import zipfile
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "iokaio/airk"
PLATFORMS = {"win32-x64", "linux-x64", "darwin-arm64", "darwin-x64"}
VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
TAG = re.compile(rf"v({VERSION})(?:-build\.[1-9][0-9]*)?")


def numeric(version):
    if not re.fullmatch(VERSION, version):
        raise ValueError("Expected a numeric package version")
    return tuple(map(int, version.split(".")))


def source_version(root=ROOT):
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    numeric(version)
    package = json.loads((root / "packages/vscode/package.json").read_text(encoding="utf-8"))
    lock = json.loads((root / "packages/vscode/package-lock.json").read_text(encoding="utf-8"))
    crates = [p for p in tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
              if p["name"].startswith("airk-") and "source" not in p]
    if (package["version"] != version or lock["version"] != version
            or lock["packages"][""]["version"] != version or not crates
            or any(p["version"] != version for p in crates)):
        raise ValueError("Native, extension and lockfile versions differ")
    return version


def api(path, payload=None, paginate=False, missing=False):
    command = ["gh", "api", f"repos/{REPOSITORY}/{path}"]
    if paginate:
        command += ["--paginate", "--slurp"]
    if payload is not None:
        command += ["--method", "POST", "--input", "-"]
    result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                            text=True, encoding="utf-8", capture_output=True, check=False)
    if result.returncode:
        if missing and "(HTTP 404)" in result.stderr:
            return None
        raise RuntimeError("GitHub API failed: " + result.stderr)
    return json.loads(result.stdout)


def marketplace():
    body = {"filters": [{"criteria": [{"filterType": 7, "value": "iokaio.airk"}],
                         "pageNumber": 1, "pageSize": 10}], "assetTypes": [], "flags": 147}
    request = Request("https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery",
                      data=json.dumps(body).encode(), headers={"Content-Type": "application/json",
                      "Accept": "application/json;api-version=7.2-preview.1"})
    with urlopen(request, timeout=60) as response:
        catalog = json.load(response)
    extensions = [e for result in catalog["results"] for e in result["extensions"]
                  if e["publisher"]["publisherName"] == "iokaio" and e["extensionName"] == "airk"]
    # AIRK already exists. An empty/malformed response is not proof of an unused version.
    if len(extensions) != 1 or not isinstance(extensions[0].get("versions"), list) or not extensions[0]["versions"]:
        raise ValueError("Cannot read the complete AIRK Marketplace version catalog")
    return extensions[0]["versions"]


def market_platforms(entries, version, channel):
    matches = [e for e in entries if e["version"] == version]
    platforms = set()
    for entry in matches:
        platform = entry.get("targetPlatform")
        preview = any(p["key"] == "Microsoft.VisualStudio.Code.PreRelease" and p["value"] == "true"
                      for p in entry["properties"])
        if platform not in PLATFORMS or platform in platforms or preview != (channel == "preview"):
            raise ValueError("Marketplace version has conflicting platforms or channel")
        platforms.add(platform)
    return platforms


def plan(version, channel, destination, entries, releases, tags, *, publish, attempt=1):
    numeric(version)
    if destination not in ("marketplace", "github") or channel not in ("stable", "preview"):
        raise ValueError("Invalid release destination/channel")
    result = {"version": version, "action": "publish", "next_version": "", "destination": destination}
    if not publish:
        return {**result, "action": "rehearse"}
    platforms = market_platforms(entries, version, channel)
    matching = [r for r in releases if TAG.fullmatch(r["tag_name"]) and TAG.fullmatch(r["tag_name"])[1] == version]
    published = [r for r in matching if r["draft"] is False]
    if len(published) > 1 or any(r["prerelease"] != (channel == "preview") for r in matching):
        raise ValueError("GitHub version has conflicting releases or channel")
    if published:
        # Completeness, asset names and server digests are required, not merely a tag.
        spec = importlib.util.spec_from_file_location("downloads", ROOT / "scripts/build-downloads.py")
        downloads = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(downloads)
        downloads.release_metadata(published[0])
    if platforms == PLATFORMS and published:
        # A rerun is a retry, never an instruction to prepare another release.
        if attempt > 1:
            return {**result, "action": "skip"}
        used = {e["version"] for e in entries}
        used.update(m[1] for tag in tags for m in [TAG.fullmatch(tag)] if m)
        used.update(m[1] for r in releases for m in [TAG.fullmatch(r["tag_name"])] if m)
        major, minor, patch = numeric(version)
        while f"{major}.{minor}.{patch}" in used:
            patch += 1
        return {**result, "action": "bump", "next_version": f"{major}.{minor}.{patch}"}
    if (destination == "marketplace" and platforms == PLATFORMS) or (destination == "github" and published):
        return {**result, "action": "skip"}
    if destination == "github" and (matching or f"v{version}" in tags):
        raise ValueError("Existing draft/tag: rerun only the failed publish job of its original run; never overwrite assets")
    return result


def changes(version, next_version, root=ROOT):
    if source_version(root) != version or numeric(next_version) <= numeric(version):
        raise ValueError("Version bump no longer matches the checkout")
    result = {}
    for name in ("Cargo.toml", "Cargo.lock", "packages/vscode/package.json", "packages/vscode/package-lock.json"):
        value = (root / name).read_text(encoding="utf-8")
        if name == "Cargo.toml":
            value = value.replace(f'version = "{version}"', f'version = "{next_version}"', 1)
        elif name == "Cargo.lock":
            value = re.sub(r'(name = "airk-[^"\n]+"\nversion = ")' + re.escape(version) + '"',
                           lambda m: m[1] + next_version + '"', value)
        else:
            # Only package/root lock entries, never dependency versions.
            data = json.loads(value)
            data["version"] = next_version
            if name.endswith("package-lock.json"):
                data["packages"][""]["version"] = next_version
            value = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
        result[name] = value
    return result


def verify_published_payload(archive, receipt):
    """The store may add signing metadata; shipped extension bytes must still match."""
    with zipfile.ZipFile(archive) as zipped:
        items = zipped.infolist()
        if len(items) > 15_000 or sum(i.file_size for i in items) > 1_073_741_824:
            raise ValueError("Published VSIX exceeds verification budget")
        if len({i.filename for i in items}) != len(items):
            raise ValueError("Published VSIX has duplicate entries")
        expected = {"extension/" + ("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): value
                    for name, value in receipt["payload"].items()}
        actual = {i.filename: hashlib.sha256(zipped.read(i)).hexdigest() for i in items
                  if not i.is_dir() and i.filename.startswith("extension/")}
        if actual != expected:
            raise ValueError("Already-published platform differs from this candidate; use the original candidate")


def download_vsix(response, archive):
    encoding = response.headers.get("Content-Encoding", "identity").lower()
    if encoding not in ("identity", "gzip"):
        raise ValueError("Unsupported Marketplace download encoding")
    # Marketplace can return a gzip-encoded VSIX even for urllib's identity request.
    stream = gzip.GzipFile(fileobj=response) if encoding == "gzip" else response
    total = 0
    try:
        while chunk := stream.read(1024 * 1024):
            total += len(chunk)
            if total > 536_870_912:
                raise ValueError("Published VSIX download exceeds 512 MiB")
            archive.write(chunk)
    finally:
        if stream is not response:
            stream.close()
    archive.seek(0)


def pending_packages(inventory, version, channel, entries):
    paths = json.loads(inventory.read_text(encoding="utf-8"))
    expected = {f"airk-{version}-{platform}.vsix": platform for platform in PLATFORMS}
    if not isinstance(paths, list) or len(paths) != 4 or {Path(p).name for p in paths} != set(expected):
        raise ValueError("Expected the verified four-platform VSIX inventory")
    existing = market_platforms(entries, version, channel)
    pending = []
    for path in paths:
        platform = expected[Path(path).name]
        if platform not in existing:
            pending.append(path)
            continue
        receipt = json.loads(Path(path).with_name("manifest.json").read_text(encoding="utf-8"))
        url = (f"https://marketplace.visualstudio.com/_apis/public/gallery/publishers/iokaio/vsextensions/airk/"
               f"{version}/vspackage?targetPlatform={platform}")
        with urlopen(url, timeout=60) as response, tempfile.TemporaryFile() as archive:
            download_vsix(response, archive)
            verify_published_payload(archive, receipt)
    return pending


def prepare_pr(decision, commit):
    if decision["action"] != "bump":
        raise ValueError("No completed release requiring a version bump")
    version, next_version = decision["version"], decision["next_version"]
    if api("git/ref/heads/main")["object"]["sha"] != commit:
        raise ValueError("Main changed since dispatch; dispatch again so a manual version bump wins")
    branch = f"release/version-{next_version}"
    pulls = api(f"pulls?state=open&base=main&head=iokaio:{branch}")
    if len(pulls) > 1:
        raise ValueError("Ambiguous version-bump PR")
    if pulls:
        return pulls[0]["html_url"]
    contents = changes(version, next_version)
    ref = api(f"git/ref/heads/{branch}", missing=True)
    if ref is None:
        parent = api(f"git/commits/{commit}")
        tree = api("git/trees", {"base_tree": parent["tree"]["sha"], "tree": [
            {"path": name, "mode": "100644", "type": "blob", "content": content} for name, content in contents.items()]})
        created = api("git/commits", {"message": f"Prepare AIRK {next_version}", "tree": tree["sha"], "parents": [commit]})
        # Use GitHub's actual authenticated automation identity; never invent a human DCO sign-off.
        api("git/refs", {"ref": f"refs/heads/{branch}", "sha": created["sha"]})
    else:
        # A previous attempt may have created the branch but failed to create the PR.
        import base64
        for name, content in contents.items():
            remote = api(f"contents/{name}?ref={branch}")
            if base64.b64decode(remote["content"]).decode() != content:
                raise ValueError("Existing version branch differs; inspect it instead of overwriting")
        comparison = api(f"compare/{commit}...{ref['object']['sha']}")
        if (comparison["ahead_by"] != 1 or comparison["behind_by"] != 0
                or {f["filename"] for f in comparison["files"]} != set(contents)):
            raise ValueError("Existing version branch contains unrelated changes")
    body = (f"Marketplace and GitHub already contain {version}. This prepares unused patch version {next_version}; "
            "no packages were published by this run.\n\n"
            "A maintainer must review the change, replace the automation commit with their own authorized "
            "signed-off commit, and approve/run the PR checks. DCO and protected-main requirements remain in force. "
            "After merge and successful main CI, dispatch publication for the new version. Update release notes "
            "as appropriate. This workflow does not auto-merge or manufacture a contributor certification.\n")
    return api("pulls", {"title": f"Prepare AIRK {next_version}", "head": branch, "base": "main",
                         "body": body, "draft": True})["html_url"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "prepare-pr", "pending"))
    parser.add_argument("--destination", required=True, choices=("marketplace", "github"))
    parser.add_argument("--channel", choices=("stable", "preview"), default="stable")
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--inventory", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    workflow = "extension-release.yml" if args.destination == "marketplace" else "github-release.yml"
    if (os.environ.get("GITHUB_REPOSITORY") != REPOSITORY or os.environ.get("GITHUB_REF") != "refs/heads/main"
            or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or os.environ.get("GITHUB_WORKFLOW_REF") != f"{REPOSITORY}/.github/workflows/{workflow}@refs/heads/main"):
        raise ValueError("Version planning requires the protected-main manual release workflow")
    version = source_version()
    if os.environ.get("AIRK_VERSION", version) != version:
        raise ValueError("Requested version differs from the checked-in version")
    if args.command == "pending":
        if not args.publish or args.destination != "marketplace" or not args.inventory or not args.output:
            raise ValueError("Pending-package verification requires Marketplace publication and inventory/output paths")
        paths = pending_packages(args.inventory, version, args.channel, marketplace())
        args.output.write_text(json.dumps(paths, indent=2) + "\n", encoding="utf-8")
        print(f"{len(paths)} platform packages remain; existing packages verified without republishing")
        return
    entries = marketplace() if args.publish else []
    releases = [r for page in api("releases?per_page=100", paginate=True) for r in page] if args.publish else []
    tags = [t["name"] for page in api("tags?per_page=100", paginate=True) for t in page] if args.publish else []
    decision = plan(version, args.channel, args.destination, entries, releases, tags,
                    publish=args.publish, attempt=1 if args.command == "prepare-pr" else int(os.environ["GITHUB_RUN_ATTEMPT"]))
    if args.command == "prepare-pr":
        if not args.publish:
            raise ValueError("Version PRs require publication dispatch")
        decision["pr"] = prepare_pr(decision, os.environ["GITHUB_SHA"])
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in decision.items():
            output.write(f"{key}={value}\n")
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
        summary.write(f"Release version decision: **{decision['action']}**, package **{version}**.\n\n")
        if decision["action"] == "skip":
            summary.write("Already published in this destination; no duplicate upload.\n")
        elif decision["action"] == "bump":
            summary.write(f"Next unused patch: **{decision['next_version']}**. Publication waits for a reviewed version PR and green main CI.\n")
        if "pr" in decision:
            summary.write(f"\nVersion PR: {decision['pr']}\n")
    print(json.dumps(decision))


if __name__ == "__main__":
    main()
