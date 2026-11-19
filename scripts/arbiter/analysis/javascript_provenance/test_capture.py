# SPDX-License-Identifier: Apache-2.0
"""Exact historical source preservation and syntax only; no JS execution."""
import hashlib
import json
from pathlib import Path
import subprocess
import unittest
import sys

ROOT=Path(__file__).resolve().parents[4]
FAMILY=ROOT/'scripts/arbiter/research/javascript_history_r1'

class Capture(unittest.TestCase):
    def test_exact_roster_and_original_inverse(self):
        rows=json.loads((FAMILY/'source-map.json').read_bytes())['files']
        self.assertEqual(len(rows),231)
        self.assertEqual(len({r['source'] for r in rows}),231)
        self.assertEqual(len({r['destination'] for r in rows}),55)
        for row in rows:
            raw=(ROOT/row['destination']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),row['maintained_sha256'])
            self.assertEqual(len(raw),row['maintained_bytes'])
            self.assertTrue(raw.startswith(b'// SPDX-License-Identifier: Apache-2.0'))
            for change in reversed(row['transformations']):
                self.assertEqual(change,{'kind':'spdx-prefix','prefix':'// SPDX-License-Identifier: Apache-2.0\n'})
                raw=raw[len(change['prefix'].encode()):]
            self.assertEqual(hashlib.sha256(raw).hexdigest(),row['original_sha256'])
            self.assertEqual(len(raw),row['original_bytes'])
    def test_directory_contains_only_mapped_templates(self):
        rows=json.loads((FAMILY/'source-map.json').read_bytes())['files']
        rows+=json.loads((FAMILY/'source_policy/source-map.json').read_bytes())['files']
        rows+=json.loads((FAMILY/'fixtures/source-map.json').read_bytes())['files']
        mapped={ROOT/r['destination'] for r in rows}
        observed={p for p in (FAMILY/'sources').iterdir() if p.name!='README.md'}
        self.assertEqual(observed,mapped)
        self.assertTrue(all(p.name.endswith('.js.in') and not p.is_symlink() for p in observed))
    def test_source_policy_gap_is_separate_exact_module(self):
        rows=json.loads((FAMILY/'source_policy/source-map.json').read_bytes())['files']
        self.assertEqual(len(rows),31)
        self.assertEqual(len({r['destination'] for r in rows}),1)
        for row in rows:
            raw=(ROOT/row['destination']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),row['maintained_sha256'])
            self.assertEqual(row['original_sha256'],'b39d1a6a57aa8436ca8785b87280b39e160398239b6bfd7b827c2325d3e619ae')
            self.assertEqual(row['transformations'],[{'kind':'spdx-prefix','prefix':'// SPDX-License-Identifier: Apache-2.0\n'}])
            prefix=b'// SPDX-License-Identifier: Apache-2.0\n'
            self.assertTrue(raw.startswith(prefix));original=raw[len(prefix):]
            self.assertEqual((hashlib.sha256(original).hexdigest(),len(original)),(row['original_sha256'],row['original_bytes']))
            self.assertEqual(len(raw),row['maintained_bytes'])
    def test_older_fixture_variants_remain_exact_and_separate(self):
        rows=json.loads((FAMILY/'fixtures/source-map.json').read_bytes())['files']
        self.assertEqual(len(rows),11)
        self.assertEqual(len({r['destination'] for r in rows}),4)
        for row in rows:
            raw=(ROOT/row['destination']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),row['maintained_sha256'])
            self.assertEqual(row['transformations'],[{'kind':'spdx-prefix','prefix':'// SPDX-License-Identifier: Apache-2.0\n'}])
            original=raw.removeprefix(b'// SPDX-License-Identifier: Apache-2.0\n')
            self.assertTrue(raw.startswith(b'// SPDX-License-Identifier: Apache-2.0\n'))
            self.assertEqual(len(original),row['original_bytes'])
            self.assertEqual(len(raw),row['maintained_bytes'])
            self.assertEqual(hashlib.sha256(original).hexdigest(),row['original_sha256'])
    def test_all_templates_parse_without_execution(self):
        source="const fs=require('fs'),vm=require('vm');const rows=JSON.parse(fs.readFileSync(0,'utf8'));for(const row of rows)new vm.Script(fs.readFileSync(row,'utf8'),{filename:row});console.log(rows.length);"
        paths=sorted(str(p) for p in (FAMILY/'sources').glob('*.js.in'))
        out=subprocess.check_output(['node','-e',source],input=json.dumps(paths).encode(),cwd=ROOT,timeout=30)
        self.assertEqual(out.strip(),b'60')

if __name__=='__main__':
    sys.path.insert(0,str(ROOT/'scripts/arbiter/analysis/source_cleanup'))
    from custody import custody_cli
    if not custody_cli([FAMILY/'source-map.json',FAMILY/'source_policy/source-map.json',FAMILY/'fixtures/source-map.json'],sys.argv[1:]):unittest.main(verbosity=2)
