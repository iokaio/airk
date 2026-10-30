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
