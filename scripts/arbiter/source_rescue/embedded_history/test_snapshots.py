# SPDX-License-Identifier: Apache-2.0
"""Read-only decoded snapshot identity controls; no recovered source imports."""
import hashlib
import json
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[4]
BASE=Path(__file__).resolve().parent/'snapshots'

class SnapshotIdentity(unittest.TestCase):
    def test_independent_container_pointer_inverse(self):
        data=json.loads((BASE/'source-provenance.json').read_bytes())
        self.assertEqual(len(data['files']),71)
        self.assertEqual(len({x['destination'] for x in data['files'] if x['disposition']=='extracted-source'}),37)
        for row in data['files']:
            raw=(ROOT/row['container']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),row['container_sha256'])
            body=json.loads(raw)
            for token in row['json_pointer'][1:].split('/'):
                token=token.replace('~1','/').replace('~0','~')
                body=body[int(token)] if isinstance(body,list) else body[token]
            decoded=body.encode('utf-8')
            self.assertEqual(hashlib.sha256(decoded).hexdigest(),row['decoded_sha256'])
            self.assertEqual(len(decoded),row['decoded_bytes'])
            actual=(ROOT/row['destination']).read_bytes()
            self.assertEqual(actual,row['prefix'].encode()+decoded)
            self.assertEqual(hashlib.sha256(actual).hexdigest(),row['maintained_sha256'])
            self.assertEqual(len(actual),row['maintained_bytes'])
            self.assertIn(b'SPDX-License-Identifier: Apache-2.0',actual.splitlines()[0])
    def test_apply_refuses_existing_provenance_before_writes(self):
        import extract
        import snapshot_plan
        from unittest.mock import patch
        for tool in (extract,snapshot_plan):
            with patch.object(Path,'write_bytes',side_effect=AssertionError('must not write')), patch.object(Path,'write_text',side_effect=AssertionError('must not write')):
                with self.assertRaisesRegex(ValueError,'Never overwrite source provenance'):
                    tool.apply({'schema':'airk.embedded-source-migration/1','files':[]})
                with self.assertRaisesRegex(ValueError,'Unexpected source map schema'):
                    tool.apply({'schema':'wrong','files':[]})

    def test_roster_and_scope(self):
        data=json.loads((BASE/'source-provenance.json').read_bytes())
        self.assertFalse(data['inspection_gaps']['complete_source_coverage'])
        expected={ROOT/x['destination'] for x in data['files'] if x['disposition']=='extracted-source'}
        self.assertEqual(expected,set(BASE.glob('*.in')))
        self.assertTrue((BASE/'README.md').is_file())

if __name__=='__main__':
    unittest.main()
