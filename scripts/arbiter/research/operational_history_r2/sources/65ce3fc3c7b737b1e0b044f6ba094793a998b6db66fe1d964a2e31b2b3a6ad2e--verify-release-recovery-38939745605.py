# SPDX-License-Identifier: Apache-2.0
"""Read-only verification of the retained build.4 draft against its Actions bundle."""
import importlib.util
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("release", root / "scripts/github-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
directory = root / "artifacts/release-recovery-38939745605-1"
commit = "b10e9388d6ed66fc2f7239278bace9fd26d5ffca"
tag = "v0.2.1-build.4"
record = release.verify_bundle(directory, commit, "0.2.1", tag, "38939745605", "1")
assert record["channel"] == "preview"
run = release.github("actions/runs/38939745605")
assert run["head_sha"] == commit and run["run_attempt"] == 1
assert run["event"] == "workflow_dispatch" and run["head_branch"] == "main"
jobs = release.github("actions/runs/38939745605/attempts/1/jobs?per_page=100")["jobs"]
required = {"preflight", "build / container", "build / Required checks", "assemble"}
required.update("build / Native / " + target for target in release.native.TARGETS)
required.update({"sign (windows-2025, x86_64-pc-windows-msvc, native-signing)",
                 "sign (macos-15, aarch64-apple-darwin, macos-signing)",
                 "sign (macos-15-intel, x86_64-apple-darwin, macos-signing)"})
successful = {job["name"] for job in jobs if job["conclusion"] == "success"}
assert required.issubset(successful), required - successful
assert release.release_id_for_tag(tag) == 415293100
draft = release.github("releases/415293100")
assert draft["id"] == 415293100 and draft["tag_name"] == tag
assert draft["draft"] is True and draft["prerelease"] is True
assert draft["target_commitish"] == commit
expected = {path.name: "sha256:" + release.digest(path) for path in directory.iterdir()}
assert len(draft["assets"]) == len(expected) == 19
assert {asset["name"]: asset.get("digest") for asset in draft["assets"]} == expected
assert all(asset["state"] == "uploaded" and asset["size"] == (directory / asset["name"]).stat().st_size
           for asset in draft["assets"])
tag_ref = release.github("git/ref/tags/" + tag, missing=True)
assert tag_ref is None or tag_ref.get("object", {}).get("sha") == commit
print(json.dumps({"verified_draft_id": draft["id"], "tag": tag, "commit": commit,
                  "assets_verified": len(expected), "channel": record["channel"],
                  "successful_prerequisites": sorted(required)}, indent=2))
