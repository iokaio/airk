# SPDX-License-Identifier: Apache-2.0
import struct
import tempfile
import unittest
from pathlib import Path
import onnx
from onnx import helper,TensorProto
from storage_normalizer import normalize_external_storage

def external(name,dtype,dims,offset,length,location='model.onnx.data'):
    t=TensorProto();t.name=name;t.data_type=dtype;t.dims.extend(dims);t.data_location=TensorProto.EXTERNAL
    for k,v in [('location',location),('offset',str(offset)),('length',str(length))]:
        e=t.external_data.add();e.key=k;e.value=v
    return t
def fixture(root,*,length=16,location='model.onnx.data',local_function=False):
    root.mkdir();data=struct.pack('<fqq',1.,7,11);(root/'model.onnx.data').write_bytes(data)
    weight=external('trained',TensorProto.FLOAT,[1],0,4)
    value=external('literal',TensorProto.INT64,[2],4,length,location)
    node=helper.make_node('Constant',[],['out'],value=value)
    m=helper.make_model(helper.make_graph([node],'synthetic-storage-only',[],[helper.make_tensor_value_info('out',TensorProto.INT64,[2])],[weight]),opset_imports=[helper.make_opsetid('',18)])
    if local_function:m.functions.add().name='unsupported-function'
    raw=m.SerializeToString(deterministic=True);(root/'model.onnx').write_bytes(raw)
    return raw,data,weight.SerializeToString(deterministic=True)

class StorageNormalizationControls(unittest.TestCase):
    def test_storage_only_inline_preserves_every_value_and_main_declaration(self):
        with tempfile.TemporaryDirectory()as d:
            base=Path(d);root=base/'model';raw,data,main=fixture(root)
            p=normalize_external_storage(root/'model.onnx',original_graph_path=base/'original.onnx',proof_path=base/'proof.json')
            m=onnx.load_model(str(root/'model.onnx'),load_external_data=False);value=m.graph.node[0].attribute[0].t
            self.assertEqual(value.raw_data,struct.pack('<qq',7,11));self.assertFalse(value.external_data)
            self.assertEqual(value.data_location,TensorProto.DEFAULT)
            self.assertEqual(m.graph.initializer[0].SerializeToString(deterministic=True),main)
            self.assertEqual((root/'model.onnx.data').read_bytes(),data);self.assertEqual((base/'original.onnx').read_bytes(),raw)
            self.assertEqual(len(p['inlined']),1);self.assertEqual(p['remaining_external_main_tensors'],1)
            self.assertTrue(p['topology_restored_identity']);self.assertFalse(p['model_executed'])
    def test_shape_range_mismatch_refuses_before_mutating_graph(self):
        with tempfile.TemporaryDirectory()as d:
            base=Path(d);root=base/'model';raw,_,_=fixture(root,length=8)
            with self.assertRaises(ValueError):normalize_external_storage(root/'model.onnx',original_graph_path=base/'original.onnx',proof_path=base/'proof.json')
            self.assertEqual((root/'model.onnx').read_bytes(),raw);self.assertFalse((base/'original.onnx').exists())
    def test_path_escape_refuses_before_mutation(self):
        with tempfile.TemporaryDirectory()as d:
            base=Path(d);root=base/'model';raw,_,_=fixture(root,location='../outside')
            with self.assertRaises(ValueError):normalize_external_storage(root/'model.onnx',original_graph_path=base/'original.onnx',proof_path=base/'proof.json')
            self.assertEqual((root/'model.onnx').read_bytes(),raw)
    def test_local_function_root_remains_refused(self):
        with tempfile.TemporaryDirectory()as d:
            base=Path(d);root=base/'model';raw,_,_=fixture(root,local_function=True)
            with self.assertRaises(ValueError):normalize_external_storage(root/'model.onnx',original_graph_path=base/'original.onnx',proof_path=base/'proof.json')
            self.assertEqual((root/'model.onnx').read_bytes(),raw)
if __name__=='__main__':unittest.main()
