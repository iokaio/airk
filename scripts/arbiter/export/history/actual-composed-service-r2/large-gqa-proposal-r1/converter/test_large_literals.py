# SPDX-License-Identifier: Apache-2.0
"""Independent Large literals; no ONNX/ORT import or graph creation."""
import struct,unittest
from gate_protocol import cache_contract,removable_attention_axis_constant
from test_attention_semantics import fixture,dense_reference,grouped_online,error
class LargeLiteral(unittest.TestCase):
    def test_q32_kv8_uses4096_not_backbone2560(self):
        ns='/airk_large_gqa/'
        inputs=['q','k','v','','',ns+'seqlen32',ns+'total32','','','','/scorer/Where_output_0']
        outputs=['context','cache_k','cache_v']
        attrs=dict(num_heads=32,kv_num_heads=8,causal=1,local_window_size=-1,do_rotary=0,softcap=0.,smooth_softmax=0,scale=struct.unpack('<f',bytes.fromhex('f304b53d'))[0])
        known={'q':(1,[1,8192,4096]),'k':(1,[1,8192,1024]),'v':(1,[1,8192,1024]),inputs[10]:(1,[1,1,8192,8192]),inputs[5]:(6,[1]),inputs[6]:(6,[1])}
        self.assertEqual(cache_contract(inputs,outputs,attrs,known,8192,ns),{'cache_k':(1,[1,8,8192,128]),'cache_v':(1,[1,8,8192,128])})
        for width in [2048,2560]:
            bad=dict(known);bad['q']=(1,[1,8192,width])
            with self.assertRaises(ValueError):cache_contract(inputs,outputs,attrs,bad,8192,ns)
        attrs['num_heads']=16
        with self.assertRaises(ValueError):cache_contract(inputs,outputs,attrs,known,8192,ns)
    def test_large_four_queries_per_kv_matches_dense(self):
        q,k,v=fixture();q=q+[[[a+3 for a in row] for row in head] for head in q]
        self.assertLess(error(dense_reference(q,k,v,[1,1,1],.5),grouped_online(q,k,v,[1,1,1],.5)),1e-12)
        self.assertGreater(error(dense_reference(q,k,v,[1,1,1],.5),grouped_online(q,k,v,[1,1,1],.5,wrong_group=True)),.1)
    def test_last_large_axis_layer_allowed_next_refused(self):
        for layer,accept in [(35,True),(36,False)]:
            name=f'/scorer/layers.{layer}/self_attn/Unsqueeze_4'
            args=['Constant_9999','Constant',[],['axis'],(7,[1],[0]),[{'name':name,'op':'Unsqueeze','inputs':['tensor','axis']}],{name}]
            if accept:self.assertEqual(removable_attention_axis_constant(*args),35)
            else:
                with self.assertRaises(ValueError):removable_attention_axis_constant(*args)
if __name__=='__main__':unittest.main()
