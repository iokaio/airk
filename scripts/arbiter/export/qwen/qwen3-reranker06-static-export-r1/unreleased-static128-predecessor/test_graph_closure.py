# SPDX-License-Identifier: Apache-2.0
import tempfile
import unittest
from pathlib import Path
from graph_closure import bind_references,graph_closure

class ExternalClosureControls(unittest.TestCase):
    def test_real_onnx_metadata_walker_and_static_abi_without_inference(self):
        import onnx
        from onnx import helper,TensorProto
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'trained.data').write_bytes(b'\0\0\0\0')
            weight=helper.make_tensor('trained',TensorProto.FLOAT,[1],[],raw=False)
            weight.data_location=TensorProto.EXTERNAL
            for key,value in [('location','trained.data'),('offset','0'),('length','4')]:
                item=weight.external_data.add();item.key=key;item.value=value
            inputs=[helper.make_tensor_value_info(name,TensorProto.INT64,[1,128])for name in ['input_ids','attention_mask','position_ids']]
            output=helper.make_tensor_value_info('logits',TensorProto.FLOAT,[1,2])
            node=helper.make_node('Constant',[],['logits'],value=helper.make_tensor('answer',TensorProto.FLOAT,[1,2],[0.,1.]))
            model=helper.make_model(helper.make_graph([node],'metadata-only-test',inputs,[output],[weight]),opset_imports=[helper.make_opsetid('',18)])
            onnx.save_model(model,str(root/'model.onnx'))
            result=graph_closure(root)
            self.assertEqual(result['external_ranges'],[dict(tensor='trained',location='trained.data',offset=0,length=4)])
    def test_full_local_closure_and_exact_ranges(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'model.onnx').write_bytes(b'graph');(root/'trained.data').write_bytes(b'01234567')
            result=bind_references(root,[('w',[('location','trained.data'),('offset','2'),('length','4')])])
            self.assertEqual([r['name']for r in result['files']],['model.onnx','trained.data'])
            self.assertEqual(result['external_ranges'],[dict(tensor='w',location='trained.data',offset=2,length=4)])
    def test_traversal_alias_metadata_and_incomplete_range_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'model.onnx').write_bytes(b'graph');(root/'trained.data').write_bytes(b'1234')
            cases=[[('location',name)]for name in ['../outside','C:outside','nested/file','..','missing']]
            cases += [[('location','trained.data'),('offset','4'),('length','1')],[('location','trained.data'),('length','-1')],[('location','trained.data'),('location','trained.data')]]
            for fields in cases:
                with self.subTest(fields=fields),self.assertRaises(ValueError):bind_references(root,[('w',fields)])
    def test_unknown_sidecar_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            for name in ['model.onnx','trained.data','forgotten.data']:(root/name).write_bytes(b'x')
            with self.assertRaises(ValueError):bind_references(root,[('w',[('location','trained.data')])])
if __name__=='__main__':unittest.main()
