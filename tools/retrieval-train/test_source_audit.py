# SPDX-License-Identifier: Apache-2.0
import hashlib
from pathlib import Path
import tempfile
import unittest

from audit_sources import audit_pass


class SourceAuditTests(unittest.TestCase):
    def test_closed_roster_refuses_extra_and_same_length_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'sources').mkdir()
            original = root / 'sources' / 'paper.txt'; original.write_bytes(b'original')
            manifest = {'files': [{'file': 'sources/paper.txt', 'bytes': 8,
                                   'sha256': hashlib.sha256(b'original').hexdigest()}]}
            self.assertEqual(len(audit_pass(root, manifest)), 1)
            extra = root / 'sources' / 'extra.txt'; extra.write_bytes(b'')
            with self.assertRaisesRegex(ValueError, 'roster differs'):
                audit_pass(root, manifest)
            extra.unlink(); original.write_bytes(b'replaced')
            with self.assertRaisesRegex(ValueError, 'bytes changed'):
                audit_pass(root, manifest)

    def test_missing_source_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'sources').mkdir()
            manifest = {'files': [{'file': 'sources/missing.txt', 'bytes': 0,
                                   'sha256': hashlib.sha256(b'').hexdigest()}]}
            with self.assertRaisesRegex(ValueError, 'roster differs'):
                audit_pass(root, manifest)


if __name__ == '__main__':
    unittest.main()
