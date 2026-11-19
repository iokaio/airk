# SPDX-License-Identifier: Apache-2.0
"""Read-only preservation controls; historical source is never imported."""
import hashlib
import json
from pathlib import Path
import unittest
import sys
import preserve

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

class PreservationTests(unittest.TestCase):
    def test_literal_prefix_preserves_non_utf8_and_crcrlf(self):
        raw=b'fragment\r\r\n\x85'
        kept,changes=preserve.owner.prefix(raw,'.cjs')
        self.assertEqual(kept,b'// SPDX-License-Identifier: Apache-2.0\n'+raw)
        self.assertEqual(preserve.owner.inverse(kept,changes),raw)
    def test_css_keeps_reviewed_header_and_bytes(self):
        raw=b'/* SPDX-License-Identifier: Apache-2.0 */\r\nbody{color:red}'
        self.assertEqual(preserve.encoded(raw,'.css'),(raw,[]))
        with self.assertRaises(ValueError):preserve.encoded(b'body{}','.css')
    def test_later_license_is_not_first_line(self):
        raw=b'filename.rs:\n// SPDX-License-Identifier: Apache-2.0\nfragment'
        self.assertEqual(preserve.owner.prefix(raw,'.cjs')[0],b'// SPDX-License-Identifier: Apache-2.0\n'+raw)
    def test_unknown_transform_and_path_refused(self):
        with self.assertRaises(ValueError):preserve.owner.inverse(b'abc',[{'kind':'normalize'}])
        for p in ['../a','artifacts/../a','C:/a','artifacts\\a']:
            with self.assertRaises(ValueError):preserve.checked(p)
    def test_all_original_and_retained_bytes_with_inverse(self):
        data=json.loads((HERE/'source-map.json').read_bytes());rows=data['files']
        self.assertEqual(len(rows),1788);self.assertEqual(len({r['destination'] for r in rows}),957)
        for row in rows:
            kept=(ROOT/row['destination']).read_bytes();original=preserve.owner.inverse(kept,row['transformations'])
            self.assertEqual((hashlib.sha256(original).hexdigest(),len(original)),(row['original_sha256'],row['original_bytes']))
            self.assertEqual((hashlib.sha256(kept).hexdigest(),len(kept)),(row['maintained_sha256'],row['maintained_bytes']))
            self.assertIn(b'SPDX-License-Identifier: Apache-2.0',kept.split(b'\n',1)[0])
        self.assertEqual(hashlib.sha256((ROOT/data['selection']['path']).read_bytes()).hexdigest(),data['selection']['sha256'])
        self.assertEqual(hashlib.sha256(preserve.HELPER.read_bytes()).hexdigest(),data['prefix_helper']['sha256'])
    def test_exact_roster_and_inert_suffixes(self):
        rows=json.loads((HERE/'source-map.json').read_bytes())['files']
        expected={(ROOT/r['destination']).resolve() for r in rows}
        expected.update((HERE/p).resolve() for p in ['README.md','sources/README.md','select.py','selection.json','reviewed-additions.json','preserve.py','source-map.json','test_preserve.py'])
        self.assertEqual({p.resolve() for p in HERE.rglob('*') if p.is_file()},expected)
        self.assertTrue(all(Path(r['destination']).name.endswith(('.rs.in','.ts.in','.tsx.in','.css.in')) for r in rows))
    def test_explicit_reviewed_additions_remain_selected(self):
        selected={r['source'] for r in json.loads((HERE/'selection.json').read_bytes())['files']}
        extras=json.loads((HERE/'reviewed-additions.json').read_bytes())['paths']
        self.assertEqual(len(extras),36);self.assertTrue(set(extras)<=selected)
        self.assertFalse(any('/p10-target/' in p or '/security-rustix-' in p or '/editor-qualification/' in p for p in selected))

if __name__=='__main__':
    sys.path.insert(0,str(ROOT/'scripts/arbiter/analysis/source_cleanup'))
    from custody import custody_cli
    if not custody_cli([HERE/'source-map.json'],sys.argv[1:]):unittest.main(verbosity=2)
