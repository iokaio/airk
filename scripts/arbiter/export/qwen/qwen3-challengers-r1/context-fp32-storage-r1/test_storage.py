# SPDX-License-Identifier: Apache-2.0
import contextlib,hashlib,io,json,sys,tempfile,unittest
from pathlib import Path
import pack_onnx as p
class StorageTests(unittest.TestCase):
 def test_byte_exact_aligned_dedup(self):
  with tempfile.TemporaryDirectory() as d:
   d=Path(d);(d/'source').write_bytes(bytes(range(100)))
   rows=p.pack_ranges(d,[dict(name='a',location='source',offset=2,length=9),dict(name='b',location='source',offset=40,length=8),dict(name='alias',location='source',offset=2,length=9)],d/'packed')
   blob=(d/'packed').read_bytes()
   self.assertEqual(blob[:9],bytes(range(2,11)));self.assertEqual(rows[1]['packed_offset'],64);self.assertEqual(rows[2]['packed_offset'],0)
 def test_portable_path_refusals(self):
  with tempfile.TemporaryDirectory() as d:
   for name in ['../x','x/../y','x\\y','C:x','/x','..','']:
    with self.subTest(name=name),self.assertRaises(ValueError):p.source_range(d,name,0,1)
 def test_range_refusals(self):
  with tempfile.TemporaryDirectory() as d:
   d=Path(d);(d/'source').write_bytes(b'ab')
   for offset,length in [(-1,1),(0,0),(1,2),(True,1)]:
    with self.subTest(offset=offset,length=length),self.assertRaises(ValueError):p.source_range(d,'source',offset,length)
 def test_no_overwrite(self):
  with tempfile.TemporaryDirectory() as d:
   d=Path(d);(d/'packed').write_bytes(b'original')
   with self.assertRaises(ValueError):p.pack_ranges(d,[],d/'packed')
   self.assertEqual((d/'packed').read_bytes(),b'original')
 def test_size_cap(self):
  with tempfile.TemporaryDirectory() as d:
   d=Path(d);(d/'source').write_bytes(b'ab');old=p.MAX_DATA;p.MAX_DATA=1
   try:
    with self.assertRaises(ValueError):p.pack_ranges(d,[dict(name='a',location='source',offset=0,length=2)],d/'packed')
   finally:p.MAX_DATA=old
 def test_end_to_end_and_source_identity(self):
  import onnx
  from onnx import helper,TensorProto as T
  root=Path(__file__).resolve().parent
  with tempfile.TemporaryDirectory(dir=root) as directory:
   d=Path(directory);source=d/'source';source.mkdir();(source/'weights').write_bytes(b'\0'*8)
   tensor=T(name='weight',data_type=T.FLOAT,dims=[1,2],data_location=T.EXTERNAL)
   for key,value in [('location','weights'),('offset','0'),('length','8')]:e=tensor.external_data.add();e.key=key;e.value=value
   names=[('input_ids',T.INT64,[1,'tokens']),('attention_mask',T.INT64,[1,'tokens']),('position_ids',T.INT64,[1,'tokens']),('decision_positions',T.INT64,[1]),('candidate_positions',T.INT64,[1,'options']),('option_mask',T.BOOL,[1,'options'])]
   graph=helper.make_graph([helper.make_node('Identity',['weight'],['scores'])],'synthetic',[helper.make_tensor_value_info(*x) for x in names],[helper.make_tensor_value_info('scores',T.FLOAT,[1,'options'])],[tensor]);model=helper.make_model(graph,opset_imports=[helper.make_opsetid('',18)]);onnx.save_model(model,source/'model.onnx')
   for i in range(3):(source/('case-%d.npz'%i)).write_bytes(b'unchanged synthetic fixture')
   pins={f.name:p.sha(f) for f in source.iterdir()};receipt=source/'receipt.json';receipt.write_text(json.dumps(dict(complete=True,stage='qwen3-scores-only-export',model_id='Qwen/Qwen3-1.7B',artifact_sha256=pins)));output=d/'output';release=d/'release.json';release.write_text(json.dumps(dict(execution_authorized=True,authorized_stage='qwen3-context-fp32-storage',fresh_output=str(output),source_export=str(source),source_export_receipt_sha256=p.sha(receipt))))
   old=sys.argv;sys.argv=['pack','--release',str(release),'--output',str(output)]
   try:
    with contextlib.redirect_stdout(io.StringIO()):p.main()
   finally:sys.argv=old
   result=json.loads((output/'receipt.json').read_bytes());self.assertTrue(result['complete']);self.assertTrue(result['unchanged_graph_except_external_locations_ranges']);self.assertEqual({f.name for f in (output/'model').iterdir()},{'model.onnx','model.onnx.data'});self.assertEqual((output/'model/model.onnx.data').read_bytes(),b'\0'*8)
   for name,pin in pins.items():self.assertEqual(p.sha(source/name),pin)
 def test_imports_no_neural_runtime(self):
  self.assertNotIn('torch',sys.modules);self.assertNotIn('onnxruntime',sys.modules)
if __name__=='__main__':unittest.main()
