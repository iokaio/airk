# SPDX-License-Identifier: Apache-2.0
"""Synthetic storage/math controls; no real model or expected-logit generation."""
import copy
import json
from pathlib import Path
import struct
import tempfile
import unittest
import onnx
import torch
from focus_w8_reference import (BLOCK, CheckedPlan, Reader, apply_candidate, cpu_state,
    decode_weight, expected_nonquantized_keys, expected_projection_keys, node_semantics,
    projection_key, quantized_layout)
BASE=Path(__file__).parent
WORK=Path(__file__).resolve().parents[4]/'artifacts/focus-w8-test-work'
WORK.mkdir(exist_ok=True)

class Weight8Controls(unittest.TestCase):
    def test_exact_synthetic_unsigned_block_math_and_scale_orientation(self):
        q=bytes(([0,127,128,255]*16)*2)
        scales=[0.5,-0.25,0.,2.]
        decoded=decode_weight(torch,q,struct.pack('<4f',*scales),2,64)
        expected=torch.tensor([[(q[n*64+k]-128)*scales[n*2+k//32]
                                for k in range(64)] for n in range(2)],dtype=torch.float32)
        self.assertTrue(torch.equal(decoded,expected))
        self.assertEqual(decoded[0,:4].tolist(),[-64.,-0.5,0.,63.5])
        self.assertEqual(decoded[0,32:36].tolist(),[32.,0.25,-0.,-31.75])
        self.assertEqual(decoded[1,32:36].tolist(),[-256.,-2.,0.,254.])
    def test_invalid_or_nonfinite_weight_storage_refuses(self):
        for q,scale,n,k in [(bytes(31),struct.pack('<f',1),1,32),
            (bytes(32),bytes(3),1,32),(bytes(32),struct.pack('<f',float('nan')),1,32),
            (bytes(32),struct.pack('<f',float('inf')),1,32),(bytes(33),bytes(4),1,33)]:
            with self.assertRaises(ValueError):decode_weight(torch,q,scale,n,k)
    def make_projection(self):
        q=onnx.helper.make_tensor('q',onnx.TensorProto.UINT8,[2,2,32],bytes(128),raw=True)
        s=onnx.helper.make_tensor('scales',onnx.TensorProto.FLOAT,[2,2],struct.pack('<4f',1,1,1,1),raw=True)
        node=onnx.helper.make_node('MatMulNBits',['activation','q','scales'],['out'],
            name='/MatMul_Q8',domain='com.microsoft',K=64,N=2,bits=8,block_size=32)
        return node,{'q':q,'scales':s}
    def test_exact_operator_metadata_and_unknown_features(self):
        node,initial=self.make_projection()
        k,n,_,_=quantized_layout(node,initial)
        self.assertEqual((k,n),(64,2))
        for attribute,value in [('accuracy_level',0),('weight_prepacked',0),('bias',0),('g_idx',0)]:
            bad=copy.deepcopy(node);bad.attribute.append(onnx.helper.make_attribute(attribute,value))
            with self.assertRaises(ValueError):quantized_layout(bad,initial)
        bad=copy.deepcopy(node);bad.input.append('zero_points')
        with self.assertRaises(ValueError):quantized_layout(bad,initial)
        bad=copy.deepcopy(node);bad.domain=''
        with self.assertRaises(ValueError):quantized_layout(bad,initial)
        initial['q'].data_type=onnx.TensorProto.FLOAT
        with self.assertRaises(ValueError):quantized_layout(node,initial)
    def test_full_publisher_rosters_and_readout_are_independent_literals(self):
        self.assertEqual(len(expected_projection_keys()),197)
        self.assertEqual(len(expected_nonquantized_keys()),114)
        self.assertFalse(expected_projection_keys() & expected_nonquantized_keys())
        self.assertEqual(projection_key('/MatMul_Q8'),'readout_rows')
        self.assertEqual(projection_key('/base/layers.27/mlp/down_proj/MatMul_Q8'),
                         'base.layers.27.mlp.down_proj.weight')
        for name in ['/base/layers.28/mlp/down_proj/MatMul_Q8',
                     '/base/layers.0/self_attn/down_proj/MatMul_Q8','/unknown/MatMul_Q8']:
            with self.assertRaises(ValueError):projection_key(name)
    def test_storage_normalization_preserves_exact_constant_semantics(self):
        with tempfile.TemporaryDirectory(dir=WORK) as temp:
            root=Path(temp).resolve();self.assertTrue(root.is_relative_to(WORK.resolve()))
            raw=struct.pack('<2f',-1.25,2.5);(root/'constant.bin').write_bytes(raw)
            tensor=onnx.helper.make_tensor('inline',onnx.TensorProto.FLOAT,[2],raw,raw=True)
            inline=onnx.helper.make_node('Constant',[],['value'],name='constant',value=tensor)
            external=copy.deepcopy(inline);value=external.attribute[0].t
            value.ClearField('raw_data');value.data_location=onnx.TensorProto.EXTERNAL
            for key,text in [('location','constant.bin'),('offset','0'),('length','8')]:
                item=value.external_data.add();item.key=key;item.value=text
            graph=root/'graph.onnx';graph.write_bytes(b'synthetic-only')
            reader=Reader(graph)
            try:
                self.assertEqual(node_semantics(inline,reader),node_semantics(external,reader))
                strict=Reader(graph,{graph.name:graph})
                try:
                    with self.assertRaisesRegex(ValueError,'file map'):node_semantics(external,strict)
                finally:strict.close()
                value.external_data[0].value='../constant.bin'
                with self.assertRaises(ValueError):node_semantics(external,reader)
            finally:reader.close()
    def test_original_state_type_shape_and_finiteness_refuse(self):
        cpu_state(torch,torch.tensor([[1.,2.]],dtype=torch.float32),(1,2))
        for value,shape in [(torch.tensor([1.],dtype=torch.float64),(1,)),
            (torch.tensor([1.]),(2,)),(torch.tensor([float('nan')]),(1,)),
            (torch.zeros(2,3).t(),(3,2))]:
            with self.assertRaises(ValueError):cpu_state(torch,value,shape)
    def test_missing_full_state_refuses_before_any_model_mutation(self):
        class SyntheticWrapper(torch.nn.Module):
            def __init__(self):super().__init__();self.weight=torch.nn.Parameter(torch.tensor([7.]))
        wrapper=SyntheticWrapper();plan=CheckedPlan();plan.signatures={}
        with self.assertRaises(ValueError):apply_candidate(torch,wrapper,plan,wrapper.state_dict())
        self.assertEqual(wrapper.weight.detach().tolist(),[7.])
        with self.assertRaises(ValueError):apply_candidate(torch,wrapper,object(),{})

if __name__=='__main__':unittest.main()
