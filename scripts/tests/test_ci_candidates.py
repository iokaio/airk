# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import importlib.util
import io
import json
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
    def signed_release_fixture(self):
        run = {**run_fixture(), "event": "workflow_dispatch", "path": ".github/workflows/github-release.yml"}
        job = dict(name="assemble", conclusion="success", started_at="2026-11-03T01:00:00Z",
                   completed_at="2026-11-03T01:02:00Z")
        artifact = dict(id=9, name=f"airk-release-{COMMIT}-123-1", expired=False,
                        digest="sha256:" + "a" * 64, created_at="2026-11-03T01:01:00Z",
                        workflow_run=dict(id=123, head_sha=COMMIT, head_branch="main"))
        def respond(path, paginate=False):
            if path == "actions/runs/123":
                return run
            if path == "actions/workflows/github-release.yml":
                return dict(id=42)
            if path == "actions/runs/123/attempts/1/jobs?per_page=100":
                return [dict(jobs=[job])]
            if path == "actions/runs/123/artifacts?per_page=100":
                return [dict(artifacts=[artifact])]
            raise AssertionError(path)
        return run, job, artifact, respond

    def test_signed_release_pins_successful_exact_attempt_and_upload_interval(self):
        for mutation in (None, "unsigned_workflow", "workflow_id", "failed", "not_complete", "job_failed",
                         "branch", "fork", "expired", "wrong_attempt", "digest", "origin", "old_upload"):
            run, job, artifact, respond = self.signed_release_fixture()
            if mutation == "unsigned_workflow": run["path"] = ".github/workflows/extension-release.yml"
            elif mutation == "workflow_id": run["workflow_id"] = 99
            elif mutation == "failed": run["conclusion"] = "failure"
            elif mutation == "not_complete": run["status"] = "in_progress"
            elif mutation == "job_failed": job["conclusion"] = "failure"
            elif mutation == "branch": run["head_branch"] = "feature"
            elif mutation == "fork": run["head_repository"]["full_name"] = "foreign/airk"
            elif mutation == "expired": artifact["expired"] = True
            elif mutation == "wrong_attempt": artifact["name"] = f"airk-release-{COMMIT}-123-2"
            elif mutation == "digest": artifact["digest"] = "sha256:bad"
            elif mutation == "origin": artifact["workflow_run"]["head_sha"] = "b" * 40
            elif mutation == "old_upload": artifact["created_at"] = "2026-11-02T01:01:00Z"
            with self.subTest(mutation=mutation), patch.object(candidates, "api", side_effect=respond):
                if mutation:
                    with self.assertRaises(ValueError): candidates.attest_signed_release(123, COMMIT)
                else:
                    record = candidates.attest_signed_release(123, COMMIT)
                    self.assertEqual(record["artifact"], dict(id=9, digest="sha256:" + "a" * 64))
                    self.assertEqual(record["attempt"], "1")

    def test_signed_release_rechecks_identity_before_emitting_packages(self):
        _, _, _, respond = self.signed_release_fixture()
        with patch.object(candidates, "api", side_effect=respond):
            record = candidates.attest_signed_release(123, COMMIT)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "out"
            with patch.object(candidates, "attest_signed_release", side_effect=[record, {**record, "attempt": "2"}]), \
                    patch.object(candidates, "download") as download:
                with self.assertRaisesRegex(ValueError, "changed during download"):
                    candidates.download_signed_release(record, COMMIT, destination, "1.8.8", "stable")
                download.assert_called_once()
                self.assertFalse(destination.exists())
            for bad in ({**record, "commit": "b" * 40}, {**record, "repository": "foreign/airk"}):
                with self.assertRaisesRegex(ValueError, "another commit/schema"):
                    candidates.download_signed_release(bad, COMMIT, destination, "1.8.8", "stable")

    def test_failed_publish_only_retry_retains_original_signed_assembly_identity(self):
        run, _, artifact, respond = self.signed_release_fixture()
        with patch.object(candidates, "api", side_effect=respond):
            original = candidates.attest_signed_release(123, COMMIT)
            # Only publish/downloads reran; attempt two has no new assembly.
            run["run_attempt"] = 2
            self.assertEqual(candidates.attest_signed_release(123, COMMIT), original)
            with patch.object(candidates, "download") as download:
                candidates.release_bundle(COMMIT, "123", "2", Path("unused"))
                download.assert_called_once_with(artifact, Path("unused"))

    def test_new_assembly_invalidates_old_preflight_instead_of_reusing_original_bytes(self):
        run, job, artifact, respond = self.signed_release_fixture()
        with patch.object(candidates, "api", side_effect=respond):
            original = candidates.attest_signed_release(123, COMMIT)
        run["run_attempt"] = 2
        replacement = {**artifact, "id": 10, "name": f"airk-release-{COMMIT}-123-2", "digest": "sha256:" + "b" * 64}
        def updated(path, paginate=False):
            if path == "actions/runs/123/artifacts?per_page=100":
                return [dict(artifacts=[artifact, replacement])]
            if path == "actions/runs/123/attempts/2/jobs?per_page=100":
                return [dict(jobs=[job])]
            return respond(path, paginate)
        with tempfile.TemporaryDirectory() as temporary, patch.object(candidates, "api", side_effect=updated):
            current = candidates.attest_signed_release(123, COMMIT)
            self.assertEqual(current["attempt"], "2")
            self.assertEqual(current["artifact"]["id"], 10)
            with patch.object(candidates, "download") as download:
                with self.assertRaisesRegex(ValueError, "changed after preflight"):
                    candidates.download_signed_release(original, COMMIT, Path(temporary) / "out", "1.8.8", "stable")
                download.assert_not_called()
            # A damaged superseding upload is not permission to choose old bytes.
            replacement["expired"] = True
            with self.assertRaisesRegex(ValueError, "expired"):
                candidates.attest_signed_release(123, COMMIT)

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
        def transfer(command, stdout, **kwargs):
            self.assertEqual(command[2], "repos/iokaio/airk/actions/artifacts/7/zip")
            stdout.write(payload)
            return candidates.subprocess.CompletedProcess(command, 0, None, b"")
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
        artifact = dict(id=9, name=f"airk-release-{COMMIT}-123-1", expired=False, digest="sha256:" + "a" * 64,
                        created_at="2026-11-03T01:01:00Z",
                        workflow_run=dict(id=123, head_sha=COMMIT, head_branch="main"))
        for conclusion in ("success", "failure", "cancelled"):
            def api(path, paginate=False):
                if path == "actions/runs/123":
                    return {**run_fixture(), "path": ".github/workflows/github-release.yml",
                            "event": "workflow_dispatch", "status": "in_progress", "run_attempt": 2}
                if "/jobs?" in path:
                    self.assertIn("/attempts/1/", path)
                    return [dict(jobs=[dict(name="assemble", conclusion=conclusion,
                                           started_at="2026-11-03T01:00:00Z", completed_at="2026-11-03T01:02:00Z")])]
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


TREE, MERGE, HEAD = "c" * 40, "d" * 40, "e" * 40


def pull_fixture():
    return dict(number=5, merged_at="2026-10-30T01:25:30Z", merge_commit_sha=COMMIT, base=dict(ref="main"),
                head=dict(sha=HEAD, ref="feature", repo=dict(full_name="iokaio/airk")))


def pr_run_fixture():
    return {**run_fixture(), "id": 777, "head_sha": HEAD, "head_branch": "feature", "event": "pull_request"}


def pr_jobs_fixture():
    names = ["Required checks", "container", "quick", "plan", "extension"]
    names += [f"Native / {target}" for target in candidates.PR_TARGETS]
    names += [f"Contracts / {os_name}" for os_name in candidates.CONTRACT_HOSTS]
    return [dict(name=name, conclusion="success") for name in names]


def pr_artifacts_fixture():
    return [dict(id=100 + i, name=f"airk-{kind}-{target}-{MERGE}-777-1", expired=False, digest="sha256:" + str(i) * 64,
                 workflow_run=dict(id=777, head_sha=HEAD, head_branch="feature"))
            for i, (kind, target) in enumerate((kind, target) for target in candidates.PR_TARGETS for kind in ("native", "vsix"))]


def main_artifacts_fixture():
    intel = candidates.MAIN_TARGETS[0]
    return [dict(id=200 + i, name=f"airk-{kind}-{intel}-{COMMIT}-123-1", expired=False, digest="sha256:" + "f" * 64,
                 workflow_run=dict(id=123, head_sha=COMMIT, head_branch="main"))
            for i, kind in enumerate(("native", "vsix"))] + [
        dict(id=300, name=f"airk-promotion-{COMMIT}-123-1", expired=False, digest="sha256:" + "9" * 64,
             workflow_run=dict(id=123, head_sha=COMMIT, head_branch="main"))]


def promotion_api(pulls=None, pr_run=None, pr_jobs=None, pr_artifacts=None, main_jobs=None, main_artifacts=None, main_run=None):
    def respond(path, paginate=False):
        if path == f"commits/{COMMIT}/pulls":
            return pull_fixture_list if pulls is None else pulls
        if path.startswith("actions/workflows/ci.yml/runs?"):
            assert f"head_sha={HEAD}" in path and "event=pull_request" in path
            return [dict(workflow_runs=[pr_run or pr_run_fixture()])]
        if path == "actions/workflows/ci.yml":
            return dict(id=42)
        if path == "actions/runs/777":
            return pr_run or pr_run_fixture()
        if path == "actions/runs/777/attempts/1/jobs?per_page=100":
            return [dict(jobs=pr_jobs_fixture() if pr_jobs is None else pr_jobs)]
        if path == "actions/runs/777/artifacts?per_page=100":
            return [dict(artifacts=pr_artifacts_fixture() if pr_artifacts is None else pr_artifacts)]
        if path == "actions/runs/123":
            return main_run or run_fixture()
        if path == "actions/runs/123/attempts/1/jobs?per_page=100":
            names = ["Required checks", "quick", "plan", f"Native / {candidates.MAIN_TARGETS[0]}"]
            return [dict(jobs=[dict(name=name, conclusion="success") for name in names] if main_jobs is None else main_jobs)]
        if path == "actions/runs/123/artifacts?per_page=100":
            return [dict(artifacts=main_artifacts_fixture() if main_artifacts is None else main_artifacts)]
        raise AssertionError(f"Unexpected API call: {path}")
    pull_fixture_list = [pull_fixture()]
    return respond


def receipt_download(tree=TREE, commit=MERGE, dirty=False):
    def write(artifact, destination):
        destination.mkdir(parents=True)
        target = next(t for t in candidates.PR_TARGETS if any(artifact["id"] == a["id"] and t in a["name"]
                                                                for a in pr_artifacts_fixture()))
        receipt = dict(commit=commit, tree=tree, dirty_checkout=dirty, target=target)
        (destination / "manifest.json").write_text(json.dumps(receipt), encoding="utf-8")
    return write


class PromotionTests(unittest.TestCase):
    def promote(self, api=None, download=None):
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(candidates, "api", side_effect=api or promotion_api()), \
                patch.object(candidates, "download", side_effect=download or receipt_download()):
            return candidates.promote(COMMIT, TREE, Path(temporary))

    def test_merged_same_repository_pr_with_identical_tree_is_promoted(self):
        record = self.promote()
        self.assertEqual(record["schema"], "airk-ci-promotion/1")
        self.assertEqual((record["commit"], record["tree"], record["pull_request"]), (COMMIT, TREE, 5))
        producer = record["producer"]
        self.assertEqual((producer["run_id"], producer["attempt"], producer["source_commit"]), ("777", "1", MERGE))
        self.assertEqual(sorted(producer["artifacts"]), sorted(a["name"] for a in pr_artifacts_fixture()))

    def test_promotion_refuses_unmerged_forked_or_ambiguous_pull_requests(self):
        merged = pull_fixture()
        for pulls in ([], [merged, merged], [{**merged, "merged_at": None}], [{**merged, "merge_commit_sha": "b" * 40}],
                      [{**merged, "base": dict(ref="release")}],
                      [{**merged, "head": {**merged["head"], "repo": dict(full_name="fork/airk")}}]):
            with self.subTest(pulls=pulls), self.assertRaises(ValueError):
                self.promote(promotion_api(pulls=pulls))

    def test_promotion_refuses_incomplete_or_mismatched_pr_producers(self):
        jobs_without_windows = [job for job in pr_jobs_fixture() if "windows-msvc" not in job["name"]]
        failed_container = [{**job, "conclusion": "failure"} if job["name"] == "container" else job for job in pr_jobs_fixture()]
        two_sources = pr_artifacts_fixture() + [{**pr_artifacts_fixture()[0], "id": 999, "name": f"airk-native-{candidates.PR_TARGETS[0]}-{'b' * 40}-777-1"}]
        for name, api in (("skipped native", promotion_api(pr_jobs=jobs_without_windows)),
                          ("failed container", promotion_api(pr_jobs=failed_container)),
                          ("missing artifact", promotion_api(pr_artifacts=pr_artifacts_fixture()[:-1])),
                          ("expired artifact", promotion_api(pr_artifacts=[{**a, "expired": True} for a in pr_artifacts_fixture()])),
                          ("ambiguous merge ref", promotion_api(pr_artifacts=two_sources)),
                          ("fork run", promotion_api(pr_run={**pr_run_fixture(), "head_repository": dict(full_name="fork/airk")})),
                          ("stale head", promotion_api(pr_run={**pr_run_fixture(), "head_sha": "b" * 40}))):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.promote(api)
        for name, download in (("tree", receipt_download(tree="b" * 40)), ("commit", receipt_download(commit="b" * 40)),
                               ("dirty", receipt_download(dirty=True))):
            with self.subTest(receipt=name), self.assertRaisesRegex(ValueError, "No eligible PR candidate"):
                self.promote(download=download)

    def attest_promoted(self, api=None, promotion=None):
        record = promotion or self.promote()
        def fetch(artifact, destination):
            self.assertEqual(artifact, dict(id=300, digest="sha256:" + "9" * 64))
            destination.mkdir(parents=True)
            (destination / "promotion.json").write_text(json.dumps(record), encoding="utf-8")
        with patch.object(candidates, "api", side_effect=api or promotion_api()), patch.object(candidates, "download", side_effect=fetch):
            return candidates.attest(123, COMMIT)

    def test_promoted_main_run_combines_pr_targets_with_its_own_intel_build(self):
        record = self.attest_promoted()
        self.assertEqual(record["schema"], "airk-ci-producer/2")
        candidates.validate_record(record, COMMIT)
        self.assertEqual((record["commit"], record["tree"], record["run_id"], record["attempt"]), (COMMIT, TREE, "123", "1"))
        self.assertEqual(len(record["artifacts"]), 8)
        self.assertEqual(record["promotion"]["producer"]["source_commit"], MERGE)
        for target in candidates.PR_TARGETS:
            self.assertEqual(candidates.native_artifact(record, target), f"airk-native-{target}-{MERGE}-777-1")
        intel = candidates.MAIN_TARGETS[0]
        self.assertEqual(candidates.native_artifact(record, intel), f"airk-native-{intel}-{COMMIT}-123-1")
        with patch.object(candidates, "attest", return_value=record):
            candidates.verify_record(record, COMMIT)

    def test_promoted_main_run_rejects_changed_producers_and_missing_intel(self):
        intel_failed = [dict(name=name, conclusion="failure" if "apple" in name else "success")
                        for name in ("Required checks", "quick", "plan", f"Native / {candidates.MAIN_TARGETS[0]}")]
        promotion = self.promote()
        for name, api, record in (
                ("intel failed", promotion_api(main_jobs=intel_failed), None),
                ("intel artifact missing", promotion_api(main_artifacts=main_artifacts_fixture()[1:]), None),
                ("duplicate promotion", promotion_api(main_artifacts=main_artifacts_fixture() + main_artifacts_fixture()[-1:]), None),
                ("manual run", promotion_api(main_run={**run_fixture(), "event": "workflow_dispatch"}), None),
                ("PR artifacts replaced", promotion_api(pr_artifacts=[{**a, "digest": "sha256:" + "8" * 64} for a in pr_artifacts_fixture()]), None),
                ("other commit", None, {**promotion, "commit": "b" * 40}),
                ("other schema", None, {**promotion, "schema": "airk-ci-promotion/0"})):
            with self.subTest(name=name), self.assertRaises(ValueError):
                if name == "manual run":
                    with patch.object(candidates, "api", side_effect=api), patch.object(candidates, "download"):
                        candidates.attest(123, COMMIT, allow_manual=True)
                else:
                    self.attest_promoted(api, record or promotion)

    def test_partial_rerun_cannot_reuse_an_earlier_attempts_promotion(self):
        base = promotion_api()
        def api(path, paginate=False):
            if path == "actions/runs/123":
                return {**run_fixture(), "run_attempt": 2}
            return base(path, paginate)
        with patch.object(candidates, "api", side_effect=api), patch.object(candidates, "download") as download, \
                self.assertRaisesRegex(ValueError, "rerun all CI jobs"):
            candidates.attest(123, COMMIT)
        download.assert_not_called()

    def test_unpromoted_records_keep_the_exact_single_run_contract(self):
        record = dict(schema="airk-ci-producer/1", commit=COMMIT, run_id="123", attempt="1",
                      artifacts={f"airk-native-{t}-{COMMIT}-123-1": {} for t in candidates.TARGETS})
        self.assertEqual(candidates.native_artifact(record, candidates.TARGETS[0]), f"airk-native-{candidates.TARGETS[0]}-{COMMIT}-123-1")
        with self.assertRaises(ValueError):
            candidates.native_artifact({"artifacts": {}}, candidates.TARGETS[0])
        with self.assertRaisesRegex(ValueError, "another commit/schema"):
            candidates.verify_record({**record, "schema": "airk-ci-producer/3"}, COMMIT)


SPLIT = "1.8.24"
LINUX, WINDOWS, ARM, INTEL = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc",
                              "aarch64-apple-darwin", "x86_64-apple-darwin")
# Reviewed, checked-in split catalog names (split plan §10.5.4, docs/split/inventory/build.md A.7):
# four native pair candidates and one universal VSIX per producer run; nine same-run release inputs.
SPLIT_PRODUCER = {f"airk-native-{LINUX}-{COMMIT}-123-1", f"airk-native-{WINDOWS}-{COMMIT}-123-1",
                  f"airk-native-{ARM}-{COMMIT}-123-1", f"airk-native-{INTEL}-{COMMIT}-123-1",
                  f"airk-vsix-universal-{COMMIT}-123-1"}
SPLIT_PROMOTED = {f"airk-native-{LINUX}-{MERGE}-777-1", f"airk-native-{WINDOWS}-{MERGE}-777-1",
                  f"airk-native-{ARM}-{MERGE}-777-1", f"airk-vsix-universal-{MERGE}-777-1",
                  f"airk-native-{INTEL}-{COMMIT}-123-1"}
SPLIT_RELEASE_INPUTS = {f"airk-vsix-universal-{COMMIT}-456",
                        f"airk-native-{LINUX}-{COMMIT}-456", f"airk-source-{LINUX}-{COMMIT}-456",
                        f"airk-signed-native-{WINDOWS}-{COMMIT}-456", f"airk-source-{WINDOWS}-{COMMIT}-456",
                        f"airk-signed-native-{ARM}-{COMMIT}-456", f"airk-source-{ARM}-{COMMIT}-456",
                        f"airk-signed-native-{INTEL}-{COMMIT}-456", f"airk-source-{INTEL}-{COMMIT}-456"}
SPLIT_STABLE_ASSETS = {"airk-1.8.24.vsix", "airk-1.8.24.vsix-manifest.json",
                       *(f"airk-1.8.24-{target}{suffix}" for target in (LINUX, WINDOWS, ARM, INTEL)
                         for suffix in (".zip", ".native-manifest.json", "-installer.manifest.json")),
                       f"airk-1.8.24-{LINUX}-installer.tar.gz", f"airk-1.8.24-{WINDOWS}-installer.exe",
                       f"airk-1.8.24-{ARM}-installer.dmg", f"airk-1.8.24-{INTEL}-installer.dmg"}


def split_artifacts(names, run_id=123, head_sha=COMMIT, head_branch="main", first=1):
    return [dict(id=first + index, name=name, expired=False, digest="sha256:" + f"{index:x}" * 64,
                 workflow_run=dict(id=run_id, head_sha=head_sha, head_branch=head_branch))
            for index, name in enumerate(sorted(names))]


def split_ci_api(artifacts):
    names = ["Required checks", "container", "quick", "plan", "extension"]
    names += [f"Native / {target}" for target in (LINUX, WINDOWS, ARM, INTEL)]
    names += [f"Contracts / {host}" for host in ("ubuntu-24.04", "windows-2025", "macos-15", "macos-15-intel")]
    def respond(path, paginate=False):
        if path == "actions/runs/123":
            return run_fixture()
        if path == "actions/workflows/ci.yml":
            return dict(id=42)
        if path == "actions/runs/123/attempts/1/jobs?per_page=100":
            return [dict(jobs=[dict(name=name, conclusion="success") for name in names])]
        if path == "actions/runs/123/artifacts?per_page=100":
            return [dict(artifacts=artifacts)]
        raise AssertionError(f"Unexpected API call: {path}")
    return respond


def split_receipt_download(tree=TREE, vsix_tree=TREE):
    def write(artifact, destination):
        destination.mkdir(parents=True)
        name = next(a["name"] for a in split_artifacts(SPLIT_PROMOTED - {f"airk-native-{INTEL}-{COMMIT}-123-1"},
                                                       777, HEAD, "feature", 100) if a["id"] == artifact["id"])
        if "universal" in name:
            receipt = dict(commit=MERGE, tree=vsix_tree, target="universal", version=SPLIT)
        else:
            target = next(t for t in (LINUX, WINDOWS, ARM) if f"-{t}-" in name)
            receipt = dict(commit=MERGE, tree=tree, dirty_checkout=False, target=target, version=SPLIT)
        (destination / "manifest.json").write_text(json.dumps(receipt), encoding="utf-8")
    return write


class SplitCatalogTests(unittest.TestCase):
    def test_split_producer_is_four_native_pairs_and_one_universal_vsix(self):
        # A retained per-target VSIX name is not part of the split catalog and is ignored.
        artifacts = split_artifacts(SPLIT_PRODUCER | {f"airk-vsix-{LINUX}-{COMMIT}-123-1"})
        with patch.object(candidates, "api", side_effect=split_ci_api(artifacts)):
            record = candidates.attest(123, COMMIT, version=SPLIT)
            with self.assertRaisesRegex(ValueError, "complete artifact platform set"):
                candidates.attest(123, COMMIT)  # The pre-split catalog needs four platform VSIXs.
        self.assertEqual(set(record["artifacts"]), SPLIT_PRODUCER)
        self.assertEqual(candidates.universal_artifact(record), f"airk-vsix-universal-{COMMIT}-123-1")
        candidates.validate_record(record, COMMIT, version=SPLIT)
        with self.assertRaisesRegex(ValueError, "exactly 8 unique|digest|identity"):
            candidates.validate_record(record, COMMIT)
        with patch.object(candidates, "attest", return_value=record) as attest:
            candidates.verify_record(record, COMMIT, version=SPLIT)
            attest.assert_called_once_with("123", COMMIT, allow_manual=False, version=SPLIT)
        self.assertEqual(candidates.catalog_args("1.8.23"), {})
        self.assertEqual(candidates.catalog_args(SPLIT), {"version": SPLIT})

    def test_universal_vsix_never_replaces_native_platform_evidence(self):
        for mutation in ("missing_native", "missing_universal", "universal_attempt", "duplicate_universal",
                         "expired_universal", "universal_origin"):
            names = set(SPLIT_PRODUCER)
            if mutation == "missing_native":
                names.discard(f"airk-native-{WINDOWS}-{COMMIT}-123-1")
            elif mutation == "missing_universal":
                names = (names - {f"airk-vsix-universal-{COMMIT}-123-1"}) | {f"airk-vsix-{t}-{COMMIT}-123-1" for t in (LINUX, WINDOWS, ARM, INTEL)}
            elif mutation == "universal_attempt":
                names = (names - {f"airk-vsix-universal-{COMMIT}-123-1"}) | {f"airk-vsix-universal-{COMMIT}-123-2"}
            artifacts = split_artifacts(names)
            universal = [a for a in artifacts if a["name"] == f"airk-vsix-universal-{COMMIT}-123-1"]
            if mutation == "duplicate_universal":
                artifacts.append({**universal[0], "id": 99})
            elif mutation == "expired_universal":
                universal[0]["expired"] = True
            elif mutation == "universal_origin":
                universal[0]["workflow_run"] = dict(id=123, head_sha="b" * 40, head_branch="main")
            with self.subTest(mutation=mutation), patch.object(candidates, "api", side_effect=split_ci_api(artifacts)), \
                    self.assertRaises(ValueError):
                candidates.attest(123, COMMIT, version=SPLIT)
        with patch.object(candidates, "api", side_effect=split_ci_api(split_artifacts(SPLIT_PRODUCER))):
            record = candidates.attest(123, COMMIT, version=SPLIT)
        for change in ("drop_native", "extra_universal", "shared_id"):
            bad = copy.deepcopy(record)
            if change == "drop_native":
                del bad["artifacts"][f"airk-native-{ARM}-{COMMIT}-123-1"]
            elif change == "extra_universal":
                bad["artifacts"][f"airk-vsix-universal-{COMMIT}-123-2"] = dict(id=77, digest="sha256:" + "7" * 64)
            else:
                bad["artifacts"][f"airk-vsix-universal-{COMMIT}-123-1"]["id"] = bad["artifacts"][f"airk-native-{ARM}-{COMMIT}-123-1"]["id"]
            with self.subTest(change=change), self.assertRaises(ValueError):
                candidates.validate_record(bad, COMMIT, version=SPLIT)
        with self.assertRaisesRegex(ValueError, "exactly one universal"):
            candidates.universal_artifact({"artifacts": {f"airk-native-{LINUX}-{COMMIT}-123-1": {}}})

    def test_split_promotion_reuses_the_pr_universal_vsix_with_the_identical_tree(self):
        pr_artifacts = split_artifacts(SPLIT_PROMOTED - {f"airk-native-{INTEL}-{COMMIT}-123-1"}, 777, HEAD, "feature", 100)
        main_artifacts = [a for a in main_artifacts_fixture() if "-vsix-" not in a["name"]]
        api = promotion_api(pr_artifacts=pr_artifacts, main_artifacts=main_artifacts)
        with tempfile.TemporaryDirectory() as temporary, patch.object(candidates, "api", side_effect=api), \
                patch.object(candidates, "download", side_effect=split_receipt_download()):
            promotion = candidates.promote(COMMIT, TREE, Path(temporary), version=SPLIT)
        self.assertEqual(set(promotion["producer"]["artifacts"]), SPLIT_PROMOTED - {f"airk-native-{INTEL}-{COMMIT}-123-1"})
        def fetch(artifact, destination):
            destination.mkdir(parents=True)
            (destination / "promotion.json").write_text(json.dumps(promotion), encoding="utf-8")
        with patch.object(candidates, "api", side_effect=api), patch.object(candidates, "download", side_effect=fetch):
            record = candidates.attest(123, COMMIT, version=SPLIT)
        self.assertEqual(record["schema"], "airk-ci-producer/2")
        self.assertEqual(set(record["artifacts"]), SPLIT_PROMOTED)
        self.assertEqual(candidates.universal_artifact(record), f"airk-vsix-universal-{MERGE}-777-1")
        candidates.validate_record(record, COMMIT, version=SPLIT)
        # A universal VSIX from another tree, or a PR run without one, leaves main to build everything.
        for name, download, artifacts in (
                ("vsix tree", split_receipt_download(vsix_tree="b" * 40), pr_artifacts),
                ("native tree", split_receipt_download(tree="b" * 40), pr_artifacts),
                ("no universal", split_receipt_download(), [a for a in pr_artifacts if "universal" not in a["name"]])):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(candidates, "api", side_effect=promotion_api(pr_artifacts=artifacts)), \
                    patch.object(candidates, "download", side_effect=download), \
                    self.assertRaisesRegex(ValueError, "No eligible PR candidate"):
                candidates.promote(COMMIT, TREE, Path(temporary), version=SPLIT)

    def test_split_release_inputs_are_one_universal_vsix_and_two_per_target(self):
        self.assertEqual(candidates.release_input_names(COMMIT, 456, version=SPLIT), SPLIT_RELEASE_INPUTS)
        self.assertEqual(candidates.release_input_names(COMMIT, 456, version=SPLIT), candidates.contract.catalog_names(
            SPLIT, "stable", COMMIT, dict(run_id="123", attempt="1"), dict(run_id="456"))[1])
        self.assertEqual(len(candidates.release_input_names(COMMIT, 456)), 12)
        run = {**run_fixture(), "id": 456, "event": "workflow_dispatch", "path": ".github/workflows/github-release.yml",
               "status": "in_progress"}
        for case in (None, "per_target_vsix", "missing_source"):
            names = set(SPLIT_RELEASE_INPUTS)
            if case == "per_target_vsix":
                names = (names - {f"airk-vsix-universal-{COMMIT}-456"}) | {f"airk-vsix-{LINUX}-{COMMIT}-456"}
            elif case == "missing_source":
                names.discard(f"airk-source-{ARM}-{COMMIT}-456")
            artifacts = split_artifacts(names, run_id=456)
            def api(path, paginate=False):
                if path == "actions/runs/456":
                    return run
                if path == "actions/runs/456/artifacts?per_page=100":
                    return [dict(artifacts=artifacts)]
                raise AssertionError(path)
            def download(artifact, destination):
                destination.mkdir(parents=True)
                (destination / "source.json").write_text("{}", encoding="utf-8")
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(candidates, "api", side_effect=api), \
                    patch.object(candidates, "download", side_effect=download) as fetched:
                output = Path(temporary) / "inputs"
                if case:
                    with self.assertRaisesRegex(ValueError, "exactly 9 unique"):
                        candidates.release_inputs(COMMIT, 456, output, version=SPLIT)
                    fetched.assert_not_called()
                else:
                    candidates.release_inputs(COMMIT, 456, output, version=SPLIT)
                    self.assertEqual({path.name for path in output.iterdir()}, SPLIT_RELEASE_INPUTS)

    def test_split_downloads_select_the_universal_vsix_once_and_never_per_target(self):
        record = dict(artifacts={name: dict(id=index + 1, digest="sha256:" + "1" * 64)
                                 for index, name in enumerate(sorted(SPLIT_PRODUCER))})
        self.assertEqual(candidates.download_names(record, "vsix", version=SPLIT), [f"airk-vsix-universal-{COMMIT}-123-1"])
        self.assertEqual(candidates.download_names(record, "native", WINDOWS, version=SPLIT),
                         [f"airk-native-{WINDOWS}-{COMMIT}-123-1"])
        with self.assertRaisesRegex(ValueError, "without --target"):
            candidates.download_names(record, "vsix", WINDOWS, version=SPLIT)
        with self.assertRaisesRegex(ValueError, "no single matching"):
            candidates.download_names(record, "vsix", WINDOWS)  # Pre-split selection finds no platform VSIX.

    def test_split_tag_verifies_exactly_the_split_release_catalog(self):
        producer = dict(schema="airk-ci-producer/1", commit=COMMIT, run_id="123", attempt="1")
        def check(assets, record_changes=None):
            record = {"schema": "airk-github-release/1", "tag": "v1.8.24", "package_version": SPLIT,
                      "commit": COMMIT, "channel": "stable", "run_id": "456", "attempt": "1",
                      "producer": producer, "assets": {name: "0" * 64 for name in assets}, **(record_changes or {})}
            data = json.dumps(record).encode()
            published = [dict(id=index + 1, name=name, size=1, digest="sha256:" + "0" * 64) for index, name in enumerate(assets)]
            published.append(dict(id=900, name="release.json", size=len(data), digest="sha256:" + hashlib.sha256(data).hexdigest()))
            published += [dict(id=901, name="release-notes.md", size=1, digest="sha256:" + "1" * 64),
                          dict(id=902, name="SHA256SUMS", size=1, digest="sha256:" + "2" * 64)]
            def api(path, paginate=False):
                if path == "releases/tags/v1.8.24":
                    return dict(draft=False, tag_name="v1.8.24", assets=published)
                if path == "git/ref/tags/v1.8.24":
                    return dict(object=dict(sha=COMMIT))
                raise AssertionError(path)
            with patch.object(candidates, "api", side_effect=api), \
                    patch.object(candidates.subprocess, "check_output", return_value=data), \
                    patch.object(candidates, "verify_record") as verified:
                candidates.verify_tag("v1.8.24", COMMIT)
                verified.assert_called_once_with(producer, COMMIT, version=SPLIT)
        check(SPLIT_STABLE_ASSETS)
        pre_split = {f"airk-1.8.24-{target}.{suffix}" for target in (LINUX, WINDOWS, ARM, INTEL)
                     for suffix in ("zip", "native-manifest.json")}
        pre_split |= {f"airk-1.8.24-{platform}.vsix{suffix}" for platform in ("linux-x64", "win32-x64", "darwin-arm64", "darwin-x64")
                      for suffix in ("", "-manifest.json")}
        for name, assets, changes in (("platform VSIX catalog", pre_split, None),
                                      ("missing installer", SPLIT_STABLE_ASSETS - {f"airk-1.8.24-{ARM}-installer.dmg"}, None),
                                      ("other version", SPLIT_STABLE_ASSETS, {"package_version": "1.8.25"})):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "split release catalog|another package version"):
                check(assets, changes)


if __name__ == "__main__":
    unittest.main()
