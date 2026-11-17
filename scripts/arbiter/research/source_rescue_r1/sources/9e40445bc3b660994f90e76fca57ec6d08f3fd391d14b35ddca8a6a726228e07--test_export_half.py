# SPDX-License-Identifier: Apache-2.0
"""Literal exporter mechanics only. No selected graphs, weights or runtimes."""
import copy, importlib.util, math, pathlib, struct, tempfile, unittest
import numpy as np
import onnx
from onnx import helper as h, TensorProto as T
import export_half as e

class ScalarControls(unittest.TestCase):
    def test_half_literal_bits(self):
        # Independent IEEE754 encodings, including tie-even and signed subnormal.
        values=[0.0,-0.0,1.0,-2.0,65504.0,2**-24,-2**-24,2**-25,1+2**-11,1+3*2**-11]
        expected=bytes.fromhex('00000080003c00c0ff7b010001800000003c023c')
        raw=struct.pack('<10f',*values)
        self.assertEqual(e.half_literal(raw),expected)
        self.assertEqual(e.half_chunk(raw,np),expected)
    def test_chunk_independent_bit_corpus(self):
        # A deterministic spread of FP32 bit patterns; expected uses CPython's
        # independent binary16 implementation, not numpy or candidate weights.
        words=[(x*2654435761)&0xffffffff for x in range(4096)]
        raw=b''.join(struct.pack('<I',x) for x in words if math.isfinite(struct.unpack('<f',struct.pack('<I',x))[0]) and abs(struct.unpack('<f',struct.pack('<I',x))[0])<=65504)
        self.assertEqual(e.half_chunk(raw,np),e.half_literal(raw))
    def test_nonfinite_overflow_refuse(self):
        for value in [float('nan'),float('inf'),-float('inf'),70000.0]:
            for function in [e.half_literal,lambda b:e.half_chunk(b,np)]:
                with self.subTest(value=value), self.assertRaises(ValueError):function(struct.pack('<f',value))
    def test_unaligned_refuse(self):
        with self.assertRaises(ValueError):e.half_chunk(b'123',np)
    def test_focus_mask(self):
        self.assertEqual(e.validate_inputs('focus',[0]*510+[1,1],list(range(512))),[2,510,512,510,512])
        self.assertEqual(e.validate_inputs('focus',[1]*512,list(range(512))),[512,0,512,0,512])
    def test_invalid_focus_masks(self):
        for mask in [[0]*512,[1]*511+[0],[0]*509+[1,0,1],[1]*511]:
            with self.subTest(mask=mask[-4:]),self.assertRaises(ValueError):e.validate_inputs('focus',mask,list(range(len(mask))))
    def test_positions_cannot_skip_pad(self):
        with self.assertRaises(ValueError):e.validate_inputs('focus',[0]*510+[1,1],[0]*510+[0,1])
    def test_context_inputs(self):
        self.assertEqual(e.validate_inputs('compact',[1,1,1],[0,1,2]),([2],[3]))
        for mask,pos in [([1,0],[0,1]),([1,1],[1,2]),([],[]),([1]*8193,list(range(8193)))]:
            with self.assertRaises(ValueError):e.validate_inputs('large',mask,pos)
    def test_literal_causal_head_group_and_nan_padding(self):
        # Literal attention with Q=K=0 means exact uniform causal averages.
        # K/V heads A=[2,4], B=[10,14], repeated consecutively => [A,A,B,B].
        expected=[[2,2,10,10],[3,3,12,12]]
        values=[[2,4],[10,14]]
        actual=[[sum(values[head//2][:row+1])/(row+1) for head in range(4)] for row in range(2)]
        self.assertEqual(actual,expected)
        # Where must select zero even if padded output is uninitialized NaN.
        raw=[float('nan'),2,3];live=[False,True,True]
        safe=np.where(np.asarray(live),np.asarray(raw,dtype=np.float16),np.float16(0))
        self.assertEqual(safe.tolist(),[0,2,3])
        self.assertTrue(math.isnan(raw[0]*0))

# Tiny synthetic graph with complete selected-model metadata geometry. No huge
# tensors allocated; rewrite exercises metadata only, not checker/inference.
def fixture(key):
    layers,heads,kv,hidden=e.GEOMETRY[key];prefix='/base/' if key=='focus' else '/scorer/'
    nodes=[];initial=[]
    def init(name,dims):
        t=T();t.name=name;t.data_type=T.FLOAT;t.dims.extend(dims);t.raw_data=struct.pack('<f',1);initial.append(t);return name
    def node(op,inputs,name,**attrs):
        nodes.append(h.make_node(op,inputs,[name+'_output_0'],name=name,**attrs));return name+'_output_0'
    embed=init(prefix[1:-1]+'.embed_tokens.weight',[4,hidden]);x=node('Gather',[embed,'input_ids'],prefix+'embed',axis=0)
    for i in range(layers):
        root=prefix+f'layers.{i}/';att=root+'self_attn/'
        for label in ['input_layernorm','post_attention_layernorm','self_attn/q_norm','self_attn/k_norm']:
            weight=init(root[1:].replace('/','.')+label.replace('/','.')+'.weight',[128 if label.endswith(('q_norm','k_norm')) else hidden])
            x=node('Mul',[x,weight],root+label+'/Mul_1')
        projections={}
        for label in ['q_proj','k_proj','v_proj']:
            weight=init(f'weight_{i}_{label}',[hidden,heads*128 if label=='q_proj' else kv*128]);projections[label]=node('MatMul',[x,weight],att+label+'/MatMul')
        q=node('Add',[projections['q_proj'],projections['q_proj']],att+'Add')
        k=node('Add',[projections['k_proj'],projections['k_proj']],att+'Add_1')
        qk=node('MatMul',[q,k],att+'MatMul')
        scale=node('Constant',[],att+'source_scale',value=h.make_tensor('s',T.FLOAT,[],[.125]))
        scaled=node('Mul',[qk,scale],att+('Mul_6' if key=='focus' else 'Mul_8'))
        # Literal source masks are shared but disconnected once attention rewrites.
        maskname='/Where_output_0' if key=='focus' else '/scorer/Where_output_0'
        masked=node('Add',[scaled,maskname],att+'Add_2')
        soft=node('Softmax',[masked],att+'Softmax',axis=-1)
        av=node('MatMul',[soft,projections['v_proj']],att+'MatMul_1')
        shape=node('Constant',[],att+'shape',value=h.make_tensor('sh',T.INT64,[3],[1,512,heads*128]))
        x=node('Reshape',[av,shape],att+('Reshape_5' if key=='focus' else 'Reshape_7'))
        for label in ['o_proj','gate_proj','up_proj','down_proj']:
            location=att if label=='o_proj' else root+'mlp/'
            weight=init(f'weight_{i}_{label}',[1,1]);x=node('MatMul',[x,weight],location+label+'/MatMul')
    weight=init(prefix[1:-1]+'.norm.weight',[hidden]);x=node('Mul',[x,weight],prefix+'norm/Mul')
    if key=='focus':
        head=init('head_weight',[hidden,2]);out=node('MatMul',[x,head],'/MatMul');name='logits'
    else:
        rows=node('Gather',[embed,'candidate_positions'],'/candidate_rows',axis=0)
        out=node('Mul',[x,rows],'/sparse_score');name='scores'
    nodes.append(h.make_node('Identity',[out],[name],name='/result'))
    # Dead dense mask placeholder proves reachability removes unused masks.
    nodes.append(h.make_node('Constant',[],[maskname],name='/source_mask',value=h.make_tensor('mask',T.FLOAT,[],[0])))
    inputs=['input_ids','attention_mask','position_ids']+([] if key=='focus' else ['decision_positions','candidate_positions','option_mask'])
    return h.make_model(h.make_graph(nodes,'literal',[h.make_tensor_value_info(v,T.INT64,[1,512]) for v in inputs],[h.make_tensor_value_info(name,T.FLOAT,[1,2])],initial),opset_imports=[h.make_opsetid('',18)])

class GraphControls(unittest.TestCase):
    def test_context_all_layers_and_preserved_abi(self):
        for key,count,qheads in [('compact',28,16),('large',36,32)]:
            m=fixture(key);original=copy.deepcopy(m);report=e.rewrite(m,key,onnx)
            attention=[n for n in m.graph.node if n.op_type=='GroupQueryAttention']
            self.assertEqual(len(attention),count)
            for n in attention:
                self.assertEqual(len(n.input),7);attrs=e.attributes(n,onnx)
                self.assertEqual(attrs,{'num_heads':qheads,'kv_num_heads':8,'causal':1,'local_window_size':-1,'do_rotary':0,'scale':.125})
                self.assertNotIn('sdpa_kernel',attrs)
            self.assertEqual(report['required_native_sdpa_kernel'],1)
            self.assertEqual([x.SerializeToString() for x in m.graph.input],[x.SerializeToString() for x in original.graph.input])
            self.assertEqual([x.SerializeToString() for x in m.graph.output],[x.SerializeToString() for x in original.graph.output])
            self.assertFalse(any(n.op_type=='Softmax' or n.name=='/source_mask' for n in m.graph.node))
    def test_focus_mask_zero_and_head_census(self):
        m=fixture('focus');report=e.rewrite(m,'focus',onnx);initial={t.name:t for t in m.graph.initializer}
        self.assertEqual(report['required_native_sdpa_kernel'],2)
        self.assertEqual(len([n for n in m.graph.node if n.op_type=='MultiHeadAttention']),28)
        for n in m.graph.node:
            if n.op_type=='MultiHeadAttention':
                self.assertEqual(len(n.input),5);self.assertEqual(n.input[3],'')
                self.assertEqual(e.attributes(n,onnx),{'num_heads':16,'unidirectional':1,'scale':.125})
        indices=onnx.numpy_helper.to_array(initial[e.NS+'attention/0/kv_indices']).tolist()
        self.assertEqual(indices,[0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7])
        wheres=[n for n in m.graph.node if n.op_type=='Where']
        self.assertEqual(len(wheres),28)
        for n in wheres:self.assertEqual(n.input[2],e.NS+'half_zero')
        self.assertEqual(initial['head_weight'].data_type,T.FLOAT)
        self.assertEqual(initial['base.embed_tokens.weight'].data_type,T.FLOAT16)
        protected=[t for t in m.graph.initializer if t.name.endswith('norm.weight')]
        self.assertEqual(len(protected),113);self.assertTrue(all(t.data_type==T.FLOAT for t in protected))
    def test_sparse_readout_same_half_embedding(self):
        m=fixture('compact');e.rewrite(m,'compact',onnx)
        n=next(n for n in m.graph.node if n.name=='/candidate_rows')
        self.assertEqual(n.input[0],'scorer.embed_tokens.weight')
        cast=next(n for n in m.graph.node if '/candidate_rows/output32' in n.name)
        self.assertEqual(e.attributes(cast,onnx),{'to':T.FLOAT})
        self.assertEqual(len([t for t in m.graph.initializer if t.name.endswith('embed_tokens.weight')]),1)
    def test_complete_weights_and_protected_node_bytes(self):
        for key in ['focus','compact','large']:
            m=fixture(key);before={n.name:n.SerializeToString(deterministic=True) for n in m.graph.node};report=e.rewrite(m,key,onnx)
            roles=report['roles'];layers=e.GEOMETRY[key][0]
            self.assertEqual(sum(r=='projection_half' for r in roles.values()),layers*7)
            self.assertEqual(sum(r=='embedding_half' for r in roles.values()),1)
            self.assertEqual(sum(r=='norm_fp32' for r in roles.values()),layers*4+1)
            for n in m.graph.node:
                if n.name in before and n.op_type=='Mul':self.assertEqual(n.SerializeToString(deterministic=True),before[n.name])
    def test_refuse_unknown_weight(self):
        m=fixture('compact');m.graph.initializer[0].name='unreviewed_embedding'
        with self.assertRaises(ValueError):e.rewrite(m,'compact',onnx)
    def test_refuse_unknown_bias(self):
        m=fixture('compact');m.graph.initializer.append(h.make_tensor('bias',T.FLOAT,[1],[1]))
        with self.assertRaises(ValueError):e.rewrite(m,'compact',onnx)
    def test_refuse_wrong_source_scale_chain(self):
        m=fixture('compact');n=next(n for n in m.graph.node if n.name.endswith('/self_attn/Mul_8'));n.input[0]='input_ids'
        with self.assertRaises(ValueError):e.rewrite(m,'compact',onnx)
    def test_refuse_foreign_op(self):
        m=fixture('compact');m.graph.node[0].domain='foreign'
        with self.assertRaises(ValueError):e.rewrite(m,'compact',onnx)
    def test_refuse_abi_change(self):
        m=fixture('compact');m.graph.output[0].name='hidden'
        with self.assertRaises(ValueError):e.rewrite(m,'compact',onnx)

class ArrayControls(unittest.TestCase):
    def test_real_tiny_stream_arrays(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);t=h.make_tensor('weight',T.FLOAT,[4],[1,-2,2**-24,-0.0]);n=h.make_tensor('norm',T.FLOAT,[2],[1.25,2])
            original=h.make_model(h.make_graph([],'tiny',[],[],[t,n]));candidate=copy.deepcopy(original);candidate.graph.initializer[0].data_type=T.FLOAT16
            report=e.convert_arrays(original,candidate,{'weight':'projection_half','norm':'norm_fp32'},root,root/'weights.bin',np,999,clock=lambda:0)
            expected=bytes.fromhex('003c00c001000080')+struct.pack('<2f',1.25,2)
            self.assertEqual((root/'weights.bin').read_bytes(),expected)
            self.assertEqual(report[0]['output_sha256'],e.sha(bytes.fromhex('003c00c001000080')))
            self.assertEqual(report[1]['source_sha256'],report[1]['output_sha256'])
            self.assertEqual(original.graph.initializer[0].data_type,T.FLOAT)
    def test_external_source_and_constant_are_retained(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);(root/'source.bin').write_bytes(struct.pack('<3f',1,-2,3.25))
            def external(name,offset,count):
                t=T();t.name=name;t.data_type=T.FLOAT;t.dims.extend([count]);t.data_location=1
                for k,v in [('location','source.bin'),('offset',str(offset)),('length',str(count*4))]:a=t.external_data.add();a.key=k;a.value=v
                return t
            t=external('x',0,2);constant=external('constant',8,1)
            m=h.make_model(h.make_graph([h.make_node('Constant',[],['c'],name='c',value=constant)],'x',[],[],[t]));candidate=copy.deepcopy(m);candidate.graph.initializer[0].data_type=T.FLOAT16
            report=e.convert_arrays(m,candidate,{'x':'projection_half'},root,root/'weights.bin',np,100,clock=lambda:0)
            self.assertEqual((root/'weights.bin').read_bytes(),bytes.fromhex('003c00c0'))
            c=candidate.graph.node[0].attribute[0].t
            self.assertEqual(c.data_location,0);self.assertEqual(c.raw_data,struct.pack('<f',3.25))
            self.assertEqual(report[1]['source_sha256'],e.sha(struct.pack('<f',3.25)))
            self.assertEqual((root/'source.bin').read_bytes(),struct.pack('<3f',1,-2,3.25))
    def test_external_overlap_refuses_before_output(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);(root/'source').write_bytes(struct.pack('<2f',1,2));initial=[]
            for name in ['a','b']:
                t=T();t.name=name;t.data_type=T.FLOAT;t.dims.extend([2]);t.data_location=1
                for k,v in [('location','source'),('length','8')]:a=t.external_data.add();a.key=k;a.value=v
                initial.append(t)
            m=h.make_model(h.make_graph([],'x',[],[],initial))
            with self.assertRaisesRegex(ValueError,'overlapping'):e.convert_arrays(m,copy.deepcopy(m),{'a':'projection_half','b':'projection_half'},root,root/'weights.bin',np,10,clock=lambda:0)
            self.assertFalse((root/'weights.bin').exists())
    def test_mid_operation_expiry_keeps_partial_truth(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);t=h.make_tensor('x',T.FLOAT,[1],[1]);m=h.make_model(h.make_graph([],'x',[],[],[t]));ticks=iter([0,10])
            with self.assertRaisesRegex(ValueError,'deadline'):e.convert_arrays(m,copy.deepcopy(m),{'x':'embedding_half'},root,root/'weights.bin',np,10,clock=lambda:next(ticks))
            self.assertEqual((root/'weights.bin').stat().st_size,0)
    def test_nonfinite_stream_keeps_source(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);t=h.make_tensor('x',T.FLOAT,[1],[float('inf')]);m=h.make_model(h.make_graph([],'x',[],[],[t]));before=m.SerializeToString()
            with self.assertRaisesRegex(ValueError,'nonfinite'):e.convert_arrays(m,copy.deepcopy(m),{'x':'embedding_half'},root,root/'weights.bin',np,10,clock=lambda:0)
            self.assertEqual(m.SerializeToString(),before)
    def test_external_range_and_path_refusal(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);(root/'data').write_bytes(struct.pack('<2f',1,2))
            t=h.make_tensor('x',T.FLOAT,[1],[0]);t.ClearField('float_data');t.data_location=1
            for k,v in [('location','../data'),('length','4')]:x=t.external_data.add();x.key=k;x.value=v
            with self.assertRaises(ValueError):e.source_bytes(t,root)
            t.external_data[0].value='data';t.external_data[1].value='8'
            with self.assertRaises(ValueError):e.source_bytes(t,root)
    def test_deadline_never_refreshed(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);t=h.make_tensor('x',T.FLOAT,[1],[1]);m=h.make_model(h.make_graph([],'x',[],[],[t]))
            with self.assertRaises(ValueError):e.convert_arrays(m,copy.deepcopy(m),{'x':'embedding_half'},root,root/'weights.bin',np,10,clock=lambda:10)
    def test_graph_authorization_requires_original(self):
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            root=pathlib.Path(tmp);p=root/'model.onnx';p.write_bytes(b'W8/failed graph')
            with self.assertRaises(ValueError):e.export_after_owner_admission(p,'focus',{},root/'out',onnx,np,10,clock=lambda:0)
            self.assertFalse((root/'out').exists())

if __name__=='__main__':unittest.main()
