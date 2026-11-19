# SPDX-License-Identifier: Apache-2.0
"""Read-only preservation checks; never import historical source copies."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
import sys

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
spec=importlib.util.spec_from_file_location('source_rescue',HERE/'rescue.py')
owner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)

class Preservation(unittest.TestCase):
    def test_literal_python_prefix_preserves_non_utf8_crlf_body(self):
        raw=b'print("caf\xe9")\r\n'
        kept,changes=owner.licensed(raw,'.py')
        self.assertEqual(kept,b'# SPDX-License-Identifier: Apache-2.0\n'+raw)
        self.assertEqual(changes[0]['prefix'],'# SPDX-License-Identifier: Apache-2.0\n')
    def test_literal_cpp_comment_prefix(self):
        raw=b'"use strict";\r\n'
        self.assertEqual(owner.licensed(raw,'.js')[0],b'// SPDX-License-Identifier: Apache-2.0\n'+raw)
    def test_embedded_license_does_not_replace_file_header(self):
        raw=b'print(1)\n# SPDX-License-Identifier: Apache-2.0\n'
        self.assertEqual(owner.licensed(raw,'.py')[0],b'# SPDX-License-Identifier: Apache-2.0\n'+raw)
    def test_existing_license_is_exact(self):
        raw=b'// SPDX-License-Identifier: Apache-2.0\r\noriginal\r\n'
        self.assertEqual(owner.licensed(raw,'.rs'),(raw,[]))
    def test_all_806_origins_have_exact_inverse(self):
        rows=json.loads((HERE/'source-map.json').read_bytes())['files']
        self.assertEqual(len(rows),806)
        unique={};prefixed=set()
        for r in rows:
            raw=(ROOT/r['destination']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),r['maintained_sha256'])
            self.assertEqual(len(raw),r['maintained_bytes'])
            for change in reversed(r['transformations']):
                self.assertEqual(change['kind'],'spdx-prefix')
                prefix=change['prefix'].encode('ascii')
                self.assertIn(prefix,(b'# SPDX-License-Identifier: Apache-2.0\n',b'// SPDX-License-Identifier: Apache-2.0\n'))
                self.assertTrue(raw.startswith(prefix));raw=raw[len(prefix):];prefixed.add(r['destination'])
            self.assertEqual(hashlib.sha256(raw).hexdigest(),r['original_sha256'])
            self.assertEqual(len(raw),r['original_bytes'])
            unique[r['destination']]=r
        self.assertEqual(len(unique),654)
        self.assertEqual(len(prefixed),226)
    def test_generated_proof_and_helper_are_authenticated(self):
        proof=json.loads((HERE/'generated-replay.json').read_bytes())
        helper=(ROOT/proof['helper']['path']).read_bytes()
        self.assertEqual(hashlib.sha256(helper).hexdigest(),proof['helper']['sha256'])
        self.assertEqual(len(proof['rows']),416)
        rows=json.loads((HERE/'generated-disposition.json').read_bytes())['files']
        self.assertEqual(len(rows),416)
        for row in rows:
            for e in row['evidence']:
                raw=(ROOT/e['path']).read_bytes()
                self.assertEqual((hashlib.sha256(raw).hexdigest(),len(raw)),(e['sha256'],e['bytes']))
if __name__=='__main__':
    sys.path.insert(0,str(ROOT/'scripts/arbiter/analysis/source_cleanup'))
    from custody import custody_cli
    if not custody_cli([HERE/'source-map.json'],sys.argv[1:]):unittest.main(verbosity=2)
