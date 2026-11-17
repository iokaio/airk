# SPDX-License-Identifier: Apache-2.0
"""Synthetic exact full-result bridge controls; no grading or model calls."""
import copy
import importlib.util
import hashlib
from pathlib import Path
import tempfile
import sys
import types
import unittest
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('candidate_grade',HERE/'grade_candidates.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
class BridgeTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=HERE,prefix='bridge-test-');self.addCleanup(self.temp.cleanup)
        self.base={'family':'control','model_json_sha256':'model','pipeline_digest':'pipeline','runtime':'same','encoder_input_sha256':'master','corpus_sha256':'corpus','quality_mask_sha256':'mask','source_manifest_sha256':'source','source_before_sha256':'roster','source_after_sha256':'roster','source_roster_count':366,'query':{'mode':'vector','limit':10,'max_bytes':32768,'full_chunks':True},'complete':True,'binary_sha256':'old','records':[{'id':f'q{i}','source_root':self.temp.name,'query_us':1,'primary_vector':{'publication':'exact','hits':[{'text':'Café 日本語 🚀','unknown_supported':{'all':[1,2,3]}}]}} for i in range(64)]}
        self.base['collections']=[{'publication':'exact','reused':False,'source_root':self.temp.name}]
    def new_control(self):
        new=copy.deepcopy(self.base)
        new['collections'][0]['reused']=True
        return new
    def test_only_outer_binary_and_timing_may_change(self):
        new=self.new_control();new['binary_sha256']='new';new['records'][0]['query_us']=100
        self.assertTrue(owner.bridge(self.base,new)['all64_full_primary_vector_equal'])
    def test_any_native_field_or_shown_byte_or_publication_difference_refuses(self):
        for change in ('text','unknown','publication'):
            with self.subTest(change=change):
                new=self.new_control();value=new['records'][0]['primary_vector']
                if change=='text':value['hits'][0]['text']='Café 日本語'
                elif change=='unknown':value['hits'][0]['unknown_supported']['all'].append(4)
                else:value['publication']='replacement'
                with self.assertRaises(ValueError):owner.bridge(self.base,new)
    def test_missing_request_and_changed_model_refuse(self):
        new=self.new_control();new['records'].pop()
        with self.assertRaises(ValueError):owner.bridge(self.base,new)
        new=self.new_control();new['records'].append(copy.deepcopy(new['records'][0]))
        with self.assertRaises(ValueError):owner.bridge(self.base,new)
        new=self.new_control();new['model_json_sha256']='different'
        with self.assertRaises(ValueError):owner.bridge(self.base,new)
    def test_unknown_outer_record_and_collection_fields_refuse(self):
        for target in ('outer','record','collection'):
            new=self.new_control();where=new if target=='outer' else new['records'][0] if target=='record' else new['collections'][0]
            where['unknown_supported']='preserve-me'
            with self.subTest(target=target),self.assertRaises(ValueError):owner.bridge(self.base,new)
    def test_invalid_reuse_values_refuse(self):
        for value in (False,None,1,'true'):
            new=self.new_control();new['collections'][0]['reused']=value
            with self.subTest(value=value),self.assertRaises(ValueError):owner.bridge(self.base,new)
    def test_mutated_helper_path_cannot_replace_captured_executed_bytes(self):
        path=Path(self.temp.name)/'helper.py';path.write_bytes(b'value=3\n');pin=hashlib.sha256(path.read_bytes()).hexdigest();p,raw=owner.captured(path,pin)
        path.write_bytes(b'raise RuntimeError("foreign replacement")\n')
        with owner.verified_modules([('_candidate_fake_a',p,raw)]) as modules:self.assertEqual(modules['_candidate_fake_a'].value,3)
    def test_foreign_cached_transitive_module_is_replaced_then_restored(self):
        name='_candidate_fake_b';foreign=types.ModuleType(name);foreign.value=99;old=sys.modules.get(name);sys.modules[name]=foreign
        try:
            b=Path(self.temp.name)/'b.py';a=Path(self.temp.name)/'a.py';b.write_bytes(b'value=4\n');a.write_bytes(b'import _candidate_fake_b\nvalue=_candidate_fake_b.value\n')
            rows=[(name,b,b.read_bytes()),('_candidate_fake_a',a,a.read_bytes())]
            with owner.verified_modules(rows) as modules:self.assertEqual(modules['_candidate_fake_a'].value,4)
            self.assertIs(sys.modules[name],foreign)
        finally:
            if old is None:sys.modules.pop(name,None)
            else:sys.modules[name]=old
if __name__=='__main__':unittest.main()
