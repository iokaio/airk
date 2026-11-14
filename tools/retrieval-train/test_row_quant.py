# SPDX-License-Identifier: Apache-2.0
import importlib.util
import unittest


@unittest.skipUnless(importlib.util.find_spec('onnx') and importlib.util.find_spec('onnxruntime'),'requires pinned isolated ONNX environment')
class PerTokenQuantizationTests(unittest.TestCase):
    def test_centered_correction_extrema_at_k32768(self):
        import numpy as np
        import onnx
        import onnxruntime as ort
        from row_quant import per_token_unsigned
        a=np.asarray([[[255.]*32768,[-255.]*32768]],dtype=np.float32)
        b=np.tile(np.asarray([255,0],dtype=np.uint8),(32768,1));zb=np.asarray([0,255],dtype=np.uint8)
        expected=a.astype(np.int64)@(b.astype(np.int64)-zb.astype(np.int64))
        self.assertEqual(set(abs(int(value)) for value in expected.flat),{2130739200})
        vi=onnx.helper.make_tensor_value_info;make=onnx.helper.make_node
        model=onnx.helper.make_model(onnx.helper.make_graph([make('DynamicQuantizeLinear',['a'],['aq','scale','za']),
            make('MatMulInteger',['aq','b','za','zb'],['integer']),make('Cast',['integer'],['float'],to=onnx.TensorProto.FLOAT),make('Mul',['float','scale'],['out'])],
            'independent-row-extrema',[vi('a',onnx.TensorProto.FLOAT,[1,2,32768])],[vi('out',onnx.TensorProto.FLOAT,[1,2,2])],
            [onnx.numpy_helper.from_array(b,'b'),onnx.numpy_helper.from_array(zb,'zb')]),opset_imports=[onnx.helper.make_opsetid('',18)])
        model.ir_version=10;per_token_unsigned(model)
        options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1;options.log_severity_level=3
        session=ort.InferenceSession(model.SerializeToString(),options,providers=['CPUExecutionProvider'])
        np.testing.assert_array_equal(session.run(None,{'a':a})[0],expected.astype(np.float32))

    def test_outlier_negative_and_zero_rows_match_independent_centered_integer_algebra(self):
        import numpy as np
        import onnx
        import onnxruntime as ort
        from row_quant import per_token_unsigned
        rng=np.random.default_rng(1908);a=rng.normal(size=(1,4,64)).astype(np.float32)
        a[:,1,:]*=10000;a[:,2,:]=0;a[:,3,:]=-np.abs(a[:,3,:])
        b=rng.integers(0,256,(64,17),dtype=np.uint8);zb=rng.integers(0,256,17,dtype=np.uint8)
        low=np.minimum(a.min(axis=-1,keepdims=True),np.float32(0));high=np.maximum(a.max(axis=-1,keepdims=True),np.float32(0))
        scale=(high-low)/np.float32(255);scale=np.where(high==low,np.float32(1),scale)
        zero=np.clip(np.rint(-low/scale),0,255).astype(np.uint8)
        quantized=np.clip(np.rint(a/scale)+zero.astype(np.float32),0,255).astype(np.uint8)
        expected=((quantized.astype(np.int64)-zero.astype(np.int64))@(b.astype(np.int64)-zb.astype(np.int64))).astype(np.float32)*scale
        vi=onnx.helper.make_tensor_value_info;make=onnx.helper.make_node
        model=onnx.helper.make_model(onnx.helper.make_graph([make('DynamicQuantizeLinear',['a'],['aq','scale','za']),
            make('MatMulInteger',['aq','b','za','zb'],['integer']),make('Cast',['integer'],['float'],to=onnx.TensorProto.FLOAT),make('Mul',['float','scale'],['out'])],
            'independent-per-token-reference',[vi('a',onnx.TensorProto.FLOAT,[1,4,64])],[vi('out',onnx.TensorProto.FLOAT,[1,4,17])],
            [onnx.numpy_helper.from_array(b,'b'),onnx.numpy_helper.from_array(zb,'zb')]),opset_imports=[onnx.helper.make_opsetid('',18)])
        model.ir_version=10;self.assertEqual(per_token_unsigned(model)['corrected_matmuls'],1)
        options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1;options.log_severity_level=3
        session=ort.InferenceSession(model.SerializeToString(),options,providers=['CPUExecutionProvider'])
        np.testing.assert_allclose(session.run(None,{'a':a})[0],expected,rtol=2e-6,atol=1e-4)
