# SPDX-License-Identifier: Apache-2.0
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release_repair", Path(__file__).parents[1] / "release-1.4.4-repair.py")
repair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair)
TOOLS = "b" * 40


def evidence():
    return {"schema": "airk-release-tooling-repair/1", "source_commit": repair.SOURCE,
            "tooling_commit": TOOLS, "producer_run_id": repair.RUN, "producer_attempt": "1",
            "runtime_source": repair.RUNTIME, "changed_paths": ["scripts/github-release.py"]}


def producer():
    return {"schema": "airk-ci-producer/1", "commit": repair.SOURCE, "run_id": repair.RUN, "attempt": "1"}


class RepairContracts(unittest.TestCase):
    def test_every_repair_workflow_entry_point_has_scoped_api_authentication(self):
        workflow = (repair.ROOT / ".github/workflows/github-release.yml").read_text(encoding="utf-8")
        commands = set()
        for step in workflow.split("\n      - "):
            for line in step.splitlines():
                if "run: python scripts/" in line and "release-1.4.4-repair.py" in line:
                    command = re.search(r"\s(preflight|download|stage-signing|assemble|bundle|publish)(?:\s|$)", line)
                    self.assertIsNotNone(command, line)
                    commands.add(command[1])
                    self.assertIn("GH_TOKEN: ${{ github.token }}", step, command[1])
        self.assertEqual(commands, {"preflight", "download", "stage-signing", "assemble", "bundle", "publish"})

    def test_complete_allowlist_and_exact_runtime_inputs_are_required(self):
        repair.validate_scope(["scripts/github-release.py", "docs/release-1.4.4.md"], "1.4.4", repair.RUNTIME)
        for path in ("Cargo.toml", "Cargo.lock", "crates/airk-cli/src/main.rs", "packages/vscode/README.md",
                     "packages/vscode/src/native.ts", "scripts/sign-native.py", "scripts/package-native.py",
                     "scripts/package-extension.py", ".github/workflows/ci.yml", "unknown", "../README.md"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                repair.validate_scope(["scripts/github-release.py", path], "1.4.4", repair.RUNTIME)
        for paths, version, runtime in (([], "1.4.4", repair.RUNTIME),
                                        (["README.md"], "1.4.5", repair.RUNTIME),
                                        (["README.md"], "1.4.4", {**repair.RUNTIME, "sha256": "0" * 64}),
                                        (["README.md"], "1.4.4", {**repair.RUNTIME, "files": 665})):
            with self.subTest(paths=paths, version=version, runtime=runtime), self.assertRaises(ValueError):
                repair.validate_scope(paths, version, runtime)

    def test_scope_requires_ancestor_clean_tree_and_cumulative_no_rename_diff(self):
        for mutation in (None, "nonancestor", "dirty", "runtime", "version", "renamed", "unrelated"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "packages/vscode").mkdir(parents=True)
                (root / "packages/vscode/package.json").write_text(json.dumps({"version": "1.4.5" if mutation == "version" else "1.4.4"}))
                calls = []
                def git(*args):
                    calls.append(args)
                    if args == ("rev-parse", "HEAD"):
                        return TOOLS.encode()
                    if args == ("merge-base", repair.SOURCE, TOOLS):
                        return ("c" * 40 if mutation == "nonancestor" else repair.SOURCE).encode()
                    if args == ("status", "--porcelain"):
                        return b" M scripts/github-release.py" if mutation == "dirty" else b""
                    self.assertEqual(args, ("diff", "--name-only", "--no-renames", "-z", repair.SOURCE, TOOLS))
                    return {"renamed": b"crates/removed.rs\0docs/release-1.4.4.md\0",
                            "unrelated": b"scripts/github-release.py\0new-file\0"}.get(mutation, b"scripts/github-release.py\0")
                builder = repair.release.native.runtime_builder()
                runtime = {**repair.RUNTIME, "sha256": "0" * 64} if mutation == "runtime" else repair.RUNTIME
                with patch.object(repair, "ROOT", root), patch.object(repair, "git", side_effect=git), \
                        patch.object(repair.release.native, "runtime_builder", return_value=builder), \
                        patch.object(builder, "source_identity", return_value=runtime):
                    if mutation:
                        with self.assertRaises(ValueError):
                            repair.scope()
                    else:
                        self.assertEqual(repair.scope(), evidence())

    def test_context_keeps_real_workflow_sha_and_refuses_wrong_channel_or_version(self):
        for tooling, version, channel in ((TOOLS, "1.4.4", "stable"), (repair.SOURCE, "1.4.4", "stable"),
                                           (TOOLS, "1.4.5", "stable"), (TOOLS, "1.4.4", "preview")):
            with self.subTest(tooling=tooling, version=version, channel=channel), \
                    patch.object(repair.release, "context", return_value=(tooling, version, "v" + version)) as context, \
                    patch.object(repair, "scope", return_value=evidence()), \
                    patch.dict(os.environ, {"GITHUB_SHA": TOOLS, "AIRK_CHANNEL": channel}):
                if (tooling, version, channel) == (TOOLS, "1.4.4", "stable"):
                    self.assertEqual(repair.context(), evidence())
                else:
                    with self.assertRaises(ValueError):
                        repair.context()
                context.assert_called_once()
                self.assertEqual(os.environ["GITHUB_SHA"], TOOLS)

    def test_only_original_producer_attempt_and_unchanged_preflight_record_are_admitted(self):
        for field, value in ((None, None), ("run_id", "1"), ("attempt", "2"), ("commit", TOOLS)):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "artifacts").mkdir()
                record = producer()
                if field:
                    record[field] = value
                (root / "artifacts/producer.json").write_text(json.dumps(record))
                (root / "artifacts/repair.json").write_text(json.dumps(evidence()))
                with patch.object(repair, "ROOT", root), patch.object(repair.producers, "verify_record") as verified:
                    if field:
                        with self.assertRaises(ValueError):
                            repair.recorded(evidence())
                        verified.assert_not_called()
                    else:
                        self.assertEqual(repair.recorded(evidence()), record)
                        verified.assert_called_once_with(record, repair.SOURCE)
                    with self.assertRaises(ValueError):
                        repair.recorded({**evidence(), "tooling_commit": "c" * 40})

    def test_signing_requires_clean_separate_original_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / "artifacts/repair-source"
            source.mkdir(parents=True)
            for commit, status in ((repair.SOURCE, b""), (TOOLS, b""), (repair.SOURCE, b" M scripts/sign-native.py")):
                def git(*args, directory):
                    self.assertEqual(directory, source)
                    return commit.encode() if args == ("rev-parse", "HEAD") else status
                with self.subTest(commit=commit, status=status), patch.object(repair, "ROOT", root), patch.object(repair, "git", side_effect=git):
                    if commit == repair.SOURCE and not status:
                        self.assertEqual(repair.original_checkout(), source)
                    else:
                        with self.assertRaises(ValueError):
                            repair.original_checkout()

    def test_preflight_never_allocates_version_or_changes_source_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "outputs"
            with patch.object(repair, "ROOT", root), patch.object(repair, "context", return_value=evidence()), \
                    patch.object(repair.release, "require_unused_tag") as unused, \
                    patch.object(repair.producers, "attest", return_value=producer()) as attest, \
                    patch.dict(os.environ, {"GITHUB_OUTPUT": str(output), "AIRK_PUBLISH": "true", "AIRK_CANDIDATE_RUN_ID": ""}), \
                    patch("sys.argv", ["repair", "preflight"]):
                repair.main()
                attest.assert_called_once_with(repair.RUN, repair.SOURCE)
                unused.assert_called_once_with("v1.4.4")
                self.assertIn("source_commit=" + repair.SOURCE, output.read_text())
                self.assertIn("action=publish", output.read_text())
                self.assertEqual(json.loads((root / "artifacts/producer.json").read_text()), producer())
                self.assertEqual(json.loads((root / "artifacts/repair.json").read_text()), evidence())

    def test_resume_authenticates_tooling_run_but_retains_source_tag_and_receipts(self):
        for command in ("bundle", "publish"):
            with self.subTest(command=command), patch.object(repair, "context", return_value=evidence()), \
                    patch.object(repair.producers, "release_bundle") as restore, \
                    patch.object(repair, "verify_repair_bundle") as verify, \
                    patch.object(repair.release, "publish") as publish, \
                    patch.dict(os.environ, {"GITHUB_SHA": TOOLS, "GITHUB_RUN_ID": "456", "GITHUB_RUN_ATTEMPT": "2"}), \
                    patch("sys.argv", ["repair", command, "--output", "bundle", "--directory", "bundle", "--resume"]):
                repair.main()
                verify.assert_called_once_with(Path("bundle"), evidence())
                if command == "bundle":
                    restore.assert_called_once_with(TOOLS, "456", "2", Path("bundle"))
                    publish.assert_not_called()
                else:
                    publish.assert_called_once_with(Path("bundle"), resume=True, source_commit=repair.SOURCE)
                    restore.assert_not_called()
                self.assertEqual(os.environ["GITHUB_SHA"], TOOLS)

    def test_bundle_cannot_cross_tooling_revision_producer_or_source(self):
        record = {"release_tooling_repair": evidence(), "producer": producer(), "attempt": "1"}
        for mutation in (None, "tooling", "producer"):
            altered = copy.deepcopy(record)
            if mutation == "tooling":
                altered["release_tooling_repair"]["tooling_commit"] = "c" * 40
            elif mutation == "producer":
                altered["producer"]["commit"] = TOOLS
            with self.subTest(mutation=mutation), patch.object(repair.release.extension, "read_receipt", return_value=altered), \
                    patch.object(repair, "producer", return_value=producer()), \
                    patch.object(repair.release, "verify_bundle") as verify, \
                    patch.dict(os.environ, {"GITHUB_RUN_ID": "456"}):
                if mutation:
                    with self.assertRaises(ValueError):
                        repair.verify_repair_bundle(Path("bundle"), evidence())
                    verify.assert_not_called()
                else:
                    repair.verify_repair_bundle(Path("bundle"), evidence())
                    verify.assert_called_once_with(Path("bundle"), repair.SOURCE, "1.4.4", "v1.4.4", "456", "1")


if __name__ == "__main__":
    unittest.main()
