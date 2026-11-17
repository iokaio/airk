# SPDX-License-Identifier: Apache-2.0
"""Synthetic old/new resource reproduction and independent source/range expectations."""
import ast
import contextlib
import hashlib
import io
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

HERE=Path(__file__).resolve().parent
OLD=HERE.parent/'source/goal_rubric.py'
NEW=HERE/'goal_rubric_streaming.py'
EXPECTED='c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6'

def digest(value):return hashlib.sha256(value).hexdigest()

class FakeBatch:
    def __init__(self,rows):self.rows=rows
    def to_pylist(self):return self.rows

class StreamingRubricTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=HERE)
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.work=self.root/'artifacts';self.work.mkdir()
        self.fixture=self.work/'fixture';self.fixture.mkdir()
        self.body='Café. Beta.'.encode()
        self.source=self.fixture/'sources/hotpot/synthetic.txt';self.source.parent.mkdir(parents=True);self.source.write_bytes(self.body)
        self.raw=self.work/'synthetic.parquet';self.raw.write_bytes(b'synthetic parquet bytes; scanner is an independent fixture')
        self.rows=[{'id':'q-synthetic','context':{'title':['SyntheticTitle'],'sentences':[['Café. ','Beta.']]},'type':'bridge','level':'easy'}]
        document={'id':'paragraph-synthetic','text':self.body.decode()}
        task={'id':'hotpot:q-synthetic','question_id':'q-synthetic','dataset':'HotpotQA','split':'test','publisher_split':'validation','question':'Synthetic question?','disagreement':False,'source_component':'component-synthetic','human_alternatives':[{'unanswerable':False,'alignment':'all_original_evidence_maps_to_allowed_text','human_annotation_id':'original-annotator','original_human_answer':'Beta','evidence':[{'allowed_locations':['paragraph-synthetic'],'source_title':'SyntheticTitle','sentence_index':1,'source_evidence_sha256':digest(b'Beta.') }]}]}
        self.corpus=self.work/'corpus.json';self.corpus.write_text(json.dumps({'experiment_partition':'CONFIRMATION','documents':[document],'tasks':[task]}))
        manifest={'experiment_partition':'CONFIRMATION','goal_ids':['hotpot:q-synthetic'],'files':[{'file':'sources/hotpot/synthetic.txt','bytes':len(self.body),'sha256':digest(self.body)}],'paragraphs':[{'id':'paragraph-synthetic','file':'sources/hotpot/synthetic.txt','byte_start':0,'byte_end':len(self.body),'original_url':'https://example.invalid/source','source_group':'source-synthetic'}]}
        self.manifest=self.fixture/'manifest.private.json';self.manifest.write_text(json.dumps(manifest))
        self.calls=[]
        arrow=types.ModuleType('pyarrow');arrow.__path__=[];arrow.set_cpu_count=lambda n:self.calls.append(('cpu',n));arrow.set_io_thread_count=lambda n:self.calls.append(('io',n))
        dataset=types.ModuleType('pyarrow.dataset')
        class Field:
            def isin(inner,values):self.calls.append(('ids',values));return 'selected-id-filter'
        dataset.field=lambda name:Field()
        dataset.ParquetFragmentScanOptions=lambda **kwargs:kwargs
        def scanner(**kwargs):
            self.calls.append(('scanner',kwargs));return types.SimpleNamespace(to_batches=lambda:[FakeBatch(self.rows)])
        dataset.dataset=lambda *args,**kwargs:types.SimpleNamespace(scanner=scanner)
        arrow.dataset=dataset
        self.modules={'pyarrow':arrow,'pyarrow.dataset':dataset}
    def module(self,path):
        source=path.read_text().replace(EXPECTED,digest(self.raw.read_bytes()))
        value=types.ModuleType('synthetic_rubric_owner');value.__file__=str(path)
        exec(compile(source,str(path),'exec'),value.__dict__);value.ROOT=self.root
        return value
    def run_owner(self,path,name,deny_whole=False,drift=False):
        owner=self.module(path);output=self.work/name
        argv=[str(path),'--corpus',str(self.corpus),'--corpus-sha256',digest(self.corpus.read_bytes()),'--fixture',str(self.fixture),'--source-manifest-sha256',digest(self.manifest.read_bytes()),'--hotpot',str(self.raw),'--output',str(output),'--partition','CONFIRMATION']
        read_bytes=Path.read_bytes
        def guarded(item):
            if deny_whole and item==self.raw:raise MemoryError('synthetic forbidden full raw parquet allocation')
            return read_bytes(item)
        if drift:
            original=self.source.read_bytes()
            self.source.write_bytes(original+b' changed')
        with patch.dict('sys.modules',self.modules),patch('sys.argv',argv),patch.object(Path,'read_bytes',guarded),contextlib.redirect_stdout(io.StringIO()):owner.main()
        return json.loads((output/'rubric.private.json').read_bytes())
    def test_old_fails_new_passes_without_full_raw_allocation(self):
        with self.assertRaisesRegex(MemoryError,'forbidden full raw'):
            self.run_owner(OLD,'old-failure',deny_whole=True)
        result=self.run_owner(NEW,'new-pass',deny_whole=True)
        self.assertEqual(len(result['tasks']),1)
    def test_unchanged_tasks_and_independent_utf8_range_expectation(self):
        old=self.run_owner(OLD,'old-equivalence');new=self.run_owner(NEW,'new-equivalence')
        self.assertEqual(old['tasks'],new['tasks'])
        for key in ('authority','projection','caller_requirements','schema','experiment_partition'):self.assertEqual(old[key],new[key])
        task=new['tasks'][0]
        self.assertEqual(task['alternatives'],[{'human_annotation_id':'original-annotator','answer':'Beta','answer_kind':'short_answer','facts':[{'original_evidence_sha256':digest(b'Beta.'),'locations':[{'file':'sources/hotpot/synthetic.txt','byte_start':7,'byte_end':12,'original_url':'https://example.invalid/source','source_group':'source-synthetic'}]}]}])
        self.assertEqual(task['source_component'],'component-synthetic')
        self.assertEqual(task['bootstrap_source_component'],'component-synthetic')
    def test_exact_selected_projection_and_scanner_caps_unchanged(self):
        self.run_owner(NEW,'projection',deny_whole=True)
        args=next(value for key,value in self.calls if key=='scanner')
        self.assertEqual(args['columns'],['id','context','type','level'])
        self.assertEqual((args['batch_size'],args['batch_readahead'],args['fragment_readahead'],args['use_threads']),(32,0,0,False))
        self.assertEqual(args['fragment_scan_options'],{'use_buffered_stream':True,'buffer_size':8192,'pre_buffer':False})
        self.assertIn(('ids',['q-synthetic']),self.calls)
    def test_changed_source_refuses_no_rubric(self):
        with self.assertRaisesRegex(ValueError,'native source bytes differ'):self.run_owner(NEW,'changed',drift=True)
        self.assertFalse((self.work/'changed').exists())
    def test_streaming_digest_matches_independent_bytes_digest(self):
        owner=self.module(NEW)
        data=b'0123456789abcdef'*10001
        path=self.work/'chunks';path.write_bytes(data)
        with patch.object(Path,'read_bytes',side_effect=AssertionError('whole-file read forbidden')):
            self.assertEqual(owner.streaming_sha256(path,4096),hashlib.sha256(data).hexdigest())
    def test_projection_grade_functions_byte_equivalent_ast(self):
        old=ast.parse(OLD.read_text());new=ast.parse(NEW.read_text())
        functions=lambda tree:{x.name:ast.dump(x,include_attributes=False) for x in tree.body if isinstance(x,ast.FunctionDef) and x.name!='main'}
        left=functions(old);right=functions(new)
        self.assertEqual({name:right[name] for name in left},left)

if __name__=='__main__':unittest.main()
