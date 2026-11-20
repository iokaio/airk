# SPDX-License-Identifier: Apache-2.0
"""Portable controls for the signed GNU scientific source selection."""
import hashlib
import io
import tarfile
import unittest

from joint_source import append_archive


class Archive:
    def __init__(self, files, commit="original"):
        out = io.BytesIO()
        with tarfile.open(fileobj=out, mode="w:", pax_headers={"comment": commit}) as tar:
            for name, payload in files:
                member = tarfile.TarInfo(name)
                member.size = len(payload)
                tar.addfile(member, io.BytesIO(payload))
        self.data = out.getvalue()
        self.sha = hashlib.sha256(self.data).hexdigest()

    def open(self, mode):
        assert mode == "rb"
        return io.BytesIO(self.data)


class JointSource(unittest.TestCase):
    def test_signed_delta_changes_only_selected_rows(self):
        base = Archive([("Cargo.toml", b"manifest"), ("crates/a.rs", b"a")])
        delta = Archive([("crates/a.rs", b"aa"), ("crates/b.rs", b"b")], "new")
        rows = {}
        self.assertEqual(2, append_archive(rows, base, base.sha, "original", delta=False))
        self.assertEqual(2, append_archive(rows, delta, delta.sha, "new", delta=True))
        self.assertEqual(rows, {
            path: (len(data), hashlib.sha256(data).hexdigest())
            for path, data in [("Cargo.toml", b"manifest"), ("crates/a.rs", b"aa"), ("crates/b.rs", b"b")]
        })

    def test_rejects_changed_bytes_or_commit(self):
        valid = Archive([("crates/a.rs", b"a")])
        for digest, commit in [("0" * 64, "original"), (valid.sha, "somebody-else")]:
            with self.subTest(digest=digest), self.assertRaises(ValueError):
                append_archive({}, valid, digest, commit, delta=True)

    def test_rejects_out_of_scope_or_duplicate_delta_entries(self):
        for names in [["Cargo.toml"], ["crates/../Cargo.lock"], ["crates/a.rs", "crates/a.rs"]]:
            invalid = Archive([(name, b"source") for name in names])
            with self.subTest(names=names), self.assertRaises(ValueError):
                append_archive({}, invalid, invalid.sha, "original", delta=True)
