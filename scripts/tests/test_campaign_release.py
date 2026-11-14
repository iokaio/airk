# SPDX-License-Identifier: Apache-2.0
"""Campaign adapters exercise real selectors/guards against independent API fixtures."""
from copy import deepcopy
from contextlib import redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
COMMIT = "a" * 40
BRANCH = "feat/advanced-split"
VERSION = "1.8.35"


def load(filename):
    spec = importlib.util.spec_from_file_location(filename.replace("-", "_"), ROOT / "scripts" / (filename + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


candidates = load("ci-candidates")
release = load("github-release")
version = load("release-version")
windows = load("ci-windows-sign")
macos = load("ci-macos-sign")
CASES = json.loads((ROOT / "fixtures/split/receipts/release_authority.json").read_text())
TARGETS = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin")
EXPECTED = {f"airk-native-{target}-{COMMIT}-123-2" for target in TARGETS} | {f"airk-vsix-universal-{COMMIT}-123-2"}


class Origin:
    def __init__(self):
        self.ci = deepcopy(CASES["live"]["run"])
        self.jobs = deepcopy(CASES["live"]["jobs"])
        self.artifacts = [{"id": index + 1, "name": name, "expired": False, "digest": "sha256:" + "d" * 64,
                           "workflow_run": {"id": 123, "head_sha": COMMIT, "head_branch": BRANCH}}
                          for index, name in enumerate(sorted(EXPECTED))]
        self.signed = {**deepcopy(self.ci), "id": 456, "workflow_id": 5, "run_attempt": 3,
                       "path": ".github/workflows/github-release.yml"}
        self.assembly = {"name": "assemble", "conclusion": "success",
                         "started_at": "2026-11-13T10:00:00Z", "completed_at": "2026-11-13T10:02:00Z"}
        self.bundle = {"id": 88, "name": f"airk-release-{COMMIT}-456-3", "expired": False,
                       "digest": "sha256:" + "e" * 64, "created_at": "2026-11-13T10:01:00Z",
                       "workflow_run": {"id": 456, "head_sha": COMMIT, "head_branch": BRANCH}}
        self.calls = []

    def api(self, path, paginate=False):
        self.calls.append((path, paginate))
        values = {"actions/runs/123": self.ci, "actions/workflows/ci.yml": {"id": 4},
                  "actions/runs/123/attempts/2/jobs?per_page=100": [{"jobs": self.jobs}],
                  "actions/runs/123/artifacts?per_page=100": [{"artifacts": self.artifacts}],
                  "actions/runs/456": self.signed, "actions/workflows/github-release.yml": {"id": 5},
                  "actions/runs/456/attempts/3/jobs?per_page=100": [{"jobs": [self.assembly]}],
                  "actions/runs/456/artifacts?per_page=100": [{"artifacts": [self.bundle]}]}
        if path not in values:
            raise AssertionError("Unexpected API call: " + path)
        return deepcopy(values[path])


class CampaignCandidateTests(unittest.TestCase):
    def test_full_actual_campaign_record_and_live_revalidation(self):
        origin = Origin()
        with patch.object(candidates, "api", side_effect=origin.api):
            record = candidates.resolve(COMMIT, 123, version=VERSION, branch=BRANCH)
            expected = {"schema": "airk-ci-producer/3", "repository": "iokaio/airk", "workflow_id": 4,
                        "commit": COMMIT, "qualified_commit": COMMIT, "source_branch": BRANCH,
                        "run_id": "123", "attempt": "2", "event": "workflow_dispatch",
                        "artifacts": {row["name"]: {"id": row["id"], "digest": row["digest"]} for row in origin.artifacts}}
            self.assertEqual(record, expected)
            candidates.validate_record(record, COMMIT, version=VERSION)
            candidates.verify_record(record, COMMIT, version=VERSION)
            self.assertEqual(set(record["artifacts"]), EXPECTED)
            origin.ci["head_branch"] = "main"
            with self.assertRaises(ValueError):
                candidates.verify_record(record, COMMIT, version=VERSION)

    def test_campaign_refuses_implicit_run_historical_catalog_or_nonmanual_event(self):
        with patch.object(candidates, "api", side_effect=AssertionError("No API needed for invalid nomination")):
            with self.assertRaises(ValueError):
                candidates.resolve(COMMIT, version=VERSION, branch=BRANCH)
            with self.assertRaises(ValueError):
                candidates.attest(123, COMMIT, allow_manual=True, version="1.8.23", branch=BRANCH)
            with self.assertRaises(ValueError):
                candidates.attest(123, COMMIT, version=VERSION, branch=BRANCH)
        origin = Origin()
        origin.ci["event"] = "push"
        with patch.object(candidates, "api", side_effect=origin.api), self.assertRaises(ValueError):
            candidates.resolve(COMMIT, 123, version=VERSION, branch=BRANCH)

    def test_all_five_artifacts_and_every_qualified_gate_are_actual_requirements(self):
        for index in range(5):
            origin = Origin()
            origin.artifacts.pop(index)
            with self.subTest(missing=index), patch.object(candidates, "api", side_effect=origin.api), self.assertRaises(ValueError):
                candidates.resolve(COMMIT, 123, version=VERSION, branch=BRANCH)
        for fault in ("branch", "expired", "promotion", "duplicate", "job"):
            origin = Origin()
            if fault == "branch":
                origin.artifacts[0]["workflow_run"]["head_branch"] = "main"
            elif fault == "expired":
                origin.artifacts[0]["expired"] = True
            elif fault == "promotion":
                origin.artifacts.append({"name": f"airk-promotion-{COMMIT}-123-2"})
            elif fault == "duplicate":
                origin.artifacts.append(deepcopy(origin.artifacts[0]))
            elif fault == "job":
                origin.jobs = [job for job in origin.jobs if job["name"] != "Native / x86_64-apple-darwin"]
            with self.subTest(fault=fault), patch.object(candidates, "api", side_effect=origin.api), self.assertRaises(ValueError):
                candidates.resolve(COMMIT, 123, version=VERSION, branch=BRANCH)

    def test_strict_campaign_decoder_never_accepts_subset_promotion_or_unreviewed_fields(self):
        origin = Origin()
        with patch.object(candidates, "api", side_effect=origin.api):
            record = candidates.resolve(COMMIT, 123, version=VERSION, branch=BRANCH)
        for key, value in (("source_branch", "main"), ("qualified_commit", None), ("promotion", None),
                           ("unexpected", True), ("event", "push"), ("schema", "airk-ci-producer/1")):
            with self.subTest(field=key), self.assertRaises(ValueError):
                candidates.validate_record({**record, key: value}, COMMIT, version=VERSION)
        extra = deepcopy(record)
        extra["artifacts"][sorted(EXPECTED)[0]]["unreviewed"] = True
        with self.assertRaises(ValueError):
            candidates.validate_record(extra, COMMIT, version=VERSION)

    def test_signed_variant_retains_real_origin_and_exact_successful_assembly(self):
        origin = Origin()
        with patch.object(candidates, "api", side_effect=origin.api):
            record = candidates.attest_signed_release(456, COMMIT, branch=BRANCH)
            self.assertEqual(record, {"schema": "airk-signed-release-producer/2", "repository": "iokaio/airk",
                "workflow_id": 5, "commit": COMMIT, "run_id": "456", "attempt": "3",
                "artifact_name": f"airk-release-{COMMIT}-456-3", "artifact": {"id": 88, "digest": "sha256:" + "e" * 64},
                "event": "workflow_dispatch", "source_branch": BRANCH, "qualified_commit": COMMIT})
            origin.bundle["workflow_run"]["head_branch"] = "main"
            with self.assertRaises(ValueError):
                candidates.attest_signed_release(456, COMMIT, branch=BRANCH)

    def test_signed_download_rechecks_campaign_origin_before_download(self):
        origin = Origin()
        with patch.object(candidates, "api", side_effect=origin.api):
            record = candidates.attest_signed_release(456, COMMIT, branch=BRANCH)
            origin.signed["head_branch"] = "main"
            with tempfile.TemporaryDirectory() as temporary, patch.object(candidates, "download") as download:
                with self.assertRaises(ValueError):
                    candidates.download_signed_release(record, COMMIT, Path(temporary) / "out", VERSION, "stable")
                download.assert_not_called()


class CampaignGuardTests(unittest.TestCase):
    def environment(self, workflow="github-release.yml"):
        return {**CASES["context"], "GITHUB_WORKFLOW_REF": f"iokaio/airk/.github/workflows/{workflow}@refs/heads/{BRANCH}",
                "GITHUB_RUN_ID": "456", "GITHUB_RUN_ATTEMPT": "3", "GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}

    def test_release_context_and_hosted_signing_guards_require_exact_campaign_sha(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.35"\n')
            manifest = root / "packages/vscode/package.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('{"version":"1.8.35"}')
            env = {**self.environment(), "RUNNER_TEMP": temporary}
            with patch.dict(os.environ, env, clear=True), patch.object(release, "ROOT", root):
                self.assertEqual(release.context(), (COMMIT, VERSION, "v1.8.35"))
                with patch.object(windows.sys, "platform", "win32"):
                    windows.guard()
                with patch.object(macos.sys, "platform", "darwin"):
                    self.assertEqual(macos.runner_directory(), root.resolve() / "airk-macos-signing")
                for invalid in ({"AIRK_QUALIFIED_SHA": "b" * 40}, {"GITHUB_EVENT_NAME": "push"},
                                {"GITHUB_REPOSITORY": "foreign/airk"}, {"GITHUB_WORKFLOW_REF": "foreign/airk"}):
                    with self.subTest(invalid=invalid), patch.dict(os.environ, invalid):
                        with self.assertRaises(ValueError):
                            release.context()
                        with patch.object(windows.sys, "platform", "win32"), self.assertRaises(ValueError):
                            windows.guard()
                        with patch.object(macos.sys, "platform", "darwin"), self.assertRaises(ValueError):
                            macos.runner_directory()

    def test_campaign_version_preparation_refuses_pr_before_any_remote_effect(self):
        env = self.environment()
        with patch.dict(os.environ, env, clear=True), patch.object(sys, "argv", ["release-version.py", "prepare-pr", "--destination", "github", "--publish"]):
            with patch.object(version, "api", side_effect=AssertionError("No remote mutation")) as api:
                with self.assertRaisesRegex(ValueError, "PR creation is forbidden"):
                    version.main()
                api.assert_not_called()

    def test_campaign_bump_reports_local_requalification_without_opening_pr(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            env = {**self.environment(), "GITHUB_OUTPUT": str(root / "output"), "GITHUB_STEP_SUMMARY": str(root / "summary")}
            decision = {"version": VERSION, "action": "bump", "next_version": "1.8.36"}
            with patch.dict(os.environ, env, clear=True), patch.object(sys, "argv", ["release-version.py", "plan", "--destination", "github", "--publish"]), \
                 patch.object(version, "source_version", return_value=VERSION), patch.object(version, "marketplace", return_value=[]), \
                 patch.object(version, "api", return_value=[]), patch.object(version, "plan", return_value=decision), \
                 patch.object(version, "prepare_pr", side_effect=AssertionError("No PR")) as prepare, redirect_stdout(io.StringIO()):
                version.main()
            prepare.assert_not_called()
            self.assertIn("bump", (root / "output").read_text())
            self.assertIn("locally once on feat/advanced-split", (root / "summary").read_text())

    def test_workflows_propagate_only_manual_frozen_nomination_and_keep_main_pr_path_separate(self):
        github = (ROOT / ".github/workflows/github-release.yml").read_text()
        extension = (ROOT / ".github/workflows/extension-release.yml").read_text()
        downloads = (ROOT / ".github/workflows/downloads.yml").read_text()
        for source, workflow in ((github, "github-release.yml"), (extension, "extension-release.yml")):
            self.assertIn("AIRK_QUALIFIED_SHA: ${{ inputs.qualified_sha }}", source)
            self.assertIn("scripts/release-authority.py context --workflow " + workflow, source)
            version_job = source.split("  version-pr:\n", 1)[1]
            self.assertIn("if: ${{ github.ref == 'refs/heads/main' &&", version_job.split("runs-on:", 1)[0])
            self.assertIn("default: false", source)
        self.assertIn("qualified_sha: ${{ inputs.qualified_sha }}", github.split("  downloads:", 1)[1])
        self.assertIn("actions: read", github.split("  downloads:", 1)[1])
        self.assertIn("actions: read", downloads.split("jobs:", 1)[0])
        self.assertIn("inputs.qualified_sha == github.sha", downloads)
        self.assertIn("--workflow downloads.yml --workflow github-release.yml", downloads)


if __name__ == "__main__":
    unittest.main()
