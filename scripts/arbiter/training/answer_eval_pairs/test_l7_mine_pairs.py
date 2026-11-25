# SPDX-License-Identifier: Apache-2.0
"""Synthetic admission and read-boundary tests for the L7 pair miner."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import sys
import unittest
from unittest.mock import Mock, mock_open, patch

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import l7_policy as policy
import mine_pairs as miner


REV = "1" * 40
SKIP = "2" * 40
ALLOW = "3" * 40
CUTOFF = "2026-01-05T00:00:00Z"
PIN = Path("synthetic-inventory.json")


def signed(document) -> tuple[bytes, str]:
    data = json.dumps(document).encode()
    return data, hashlib.sha256(data).hexdigest()


def inventory(**repos):
    return {"schema": policy.PROTECTED_SCHEMA, "paths": repos}


class ProtectedPaths(unittest.TestCase):
    def admit(self, document):
        data, sha256 = signed(document)
        with patch.object(Path, "open", mock_open(read_data=data)):
            return policy.load_protected_paths(PIN, sha256)

    def test_inventory_demands_both_repositories_and_exact_hash(self):
        data, digest = signed(inventory(bookbuilder=["src/Private.py"], munarium=["src/held.py"]))
        with patch.object(Path, "open", mock_open(read_data=data)):
            paths = policy.load_protected_paths(PIN, digest)
            self.assertEqual(paths["bookbuilder"], {"src/private.py"})
            with self.assertRaisesRegex(ValueError, "SHA-256 differs"):
                policy.load_protected_paths(PIN, "0" * 64)
        for invalid in (inventory(munarium=["src/held.py"]),
                        inventory(bookbuilder=[], munarium=["src/held.py"]),
                        inventory(bookbuilder=["src/x", "SRC/X"], munarium=["src/held.py"]),
                        inventory(bookbuilder=["src/../held"], munarium=["src/held.py"]),
                        inventory(bookbuilder=["C:/held"], munarium=["src/held.py"]),
                        inventory(bookbuilder=["src\\held"], munarium=["src/held.py"])):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                self.admit(invalid)

    def test_reads_the_owning_fixture_denies(self):
        denies = policy.bookbuilder_denies()
        self.assertEqual(len(denies), 2)
        self.assertTrue(policy.denied_by_fixture("IRONSPIRE/scripts/set-rss-key.ps1", denies))
        self.assertTrue(policy.denied_by_fixture("ironspire/scripts/set-sendgrid-key.ps1", denies))
        self.assertFalse(policy.denied_by_fixture("other/scripts/set-rss-key.ps1", denies))


class PathOnlyGit(unittest.TestCase):
    def setUp(self):
        self.exclude = miner.l7_excluder("bookbuilder",
            {"bookbuilder": frozenset({"src/protected.py"})}, policy.bookbuilder_denies())

    def test_mixed_protected_commit_never_requests_patch_numstat_or_message(self):
        raw_header = f":000000 100644 {'0' * 40} {'5' * 40} A"
        protected = "SRC/Protected.py"
        allowed = "src/available.py"

        def run(*args):
            if args[0] == "log":
                self.assertEqual(args[-2:], (REV, "--"))
                self.assertNotIn("--numstat", args)
                return f"{SKIP}\n{ALLOW}\n"
            if args[0] == "diff-tree":
                if SKIP in args:
                    return f"{raw_header}\0{protected}\0{raw_header}\0{allowed}\0"
                self.assertIn(ALLOW, args)
                return f"{raw_header}\0{allowed}\0"
            self.assertEqual(args[0], "show")
            self.assertNotIn(SKIP, args)
            self.assertIn(ALLOW, args)
            self.assertNotIn(protected, args)
            if "--numstat" in args:
                self.assertIn("--no-textconv", args)
                self.assertEqual(args[-2:], ("--", allowed))
                return f"5\t0\t{allowed}\0"
            if "-U0" in args:
                self.assertIn("--no-textconv", args)
                self.assertEqual(args[-2:], ("--", allowed))
                return f"+++ b/{allowed}\n@@ -0,0 +1,5 @@\n"
            self.assertIn("%B", args[-3])
            return "Implement daily event timeline output for user reports"

        git = Mock()
        git.run.side_effect = run
        git.run_bytes.side_effect = lambda *args: run(*args).encode("utf-8")
        args = SimpleNamespace(before=CUTOFF, max_files=40, min_message_chars=1, min_message_words=1,
                               max_added_lines=100, max_evidence=10, max_instruction_chars=120)
        stats = miner.Stats(repo="bookbuilder")
        pairs = miner.mine_commits("bookbuilder", git, args, stats, revision=REV, exclude=self.exclude)
        self.assertEqual([pair["commit"] for pair in pairs], [ALLOW])
        self.assertEqual(pairs[0]["evidence"], [{"path": allowed, "lines": [1, 5]}])
        self.assertEqual(stats.excluded["bookbuilder:commit_file:protected_path"], 1)

    def test_admitted_legacy_encodings_do_not_abort_valid_history(self):
        allowed = "src/available.py"
        header = f":000000 100644 {'0' * 40} {'5' * 40} A"

        def raw_git(command, **_kwargs):
            args = command[6:]
            if args[0] == "log":
                output = f"{SKIP}\n{ALLOW}\n".encode()
            elif args[0] == "diff-tree":
                output = f"{header}\0{allowed}\0".encode()
            elif "--numstat" in args:
                self.assertEqual(args[-2:], ["--", allowed])
                output = f"2\t0\t{allowed}\0".encode()
            elif "--format=%B" in args:
                output = (b"Implement daily event timeline \xff" if SKIP in args
                          else b"Implement daily event timeline output for user reports")
            else:
                self.assertIn("-U0", args)
                self.assertIn(ALLOW, args)
                self.assertEqual(args[-2:], ["--", allowed])
                output = f"+++ b/{allowed}\n@@ -0,0 +4,2 @@\n".encode() + b"+old \xff text\n+new text\n"
            return SimpleNamespace(returncode=0, stdout=output, stderr=b"")

        args = SimpleNamespace(before=CUTOFF, max_files=40, min_message_chars=1, min_message_words=1,
                               max_added_lines=100, max_evidence=10, max_instruction_chars=120)
        stats = miner.Stats(repo="bookbuilder")
        with patch.object(miner.subprocess, "run", side_effect=raw_git):
            git = miner.Git(Path("synthetic-bookbuilder"), l7=True)
            pairs = miner.mine_commits("bookbuilder", git, args, stats, revision=REV, exclude=self.exclude)

        self.assertEqual([pair["commit"] for pair in pairs], [ALLOW])
        self.assertEqual(pairs[0]["evidence"], [{"path": allowed, "lines": [4, 5]}])
        self.assertEqual(stats.excluded["bookbuilder:commit:invalid_message_encoding"], 1)

    def test_snapshot_filters_before_blob_and_rejects_direct_followup_access(self):
        git = Mock(l7=True)
        rows = (("100644", "src/available.py"), ("100644", "src/protected.py"),
                ("100644", "ironspire/scripts/set-rss-key.ps1"), ("120000", "src/link.py"))
        git.run.return_value = "".join(f"{mode} blob {'4' * 40}\t{path}\0" for mode, path in rows)
        git.blob.return_value = b"def available():\n    return 1\n"
        snapshot = miner.Snapshot("bookbuilder", git, REV, miner.Stats(), exclude=self.exclude)
        self.assertEqual(snapshot.paths, ["src/available.py"])
        for _, path in rows[1:]:
            self.assertIsNone(snapshot.text(path))
        git.run.assert_called_once_with("ls-tree", "-r", "--full-tree", "-z", REV, "--")
        git.blob.assert_called_once_with(REV, "src/available.py")

    def test_exact_revision_cli_rejects_ref_and_abbreviation(self):
        self.assertEqual(miner.revision_arg(f"bookbuilder={REV}"), ("bookbuilder", REV))
        for value in ("bookbuilder=HEAD", "munarium=abcdef0", f"airk={REV}", f"bookbuilder={REV}^"):
            with self.subTest(value=value), self.assertRaises(miner.argparse.ArgumentTypeError):
                miner.revision_arg(value)


if __name__ == "__main__":
    unittest.main()
