# SPDX-License-Identifier: Apache-2.0
"""Literal controls for exporter-global attention shape constants; no graph imports."""
import unittest,copy
from gate_protocol import removable_attention_axis_constant as allow

class RemovedAxis(unittest.TestCase):
    def setUp(self):
        self.args=['Constant_2599','Constant',[],['onnx::Unsqueeze_428'],(7,[1],[0]),
            [{'name':'/scorer/layers.0/self_attn/Unsqueeze_4','op':'Unsqueeze','inputs':['/scorer/layers.0/self_attn/Gather_2_output_0','onnx::Unsqueeze_428']}],
            {'/scorer/layers.0/self_attn/Unsqueeze_4'}]
    def test_literal_global_constant_is_exclusive_attention_axis(self):
        self.assertEqual(allow(*self.args),0)
        self.assertFalse(self.args[0].startswith('/scorer/layers.0/self_attn/'))
    def test_second_literal_exclusive_attention_axis(self):
        self.args[0]='Constant_2601';self.args[3]=['onnx::Unsqueeze_430']
        self.args[5]=[{'name':'/scorer/layers.0/self_attn/Unsqueeze_5','op':'Unsqueeze','inputs':['/scorer/layers.0/self_attn/Gather_1_output_0','onnx::Unsqueeze_430']}]
        self.args[6]={'/scorer/layers.0/self_attn/Unsqueeze_5'}
        self.assertEqual(allow(*self.args),0)
    def test_value_type_shape_and_operator_changes_refused(self):
        for index,value in [(0,'arbitrary_constant'),(1,'Identity'),(2,['upstream']),
                            (3,[]),(3,['x','y']),(4,(1,[1],[0.])),(4,(7,[],0)),(4,(7,[1],[1]))]:
            args=copy.deepcopy(self.args);args[index]=value
            with self.subTest(index=index,value=value),self.assertRaises(ValueError):allow(*args)
    def test_retained_consumer_refused(self):
        self.args[6]=set()
        with self.assertRaises(ValueError):allow(*self.args)
    def test_nonattention_projection_and_data_input_refused(self):
        for user in [
            {'name':'/scorer/readout/Unsqueeze','op':'Unsqueeze','inputs':['x','onnx::Unsqueeze_428']},
            {'name':'/scorer/layers.0/self_attn/q_proj/Unsqueeze','op':'Unsqueeze','inputs':['x','onnx::Unsqueeze_428']},
            {'name':'/scorer/layers.0/self_attn/Unsqueeze_4','op':'Unsqueeze','inputs':['onnx::Unsqueeze_428','other_axis']}]:
            args=copy.deepcopy(self.args);args[5]=[user];args[6]={user['name']}
            with self.subTest(user=user),self.assertRaises(ValueError):allow(*args)
    def test_shared_axis_refused_even_if_attention_only(self):
        self.args[5].append(copy.deepcopy(self.args[5][0]))
        with self.assertRaises(ValueError):allow(*self.args)
if __name__=='__main__':unittest.main()