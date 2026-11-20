# SPDX-License-Identifier: Apache-2.0
"""Focused controls for source and shared-library archive membership verification."""
import hashlib
import os
from pathlib import Path
import stat
import tempfile
import unittest

from configure_ort import BASE
from source_seal import match
from unpack import guarded
from verified_inputs import identity, verify


class SourceAuthentication(unittest.TestCase):
    def setUp(self):
        self.uid = identity()
        guarded(BASE, self.uid, stat.S_ISDIR)
        self.directory = tempfile.TemporaryDirectory(dir=BASE)
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        guarded(self.root, self.uid, stat.S_ISDIR)
        self.path = self.root / "libexample.so.1"
        self.path.write_bytes(b"original")
        guarded(self.path, self.uid, stat.S_ISREG)
        self.sha = hashlib.sha256(b"original").hexdigest()
        self.rows = {self.path.name: {"sha256": self.sha, "bytes": 8}}

    def test_same_size_cuda_change(self):
        verify(self.root, self.rows)
        self.path.write_bytes(b"replaced")
        with self.assertRaisesRegex(ValueError, "bytes changed"):
            verify(self.root, self.rows)

    def test_symlink_resolves_only_to_declared_member(self):
        symlink = self.root / "libexample.so"
        symlink.symlink_to(self.path.name)
        guarded(symlink, self.uid, stat.S_ISLNK)
        self.rows[symlink.name] = {"link": self.path.name}
        verify(self.root, self.rows)
        self.rows[symlink.name] = {"link": "../external"}
        with self.assertRaisesRegex(ValueError, "symlink changed"):
            verify(self.root, self.rows)

    def test_missing_and_extra_sources_fail(self):
        two = {self.path.name: self.sha, "extra.dat": self.sha}
        with self.assertRaisesRegex(ValueError, "roster changed"):
            match(self.root, two)
        self.assertEqual(match(self.root, {self.path.name: self.sha})["files"], 1)
        with self.assertRaisesRegex(ValueError, "changed compiled source"):
            match(self.root, {})


if __name__ == "__main__":
    os.umask(0o077)
    unittest.main()
