# SPDX-License-Identifier: Apache-2.0
"""In-memory refusal controls; no files, downloaded pages, models or processes."""
import copy
import hashlib
import json
import unittest
import verify_dispositions as owner

ROOT = 'artifacts/advanced-split/models/synthetic-reference'
HTML = b'<html>synthetic metadata only</html>'
H = hashlib.sha256(HTML).hexdigest()


def fixture(kind='qasper_versioned'):
    name = '1234.56789v1.html' if kind == 'qasper_versioned' else 'arxiv-1234.56789.html'
    url = 'https://arxiv.org/abs/1234.56789v1'
    packet = {'metadata_only': True, 'no_corpus_or_paper_text_adoption': True,
              'requests': [{'paper_id': '1234.56789', 'version': '1', 'file': 'C:/retained/' + name,
                            'url': url, 'final_url': url, 'sha256': H, 'bytes': len(HTML)}],
              'records': [{'paper_id': '1234.56789', 'metadata_sha256': H}]}
    return make(kind, packet, name)


def make(kind, packet, name):
    raw = json.dumps(packet).encode()
    evidence = {'path': ROOT + '/receipt.json', 'sha256': hashlib.sha256(raw).hexdigest(), 'bytes': len(raw)}
    family = {'kind': kind, 'source_root': ROOT, 'expected_files': 1, 'evidence': [evidence]}
    data = {evidence['path']: raw, ROOT + '/' + name: HTML}
    return family, data


class Dispositions(unittest.TestCase):
    def test_versioned_row_binds_complete_reference_and_evidence(self):
        f, data = fixture()
        row = owner.family_rows(f, data.__getitem__)[0]
        self.assertEqual(row['reference'], {'url': 'https://arxiv.org/abs/1234.56789v1', 'content_sha256': H, 'provenance_gap': None})
        self.assertEqual(row['source'], ROOT + '/1234.56789v1.html')
        self.assertEqual(row['source_bytes'], len(HTML))
        self.assertEqual(row['source_sha256'], H)
        self.assertEqual(row['disposition'], 'upstream_reference_document')
        self.assertEqual(row['evidence'], f['evidence'])
        self.assertNotIn('upstream', row)

    def test_receipt_bytes_or_length_drift_refuses(self):
        f, data = fixture()
        data[f['evidence'][0]['path']] += b' '
        with self.assertRaisesRegex(ValueError, 'evidence differs'):
            owner.family_rows(f, data.__getitem__)
        f, data = fixture()
        f['evidence'][0]['bytes'] += 1
        with self.assertRaisesRegex(ValueError, 'evidence differs'):
            owner.family_rows(f, data.__getitem__)

    def test_source_body_drift_refuses(self):
        f, data = fixture()
        data[ROOT + '/1234.56789v1.html'] = b'other document'
        with self.assertRaisesRegex(ValueError, 'source differs'):
            owner.family_rows(f, data.__getitem__)

    def test_metadata_flags_url_filename_and_byte_length_refuse(self):
        for key, value in [('url', 'https://foreign.example/'), ('final_url', 'https://arxiv.org/abs/other'), ('file', 'different.html'), ('bytes', 1)]:
            f, data = fixture()
            packet = json.loads(data[f['evidence'][0]['path']])
            packet['requests'][0][key] = value
            changed, data = make('qasper_versioned', packet, '1234.56789v1.html')
            with self.subTest(field=key), self.assertRaises(ValueError):
                owner.family_rows(changed, data.__getitem__)
        for key in ['metadata_only', 'no_corpus_or_paper_text_adoption']:
            f, data = fixture()
            packet = json.loads(data[f['evidence'][0]['path']]); packet[key] = False
            f, data = make('qasper_versioned', packet, '1234.56789v1.html')
            with self.assertRaisesRegex(ValueError, 'metadata-only'):
                owner.family_rows(f, data.__getitem__)

    def test_bulk_duplicate_and_invalid_ids_refuse(self):
        f, data = fixture('qasper_bulk')
        self.assertEqual(owner.family_rows(f, data.__getitem__)[0]['reference']['url'], 'https://arxiv.org/abs/1234.56789')
        packet = json.loads(data[f['evidence'][0]['path']])
        packet['records'] *= 2
        f, data = make('qasper_bulk', packet, 'arxiv-1234.56789.html')
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            owner.family_rows(f, data.__getitem__)
        packet['records'] = [{'paper_id': '../escape', 'metadata_sha256': H}]
        f, data = make('qasper_bulk', packet, 'arxiv-1234.56789.html')
        with self.assertRaisesRegex(ValueError, 'identifier'):
            owner.family_rows(f, data.__getitem__)

    def test_hotpot_gap_cannot_be_silently_removed(self):
        f, data = make('hotpot_reference', {}, 'publisher-homepage.html')
        f['reference'] = {'url': 'https://hotpotqa.github.io/', 'content_sha256': H, 'bytes': len(HTML), 'provenance_gap': 'original acquisition receipt unavailable'}
        self.assertEqual(owner.family_rows(f, data.__getitem__)[0]['reference']['provenance_gap'], f['reference']['provenance_gap'])
        f['reference']['provenance_gap'] = None
        with self.assertRaisesRegex(ValueError, 'remain explicit'):
            owner.family_rows(f, data.__getitem__)

    def test_wrong_count_unknown_family_and_path_escape_refuse(self):
        f, data = fixture()
        for updates in ({'expected_files': 2}, {'expected_files': True}, {'kind': 'invented'}, {'source_root': 'artifacts/advanced-split/models/../source'}):
            changed = dict(f, **updates)
            with self.subTest(updates=updates), self.assertRaises(ValueError):
                owner.family_rows(changed, data.__getitem__)
        with self.assertRaises(ValueError):owner.relative_path('scripts/private.html')

    def test_project_rejects_duplicate_sources_and_wrong_schema(self):
        f, data = fixture()
        definitions = {'schema': 'airk.model-metadata-reference-families/1', 'expected_files': 2, 'families': [f, copy.deepcopy(f)]}
        with self.assertRaisesRegex(ValueError, 'roster differs'):
            owner.project(definitions, data.__getitem__)
        definitions.update(schema='invented', expected_files=1, families=[f])
        with self.assertRaisesRegex(ValueError, 'definitions'):
            owner.project(definitions, data.__getitem__)


if __name__ == '__main__':
    unittest.main(verbosity=2)
