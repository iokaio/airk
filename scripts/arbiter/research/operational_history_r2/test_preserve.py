# SPDX-License-Identifier: Apache-2.0
"""Read-only relocation/inverse controls; no historical script import."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
spec=importlib.util.spec_from_file_location('operational_preserve',HERE/'preserve.py')
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)

class Preservation(unittest.TestCase):
    def test_literal_language_headers_and_inverse(self):
        raw=b'original\r\n\xe9'
        for ext,expected in [('.py',b'# '),('.ps1',b'# '),('.sh',b'# '),('.cmd',b'REM '),('.cjs',b'// '),('.mjs',b'// ')]:
            kept,changes=owner.prefix(raw,ext)
            self.assertEqual(kept,expected+b'SPDX-License-Identifier: Apache-2.0\n'+raw)
            self.assertEqual(owner.inverse(kept,changes),raw)
    def test_later_spdx_is_not_file_header(self):
        raw=b'code\n# SPDX-License-Identifier: Apache-2.0\n'
        self.assertEqual(owner.prefix(raw,'.py')[0],b'# SPDX-License-Identifier: Apache-2.0\n'+raw)
    def test_unknown_transformation_refused(self):
        with self.assertRaises(ValueError):owner.inverse(b'abc',[{'kind':'normalize'}])
    def test_every_origin_has_exact_inverse(self):
        rows=json.loads((HERE/'source-map.json').read_bytes())['files']
        self.assertEqual(len(rows),2413)
        for r in rows:
            kept=(ROOT/r['destination']).read_bytes();raw=(ROOT/r['source']).read_bytes()
            self.assertEqual((hashlib.sha256(kept).hexdigest(),len(kept)),(r['maintained_sha256'],r['maintained_bytes']))
            self.assertEqual((hashlib.sha256(raw).hexdigest(),len(raw)),(r['original_sha256'],r['original_bytes']))
            self.assertEqual(owner.inverse(kept,r['transformations']),raw)
            self.assertIn(b'SPDX-License-Identifier: Apache-2.0',kept.split(b'\n',1)[0])
    def test_receiving_folder_has_no_unmapped_source(self):
        rows=json.loads((HERE/'source-map.json').read_bytes())['files']
        expected={(ROOT/r['destination']).resolve() for r in rows if r['destination'].startswith(HERE.relative_to(ROOT).as_posix()+'/')}
        owned=['README.md','sources/README.md','preserve.py','test_preserve.py','source-map.json','routing-notes.json','build-command-review.json','review_sources.py','prove_templates.py','prove_templates_r1.py.in','prove_upstream.py','prove_upstream_r1.py.in']
        expected.update((HERE/name).resolve() for name in owned)
        actual={p.resolve() for p in HERE.rglob('*') if p.is_file()}
        self.assertEqual(actual,expected)
        self.assertEqual(len(list((HERE/'sources').glob('*.rs'))),0)
    def test_historical_template_parameters_are_bounded(self):
        module_spec=importlib.util.spec_from_file_location('template_proof',HERE/'prove_templates.py')
        template=importlib.util.module_from_spec(module_spec);module_spec.loader.exec_module(template)
        valid=b'@echo off\r\n"C:\\owned\\airk.exe" %*\r\nexit /b %errorlevel%\r\n'
        self.assertEqual(template.reproduce(valid,template.LITERAL),'C:\\owned\\airk.exe')
        for raw in [valid.replace(b'owned',b'%owned%'),valid.replace(b'owned',b'!owned!'),valid+b'extra',valid.replace(b'C:\\owned\\',b'')]:
            with self.assertRaises(ValueError):template.reproduce(raw,template.LITERAL)
    def test_three_requested_historical_sources_have_literal_pins(self):
        rows={r['source']:r for r in json.loads((HERE/'source-map.json').read_bytes())['files']}
        for name,pin in [('p8-frozen-replay.py','623baa3081a755bbf991c4ff6a55b61440d4fb93d88c85fbf573ed74242036d8'),('p9-frozen-replay.py','ae3d0a5e97e9b5ae10024074b31ef16d6e3cdcf876b284d9bd0425c4b03ea887'),('build-p6f-guarded-qualification-pair.py','1c1ff94b2e23ca38c42e996c79ac5a59a97e8c279f814052f77661154155eae8')]:self.assertEqual(rows['artifacts/v190/'+name]['original_sha256'],pin)
if __name__=='__main__':unittest.main(verbosity=2)
