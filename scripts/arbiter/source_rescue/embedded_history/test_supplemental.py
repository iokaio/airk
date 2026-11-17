# SPDX-License-Identifier: Apache-2.0
"""Independent no-execution checks for the supplemental extracted source."""
import ast
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import unittest
BASE=Path(__file__).resolve().parent/'supplemental'
ROOT=BASE.parents[4]
class SupplementalIdentity(unittest.TestCase):
    def test_all_five_pointer_inverses(self):
        for name,count in [('source-provenance.json',4),('encoded-source-provenance.json',1)]:
            doc=json.loads((BASE/name).read_bytes());self.assertEqual(len(doc['files']),count)
            for row in doc['files']:
                raw=(ROOT/row['container']).read_bytes();self.assertEqual(hashlib.sha256(raw).hexdigest(),row['container_sha256']);self.assertEqual(len(raw),row['container_bytes'])
                field=json.loads(raw)
                for p in row['json_pointer'][1:].split('/'):
                    p=p.replace('~1','/').replace('~0','~');field=field[int(p)] if isinstance(field,list) else field[p]
                if name.startswith('encoded'):
                    self.assertEqual(hashlib.sha256(field.encode()).hexdigest(),row['encoded_field_sha256'])
                    self.assertEqual(len(field.encode()),row['encoded_field_bytes'])
                    literals=re.findall(r"b64decode\('([A-Za-z0-9+/=]+)'\)",field);self.assertEqual(len(literals),1)
                    body=base64.b64decode(literals[0],validate=True);body.decode('utf-8');ast.parse(body)
                    self.assertEqual(row['decode_method'],'single-python-b64decode-literal/1')
                else:body=field.encode()
                self.assertEqual(hashlib.sha256(body).hexdigest(),row['decoded_sha256']);self.assertEqual(len(body),row['decoded_bytes'])
                actual=(ROOT/row['destination']).read_bytes();self.assertEqual(actual,row['prefix'].encode()+body)
                self.assertEqual(hashlib.sha256(actual).hexdigest(),row['maintained_sha256']);self.assertEqual(len(actual),row['maintained_bytes'])
    def test_unexplained_source_fields_remain_unresolved(self):
        import classify_remainder
        for document,location in [({'script':'print(1)'},'/script'),({'rejected_hunks':[{'text':'diff --git a/a b/a'}]},'/rejected_hunks/0/text'),({'stdout':'def f(): return 1'},'/stdout'),({'name':'authored','description':'def f(): return 1'},'/description'),({'observations':[{'text':'def f(): return 1'}]},'/observations/0/text')]:
            self.assertEqual(classify_remainder.classify(document,location,'def f(): return 1')['disposition'],'unresolved')
    def test_corpus_requires_structural_input_roles(self):
        import classify_remainder
        name='artifacts/v190/p13a-additional-corpus-r1.json'
        raw=(ROOT/name).read_bytes();doc=json.loads(raw);text=doc['sources']['src/publish.rs'];location='/sources/src~1publish.rs'
        self.assertEqual(classify_remainder.classify(doc,location,text,container=name,raw=raw)['disposition'],'private_retrieval_fixture_input')
        altered=dict(doc);altered['unreviewed']='value';changed=json.dumps(altered).encode()
        self.assertEqual(classify_remainder.classify(altered,location,text,container=name,raw=changed)['disposition'],'unresolved')
        with self.assertRaises(ValueError):
            classify_remainder.classify(doc,location,'substituted source',container=name,raw=raw)

    def test_decoder_refuses_ambiguous_or_invalid_literal(self):
        spec=importlib.util.spec_from_file_location('supplemental_rescue',BASE/'rescue.py');tool=importlib.util.module_from_spec(spec);spec.loader.exec_module(tool)
        self.assertEqual(tool.decode("b64decode('eA==')"),b'x')
        for field in ["missing", "b64decode('eA==') b64decode('eA==')", "b64decode('=')", "b64decode('/w==')", "b64decode('bad-') b64decode('eA==')"]:
            with self.assertRaises((ValueError,UnicodeError)):
                tool.decode(field)
if __name__=='__main__':unittest.main()
