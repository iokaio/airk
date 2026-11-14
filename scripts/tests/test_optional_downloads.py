# SPDX-License-Identifier: Apache-2.0
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from scripts.tests.test_optional_publication import PARTS, hypothetical_projection, document

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / (name + '.py'))
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


class OptionalDownloads(unittest.TestCase):
    def test_published_site_requires_all_native_parts_digest_and_size_before_advertising(self):
        site = load('build-downloads')
        record = document('release')
        encoded = json.dumps(record).encode()
        assets = dict(record['assets'])
        assets.update({'release.json': hashlib.sha256(encoded).hexdigest(), 'release-notes.md': 'e'*64, 'SHA256SUMS': 'f'*64})
        owner = hypothetical_projection()
        release = {'tag_name': 'v1.9.0', 'draft': False, 'prerelease': False, 'published_at': '2026-11-14T00:00:00Z',
            'assets': [{'id': index+1, 'name': name, 'state': 'uploaded',
                'size': owner['assets'][name]['bytes'] if name in owner['assets'] else len(encoded) if name == 'release.json' else 1234,
                'digest': 'sha256:'+digest, 'browser_download_url': site.GITHUB+'/releases/download/v1.9.0/'+name}
                for index,(name,digest) in enumerate(assets.items())]}
        with patch.object(site.optional, 'projection', return_value=owner):
            result = site.select_releases([release], lambda _: encoded)
            html = site.render_downloads(result)
            self.assertIn('Optional local model assets', html)
            for name in PARTS: self.assertIn('/v1.9.0/'+name+'"', html)
            for fault in ('size', 'digest', 'missing', 'extra'):
                changed = deepcopy(release)
                row = next(row for row in changed['assets'] if row['name'] == 'arbiter-a.part01')
                if fault == 'size': row['size'] += 1
                elif fault == 'digest': row['digest'] = 'sha256:'+'0'*64
                elif fault == 'missing': changed['assets'].remove(row)
                else: changed['assets'].append({**row, 'id':999, 'name':'foreign.part01'})
                with self.subTest(fault=fault), self.assertRaises(ValueError): site.select_releases([changed], lambda _: encoded)

    def test_actual_downloaded_part_bytes_cannot_be_resealed_around_native_pins(self):
        published = load('verify-published-release')
        owner = hypothetical_projection()
        record = {'assets': {name: asset['sha256'] for name, asset in owner['assets'].items()}}
        sums = dict(record['assets'])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name,data in PARTS.items(): (root/name).write_bytes(data)
            checks = published.Checks()
            self.assertEqual(published.verify_optional_parts(root,record,sums,owner,checks),record['assets'])
            self.assertEqual(checks.failed,[])
            for fault in ('bytes', 'resealed', 'missing'):
                for name,data in PARTS.items(): (root/name).write_bytes(data)
                changed_record,changed_sums = deepcopy(record),dict(sums)
                path = root/'arbiter-a.part01'
                if fault == 'missing':path.unlink()
                else:
                    path.write_bytes(b'x')
                    if fault == 'resealed':
                        changed_record['assets'][path.name] = changed_sums[path.name] = hashlib.sha256(b'x').hexdigest()
                checks = published.Checks()
                verified = published.verify_optional_parts(root,changed_record,changed_sums,owner,checks)
                self.assertEqual(len(checks.failed),1)
                self.assertNotIn(path.name,verified)


if __name__ == '__main__':
    unittest.main()
