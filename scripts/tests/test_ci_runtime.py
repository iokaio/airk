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
        self.assertIn("--test-name-pattern '^four cold (pinned|source)", validation)
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
