# SPDX-License-Identifier: Apache-2.0
"""Metadata reuse must preserve every live source fence and per-ledger state."""
from contextlib import contextmanager
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


@contextmanager
def fixture():
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory); source = root/'sources'/'paper'; source.mkdir(parents=True)
        text = 'First line.\nCafé 日本語 🚀.\nLast line.'.encode()
        path = source/'text.txt'; path.write_bytes(text)
        row = {'file': 'sources/paper/text.txt', 'bytes': len(text),
               'sha256': hashlib.sha256(text).hexdigest(),
               'paragraphs': [{'original_url': 'https://example.invalid/original'}]}
        rows = [row]
        for index in range(17):
            name = 'other-'+str(index)+'.txt'; (source/name).write_bytes(b'irrelevant')
            rows.append({'file': 'sources/paper/'+name, 'bytes': 10,
                         'sha256': hashlib.sha256(b'irrelevant').hexdigest(),
                         'paragraphs': [{'original_url': 'https://example.invalid/other'}]})
        manifest = json.dumps({'files': rows}).encode()
        (root/'manifest.private.json').write_bytes(manifest)
        hit = {'citation': {'path': 'text.txt', 'content_hash': row['sha256'],
                           'start_line': 2, 'end_line': 2},
               'text': 'Café 日本語 🚀.\n', 'truncated': False,
               'future_field': {'preserved': True}}
        value = {'publication': {'token': 'first', 'future_identity': 'preserved'}, 'hits': [hit]}
        yield root, source, path, text, manifest, value


class FrozenSourceMetadataTests(unittest.TestCase):
    def test_cache_from_another_fixture_is_not_an_authority_or_path_alias(self):
        from goal_sources import SourceLedger, FrozenSourceMetadata
        with fixture() as first, fixture() as second:
            root, source, path, raw, manifest, value = first
            other_root, other_source, _, _, other_manifest, _ = second
            self.assertEqual(manifest, other_manifest)
            expected = hashlib.sha256(manifest).hexdigest()
            cache = FrozenSourceMetadata(root, expected)
            with self.assertRaises(ValueError):
                SourceLedger(other_root, other_source, expected, metadata_cache=cache)

    def test_second_ledger_reuses_roster_but_resolves_every_used_hit(self):
        from goal_sources import SourceLedger, FrozenSourceMetadata
        with fixture() as (root, source, path, raw, manifest, value):
            expected = hashlib.sha256(manifest).hexdigest()
            cache = FrozenSourceMetadata(root, expected)
            first = SourceLedger(root, source, expected, metadata_cache=cache)
            first.observe(value)
            observed = []; original_resolve = Path.resolve
            def resolve(current, *args, **kwargs):
                observed.append(current)
                return original_resolve(current, *args, **kwargs)
            with patch.object(Path, 'resolve', new=resolve):
                second = SourceLedger(root, source, expected, metadata_cache=cache)
                second.observe(value)
                second.observe(value)
            self.assertFalse(any(p.name.startswith('other-') for p in observed))
            self.assertGreaterEqual(sum(p.name == 'text.txt' for p in observed), 2)
            self.assertEqual(second.visible_candidates()[0]['text'], value['hits'][0]['text'])
            self.assertEqual(second.visible_candidates()[0]['native_observations'][0]['full_hit'], value['hits'][0])

    def test_manifest_and_actual_source_drift_refuse_despite_cached_metadata(self):
        from goal_sources import SourceLedger, FrozenSourceMetadata
        with fixture() as (root, source, path, raw, manifest, value):
            expected = hashlib.sha256(manifest).hexdigest(); cache = FrozenSourceMetadata(root, expected)
            ledger = SourceLedger(root, source, expected, metadata_cache=cache)
            path.write_bytes(b'X'*len(raw))
            with self.assertRaises(ValueError): ledger.observe(value)
            path.write_bytes(raw)
            changed = manifest+b' '
            (root/'manifest.private.json').write_bytes(changed)
            with self.assertRaises(ValueError): ledger.observe(value)
            with self.assertRaises(ValueError): SourceLedger(root, source, expected, metadata_cache=cache)

    def test_cached_metadata_does_not_share_publication_records_or_mutable_rows(self):
        from goal_sources import SourceLedger, FrozenSourceMetadata
        with fixture() as (root, source, path, raw, manifest, value):
            expected = hashlib.sha256(manifest).hexdigest(); cache = FrozenSourceMetadata(root, expected)
            one = SourceLedger(root, source, expected, metadata_cache=cache)
            two = SourceLedger(root, source, expected, metadata_cache=cache)
            one.observe(value)
            self.assertEqual(two.records, {}); self.assertIsNone(two.publication)
            changed = copy.deepcopy(value); changed['publication']['token'] = 'second'
            two.observe(changed)
            self.assertEqual(one.publication, 'first'); self.assertEqual(two.publication, 'second')
            one.files['text.txt']['sha256'] = '0'*64
            self.assertNotEqual(one.files['text.txt']['sha256'], two.files['text.txt']['sha256'])
            with self.assertRaises(ValueError): one.observe(value)
            two.observe(changed)

    def test_current_canonical_hit_path_cannot_escape_cached_index_scope(self):
        from goal_sources import SourceLedger, FrozenSourceMetadata
        with fixture() as (root, source, path, raw, manifest, value):
            expected = hashlib.sha256(manifest).hexdigest(); cache = FrozenSourceMetadata(root, expected)
            ledger = SourceLedger(root, source, expected, metadata_cache=cache)
            foreign = root/'foreign.txt'; foreign.write_bytes(raw)
            original_resolve = Path.resolve
            def resolve(current, *args, **kwargs):
                if current == path:
                    return original_resolve(foreign, *args, **kwargs)
                return original_resolve(current, *args, **kwargs)
            with patch.object(Path, 'resolve', new=resolve):
                with self.assertRaises(ValueError): ledger.observe(value)
            self.assertEqual(ledger.records, {})

    def test_cached_and_uncached_forward_results_and_refusals_match(self):
        from goal_sources import SourceLedger, FrozenSourceMetadata
        with fixture() as (root, source, path, raw, manifest, value):
            expected = hashlib.sha256(manifest).hexdigest(); cache = FrozenSourceMetadata(root, expected)
            old = SourceLedger(root, source, expected)
            cached = SourceLedger(root, source, expected, metadata_cache=cache)
            for ledger in (old, cached):
                ledger.observe(value); ledger.observe(value)
                with self.assertRaises(ValueError): ledger.observe({**value, 'publication': {'token': 'foreign'}})
            self.assertEqual(old.records, cached.records)
            self.assertEqual(old.visible_candidates(), cached.visible_candidates())
            self.assertEqual(old.read_followup('r1'), cached.read_followup('r1'))


if __name__ == '__main__':
    unittest.main()
