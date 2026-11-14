# SPDX-License-Identifier: Apache-2.0
"""Consume opaque native-emitted publication cases; never mirror Core Profile."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('optional_native_projection', ROOT / 'scripts/optional-publication.py')
OWNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNER)


class NativePublicationProjection(unittest.TestCase):
    def test_native_case_capture_is_required_and_hypothetical_projection_stays_nonproduction(self):
        cases_path = ROOT / 'fixtures/arbiter/publication_cases.json'
        self.assertTrue(cases_path.is_file(), 'capture the owning native publication semantic cases first')
        capture = json.loads(cases_path.read_bytes())
        self.assertEqual(capture['schema'], 'airk.arbiter.publication-cases/1')
        self.assertIs(capture['hypothetical'], True)
        self.assertEqual(len(capture['cases']), 6)
        positives = [row for row in capture['cases'] if row['accepted']]
        self.assertEqual(len(positives), 1)
        for row in capture['cases']:
            if not row['accepted']:
                self.assertNotIn('projection', row, 'native refusal cannot supply publication authority')
        projection = positives[0]['projection']
        self.assertEqual(len(projection['profiles']), 4)
        self.assertNotEqual(cases_path, OWNER.PROJECTION)
        # A test-only explicit source path exercises exact opaque transfer. The
        # production command has no source-path override and reads only the
        # actual qualified compiled-catalog capture, never this case document.
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'hypothetical.json'
            path.write_text(json.dumps(projection), encoding='utf-8')
            transferred = OWNER.projection(projection['tag'], path=path)
            self.assertEqual(transferred, projection)
            OWNER.validate_projection(transferred, projection)
            changed = json.loads(json.dumps(transferred))
            changed['compiled_catalog_sha256'] = '0' * 64
            with self.assertRaises(ValueError): OWNER.validate_projection(changed, projection)
            with self.assertRaises(ValueError): OWNER.projection('v0.0.0', path=path)


if __name__ == '__main__':
    unittest.main()
