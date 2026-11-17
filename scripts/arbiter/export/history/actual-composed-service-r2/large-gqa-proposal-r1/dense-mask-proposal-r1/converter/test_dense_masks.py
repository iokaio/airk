# SPDX-License-Identifier: Apache-2.0
"""Original explicit-mask expression vs dense interface; not native kernel qualification."""
import math,unittest
from gate_protocol import cache_contract
from transformer import validate_prior_node
import struct
TYPES=[1,17,128,8192]
def original_bias(query,key,live):return 0. if key<=query and live[key] else -3.4028234663852886e38
def dense_weight(query,key,live):
    causal=(key<=query)
    allowed=causal and bool(live[key])
    return 0. if allowed else -3.4028234663852886e38
class Masks(unittest.TestCase):
    def test_full_physical_domains_match_original_explicit_bias(self):
        for tokens in TYPES:
            for pattern in [0,1,2]:
                live=[1]*tokens
                if pattern==1 and tokens>1:live[1]=0
                if pattern==2 and tokens>1:live[-1]=0
                for query in sorted(set([0,tokens//2,tokens-1])):
                    for key in range(tokens):self.assertEqual(original_bias(query,key,live),dense_weight(query,key,live))
    def test_literal_boundary_causal_and_padding_refusals(self):
        for tokens in TYPES:
            live=[1]*tokens
            self.assertEqual(original_bias(0,0,live),0.)
            if tokens>1:
                self.assertLess(original_bias(0,tokens-1,live),-1e30)
                self.assertNotEqual(original_bias(0,tokens-1,live),0.)
                live[0]=0;self.assertLess(original_bias(tokens-1,0,live),-1e30)
    def test_dense_native_contract_refuses_flash_or_wrong_bias_source(self):
        ns='/airk_large_gqa/';inputs=['q','k','v','','',ns+'seqlen32',ns+'total32','','','','/scorer/Where_output_0'];outputs=['c','kcache','vcache']
        attrs=dict(num_heads=32,kv_num_heads=8,causal=0,local_window_size=-1,do_rotary=0,softcap=0.,smooth_softmax=0,scale=struct.unpack('<f',bytes.fromhex('f304b53d'))[0])
        for t in TYPES:
            known={'q':(1,[1,t,4096]),'k':(1,[1,t,1024]),'v':(1,[1,t,1024]),inputs[10]:(1,[1,1,t,t]),inputs[5]:(6,[1]),inputs[6]:(6,[1])}
            self.assertEqual(cache_contract(inputs,outputs,attrs,known,t,ns),{'kcache':(1,[1,8,t,128]),'vcache':(1,[1,8,t,128])})
            changed=dict(attrs);changed['causal']=1
            with self.assertRaises(ValueError):cache_contract(inputs,outputs,changed,known,t,ns)
            bad=list(inputs);bad[10]='dropped_original_bias'
            with self.assertRaises(ValueError):cache_contract(bad,outputs,attrs,known,t,ns)
    def test_exact_source_node_only_accepts_causal_one_before_conversion(self):
        attrs=dict(num_heads=32,kv_num_heads=8,causal=1,local_window_size=-1,do_rotary=0,softcap=0.,smooth_softmax=0,scale=struct.unpack('<f',bytes.fromhex('f304b53d'))[0])
        inputs=['q','k','v','','','seqlen','total','','','','/scorer/Where_output_0'];outputs=['context','kcache','vcache']
        validate_prior_node(attrs,inputs,outputs,'com.microsoft')
        for key,value in [('causal',0),('num_heads',16),('kv_num_heads',4),('scale',1.),('do_rotary',1),('softcap',1.),('smooth_softmax',1),('local_window_size',128)]:
            bad=dict(attrs);bad[key]=value
            with self.assertRaises(ValueError):validate_prior_node(bad,inputs,outputs,'com.microsoft')
        for index,value in [(3,'past'),(7,'rotary'),(10,'new_bias')]:
            bad=list(inputs);bad[index]=value
            with self.assertRaises(ValueError):validate_prior_node(attrs,bad,outputs,'com.microsoft')
    def test_nontrivial_qk_mask_really_matters(self):
        logits=[.3,-.7,1.2,2.];values=[3.,7.,11.,19.]
        def output(bias):
            z=[x+y for x,y in zip(logits,bias)];m=max(z);w=[math.exp(x-m) for x in z];return sum(a*b for a,b in zip(w,values))/sum(w)
        causal=[0.,0.,-3.4028234663852886e38,-3.4028234663852886e38]
        self.assertAlmostEqual(output(causal),4.075765685479981,places=12)
        self.assertGreater(abs(output(causal)-output([0.]*4)),5.)
if __name__=='__main__':unittest.main()
