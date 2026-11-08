# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


promotion = load("promote_vsix", ROOT / "scripts/promote-vsix.py")
fixtures = load("promotion_fixtures", Path(__file__).with_name("test_github_release.py"))
candidates = load("producer_fixtures", Path(__file__).with_name("test_ci_candidates.py"))
release = promotion.release
COMMIT = fixtures.COMMIT
VERSION = fixtures.PAIR_VERSION


def producer():
    return dict(schema="airk-ci-producer/1", repository="iokaio/airk", commit=COMMIT,
                workflow_id=42, run_id="123", attempt="1", event="push",
                artifacts=candidates.candidates.select_artifacts(candidates.artifacts_fixture(), COMMIT, 123, 1))


class PromotionContracts(unittest.TestCase):
    def seal_loader_fixture(self, directory, version=VERSION):
        """Add the actual loader graph contract to fictional signed-layout bytes."""
        receipt = json.loads((directory / "manifest.json").read_text())
        archive = directory / receipt["archive"]
        with zipfile.ZipFile(archive) as zipped:
            entries = {info.filename: (info, zipped.read(info)) for info in zipped.infolist()}
        package = release.extension.package
        for name in package.update_inventory_paths():
            key = "extension/" + name
            if name == "package.json":
                data = json.dumps({"name": "airk", "publisher": "iokaio", "version": version}).encode()
            elif key in entries:
                continue
            else:
                data = b"fictional captured interface bytes"
            entries[key] = (zipfile.ZipInfo(key), data)
        files = {name: package.digest(entries["extension/" + name][1]) for name in package.update_inventory_paths()}
        snapshot = entries["extension/package.json"][1].decode()
        key = "extension/airk-update.json"
        entries[key] = (zipfile.ZipInfo(key), package.encode_update_inventory(files, version, snapshot))
        with zipfile.ZipFile(archive, "w") as zipped:
            for info, data in entries.values():
                zipped.writestr(info, data)
        receipt["archive_sha256"] = release.digest(archive)
        receipt["payload"] = {name.removeprefix("extension/"): package.digest(data)
                              for name, (_, data) in entries.items() if name.startswith("extension/")}
        fixtures.write_receipt(directory, receipt)

    def signed_bundle(self, root):
        record = producer()
        for target in release.native.TARGETS:
            original_dir = root / "originals" / target
            fixtures.pair_fixture(original_dir, target)
            release.write_source_record(root / "in" / f"airk-source-{target}-{COMMIT}-456",
                                        original_dir, record, target, COMMIT)
            signed = target in release.SIGNED_TARGETS
            prefix = "airk-signed" if signed else "airk"
            native_dir = root / "in" / f"{prefix}-native-{target}-{COMMIT}-456"
            files, receipt, executables = fixtures.pair_fixture(native_dir, target, signed)
            fixtures.pair_vsix(root / "in" / f"{prefix}-vsix-{target}-{COMMIT}-456",
                               native_dir, files, receipt, executables)
        release.assemble(root / "in", root / "bundle", COMMIT, VERSION, "v" + VERSION,
                         "stable", "456", "1", record, pinned=True, source_records=True)
        return record

    def test_marketplace_copies_exact_signed_release_vsixs_and_rechecks_native_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            producer_record = self.signed_bundle(root)
            with patch.object(release.producers, "verify_record") as verify:
                release.stage_marketplace_bundle(root / "bundle", root / "out", COMMIT, VERSION,
                                                 "stable", "456", "1")
                verify.assert_called_once_with(producer_record, COMMIT)
            for target, platform in release.native.TARGETS.items():
                name = f"airk-{VERSION}-{platform}.vsix"
                self.assertEqual((root / "out" / target / name).read_bytes(),
                                 (root / "bundle" / name).read_bytes())
                native = json.loads((root / "bundle" / f"airk-{VERSION}-{target}.native-manifest.json").read_text())
                self.assertEqual(native["signed_distribution"], target in release.SIGNED_TARGETS)
            with patch.object(release.producers, "verify_record"):
                with self.assertRaisesRegex(ValueError, "different channel"):
                    release.stage_marketplace_bundle(root / "bundle", root / "wrong", COMMIT, VERSION,
                                                     "preview", "456", "1")
            self.assertFalse((root / "wrong").exists())

    def test_marketplace_refuses_unsigned_native_even_with_resealed_bundle_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.signed_bundle(root)
            target = "aarch64-apple-darwin"
            manifest = root / "bundle" / f"airk-{VERSION}-{target}.native-manifest.json"
            receipt = json.loads(manifest.read_text())
            receipt["signed_distribution"] = False
            manifest.write_text(json.dumps(receipt))
            record_path = root / "bundle/release.json"
            record = json.loads(record_path.read_text())
            record["assets"][manifest.name] = release.digest(manifest)
            record_path.write_text(json.dumps(record))
            paths = sorted(p for p in (root / "bundle").iterdir() if p.name != "SHA256SUMS")
            (root / "bundle/SHA256SUMS").write_text("".join(f"{release.digest(p)}  {p.name}\n" for p in paths))
            with patch.object(release.producers, "verify_record"):
                with self.assertRaisesRegex(ValueError, "provenance or signing"):
                    release.stage_marketplace_bundle(root / "bundle", root / "out", COMMIT, VERSION,
                                                     "stable", "456", "1")
            self.assertFalse((root / "out").exists())

    def test_marketplace_publication_requires_exact_already_published_signed_set(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.signed_bundle(root)
            published = dict(draft=False, tag_name="v" + VERSION, target_commitish=COMMIT, prerelease=False,
                             assets=[dict(name=p.name, digest="sha256:" + release.digest(p))
                                     for p in (root / "bundle").iterdir()])
            tag = dict(object=dict(sha=COMMIT))
            for mutation in (None, "missing", "draft", "tag", "channel", "different_signed_run", "duplicate"):
                altered = json.loads(json.dumps(published))
                altered_tag = tag
                if mutation == "missing": altered = None
                elif mutation == "draft": altered["draft"] = True
                elif mutation == "tag": altered_tag = dict(object=dict(sha="b" * 40))
                elif mutation == "channel": altered["prerelease"] = True
                elif mutation == "different_signed_run": altered["assets"][0]["digest"] = "sha256:" + "b" * 64
                elif mutation == "duplicate": altered["assets"].append(altered["assets"][0])
                output = root / (mutation or "valid")
                with self.subTest(mutation=mutation), patch.object(release.producers, "verify_record"), \
                        patch.object(release, "github", side_effect=[altered, altered_tag]):
                    if mutation:
                        with self.assertRaises(ValueError):
                            release.stage_marketplace_bundle(root / "bundle", output, COMMIT, VERSION,
                                                             "stable", "456", "1", published_only=True)
                        self.assertFalse(output.exists())
                    else:
                        release.stage_marketplace_bundle(root / "bundle", output, COMMIT, VERSION,
                                                         "stable", "456", "1", published_only=True)
                        self.assertEqual(len(list(output.iterdir())), 4)

    def test_marketplace_detects_byte_drift_during_final_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.signed_bundle(root)
            copytree = release.shutil.copytree
            def changed(source, destination, *args, **kwargs):
                result = copytree(source, destination, *args, **kwargs)
                if Path(source).name == "vsix":
                    path = Path(destination) / "x86_64-pc-windows-msvc" / f"airk-{VERSION}-win32-x64.vsix"
                    path.write_bytes(b"changed after verification")
                return result
            with patch.object(release.producers, "verify_record"), \
                    patch.object(release.shutil, "copytree", side_effect=changed):
                with self.assertRaisesRegex(ValueError, "changed the signed producer"):
                    release.stage_marketplace_bundle(root / "bundle", root / "out", COMMIT, VERSION,
                                                     "stable", "456", "1")

    def test_assembly_inputs_require_same_run_digests_and_all_platform_outputs(self):
        run = {**candidates.run_fixture(), "id": 456, "event": "workflow_dispatch",
               "path": ".github/workflows/github-release.yml", "status": "in_progress", "conclusion": None}
        entries = []
        for target in release.producers.TARGETS:
            prefix = "airk" if target == "x86_64-unknown-linux-gnu" else "airk-signed"
            names = [f"{prefix}-{kind}-{target}-{COMMIT}-456" for kind in ("native", "vsix")]
            names.append(f"airk-source-{target}-{COMMIT}-456")
            for name in names:
                entries.append(dict(id=len(entries) + 1, name=name, expired=False, digest="sha256:" + "a" * 64,
                                    workflow_run=dict(id=456, head_sha=COMMIT, head_branch="main")))
        for mutation in (None, "missing", "duplicate", "digest", "origin", "expired"):
            altered = json.loads(json.dumps(entries))
            if mutation == "missing":
                altered.pop()
            elif mutation == "duplicate":
                altered.append(altered[0])
            elif mutation == "origin":
                altered[0]["workflow_run"]["id"] = 123
            elif mutation:
                altered[0][mutation] = True if mutation == "expired" else "bad"
            def api(path, paginate=False):
                if path == "actions/runs/456":
                    return run
                self.assertEqual(path, "actions/runs/456/artifacts?per_page=100")
                return [dict(artifacts=altered)]
            def download(_artifact, destination):
                destination.mkdir()
                (destination / ("source.json" if destination.name.startswith("airk-source-") else "manifest.json")).write_text("{}")
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(release.producers, "api", side_effect=api), \
                    patch.object(release.producers, "download", side_effect=download) as fetched:
                destination = Path(temporary) / "inputs"
                if mutation:
                    with self.assertRaises(ValueError):
                        release.producers.release_inputs(COMMIT, "456", destination)
                    fetched.assert_not_called()
                else:
                    release.producers.release_inputs(COMMIT, "456", destination)
                    self.assertEqual(fetched.call_count, 12)

    def test_pinned_record_checks_are_local_and_refuse_incomplete_or_changed_identity(self):
        for mutation in (None, "commit", "artifact", "digest", "extra", "duplicate_id"):
            record = producer()
            names = list(record["artifacts"])
            if mutation == "commit":
                record["commit"] = "b" * 40
            elif mutation == "artifact":
                del record["artifacts"][names[0]]
            elif mutation == "digest":
                record["artifacts"][names[0]]["digest"] = "bad"
            elif mutation == "extra":
                record["artifacts"]["extra"] = record["artifacts"][names[0]]
            elif mutation == "duplicate_id":
                record["artifacts"][names[0]]["id"] = record["artifacts"][names[1]]["id"]
            with self.subTest(mutation=mutation), patch.object(release.producers, "api") as api:
                if mutation:
                    with self.assertRaises(ValueError):
                        release.producers.validate_record(record, COMMIT)
                else:
                    release.producers.validate_record(record, COMMIT)
                api.assert_not_called()

    def test_linux_reuses_archive_and_receipt_bytes_exactly(self):
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files, receipt, executables = fixtures.pair_fixture(root / "native", target)
            fixtures.pair_vsix(root / "vsix", root / "native", files, receipt, executables)
            with patch.object(release.producers, "api") as api, patch.object(release.subprocess, "run") as run:
                promotion.promote(root / "vsix", root / "native", root / "native", root / "out",
                                  target, COMMIT, VERSION, "stable")
                api.assert_not_called()
                run.assert_not_called()
            self.assertEqual({p.name: p.read_bytes() for p in (root / "out").iterdir()},
                             {p.name: p.read_bytes() for p in (root / "vsix").iterdir()})

    def test_modern_promotion_preserves_signed_single_image_and_complete_loader_graph(self):
        version = "1.8.14"
        for target in release.SIGNED_TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                files, original, executables = fixtures.pair_fixture(root / "original", target, version=version)
                fixtures.pair_vsix(root / "vsix", root / "original", files, original, executables)
                self.seal_loader_fixture(root / "vsix", version)
                _, signed, _ = fixtures.pair_fixture(root / "signed", target, True, version)
                result = promotion.promote(root / "vsix", root / "original", root / "signed", root / "out", target, COMMIT, version, "stable")
                with zipfile.ZipFile(root / "vsix" / result["archive"]) as before, zipfile.ZipFile(root / "out" / result["archive"]) as after:
                    old = release.extension.package.read_update_inventory(before, version, result["platform"])
                    new = release.extension.package.read_update_inventory(after, version, result["platform"])
                    self.assertEqual({name for name in old["files"] if old["files"][name] != new["files"][name]}, {"native/manifest.json"})
                    wrapper = json.loads(after.read("extension/native/manifest.json"))
                    self.assertEqual(wrapper["payload"], {executables[0]: signed["payload"][executables[0]]})
                    for name, expected in signed["payload"].items():
                        self.assertEqual(release.hashlib.sha256(after.read("extension/native/runtime/" + name)).hexdigest(), expected)
                    self.assertFalse(any("airk-embed" in name or "airk-launcher" in name for name in after.namelist()))

    def test_signed_promotion_preserves_every_nonnative_extension_byte(self):
        for target in release.SIGNED_TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                files, original, executables = fixtures.pair_fixture(root / "original", target)
                fixtures.pair_vsix(root / "vsix", root / "original", files, original, executables)
                _, signed, _ = fixtures.pair_fixture(root / "signed", target, True)
                result = promotion.promote(root / "vsix", root / "original", root / "signed", root / "out",
                                           target, COMMIT, VERSION, "stable")
                with zipfile.ZipFile(root / "vsix" / result["archive"]) as before, \
                        zipfile.ZipFile(root / "out" / result["archive"]) as after:
                    for name in before.namelist():
                        if not name.startswith("extension/native/"):
                            self.assertEqual(before.read(name), after.read(name), name)
                    embedded = json.loads(after.read("extension/native/candidate.json"))
                    self.assertEqual(embedded, signed)
                native = release.native.runtime_builder().runtime_names("windows" if "windows" in target else "macos")[0]
                self.assertNotEqual(original["payload"][native], signed["payload"][native])

    def test_signed_promotion_refreshes_only_the_native_wrapper_in_the_loader_inventory(self):
        for target in sorted(release.SIGNED_TARGETS):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                files, original, executables = fixtures.pair_fixture(root / "original", target)
                fixtures.pair_vsix(root / "vsix", root / "original", files, original, executables)
                self.seal_loader_fixture(root / "vsix")
                fixtures.pair_fixture(root / "signed", target, True)
                result = promotion.promote(root / "vsix", root / "original", root / "signed", root / "out",
                                           target, COMMIT, VERSION, "stable")
                with zipfile.ZipFile(root / "vsix" / result["archive"]) as before, \
                        zipfile.ZipFile(root / "out" / result["archive"]) as after:
                    old = release.extension.package.read_update_inventory(before, VERSION, result["platform"])
                    new = release.extension.package.read_update_inventory(after, VERSION, result["platform"])
                    self.assertEqual(new["package_manifest"], old["package_manifest"])
                    changed = {name for name in old["files"] if old["files"][name] != new["files"][name]}
                    self.assertEqual(changed, {"native/manifest.json"})
                    self.assertEqual(new["files"]["native/manifest.json"],
                                     release.extension.package.digest(after.read("extension/native/manifest.json")))
                    for name in before.namelist():
                        if not name.startswith("extension/native/") and name != "extension/airk-update.json":
                            self.assertEqual(before.read(name), after.read(name), name)
                    self.assertEqual(json.loads(after.read("extension/native/candidate.json"))["signed_distribution"], True)


    def test_signed_promotion_refuses_a_different_tested_source_before_writing(self):
        target = "x86_64-pc-windows-msvc"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files, original, executables = fixtures.pair_fixture(root / "original", target)
            fixtures.pair_vsix(root / "vsix", root / "original", files, original, executables)
            _, signed, _ = fixtures.pair_fixture(root / "signed", target, True)
            signed["signing"]["components"][executables[0]]["source_sha256"] = "0" * 64
            fixtures.write_receipt(root / "signed", signed)
            with self.assertRaisesRegex(ValueError, "did not originate"):
                promotion.promote(root / "vsix", root / "original", root / "signed", root / "out",
                                  target, COMMIT, VERSION, "stable")
            self.assertFalse((root / "out").exists())

    def test_extension_drift_between_admission_and_streaming_is_rejected(self):
        target = "x86_64-pc-windows-msvc"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files, original, executables = fixtures.pair_fixture(root / "original", target)
            fixtures.pair_vsix(root / "vsix", root / "original", files, original, executables)
            fixtures.pair_fixture(root / "signed", target, True)
            receipt, archive = promotion.verified_vsix(root / "vsix", root / "original", target, COMMIT, VERSION)
            with zipfile.ZipFile(archive) as zipped:
                entries = [(info, zipped.read(info)) for info in zipped.infolist()]
            with zipfile.ZipFile(archive, "w") as zipped:
                for info, data in entries:
                    zipped.writestr(info, b"changed after admission" if info.filename == "extension/package.json" else data)
            with patch.object(promotion, "verified_vsix", return_value=(receipt, archive)), \
                    self.assertRaisesRegex(ValueError, "changed during promotion"):
                promotion.promote(root / "vsix", root / "original", root / "signed", root / "out",
                                  target, COMMIT, VERSION, "stable")

    def test_channel_conversion_preserves_extension_payload_and_records_new_channel(self):
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files, receipt, executables = fixtures.pair_fixture(root / "native", target)
            fixtures.pair_vsix(root / "vsix", root / "native", files, receipt, executables)
            source = root / "vsix" / f"airk-{VERSION}-linux-x64.vsix"
            with zipfile.ZipFile(source) as zipped:
                contents = [(info, zipped.read(info)) for info in zipped.infolist()]
            with zipfile.ZipFile(source, "w") as zipped:
                for info, data in contents:
                    if info.filename == "extension.vsixmanifest":
                        data = data.replace(b"</PackageManifest>", b"<Properties/></PackageManifest>")
                    zipped.writestr(info, data)
            original = json.loads((root / "vsix/manifest.json").read_text())
            original["archive_sha256"] = release.digest(source)
            fixtures.write_receipt(root / "vsix", original)
            result = promotion.promote(root / "vsix", root / "native", root / "native", root / "out",
                                       target, COMMIT, VERSION, "preview")
            self.assertTrue(result["pre_release"])
            normalized = {"readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name: value
                          for name, value in original["payload"].items()}
            self.assertEqual(result["payload"], normalized)

    def test_assembly_uses_verified_source_receipts_without_live_queries_or_original_archives(self):
        record = producer()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for target in release.native.TARGETS:
                original_dir = root / "originals" / target
                fixtures.pair_fixture(original_dir, target)
                source_dir = root / "in" / f"airk-source-{target}-{COMMIT}-456"
                release.write_source_record(source_dir, original_dir, record, target, COMMIT)
                self.assertEqual(json.loads((source_dir / "source.json").read_text())["manifest_sha256"],
                                 release.digest(original_dir / "manifest.json"))
                signed = target in release.SIGNED_TARGETS
                prefix = "airk-signed" if signed else "airk"
                native_dir = root / "in" / f"{prefix}-native-{target}-{COMMIT}-456"
                files, receipt, executables = fixtures.pair_fixture(native_dir, target, signed)
                fixtures.pair_vsix(root / "in" / f"{prefix}-vsix-{target}-{COMMIT}-456",
                                   native_dir, files, receipt, executables)
            with patch.object(release.producers, "verify_record") as remote:
                result = release.assemble(root / "in", root / "out", COMMIT, VERSION, "v" + VERSION,
                    "stable", "456", "1", record, pinned=True, source_records=True)
                remote.assert_not_called()
            self.assertEqual(len(result["assets"]), 16)
            source = root / "in" / f"airk-source-x86_64-unknown-linux-gnu-{COMMIT}-456"
            original = json.loads((source / "source.json").read_text())
            for key, bad in (("producer_sha256", "0" * 64), ("manifest_json", "{}"), ("artifact", {})):
                with self.subTest(key=key):
                    (source / "source.json").write_text(json.dumps({**original, key: bad}))
                    with self.assertRaises(ValueError):
                        release.source_receipt(source, record, "x86_64-unknown-linux-gnu", COMMIT, VERSION)
            with patch.object(release, "context", return_value=(COMMIT, VERSION, "v" + VERSION)), \
                    patch.dict(os.environ, {"GITHUB_RUN_ID": "456", "GITHUB_RUN_ATTEMPT": "1"}), \
                    patch.object(release.producers, "verify_record", side_effect=ValueError("producer changed")) as remote, \
                    patch.object(release, "github") as github, patch.object(release.subprocess, "run") as mutation:
                with self.assertRaisesRegex(ValueError, "producer changed"):
                    release.publish(root / "out")
                remote.assert_called_once_with(record, COMMIT)
                github.assert_not_called()
                mutation.assert_not_called()

    def test_workflow_keeps_publication_manual_and_only_historical_repair_builds(self):
        workflow = (ROOT / ".github/workflows/github-release.yml").read_text()
        marketplace = (ROOT / ".github/workflows/extension-release.yml").read_text()
        self.assertNotIn("workflow_run:", marketplace)
        self.assertNotIn("  push:", marketplace)
        self.assertIn("default: false", marketplace)
        for step in workflow.split("\n      - "):
            if any(command in step for command in ("run: cargo fetch", "run: rustup show", "run: npm ci", "run: npm run compile")):
                self.assertIn("if: ${{ inputs.repair_1_4_4 }}", step)
        self.assertIn("--pinned-producer --source-records", workflow)
        self.assertIn("--source-manifest artifacts/tested-native/manifest.json", workflow)
        self.assertIn("signed_run_id:", marketplace)
        self.assertIn("signed-release-download --record artifacts/signed-producer.json", marketplace)
        self.assertNotIn("scripts/package-extension.py", marketplace)
        self.assertNotIn("run: npm run compile", marketplace)
        self.assertNotIn("scripts/ci-candidates.py download --target", marketplace)
        self.assertNotIn("rehearsal_run_id", marketplace)
        self.assertIn("--output artifacts/release --published-only", marketplace)
