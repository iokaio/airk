# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("ci_candidates", Path(__file__).parents[1] / "ci-candidates.py")
candidates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(candidates)
COMMIT = "a" * 40


def run_fixture():
    return dict(id=123, run_attempt=1, head_sha=COMMIT, head_branch="main", event="push",
                path=".github/workflows/ci.yml", workflow_id=42, status="completed", conclusion="success",
                repository=dict(full_name="iokaio/airk"), head_repository=dict(full_name="iokaio/airk"))


def artifacts_fixture():
    return [dict(id=i + 1, name=f"airk-{kind}-{target}-{COMMIT}-123-1", expired=False,
                 digest="sha256:" + str(i) * 64,
                 workflow_run=dict(id=123, head_sha=COMMIT, head_branch="main"))
            for i, (kind, target) in enumerate((kind, target) for target in candidates.TARGETS for kind in ("native", "vsix"))]


def rehearsal_fixture():
    run = {**run_fixture(), "event": "workflow_dispatch", "path": ".github/workflows/extension-release.yml"}
    jobs = [dict(name="preflight", conclusion="success"), dict(name="publish", conclusion="skipped"),
            dict(name="verify-identity", conclusion="skipped")]
    jobs += [dict(name=f"build ({target})", conclusion="success", started_at="2026-10-16T01:00:00Z",
                  completed_at="2026-10-16T01:01:00Z") for target in candidates.TARGETS]
    artifacts = [dict(id=i + 1, name=f"airk-vsix-{target}-{COMMIT}-123", expired=False,
                      digest="sha256:" + str(i) * 64, created_at="2026-10-16T01:00:50Z",
                      workflow_run=dict(id=123, head_sha=COMMIT, head_branch="main"))
                 for i, target in enumerate(candidates.TARGETS)]
    return run, jobs, artifacts


def rehearsal_api(run, jobs, artifacts, workflow_id=42):
    def respond(path, paginate=False):
        if path == "actions/runs/123":
            return run
        if path == "actions/workflows/extension-release.yml":
            return dict(id=workflow_id)
        if path == f"actions/runs/123/attempts/{run['run_attempt']}/jobs?per_page=100":
            return [dict(jobs=jobs)]
        if path == "actions/runs/123/artifacts?per_page=100":
            return [dict(artifacts=artifacts)]
        raise AssertionError(f"Unexpected API call: {path}")
    return respond


class CandidateTests(unittest.TestCase):
    def test_rehearsal_requires_successful_main_origin_and_all_build_jobs(self):
        for corruption in (None, "workflow", "branch", "commit", "fork", "event", "running", "failed",
                           "published", "identity", "missing", "duplicate", "build_failed"):
            run, jobs, artifacts = rehearsal_fixture()
            workflow_id = 42
            if corruption == "workflow":
                workflow_id = 43
            elif corruption in ("branch", "commit", "fork", "event", "running", "failed"):
                key, value = {"branch": ("head_branch", "feature"), "commit": ("head_sha", "b" * 40),
                              "fork": ("head_repository", {"full_name": "fork/airk"}),
                              "event": ("event", "pull_request"), "running": ("status", "in_progress"),
                              "failed": ("conclusion", "failure")}[corruption]
                run[key] = value
            elif corruption == "published":
                jobs[1]["conclusion"] = "success"
            elif corruption == "identity":
                jobs[2]["conclusion"] = "success"
            elif corruption == "missing":
                jobs.pop()
            elif corruption == "duplicate":
                jobs.append(jobs[-1])
            elif corruption == "build_failed":
                jobs[-1]["conclusion"] = "failure"
            with self.subTest(corruption=corruption), \
                    patch.object(candidates, "api", side_effect=rehearsal_api(run, jobs, artifacts, workflow_id)):
                if corruption:
                    with self.assertRaises(ValueError):
                        candidates.attest_rehearsal(123, COMMIT)
                else:
                    record = candidates.attest_rehearsal(123, COMMIT)
                    self.assertEqual(record["attempt"], "1")
                    self.assertEqual(len(record["artifacts"]), 4)

    def test_rehearsal_rejects_stale_expired_unbound_or_incomplete_uploads(self):
        for corruption in ("missing", "duplicate", "expired", "digest", "origin", "old_attempt", "after_job", "no_timezone"):
            run, jobs, artifacts = rehearsal_fixture()
            if corruption == "missing":
                artifacts.pop()
            elif corruption == "duplicate":
                artifacts.append(artifacts[0])
            elif corruption == "expired":
                artifacts[0]["expired"] = True
            elif corruption == "digest":
                artifacts[0]["digest"] = "bad"
            elif corruption == "origin":
                artifacts[0]["workflow_run"]["id"] = 456
            else:
                artifacts[0]["created_at"] = {"old_attempt": "2026-10-15T01:00:50Z",
                                              "after_job": "2026-10-16T01:02:00Z",
                                              "no_timezone": "2026-10-16T01:00:50"}[corruption]
            with self.subTest(corruption=corruption), \
                    patch.object(candidates, "api", side_effect=rehearsal_api(run, jobs, artifacts)), \
                    self.assertRaises(ValueError):
                candidates.attest_rehearsal(123, COMMIT)

    def test_rehearsal_download_pins_ids_and_rechecks_after_transfer(self):
        run, jobs, artifacts = rehearsal_fixture()
        with patch.object(candidates, "api", side_effect=rehearsal_api(run, jobs, artifacts)):
            record = candidates.attest_rehearsal(123, COMMIT)
        for change in (None, "before", "during"):
            changed = {**record, "attempt": "2"}
            replies = [changed] if change == "before" else [record, changed if change == "during" else record]
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(candidates, "attest_rehearsal", side_effect=replies), \
                    patch.object(candidates, "download") as download:
                output = Path(temporary) / "release"
                if change:
                    with self.assertRaisesRegex(ValueError, "changed"):
                        candidates.download_rehearsal(record, COMMIT, output)
                else:
                    candidates.download_rehearsal(record, COMMIT, output)
                    for name, artifact in record["artifacts"].items():
                        download.assert_any_call(artifact, output / name)
                self.assertEqual(download.call_count, 0 if change == "before" else 4)

    def test_rehearsal_download_rejects_wrong_record_and_existing_destination(self):
        run, jobs, artifacts = rehearsal_fixture()
        with patch.object(candidates, "api", side_effect=rehearsal_api(run, jobs, artifacts)):
            record = candidates.attest_rehearsal(123, COMMIT)
        for bad in ({**record, "schema": "unknown/1"}, {**record, "commit": "b" * 40}):
            with patch.object(candidates, "attest_rehearsal") as attest, \
                    patch.object(candidates, "download") as download, self.assertRaises(ValueError):
                candidates.download_rehearsal(bad, COMMIT, Path("unused"))
            attest.assert_not_called()
            download.assert_not_called()
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(candidates, "attest_rehearsal", return_value=record), \
                patch.object(candidates, "download") as download:
            marker = Path(temporary) / "keep.txt"
            marker.write_text("existing data", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "fresh"):
                candidates.download_rehearsal(record, COMMIT, Path(temporary))
            download.assert_not_called()
            self.assertEqual(marker.read_text(encoding="utf-8"), "existing data")

    def test_producer_must_be_exact_successful_main_origin(self):
        valid = run_fixture()
        candidates.verify_run(valid, COMMIT, "ci.yml", ("push",))
        for key, value in (("head_sha", "b" * 40), ("head_branch", "fork"), ("event", "pull_request"),
                           ("status", "in_progress"), ("conclusion", "failure"), ("path", ".github/workflows/other.yml"),
                           ("head_repository", {"full_name": "fork/airk"}), ("repository", {"full_name": "fork/airk"})):
            with self.subTest(key=key), self.assertRaises(ValueError):
                candidates.verify_run({**valid, key: value}, COMMIT, "ci.yml", ("push",))

    def test_complete_digest_bound_single_attempt_artifacts(self):
        valid = artifacts_fixture()
        self.assertEqual(len(candidates.select_artifacts(valid, COMMIT, 123, 1)), 8)
        for corruption in ("missing", "duplicate", "expired", "digest", "attempt", "commit", "run"):
            bad = copy.deepcopy(valid)
            if corruption == "missing":
                bad.pop()
            elif corruption == "duplicate":
                bad.append(bad[0])
            elif corruption == "attempt":
                bad[0]["name"] = bad[0]["name"][:-1] + "2"
            elif corruption in ("commit", "run"):
                bad[0]["workflow_run"]["head_sha" if corruption == "commit" else "id"] = "bad"
            else:
                bad[0][corruption] = True if corruption == "expired" else None
            with self.subTest(corruption=corruption), self.assertRaises((ValueError, TypeError)):
                candidates.select_artifacts(bad, COMMIT, 123, 1)

    def test_missing_failed_or_duplicate_required_job_rejects_promotion(self):
        names = ["Required checks", "container", "quick", "plan", "extension"]
        names += [f"Native / {target}" for target in candidates.TARGETS]
        names += [f"Contracts / {os_name}" for os_name in ("ubuntu-24.04", "windows-2025", "macos-15", "macos-15-intel")]
        for corruption in (None, "missing", "failed", "duplicate", "workflow"):
            jobs = [dict(name=name, conclusion="success") for name in names]
            if corruption == "missing":
                jobs.pop()
            elif corruption == "failed":
                jobs[0]["conclusion"] = "failure"
            elif corruption == "duplicate":
                jobs.append(jobs[0])
            def api(path, paginate=False):
                if path == "actions/runs/123":
                    return run_fixture()
                if path == "actions/workflows/ci.yml":
                    return dict(id=43 if corruption == "workflow" else 42)
                if "/jobs?" in path:
                    return [dict(jobs=jobs)]
                return [dict(artifacts=artifacts_fixture())]
            with self.subTest(corruption=corruption), patch.object(candidates, "api", side_effect=api):
                if corruption:
                    with self.assertRaises(ValueError):
                        candidates.attest(123, COMMIT)
                else:
                    self.assertEqual(candidates.attest(123, COMMIT)["attempt"], "1")

    def test_preflight_record_cannot_change_during_signing(self):
        record = dict(schema="airk-ci-producer/1", commit=COMMIT, run_id="123", attempt="1", event="push")
        with patch.object(candidates, "attest", return_value={**record, "attempt": "2"}):
            with self.assertRaisesRegex(ValueError, "changed after preflight"):
                candidates.verify_record(record, COMMIT)

    def test_outer_artifact_cannot_escape_or_replace_files(self):
        for name in ("../outside", "/absolute", "dir/file", "back\\slash", ".."):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with zipfile.ZipFile(root / "in.zip", "w") as archive:
                    archive.writestr(name, b"fixture")
                with self.assertRaises(ValueError):
                    candidates.unpack_artifact(root / "in.zip", root / "out")
                self.assertFalse((root / "out").exists())

    def test_download_authenticates_bytes_before_extracting(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("manifest.json", b"original fixture")
        payload = buffer.getvalue()
        def transfer(command, stdout, check):
            self.assertEqual(command[-1], "repos/iokaio/airk/actions/artifacts/7/zip")
            stdout.write(payload)
        for corrupt in (False, True):
            with self.subTest(corrupt=corrupt), tempfile.TemporaryDirectory() as temporary:
                destination = Path(temporary) / "out"
                digest = "0" * 64 if corrupt else hashlib.sha256(payload).hexdigest()
                with patch.object(candidates.subprocess, "run", side_effect=transfer):
                    if corrupt:
                        with self.assertRaisesRegex(ValueError, "digest differs"):
                            candidates.download(dict(id=7, digest="sha256:" + digest), destination)
                        self.assertFalse(destination.exists())
                    else:
                        candidates.download(dict(id=7, digest="sha256:" + digest), destination)
                        self.assertEqual((destination / "manifest.json").read_bytes(), b"original fixture")

    def test_publish_retry_requires_a_successful_assembly_from_same_run(self):
        artifact = dict(id=9, name=f"airk-release-{COMMIT}-123-1", expired=False, digest="sha256:" + "a" * 64)
        for conclusion in ("success", "failure", "cancelled"):
            def api(path, paginate=False):
                if path == "actions/runs/123":
                    return {**run_fixture(), "path": ".github/workflows/github-release.yml",
                            "event": "workflow_dispatch", "status": "in_progress", "run_attempt": 2}
                if "/jobs?" in path:
                    self.assertIn("/attempts/1/", path)
                    return [dict(jobs=[dict(name="assemble", conclusion=conclusion)])]
                return [dict(artifacts=[artifact])]
            with self.subTest(conclusion=conclusion), patch.object(candidates, "api", side_effect=api), \
                    patch.object(candidates, "download") as download:
                if conclusion == "success":
                    candidates.release_bundle(COMMIT, "123", "2", Path("unused"))
                    download.assert_called_once_with(artifact, Path("unused"))
                else:
                    with self.assertRaisesRegex(ValueError, "assembly did not succeed"):
                        candidates.release_bundle(COMMIT, "123", "2", Path("unused"))
                    download.assert_not_called()


if __name__ == "__main__":
    unittest.main()
