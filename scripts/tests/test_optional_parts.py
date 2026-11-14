# SPDX-License-Identifier: Apache-2.0
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from scripts.tests.test_optional_publication import hypothetical_projection

SPEC = importlib.util.spec_from_file_location('optional_parts', Path(__file__).parents[1] / 'optional-parts.py')
OWNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNER)


class OptionalParts(unittest.TestCase):
    def test_official_original_part_requires_native_digest_size_name_and_authenticated_nomination(self):
        owner = hypothetical_projection()
        pin = owner['assets']['arbiter-a.part01']
        original = {'id': 42, 'url': OWNER.transport.API + '42', 'name': 'arbiter-a.part01',
                    'state': 'uploaded', 'size': 1, 'digest': 'sha256:' + pin['sha256']}
        for fault in (None, 'id', 'url', 'name', 'state', 'size', 'digest', 'payload', 'oversize'):
            asset = dict(original)
            payload = b'a'
            if fault == 'payload': payload = b'b'
            elif fault == 'oversize': payload = b'aa'
            elif fault:
                asset[fault] = {'id': 41, 'url': 'https://other.invalid', 'name': 'foreign.bin',
                    'state': 'new', 'size': True, 'digest': 'sha256:' + '0' * 64}[fault]
            def fetch(url, token, binary=False):
                self.assertEqual(url, original['url'])
                self.assertEqual(token, 'private-token')
                return io.BytesIO(payload if binary else json.dumps(asset).encode())
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary, patch.object(OWNER.transport, 'fetch', side_effect=fetch):
                destination = Path(temporary) / 'part'
                if fault is None:
                    result = OWNER.acquire('arbiter-a.part01', destination, 42, 'private-token', owner)
                    self.assertEqual(result['sha256'], pin['sha256'])
                    self.assertEqual((destination / 'arbiter-a.part01').read_bytes(), b'a')
                else:
                    with self.assertRaises(ValueError): OWNER.acquire('arbiter-a.part01', destination, 42, 'private-token', owner)
                    if fault not in ('payload', 'oversize'): self.assertFalse(destination.exists())
        with patch.object(OWNER.transport, 'fetch') as request:
            for name, nomination, token in [('unknown', 42, 'token'), ('arbiter-a.part01', True, 'token'), ('arbiter-a.part01', 42, '')]:
                with self.assertRaises(ValueError): OWNER.acquire(name, Path('unused'), nomination, token, owner)
            request.assert_not_called()


if __name__ == '__main__':
    unittest.main()
