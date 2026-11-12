# SPDX-License-Identifier: Apache-2.0
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("ci_runtime", Path(__file__).parents[1] / "ci-runtime.py")
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)
candidates_module = runtime.load("ci-candidates")
TARGET = "x86_64-unknown-linux-gnu"
IDENTITY = dict(commit="a" * 40, tree="b" * 40, run_id="123", target=TARGET, qualification=False)


class RuntimeTransferTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = {"sha256": "1" * 64, "files": 4}
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion="1.4.0"\n', encoding="utf-8")
        self.builder = runtime.package.runtime_builder()
        self.builder.source_identity = lambda: self.source
        for mock in (patch.object(runtime, "ROOT", self.root),
                     patch.object(runtime, "identity", return_value=IDENTITY),
                     patch.object(runtime.package, "runtime_builder", return_value=self.builder),
                     patch.dict(os.environ, GITHUB_RUN_ATTEMPT="1")):
            mock.start()
            self.addCleanup(mock.stop)
        self.original = self.root / "original"
        self.original.mkdir()
        for name in ("airk", "airk-embed", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md",
                     "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
            destination = self.original / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(b"original fictional fixture")
        (self.original / "notices/inventory.json").write_text("[]", encoding="utf-8")
        pair = dict(schema=1, version="1.4.0", os="linux", arch="x86_64",
                    airk_sha256=runtime.package.sha256(self.original / "airk"),
                    embed_sha256=runtime.package.sha256(self.original / "airk-embed"))
        (self.original / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
        (self.original / "airk-build.json").write_text(json.dumps(dict(schema=1, source=self.source, runtime=pair,
                                                                      profile="release", rustc="host: " + TARGET)), encoding="utf-8")
        install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
        install["files"] = runtime.package.inventory(self.original)
        (self.original / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
        self.transfer = self.root / "transfer"
        runtime.seal(self.original, self.transfer, TARGET)

    def receipt(self):
        return json.loads((self.transfer / "transfer.json").read_text(encoding="utf-8"))

    def write_receipt(self, receipt):
        (self.transfer / "transfer.json").write_text(json.dumps(receipt), encoding="utf-8")

    def test_actual_archive_round_trip_keeps_full_payload_and_executable_modes(self):
        with patch.dict(os.environ, GITHUB_RUN_ATTEMPT="2"):
            runtime.restore(self.transfer, self.root / "restored", TARGET)
        self.assertEqual(runtime.package.inventory(self.original), runtime.package.inventory(self.root / "restored"))
        with zipfile.ZipFile(self.transfer / "runtime.zip") as archive:
            for name in ("airk", "airk-embed"):
                self.assertEqual((archive.getinfo(name).external_attr >> 16) & 0o777, 0o755)
        if os.name != "nt":
            self.assertEqual((self.root / "restored/airk").stat().st_mode & 0o777, 0o755)
        with self.assertRaisesRegex(ValueError, "fresh"):
            runtime.restore(self.transfer, self.root / "restored", TARGET)

    def test_current_mac_transfer_preserves_and_requires_launcher_executable_mode(self):
        target = "aarch64-apple-darwin"
        version = "1.8.8"
        (self.root / "Cargo.toml").write_text(f'[workspace.package]\nversion="{version}"\n', encoding="utf-8")
        original = self.root / "mac-original"
        original.mkdir()
        for name in ("airk", "airk-embed", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md",
                     "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json", "notices/inventory.json"):
            destination = original / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(self.original / name, destination)
        self.builder.arrange_macos_bundle(original, version)
        main, helper = self.builder.runtime_names("macos")
        launcher = self.builder.launcher_name("macos", version)
        pair = dict(schema=1, version=version, os="macos", arch="aarch64",
                    airk_sha256=runtime.package.sha256(original / main), embed_sha256=runtime.package.sha256(original / helper))
        (original / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
        (original / "airk-build.json").write_text(json.dumps(dict(schema=1, source=self.source, runtime=pair, profile="release", rustc="host: " + target)), encoding="utf-8")
        install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
        install["files"] = runtime.package.inventory(original)
        (original / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
        transfer = self.root / "mac-transfer"
        destination = self.root / "mac-restored"
        with patch.object(runtime, "identity", return_value={**IDENTITY, "target": target}), patch.object(self.builder, "inference_assets", return_value={}):
            runtime.seal(original, transfer, target)
            runtime.restore(transfer, destination, target)
            self.assertEqual(runtime.package.inventory(original), runtime.package.inventory(destination))
            if os.name != "nt":
                self.assertEqual((destination / launcher).stat().st_mode & 0o777, 0o755)
            # Simulate a producer dropping only the standalone image's execute
            # bit; update the archive digest so the mode guard itself is tested.
            archive = transfer / "runtime.zip"
            archive.unlink()
            payload = runtime.package.inventory(original)
            runtime.package.write_archive(original, archive, payload, (main, helper))
            receipt = json.loads((transfer / "transfer.json").read_text(encoding="utf-8"))
            receipt["archive_sha256"] = runtime.package.sha256(archive)
            (transfer / "transfer.json").write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "executable permission"):
                runtime.restore(transfer, self.root / "mac-refused", target)
            self.assertFalse((self.root / "mac-refused").exists())

    def test_other_source_target_run_qualification_and_future_attempt_are_refused(self):
        original = self.receipt()
        for key, value in (("commit", "c" * 40), ("tree", "c" * 40), ("run_id", "124"),
                           ("target", "aarch64-apple-darwin"), ("qualification", True), ("build_attempt", 2),
                           ("build_attempt", True), ("schema", "unknown")):
            with self.subTest(key=key, value=value), self.assertRaisesRegex(ValueError, "another source"):
                self.write_receipt({**original, key: value})
                runtime.restore(self.transfer, self.root / "refused", TARGET)
            self.assertFalse((self.root / "refused").exists())
        self.write_receipt(original)
        self.source = {"sha256": "2" * 64, "files": 4}
        with self.assertRaisesRegex(ValueError, "source/pair"):
            runtime.restore(self.transfer, self.root / "changed-source", TARGET)
        self.assertFalse((self.root / "changed-source").exists())

    def test_archive_corruption_unexpected_files_and_unsafe_names_fail_before_extraction(self):
        archive = self.transfer / "runtime.zip"
        original = archive.read_bytes()
        archive.write_bytes(original + b"changed")
        with self.assertRaisesRegex(ValueError, "digest differs"):
            runtime.restore(self.transfer, self.root / "bad", TARGET)
        archive.write_bytes(original)
        with zipfile.ZipFile(archive, "a") as zipped:
            zipped.writestr("unexpected", b"not in inventory")
        receipt = self.receipt()
        receipt["archive_sha256"] = runtime.package.sha256(archive)
        self.write_receipt(receipt)
        with self.assertRaisesRegex(ValueError, "entries differ"):
            runtime.restore(self.transfer, self.root / "bad", TARGET)
        for name in ("../escape", "/absolute", "a\\b", "a/./b", "C:/escape", "a//b"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "Unsafe"):
                runtime.payload_names({name: "a" * 64})
        self.assertFalse((self.root / "bad").exists())

    def test_resume_requires_exact_successful_job_upload_interval_and_authenticated_download(self):
        candidates = candidates_module
        artifact = dict(id=7, name=runtime.artifact_name(IDENTITY), expired=False, digest="sha256:" + "d" * 64,
                        workflow_run=dict(id=123), created_at="2026-11-01T01:00:30Z")
        job = dict(name=f"Native build / {TARGET}", conclusion="success", run_attempt=1,
                   started_at="2026-11-01T01:00:00Z", completed_at="2026-11-01T01:01:00Z")
        def download(selected, destination):
            self.assertEqual(selected, dict(id=7, digest=artifact["digest"]))
            shutil.copytree(self.transfer, destination)
        for corruption in (None, "missing", "expired", "duplicate", "failed", "current", "late", "origin", "receipt_attempt"):
            artifacts, jobs = [copy.deepcopy(artifact)], [copy.deepcopy(job)]
            if corruption == "missing":
                artifacts = []
            elif corruption == "expired":
                artifacts[0]["expired"] = True
            elif corruption == "duplicate":
                artifacts.append(artifacts[0])
            elif corruption == "failed":
                jobs[0]["conclusion"] = "failure"
            elif corruption == "current":
                jobs[0]["run_attempt"] = 2
            elif corruption == "late":
                artifacts[0]["created_at"] = "2026-11-01T02:00:00Z"
            elif corruption == "origin":
                artifacts[0]["workflow_run"]["id"] = 124
            elif corruption == "receipt_attempt":
                self.write_receipt({**self.receipt(), "build_attempt": 2})
            with self.subTest(corruption=corruption), patch.dict(os.environ, GITHUB_RUN_ATTEMPT="2"), \
                    patch.object(runtime, "load", return_value=candidates), \
                    patch.object(candidates, "run_artifacts", return_value=artifacts), \
                    patch.object(candidates, "api", return_value=[dict(jobs=jobs)]), \
                    patch.object(candidates, "download", side_effect=download) as downloaded:
                transfer, destination = self.root / f"download-{corruption}", self.root / f"restored-{corruption}"
                if corruption is None:
                    self.assertTrue(runtime.resume(transfer, destination, TARGET))
                    downloaded.assert_called_once()
                elif corruption in ("missing", "expired", "failed"):
                    self.assertFalse(runtime.resume(transfer, destination, TARGET))
                    downloaded.assert_not_called()
                else:
                    with self.assertRaises(ValueError):
                        runtime.resume(transfer, destination, TARGET)


_native_spec = importlib.util.spec_from_file_location("split_native_fixture", Path(__file__).with_name("test_package_native.py"))
native_fixture = importlib.util.module_from_spec(_native_spec)
_native_spec.loader.exec_module(native_fixture)


class SplitTransferTests(unittest.TestCase):
    """airk-ci-runtime/2: both images, the admin binding and current source identity."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = {"sha256": "1" * 64, "files": 4}
        self.admin = {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64}
        self.version = native_fixture.SPLIT_VERSION
        (self.root / "Cargo.toml").write_text(f'[workspace.package]\nversion="{self.version}"\n', encoding="utf-8")
        self.builder = runtime.package.runtime_builder()
        self.builder.source_identity = lambda: self.source
        original_load = self.builder.load_script
        admin = type("Admin", (), {"digests": staticmethod(lambda: dict(self.admin))})
        self.builder.load_script = lambda name: admin if name == "build-admin-assets" else original_load(name)
        for mock in (patch.object(runtime, "ROOT", self.root),
                     patch.object(runtime, "identity", return_value=IDENTITY),
                     patch.object(runtime.package, "runtime_builder", return_value=self.builder),
                     patch.object(native_fixture.package, "runtime_builder", return_value=self.builder),
                     patch.dict(os.environ, GITHUB_RUN_ATTEMPT="1")):
            mock.start()
            self.addCleanup(mock.stop)
        self.original = self.root / "original"
        self.original.mkdir()
        self.pair, self.build = native_fixture.split_runtime(self.original, TARGET, self.source, self.version)
        self.transfer = self.root / "transfer"
        runtime.seal(self.original, self.transfer, TARGET)

    def receipt(self):
        return json.loads((self.transfer / "transfer.json").read_text(encoding="utf-8"))

    def test_seal_binds_both_images_and_admin_and_restores_the_exact_pair(self):
        receipt = self.receipt()
        self.assertEqual(receipt["schema"], "airk-ci-runtime/2")
        self.assertEqual(receipt["airk_sha256"], runtime.package.sha256(self.original / "airk"))
        self.assertEqual(receipt["airkctl_sha256"], runtime.package.sha256(self.original / "airkctl"))
        self.assertEqual(receipt["admin_assets"], {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64})
        self.assertEqual(receipt["source"], {"sha256": "1" * 64, "files": 4})
        with patch.dict(os.environ, GITHUB_RUN_ATTEMPT="2"):
            runtime.restore(self.transfer, self.root / "restored", TARGET)
        self.assertEqual(runtime.package.inventory(self.original), runtime.package.inventory(self.root / "restored"))
        with zipfile.ZipFile(self.transfer / "runtime.zip") as archive:
            for name in ("airk", "airkctl"):
                self.assertEqual((archive.getinfo(name).external_attr >> 16) & 0o777, 0o755)

    def test_changed_frontend_or_admin_input_rejects_reuse_before_extraction(self):
        # A frontend-only change changes source_identity(); a different embedded bundle changes the digests.
        for change in ("source", "admin"):
            with self.subTest(change=change):
                saved_source, saved_admin = self.source, self.admin
                if change == "source":
                    self.source = {"sha256": "2" * 64, "files": 5}
                else:
                    self.admin = {"inventory_sha256": "5" * 64, "bundle_sha256": "4" * 64}
                destination = self.root / f"refused-{change}"
                with self.assertRaisesRegex(ValueError, "other source or frontend inputs" if change == "source"
                                            else "different admin bundle"):
                    runtime.restore(self.transfer, destination, TARGET)
                self.assertFalse(destination.exists())
                self.source, self.admin = saved_source, saved_admin

    def test_schema_gating_both_directions(self):
        receipt = self.receipt()
        for schema in ("airk-ci-runtime/1", "airk-ci-runtime/3"):
            with self.subTest(schema=schema):
                (self.transfer / "transfer.json").write_text(json.dumps({**receipt, "schema": schema}), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "another source"):
                    runtime.restore(self.transfer, self.root / "refused", TARGET)
        (self.transfer / "transfer.json").write_text(json.dumps(receipt), encoding="utf-8")
        # The split receipt never qualifies a pre-split checkout.
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.23"\n', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "another source"):
            runtime.restore(self.transfer, self.root / "refused", TARGET)
        self.assertFalse((self.root / "refused").exists())

    def test_receipt_missing_an_image_digest_or_admin_binding_is_refused(self):
        receipt = self.receipt()
        for key in ("airkctl_sha256", "admin_assets"):
            with self.subTest(key=key):
                (self.transfer / "transfer.json").write_text(
                    json.dumps({k: v for k, v in receipt.items() if k != key}), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "split contract: missing_field"):
                    runtime.restore(self.transfer, self.root / "refused", TARGET)

    def test_payload_without_airk_cannot_be_sealed(self):
        (self.original / "airk").unlink()
        native_fixture.seal_split(self.original, self.pair, self.build)
        with self.assertRaisesRegex(ValueError, "executable missing: airk"):
            runtime.seal(self.original, self.root / "other-transfer", TARGET)
        self.assertFalse((self.root / "other-transfer").exists())


class WorkflowSplitTests(unittest.TestCase):
    def test_validation_needs_successful_build_and_cannot_rebuild_or_publish_before_smoke(self):
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        build = workflow.split("\n  native-build:\n")[1].split("\n  native:\n")[0]
        validation = workflow.split("\n  native:\n")[1].split("\n  container:\n")[0]
        self.assertIn("needs: [plan, native-build]", validation)
        self.assertIn("needs.native-build.result == 'success'", validation)
        for command in ("cargo build", "cargo test", "cargo nextest", "build-service-runtime.py"):
            self.assertNotIn(command, validation)
        self.assertLess(validation.index("ci-runtime.py restore"), validation.index("check-service-clients.py"))
        self.assertLess(validation.index("mcp-smoke-1"), validation.index("package-native.py"))
        self.assertIn("--test-name-pattern '^four cold HTTPS (sequential|concurrent) clients observe first status without diagnostic warmup$'", validation)
        self.assertIn('if [ "$AIRK_QUALIFICATION" = "true" ]', validation)
        self.assertIn("CARGO_PROFILE_DEV_DEBUG: '0'", build)
        self.assertIn("CARGO_PROFILE_TEST_DEBUG: '0'", build)
        self.assertNotIn("CARGO_PROFILE_RELEASE", workflow)
        self.assertIn("if: ${{ !inputs.fresh_build }}", build)
        self.assertNotIn("continue-on-error", build + validation)
        for step in build.split("      - ")[1:]:
            if any(command in step for command in ("cargo nextest", "cargo clippy", "cargo test", "build-service-runtime.py")):
                self.assertIn("steps.resume.outputs.restored != 'true'", step)


if __name__ == "__main__":
    unittest.main()
