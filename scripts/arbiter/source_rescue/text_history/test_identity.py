# SPDX-License-Identifier: Apache-2.0
"""Read-only proof of original text identity; recovered programs are not imported."""
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import unittest
BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[3]
class TextIdentity(unittest.TestCase):
    def test_all_original_source_inverses(self):
        doc=json.loads((BASE/'source-provenance.json').read_bytes())
        self.assertEqual(len(doc['files']),27);self.assertEqual(len({r['destination'] for r in doc['files']}),23)
        for row in doc['files']:
            original=(ROOT/row['source']).read_bytes();self.assertEqual((len(original),hashlib.sha256(original).hexdigest()),(row['bytes'],row['original_sha256']))
            prefix=row['changes'][0]['prefix'].encode() if row['changes'] else b''
            maintained=(ROOT/row['destination']).read_bytes();self.assertEqual(maintained,prefix+original)
            self.assertEqual((len(maintained),hashlib.sha256(maintained).hexdigest()),(row['maintained_bytes'],row['maintained_sha256']))
    def test_raw_text_encoded_probe_identity(self):
        doc=json.loads((BASE/'encoded-source-provenance.json').read_bytes());self.assertEqual(len(doc['files']),1);row=doc['files'][0]
        self.assertEqual(row['container_format'],'utf8-text');self.assertEqual(row['json_pointer'],'')
        raw=(ROOT/row['container']).read_bytes();text=raw.decode('utf-8');self.assertEqual((len(raw),hashlib.sha256(raw).hexdigest()),(row['container_bytes'],row['container_sha256']))
        self.assertEqual((len(raw),hashlib.sha256(raw).hexdigest()),(row['encoded_field_bytes'],row['encoded_field_sha256']))
        matches=re.findall(r"b64decode\('([A-Za-z0-9+/=]+)'\)",text);self.assertEqual(len(matches),1);body=base64.b64decode(matches[0],validate=True);body.decode('utf-8')
        self.assertEqual((len(body),hashlib.sha256(body).hexdigest()),(row['decoded_bytes'],row['decoded_sha256']))
        actual=(ROOT/row['destination']).read_bytes();self.assertEqual(actual,row['prefix'].encode()+body)
        self.assertEqual((len(actual),hashlib.sha256(actual).hexdigest()),(row['maintained_bytes'],row['maintained_sha256']))
    def test_roster_and_no_overwrite(self):
        expected={'README.md','rescue.py','decode_probe.py','test_identity.py','source-provenance.json','encoded-source-provenance.json'}
        for name in ('source-provenance.json','encoded-source-provenance.json'):
            expected.update(Path(r['destination']).name for r in json.loads((BASE/name).read_bytes())['files'])
        self.assertEqual({p.name for p in BASE.iterdir()},expected)
        for toolname in ('rescue','decode_probe'):
            spec=importlib.util.spec_from_file_location(toolname,BASE/(toolname+'.py'));tool=importlib.util.module_from_spec(spec);spec.loader.exec_module(tool)
            with self.assertRaises(ValueError):tool.apply(json.loads((BASE/('source-provenance.json' if toolname=='rescue' else 'encoded-source-provenance.json')).read_bytes()))
    def test_raw_decoder_refuses_invalid_utf8_and_ambiguous_literal(self):
        spec=importlib.util.spec_from_file_location('decode_probe',BASE/'decode_probe.py');tool=importlib.util.module_from_spec(spec);spec.loader.exec_module(tool)
        for raw in (b'\xff',b"b64decode('eA==') b64decode('eA==')",b"b64decode('/w==')",b"b64decode('bad-') b64decode('eA==')"):
            with self.assertRaises((ValueError,UnicodeError)):tool.decode(raw)
if __name__=='__main__':unittest.main(verbosity=2)
