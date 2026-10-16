# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("github_release", Path(__file__).parents[1] / "github-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
COMMIT = "a" * 40
VERSION = "0.1.0"
TAG = "v0.1.0-build.42"
SUFFIX = COMMIT + "-123-1"


def write_receipt(directory, receipt):
    path = directory / "manifest.json"
    path.write_text(json.dumps(receipt), encoding="utf-8")
    (directory / "SHA256SUMS").write_text(
        f'{release.digest(directory / receipt["archive"])}  {receipt["archive"]}\n'
        f'{release.digest(path)}  manifest.json\n', encoding="utf-8")


def native_fixture(directory, target, signed=False):
    directory.mkdir(parents=True)
    executable = "airk.exe" if "windows" in target else "airk"
    files = {executable: b"signed fixture" if signed else b"original fixture",
             "LICENSE": b"fixture license", "NOTICE": b"fixture notice", "THIRD_PARTY_NOTICES.md": b"fixture"}
    payload = {name: release.hashlib.sha256(data).hexdigest() for name, data in files.items()}
    archive = directory / f"airk-{VERSION}-{target}.zip"
    with zipfile.ZipFile(archive, "w") as zipped:
        for name, data in files.items():
            info = zipfile.ZipInfo(name)
            info.external_attr = (0o100755 if name == executable else 0o100644) << 16
            zipped.writestr(info, data)
    signing = None
    if signed:
        signing = {"schema": "airk-native-signing/1", "target": target, "commit": COMMIT,
                   "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
                   "source_sha256": release.hashlib.sha256(b"original fixture").hexdigest(),
                   "binary_sha256": payload[executable], "identity": "c" * 40}
        if "windows" in target:
            signing.update(platform="Windows", azure=release.signer.AZURE_SIGNING,
                           verification={"trust": "authenticode_valid", "timestamp_present": True, "signer_thumbprint": "c" * 40})
        else:
            signing.update(platform="Darwin", team_id="5PX7D99PXA",
                           verification={"trust": "apple_developer_id", "team_id": "5PX7D99PXA",
                                         "timestamp_present": True, "hardened_runtime": True},
                           notarization={"status": "Accepted", "id": "fixture-notary-id"})
    receipt = {"schema": "airk-native-candidate/1", "target": target, "vsix_target": release.native.TARGETS[target],
               "version": VERSION, "commit": COMMIT, "dirty_checkout": False,
               "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
               "toolchain_file_sha256": release.digest(release.ROOT / "rust-toolchain.toml"),
               "signed_distribution": signed, "signing": signing, "archive": archive.name,
               "archive_sha256": release.digest(archive), "payload": payload}
    write_receipt(directory, receipt)
    return files, receipt


def fixtures(directory, channel="preview"):
    for target, platform in release.native.TARGETS.items():
        signed = target in release.SIGNED_TARGETS
        prefix = "airk-signed" if signed else "airk"
        native_dir = directory / f"{prefix}-native-{target}-{SUFFIX}"
        files, receipt = native_fixture(native_dir, target, signed)
        if signed:
            native_fixture(directory / f"airk-native-{target}-{SUFFIX}", target)
        vsix_dir = directory / f"{prefix}-vsix-{target}-{SUFFIX}"
        vsix_dir.mkdir()
        executable = "airk.exe" if "windows" in target else "airk"
        payload = {"package.json": b"{}", "README.md": b"fixture", "LICENSE": b"fixture",
                   "native/manifest.json": (native_dir / "manifest.json").read_bytes(),
                   **{"native/" + name: data for name, data in files.items()}}
        archive = vsix_dir / f"airk-{VERSION}-{platform}.vsix"
        with zipfile.ZipFile(archive, "w") as zipped:
            for name, data in payload.items():
                normalized = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
                info = zipfile.ZipInfo("extension/" + normalized)
                info.external_attr = (0o100755 if name == "native/" + executable else 0o100644) << 16
                zipped.writestr(info, data)
            zipped.writestr("[Content_Types].xml", "<Types/>")
            flag = '<Property Id="Microsoft.VisualStudio.Code.PreRelease" Value="true"/>' if channel == "preview" else ""
            zipped.writestr("extension.vsixmanifest", f'<PackageManifest><Identity Id="airk" Publisher="iokaio" Version="{VERSION}" TargetPlatform="{platform}"/><DisplayName>AIRK</DisplayName>{flag}</PackageManifest>')
        vsix = {"schema": "airk-extension-candidate/1", "commit": COMMIT, "version": VERSION, "platform": platform,
                "extension_id": "iokaio.airk", "display_name": "AIRK", "pre_release": channel == "preview",
                "archive": archive.name, "archive_sha256": release.digest(archive),
                "native_manifest_sha256": release.digest(native_dir / "manifest.json"),
                "native_archive_sha256": receipt["archive_sha256"],
                "npm_lock_sha256": release.digest(release.ROOT / "packages/vscode/package-lock.json"),
                "payload": {name: release.hashlib.sha256(data).hexdigest() for name, data in payload.items()}}
        write_receipt(vsix_dir, vsix)


class GithubReleaseContracts(unittest.TestCase):
    def assemble(self, directory, output, channel="preview"):
        return release.assemble(directory, output, COMMIT, VERSION, TAG, channel, "123", "1")

    def test_promotion_keeps_producer_and_signing_run_distinct(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures(root / "in")
            for path in list((root / "in").iterdir()):
                if path.name.startswith("airk-signed-") or "x86_64-unknown-linux-gnu" in path.name:
                    shutil.copytree(path, path.with_name(path.name.replace(SUFFIX, COMMIT + "-456")))
            producer = dict(schema="airk-ci-producer/1", commit=COMMIT, run_id="123", attempt="1")
            with patch.object(release.producers, "verify_record") as verify:
                record = release.assemble(root / "in", root / "out", COMMIT, VERSION, TAG, "preview", "456", "2", producer)
                verify.assert_called_once_with(producer, COMMIT)
            self.assertEqual(record["producer"], producer)
            self.assertEqual(record["run_id"], "456")
            self.assertEqual(record["attempt"], "2")
            release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "456", "2")

    def test_resume_uses_original_assembly_and_never_overwrites_conflicts(self):
        for state in ("draft", "missing", "published", "conflicting", "extra", "future"):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in")
                self.assemble(root / "in", root / "out")
                assets = [{"name": p.name, "digest": "sha256:" + release.digest(p)} for p in (root / "out").iterdir()]
                complete = dict(id=7, tag_name=TAG, draft=True, target_commitish=COMMIT, prerelease=True, assets=assets)
                remote = {**complete, "assets": list(assets)}
                if state == "missing":
                    remote["assets"].pop()
                elif state == "published":
                    remote["draft"] = False
                elif state == "conflicting":
                    remote["assets"][0] = {**assets[0], "digest": "sha256:" + "0" * 64}
                elif state == "extra":
                    remote["assets"].append(dict(name="unrecognized", digest=None))
                responses = iter([remote, complete])
                def api(path, **kwargs):
                    if path == "releases/7":
                        return next(responses)
                    if path.startswith("git/ref/"):
                        return {"object": {"sha": COMMIT}} if state == "published" else None
                    self.fail(path)
                with patch.object(release, "context", return_value=(COMMIT, VERSION, TAG)), \
                        patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "0" if state == "future" else "2"}), \
                        patch.object(release, "release_id_for_tag", return_value=7), \
                        patch.object(release, "github", side_effect=api), \
                        patch.object(release.subprocess, "run") as run:
                    if state in ("conflicting", "extra", "future"):
                        with self.assertRaises(ValueError):
                            release.publish(root / "out", resume=True)
                        run.assert_not_called()
                    else:
                        release.publish(root / "out", resume=True)
                        self.assertEqual(run.call_count, {"draft": 1, "missing": 2, "published": 0}[state])
                        for call in run.call_args_list:
                            self.assertNotIn("--clobber", call.args[0])
                            self.assertNotIn("create", call.args[0])

    def test_complete_platforms_form_verifiable_public_bundle(self):
        for channel in ("preview", "stable"):
            with self.subTest(channel=channel), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in", channel)
                record = self.assemble(root / "in", root / "out", channel)
                self.assertEqual(len(record["assets"]), 16)
                self.assertEqual(len(list((root / "out").iterdir())), 19)
                self.assertEqual(release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "123", "1"), record)
                with self.assertRaisesRegex(ValueError, "another run"):
                    release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "123", "2")
                (root / "out" / next(iter(record["assets"]))).write_bytes(b"tampered")
                with self.assertRaisesRegex(ValueError, "digest mismatch"):
                    release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "123", "1")

    def test_native_identity_signing_notarization_and_path_fail_closed(self):
        cases = (
            ("dirty_checkout", True), ("commit", "b" * 40), ("signed_distribution", False),
            ("signing.notarization.status", "Rejected"), ("signing.team_id", "WRONGTEAM1"),
            ("signing.binary_sha256", "0" * 64), ("signing.verification.timestamp_present", False),
            ("archive", "../outside.zip"),
        )
        for field, value in cases:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = "aarch64-apple-darwin"
                _, receipt = native_fixture(root / "in", target, True)
                selected = receipt
                keys = field.split(".")
                for key in keys[:-1]:
                    selected = selected[key]
                selected[keys[-1]] = value
                if field == "archive":
                    (root / "in/manifest.json").write_text(json.dumps(receipt), encoding="utf-8")
                else:
                    write_receipt(root / "in", receipt)
                with self.assertRaises(ValueError):
                    release.verify_native(root / "in", target, COMMIT, VERSION, True)

    def test_missing_platform_and_wrong_vsix_pair_never_stage_output(self):
        for mutation in ("missing", "pair", "source", "unexpected"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in")
                target = "x86_64-pc-windows-msvc"
                directory = root / "in" / f"airk-signed-vsix-{target}-{SUFFIX}"
                if mutation == "missing":
                    (directory / "manifest.json").unlink()
                elif mutation == "unexpected":
                    (directory / "private.env").write_text("fictional excluded data")
                elif mutation == "pair":
                    receipt = json.loads((directory / "manifest.json").read_text())
                    receipt["native_manifest_sha256"] = "0" * 64
                    write_receipt(directory, receipt)
                else:
                    directory = root / "in" / f"airk-native-{target}-{SUFFIX}"
                    receipt = json.loads((directory / "manifest.json").read_text())
                    archive = directory / receipt["archive"]
                    with zipfile.ZipFile(archive) as zipped:
                        entries = [(entry, zipped.read(entry)) for entry in zipped.infolist()]
                    changed = b"different original fixture"
                    with zipfile.ZipFile(archive, "w") as zipped:
                        for entry, data in entries:
                            zipped.writestr(entry, changed if entry.filename == "airk.exe" else data)
                    receipt["payload"]["airk.exe"] = release.hashlib.sha256(changed).hexdigest()
                    receipt["archive_sha256"] = release.digest(archive)
                    write_receipt(directory, receipt)
                with self.assertRaises((ValueError, FileNotFoundError)):
                    self.assemble(root / "in", root / "out")
                self.assertFalse((root / "out").exists())

    def test_build_identity_and_existing_tag(self):
        self.assertEqual(release.identity(VERSION, 42), TAG)
        self.assertNotEqual(release.identity(VERSION, 43), TAG)
        for version, build in ((VERSION, 0), (VERSION, "42\nbad"), ("../version", 42), ("0.1.0-beta", 42)):
            with self.assertRaises(ValueError):
                release.identity(version, build)
        with patch.object(release, "github", return_value={"id": 1}), patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "already exists"):
                release.require_unused_tag(TAG)
            run.assert_not_called()

    def test_only_manual_airk_main_context_is_admitted(self):
        environment = {"GITHUB_REPOSITORY": release.REPOSITORY, "GITHUB_REF": "refs/heads/main",
                       "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_WORKFLOW_REF": release.WORKFLOW,
                       "GITHUB_SHA": COMMIT, "GITHUB_RUN_NUMBER": "42", "GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1"}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n', encoding="utf-8")
            manifest = root / "packages/vscode/package.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('{"version":"0.1.0"}', encoding="utf-8")
            with patch.dict(os.environ, environment, clear=True), patch.object(release, "ROOT", root):
                self.assertEqual(release.context(), (COMMIT, VERSION, TAG))
                for key, value in (("GITHUB_REF", "refs/tags/v0.1.0"), ("GITHUB_EVENT_NAME", "pull_request"),
                                   ("GITHUB_WORKFLOW_REF", release.WORKFLOW.replace("github-release", "ci"))):
                    with patch.dict(os.environ, {key: value}), self.assertRaises(ValueError):
                        release.context()
                manifest.write_text('{"version":"0.1.1"}', encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "versions differ"):
                    release.context()

    def test_publish_checks_uploaded_assets_before_making_draft_public(self):
        cases = ((channel, corruption) for channel in ("preview", "stable")
                 for corruption in (None, "missing_asset", "digest", "tag", "id", "commit", "channel", "published", "tag_ref"))
        for channel, corruption in cases:
            with self.subTest(channel=channel, corruption=corruption), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in", channel)
                self.assemble(root / "in", root / "out", channel)
                assets = [{"name": p.name, "digest": "sha256:" + release.digest(p)} for p in (root / "out").iterdir()]
                if corruption == "missing_asset":
                    assets.pop()
                elif corruption == "digest":
                    assets[0]["digest"] = "sha256:" + "0" * 64
                remote = {"id": 7, "tag_name": TAG, "draft": True, "target_commitish": COMMIT,
                          "prerelease": channel == "preview", "assets": assets}
                changes = {"tag": ("tag_name", "other-tag"), "id": ("id", 8),
                           "commit": ("target_commitish", "b" * 40), "channel": ("prerelease", channel != "preview"),
                           "published": ("draft", False)}
                if corruption in changes:
                    key, value = changes[corruption]
                    remote[key] = value

                def api(path, **kwargs):
                    if path == "releases?per_page=100":
                        self.assertTrue(kwargs.get("paginate"))
                        return [[{"id": 6, "tag_name": "unrelated"}], [{"id": 7, "tag_name": TAG}]]
                    if path == "releases/7":
                        return remote
                    if path == f"git/ref/tags/{TAG}":
                        return {"object": {"sha": "b" * 40}} if corruption == "tag_ref" else None
                    self.fail("Unexpected API endpoint (drafts cannot be fetched by tag): " + path)

                with patch.object(release, "context", return_value=(COMMIT, VERSION, TAG)), \
                        patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1"}), \
                        patch.object(release, "require_unused_tag"), \
                        patch.object(release, "github", side_effect=api), \
                        patch.object(release.subprocess, "run") as run:
                    if corruption:
                        with self.assertRaisesRegex(ValueError, "draft retained"):
                            release.publish(root / "out")
                        self.assertEqual(run.call_count, 1)
                    else:
                        release.publish(root / "out")
                        self.assertEqual(run.call_count, 2)
                        self.assertEqual(run.call_args.args[0], [
                            "gh", "api", f"repos/{release.REPOSITORY}/releases/7", "--method", "PATCH",
                            "-F", "draft=false", "-f", "make_latest=" + ("true" if channel == "stable" else "false")])
                    creation = run.call_args_list[0].args[0]
                    self.assertIn("--draft", creation)
                    self.assertEqual(creation[creation.index("--target") + 1], COMMIT)

    def test_existing_draft_without_git_tag_blocks_creation(self):
        def api(path, **kwargs):
            if path == "releases?per_page=100":
                return [[{"id": 1, "tag_name": "unrelated"}], [{"id": 7, "tag_name": TAG, "draft": True}]]
            return None

        with patch.object(release, "github", side_effect=api), patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "already exists"):
                release.require_unused_tag(TAG)
            run.assert_not_called()

    def test_release_lookup_paginates_and_rejects_ambiguous_or_invalid_ids(self):
        response = subprocess.CompletedProcess([], 0, json.dumps([
            [{"id": 1, "tag_name": "unrelated"}], [{"id": 7, "tag_name": TAG, "draft": True}]]), "")
        with patch.object(release.subprocess, "run", return_value=response) as run:
            self.assertEqual(release.release_id_for_tag(TAG), 7)
            self.assertEqual(run.call_args.args[0], ["gh", "api", f"repos/{release.REPOSITORY}/releases?per_page=100",
                                                   "--paginate", "--slurp"])
        for entries in ([{"tag_name": TAG, "id": 7}] * 2,
                        [{"tag_name": TAG, "id": "7"}], [{"tag_name": TAG, "id": True}],
                        [{"tag_name": TAG, "id": 0}], [{"tag_name": TAG}]):
            with self.subTest(entries=entries), patch.object(release, "github", return_value=[entries]):
                with self.assertRaisesRegex(ValueError, "release identity"):
                    release.release_id_for_tag(TAG)
        with patch.object(release, "github", return_value=[[]]):
            self.assertIsNone(release.release_id_for_tag(TAG))

    def test_failed_draft_discovery_never_publishes(self):
        for failure in (None, RuntimeError("network unavailable")):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in")
                self.assemble(root / "in", root / "out")
                with patch.object(release, "context", return_value=(COMMIT, VERSION, TAG)), \
                        patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1"}), \
                        patch.object(release, "require_unused_tag"), \
                        patch.object(release, "release_id_for_tag", return_value=None,
                                     side_effect=failure), \
                        patch.object(release.subprocess, "run") as run:
                    with self.assertRaises((ValueError, RuntimeError)):
                        release.publish(root / "out")
                    self.assertEqual(run.call_count, 1)
                    self.assertIn("create", run.call_args.args[0])

    def test_signing_stage_restores_only_tested_executable_and_refuses_reuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = "aarch64-apple-darwin"
            native_fixture(root / "in", target)
            for name in ("Cargo.lock", "rust-toolchain.toml"):
                (root / name).write_bytes((release.ROOT / name).read_bytes())
            with patch.object(release, "ROOT", root):
                release.stage_signing(root / "in", target, COMMIT, VERSION)
                output = root / "target" / target / "release"
                self.assertEqual([p.name for p in output.iterdir()], ["airk"])
                self.assertEqual((output / "airk").read_bytes(), b"original fixture")
                with self.assertRaises(FileExistsError):
                    release.stage_signing(root / "in", target, COMMIT, VERSION)

    def test_network_failure_is_not_mistaken_for_absent_release(self):
        failed = subprocess.CompletedProcess([], 1, "", "network unavailable")
        with patch.object(release.subprocess, "run", return_value=failed), self.assertRaises(RuntimeError):
            release.github("releases/tags/" + TAG, missing=True)


if __name__ == "__main__":
    unittest.main()
