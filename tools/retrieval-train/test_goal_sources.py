# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from goal_sources import SourceLedger, ResidentBinding


class NativeSourceProjectionTests(unittest.TestCase):
    def test_resident_first_publication_and_foreign_scope_refuse_original_native_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'sources'/'paper';source.mkdir(parents=True)
            raw=b'Original source.\n';(source/'text.txt').write_bytes(raw)
            manifest={'files':[{'file':'sources/paper/text.txt','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'paragraphs':[{'original_url':'https://example.invalid/original'}]}]}
            manifest_bytes=json.dumps(manifest).encode();(root/'manifest.private.json').write_bytes(manifest_bytes)
            publication={'token':'original-publication','collection':'original-collection','future_identity':'preserved'}
            row={'source_root':str(source),'index_id':'original-index','publication':publication,
                 'definition':{'id':'original-index','tombstone':False,'source_revision':7,'backing':{'root':str(source),'collection':'original-collection'}},
                 'source_roots':[{'id':'original-source-root','canonical_path':str(source)}]}
            receipt={'schema':'airk.private-arbiter-index-bindings/1','evaluation':True,'release_qualified':False,
                     'instance':'original-instance','namespace':'original-catalog','bindings':[row]}
            path=root/'bindings.json';path.write_text(json.dumps(receipt))
            ledger=SourceLedger(root,source,hashlib.sha256(manifest_bytes).hexdigest(),native_bindings=path)
            value={'publication':publication,'hits':[],'scope_observation':{'members':[{'index_id':'original-index','source_root_ids':['original-source-root'],'source_revision':7}]}}
            self.assertEqual(ledger.observe(value),[])
            for changed in [
                {**value,'publication':{**publication,'token':'foreign-first-publication'}},
                {**value,'scope_observation':{'members':[{'index_id':'foreign-index','source_root_ids':['original-source-root'],'source_revision':7}]}},
            ]:
                with self.assertRaisesRegex(ValueError,'original binding'):ledger.observe(changed)
                self.assertEqual(ledger.publication,'original-publication')
                self.assertEqual(ledger.records,{})
            binding=ResidentBinding(path,index_id='original-index')
            scope={'index_id':'original-index','catalog':'original-catalog','publication':publication}
            binding.scope(scope)
            for changed in [{**scope,'catalog':'replacement-catalog'},{**scope,'index_id':'foreign-index'},{**scope,'publication':{**publication,'token':'replacement'}}]:
                with self.assertRaisesRegex(ValueError,'scope index/catalog/publication'):binding.scope(changed)
            path.write_text(json.dumps({**receipt,'namespace':'replacement-catalog'}))
            with self.assertRaisesRegex(ValueError,'receipt drift'):ledger.observe(value)

    def test_unicode_bytes_and_index_root_are_authenticated_without_losing_full_hit(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'sources'/'paper';source.mkdir(parents=True)
            raw='First line.\nCafé 日本語 🚀.\nLast line.'.encode();(source/'text.txt').write_bytes(raw);sha=hashlib.sha256(raw).hexdigest()
            manifest={'files':[{'file':'sources/paper/text.txt','bytes':len(raw),'sha256':sha,'paragraphs':[{'original_url':'https://example.invalid/original'}]}]}
            (root/'manifest.private.json').write_text(json.dumps(manifest))
            ledger=SourceLedger(root,source,hashlib.sha256((root/'manifest.private.json').read_bytes()).hexdigest())
            hit={'citation':{'path':'text.txt','content_hash':sha,'start_line':2,'end_line':2,'location':{'Lines':{'start':2,'end':2}}},'text':'Café 日本語 🚀.\n','truncated':False,'future_field':{'retained':True}}
            other={**hit,'citation':{**hit['citation'],'start_line':1,'end_line':1},'text':'First line.\n'}
            value={'publication':{'token':'frozen','extra':3},'hits':[hit,other]}
            self.assertEqual(ledger.observe(value),['r1','r2']);record=ledger.visible_candidates()[0]
            self.assertEqual(record['id'],'r1')
            self.assertEqual(raw[record['byte_start']:record['byte_end']],hit['text'].encode())
            self.assertEqual(record['native_observations'][0]['full_hit'],hit)
            ledger.observe({**value,'hits':[other]})
            self.assertEqual([record['id'] for record in ledger.visible_candidates()],['r2','r1'])
            self.assertEqual(ledger.read_followup('r1'),('context',{'path':'text.txt','line':2,'before':0,'after':32,'max_bytes':4096,'revision':'frozen','response_mode':'full','include_status':True}))
            with self.assertRaisesRegex(ValueError,'unreturned'):ledger.read_followup('r999')
            bad={**hit,'citation':{**hit['citation'],'content_hash':'0'*64}}
            with self.assertRaisesRegex(ValueError,'identity'):ledger.observe({'publication':value['publication'],'hits':[bad]})
            with self.assertRaisesRegex(ValueError,'publication'):ledger.observe({'publication':{'token':'changed'},'hits':[]})
