# SPDX-License-Identifier: Apache-2.0
"""Synthetic protected-corpus checks without model, Git child or private input."""

import hashlib
import io
import json
import runpy
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import build_examples as examples
import corpus_admission as admission


PINNED_COMMIT = "a" * 40
PROTECTION = {
    "schema": "airk.private.answer-eval.protected-paths/1",
    "paths": {"bookbuilder": ["docs/Protected.md"], "munarium": ["src/Excluded.rs"]},
}


def encode(value):
    return json.dumps(value, ensure_ascii=False).encode("utf-8")


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def checkouts():
    return ["bookbuilder=synthetic/bookbuilder", "munarium=synthetic/munarium"]


def pair(repo, path="src/allowed.rs", **extra):
    return {"repo": repo, "commit": PINNED_COMMIT, "split": "train", "evidence": [{"path": path}], **extra}


class CorpusAdmissionTests(unittest.TestCase):
    def files(self, bookbuilder=None):
        root = Path.cwd() / "synthetic-corpus"
        paths = root / "protected.json"
        pairs = {"bookbuilder": encode(bookbuilder or pair("bookbuilder")),
                 "munarium": encode(pair("munarium"))}
        files = {paths: encode(PROTECTION), **{root / f"{repo}.jsonl": raw for repo, raw in pairs.items()}}
        policy = runpy.run_path(str(admission.POLICY))
        deny_sha = policy["deny_policy_sha256"](policy["bookbuilder_denies"]())
        stats = {"schema": "airk.private.answer-eval-pairs/1",
                 "mine": {"mode": "l7", "protected_paths_sha256": sha(files[paths]),
                          "bookbuilder_deny_policy_sha256": deny_sha,
                          "repos": {repo: {"head": PINNED_COMMIT, "snapshot_commit": PINNED_COMMIT} for repo in pairs}},
                 "pair_sha256": {repo: sha(raw) for repo, raw in pairs.items()}}
        files[root / "stats.json"] = encode(stats)
        for key, relative in admission.REPLAY_FILES.items():
            files[root / relative] = encode(["independent", key])
        return root, paths, files

    def mock_files(self, files):
        real_open, real_file = Path.open, Path.is_file

        def open_file(path, *args, **kwargs):
            if path in files:
                return io.BytesIO(files[path])
            return real_open(path, *args, **kwargs)

        return (patch.object(Path, "open", open_file),
                patch.object(Path, "is_file", lambda path: path in files or real_file(path)))

    def test_both_pinned_repositories_and_separate_general_replay(self):
        root, paths, files = self.files()
        first, second = self.mock_files(files)
        with first, second:
            pins = {}
            repos, _, blocked, admitted = admission.admit_local(
                root, checkouts(), sha(files[root / "stats.json"]), paths, sha(files[paths]), pins)
            self.assertEqual(set(repos), admission.REPOSITORIES)
            self.assertEqual(set(admitted), admission.REPOSITORIES)
            self.assertTrue(blocked["bookbuilder"]("DOCS/protected.MD"))
            self.assertTrue(blocked["bookbuilder"]("ironspire/scripts/SET-SENDGRID-KEY.ps1"))
            self.assertTrue(blocked["munarium"]("src/excluded.rs"))
            self.assertFalse(blocked["bookbuilder"]("src/excluded.rs"))
            self.assertTrue(blocked["munarium"]("../src/allowed.rs"))
            replay_pins = [f"{name}={sha(files[root / relative])}" for name, relative in admission.REPLAY_FILES.items()]
            replay = admission.admit_replay(root, replay_pins, pins)
            self.assertEqual(json.loads(replay["roster"]), ["independent", "roster"])
            self.assertIn(str(paths.resolve()), pins)
            self.assertIn(str((root / admission.REPLAY_FILES["census"]).resolve()), pins)
            with self.assertRaisesRegex(ValueError, "requires exactly"):
                admission.admit_replay(root, replay_pins[:-1], {})

    def test_tamper_and_missing_repository_refuse_before_returning_pairs(self):
        root, paths, files = self.files()
        first, second = self.mock_files(files)
        with first, second:
            with self.assertRaisesRegex(ValueError, "requires exactly"):
                admission.admit_local(root, checkouts()[:1], "0" * 64, paths, sha(files[paths]), {})
            with self.assertRaisesRegex(ValueError, "inventory.*SHA-256 differs"):
                admission.admit_local(root, checkouts(), sha(files[root / "stats.json"]), paths, "0" * 64, {})
            files[root / "munarium.jsonl"] += b" "
            with self.assertRaisesRegex(ValueError, "SHA-256 differs: munarium.jsonl"):
                admission.admit_local(root, checkouts(), sha(files[root / "stats.json"]), paths, sha(files[paths]), {})

    def test_mined_receipt_refuses_wrong_deny_semantics(self):
        root, paths, files = self.files()
        stat_path = root / "stats.json"
        stats = json.loads(files[stat_path])
        stats["mine"]["bookbuilder_deny_policy_sha256"] = "0" * 64
        files[stat_path] = encode(stats)
        first, second = self.mock_files(files)
        with first, second:
            with self.assertRaisesRegex(ValueError, "differs from the current Bookbuilder deny policy"):
                admission.admit_local(root, checkouts(), sha(files[stat_path]), paths, sha(files[paths]), {})

    def test_protected_evidence_and_instruction_paths_refuse_admitted_pairs(self):
        for bookbuilder in (pair("bookbuilder", "docs/PROTECTED.md"),
                            pair("bookbuilder", source={"path": "docs/Protected.md"}),
                            pair("bookbuilder", source={"path": "ironspire/scripts/set-rss-key.ps1"})):
            root, paths, files = self.files(bookbuilder)
            first, second = self.mock_files(files)
            with self.subTest(bookbuilder=bookbuilder), first, second:
                with self.assertRaisesRegex(ValueError, "protected source or evidence path"):
                    admission.admit_local(root, checkouts(), sha(files[root / "stats.json"]), paths, sha(files[paths]), {})

    def test_repository_blocks_protected_positive_and_negative_source_reads(self):
        miner, _ = examples.load_miner()
        matcher = Mock(side_effect=lambda path: path.casefold() == "src/private.md")
        with patch.object(miner, "Git") as create:
            git = create.return_value
            git.blob.return_value = b"allowed visible text"
            git.run.return_value = "".join(
                f"{mode} {kind} {PINNED_COMMIT}\t{path}\0" for mode, kind, path in (
                    ("100644", "blob", "src/PRIVATE.md"),
                    ("100644", "blob", "src/allowed.rs"),
                    ("120000", "blob", "src/linked.rs"),
                    ("160000", "commit", "src/module.rs"),
                    ("100644", "blob", "src/../redirected.rs"),
                )
            )
            checkout = Path("fake")
            repository = examples.Repository(miner, "bookbuilder", checkout, PINNED_COMMIT, protected=matcher)
            create.assert_called_once_with(checkout, l7=True)
            self.assertIsNone(repository.lines(PINNED_COMMIT, "src/Private.md"))
            self.assertIsNone(repository.lines(PINNED_COMMIT, "src/../redirected.rs"))
            git.run.assert_not_called()
            git.blob.assert_not_called()
            windows = repository.snapshot_windows()
            self.assertEqual([row["path"] for row in windows], ["src/allowed.rs"])
            git.run.assert_called_once_with("ls-tree", "-r", "--full-tree", "-z", PINNED_COMMIT, "--")
            git.blob.assert_called_once_with(PINNED_COMMIT, "src/allowed.rs")
            self.assertIsNone(repository.lines(PINNED_COMMIT, "src/linked.rs"))
            self.assertIsNone(repository.lines(PINNED_COMMIT, "src/module.rs"))
            git.blob.assert_called_once_with(PINNED_COMMIT, "src/allowed.rs")
            legacy = examples.Repository(miner, "bookbuilder", checkout, PINNED_COMMIT)
            create.assert_called_with(checkout)
            self.assertTrue(legacy.readable("src/Private.md"))

    def test_strict_historical_reads_resolve_exact_regular_blob(self):
        miner, _ = examples.load_miner()
        historic = "b" * 40
        with patch.object(miner, "Git") as create:
            git = create.return_value
            repository = examples.Repository(miner, "bookbuilder", Path("fake"), PINNED_COMMIT,
                                             protected=lambda _path: False)
            git.run.side_effect = [
                f"120000 blob {PINNED_COMMIT}\tsrc/linked.rs\0",
                f"100755 blob {PINNED_COMMIT}\tsrc/actual.rs\0",
            ]
            git.blob.return_value = b"indexed source"
            self.assertIsNone(repository.lines(historic, "src/linked.rs"))
            git.blob.assert_not_called()
            self.assertEqual(repository.lines(historic, "src/actual.rs"), ["indexed source"])
            git.run.assert_called_with("ls-tree", "-r", "--full-tree", "-z", historic, "--", "src/actual.rs")
            git.blob.assert_called_once_with(historic, "src/actual.rs")
            with self.assertRaisesRegex(ValueError, "exact Git commit"):
                repository.lines("untrusted-treeish", "src/actual.rs")
            self.assertEqual(git.run.call_count, 2)

    def test_malformed_snapshot_listing_is_refused_before_any_blob(self):
        miner, _ = examples.load_miner()
        with patch.object(miner, "Git") as create:
            git = create.return_value
            repository = examples.Repository(miner, "bookbuilder", Path("fake"), PINNED_COMMIT,
                                             protected=lambda _path: False)
            for malformed in ("100644 blob file.rs", "nonsense\tsrc/file.rs\0"):
                with self.subTest(malformed=malformed):
                    git.run.return_value = malformed
                    with self.assertRaisesRegex(ValueError, "malformed Focus Git tree"):
                        repository.snapshot_windows()
            git.blob.assert_not_called()
