# SPDX-License-Identifier: Apache-2.0
"""Pure provenance controls; no fixture writes or candidate imports."""
import hashlib
from pathlib import Path
import unittest
from unittest import mock
import project as p

class FixtureControls(unittest.TestCase):
    def test_canonical_paths(self):
        for path in ('../x', '/x', 'a/../x', 'a//x', 'a\\x', 'C:/x'):
            with self.assertRaises(ValueError): p.relative(path)
        self.assertEqual(p.relative('a/source.cs'), 'a/source.cs')

    def test_external_root_rejects_repository_and_descendants(self):
        root = Path.cwd()
        for value in (str(root), str(root / 'artifacts' / 'copy'), 'relative'):
            with self.assertRaises(ValueError): p.validate_external_root(value, root)
        self.assertEqual(p.validate_external_root(str(root.parent / 'external'), root), root.parent / 'external')

    def test_duplicate_evidence_keys(self):
        with self.assertRaises(ValueError): p.parse(b'{"sha256":"a","sha256":"b"}')

    def test_summary_requires_all_levels_and_copy_isolation(self):
        rows = [dict(workspace='bookbuilder', level=x, isolation_and_citations_passed=True, one_file_edit_in_isolated_copy={}) for x in ('essential', 'balanced', 'detailed')]
        p.validate_summary(dict(runs=rows, original_workspaces_unchanged=True))
        for bad in (dict(runs=rows, original_workspaces_unchanged=False), dict(runs=rows[:-1], original_workspaces_unchanged=True)):
            with self.assertRaises(ValueError): p.validate_summary(bad)
        rows[0]['isolation_and_citations_passed'] = False
        with self.assertRaises(ValueError): p.validate_summary(dict(runs=rows, original_workspaces_unchanged=True))

    def test_current_original_status_never_fabricates_equality(self):
        source = dict(sha256=hashlib.sha256(b'abc').hexdigest(), bytes=3)
        path = Path('D:/original.cs')
        for raw, expected in ((b'abc', 'matches'), (b'abd', 'differs')):
            with mock.patch.object(p, 'capture', return_value=raw): self.assertEqual(p.original_observation(path, source)['status'], expected)
        for error, expected in ((FileNotFoundError(), 'missing'), (PermissionError(), 'unavailable')):
            with mock.patch.object(p, 'capture', side_effect=error): self.assertEqual(p.original_observation(path, source)['status'], expected)

    def test_historical_witness_requires_root_digest_hash_and_size(self):
        source = dict(sha256='a'*64, bytes=7)
        inventory = dict(root='D:/original', files={'a.cs':dict(state=dict(source_hash='a'*64), source_bytes=7)})
        ip = dict(path='inventory.json', sha256='b'*64, bytes=99)
        mp = dict(path='manifest.json', sha256='c'*64, bytes=88)
        manifest = dict(inventory_digest='b'*64)
        result = p.historical_witness(inventory, manifest, ip, mp, 'a.cs', source, 'D:/original')
        self.assertEqual(result['source_sha256'], 'a'*64)
        self.assertIsNone(p.historical_witness(inventory, manifest, ip, mp, 'absent.cs', source, 'D:/original'))
        self.assertIsNone(p.historical_witness(inventory, manifest, ip, mp, 'a.cs', dict(source, bytes=8), 'D:/original'))
        with self.assertRaises(ValueError): p.historical_witness(inventory, manifest, ip, mp, 'a.cs', source, 'D:/other')
        with self.assertRaises(ValueError): p.historical_witness(inventory, dict(inventory_digest='d'*64), ip, mp, 'a.cs', source, 'D:/original')

    def test_role_only_gap_is_narrow_and_explicit(self):
        rel = 'project/obj/Release/net8.0/Project.AssemblyInfo.cs'
        self.assertTrue(p.allow_gap(rel, dict(status='differs'), None))
        self.assertFalse(p.allow_gap(rel, dict(status='matches'), None))
        self.assertFalse(p.allow_gap(rel, dict(status='differs'), {'inventory': {}}))
        with self.assertRaises(ValueError): p.allow_gap('project/implementation.cs', dict(status='differs'), None)
        self.assertEqual(p.GAP, 'No exact historical hash witness; source role established by pinned copy chain')

if __name__ == '__main__': unittest.main()
