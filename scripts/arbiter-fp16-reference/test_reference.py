# SPDX-License-Identifier: Apache-2.0
"""Tiny CPU literals only; no selected model/checkpoint, ONNX, native or GPU."""
import copy,hashlib,json,math,pathlib,struct,tempfile,unittest
import torch as t
import importlib.util
spec=importlib.util.spec_from_file_location('airk_fp16_reference',pathlib.Path(__file__).resolve().parents[1]/'arbiter-fp16-reference.py')
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)

def config():return {'model_type':'qwen3','hidden_act':'silu','tie_word_embeddings':True,'attention_bias':False,'use_sliding_window':False,'rope_scaling':None,'rope_theta':1000000,'num_hidden_layers':1,'hidden_size':2,'num_attention_heads':2,'num_key_value_heads':1,'head_dim':2,'intermediate_size':3,'rms_norm_eps':1e-6,'vocab_size':10000}
def state(c):
 out={'embed_tokens.weight':t.zeros(c['vocab_size'],2,dtype=t.float32),'norm.weight':t.ones(2,dtype=t.float32)}
 root='layers.0.'
 out.update({root+'input_layernorm.weight':t.ones(2),root+'post_attention_layernorm.weight':t.ones(2),root+'self_attn.q_norm.weight':t.ones(2),root+'self_attn.k_norm.weight':t.ones(2)})
 for n,shape in {'self_attn.q_proj':(4,2),'self_attn.k_proj':(2,2),'self_attn.v_proj':(2,2),'self_attn.o_proj':(2,4),'mlp.gate_proj':(3,2),'mlp.up_proj':(3,2),'mlp.down_proj':(2,3)}.items():out[root+n+'.weight']=t.zeros(shape)
 out['embed_tokens.weight'][1]=t.tensor([1.,2.]);out['embed_tokens.weight'][2]=t.tensor([3.,4.]);out['embed_tokens.weight'][3]=t.tensor([5.,6.])
 out['embed_tokens.weight'][2152]=t.tensor([3.0005,4.001]);out['embed_tokens.weight'][9693]=t.tensor([5.002,6.003])
 return out

def native(focus=False):
 if focus:return {'prompt':'literalCPUfixture','input_ids':[0]*510+[1,1],'attention_mask':[0]*510+[1,1],'position_ids':list(range(512)),'decision_position':511,'candidate_positions':[2152,9693],'option_mask':[True,True],'input_digest':'opaque-native-control-digest'}
 return {'prompt':'literalCPUfixture','input_ids':[1,1,1],'attention_mask':[1,1,1],'position_ids':[0,1,2],'decision_position':2,'candidate_positions':[2,3],'option_mask':[True,False],'input_digest':'opaque-native-control-digest'}

class LiteralControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):t.set_num_threads(1)
 def test_half_rounding_independent_bits(self):
  values=t.tensor([0.,-0.,1.,-2.,65504.,2**-24,-2**-24,2**-25,1+2**-11,1+3*2**-11],dtype=t.float32).half()
  self.assertEqual(values.numpy().tobytes(),bytes.fromhex('00000080003c00c0ff7b010001800000003c023c'))
 def test_rms_fp32_scalar_oracle(self):
  out=r.rms(t,t.tensor([3.,4.]),t.tensor([2.,3.]),1e-6)
  expected=t.tensor([6/math.sqrt(12.5+1e-6),12/math.sqrt(12.5+1e-6)],dtype=t.float32)
  self.assertEqual(out.dtype,t.float32);self.assertTrue(t.allclose(out,expected,rtol=0,atol=1e-6))
 def test_rms_rejects_half_island(self):
  with self.assertRaises(ValueError):r.rms(t,t.ones(2).half(),t.ones(2),1e-6)
 def test_noninterleaved_rope_literal(self):
  q=t.tensor([[[[1.,2.,3.,4.]]]]);out,_=r.rotary(t,q,q,t.tensor([[2]]),1000000)
  angles=[2,.002,2,.002];rotated=[-3,-4,1,2];expected=t.tensor([1*math.cos(2)-3*math.sin(2),2*math.cos(.002)-4*math.sin(.002),3*math.cos(2)+math.sin(2),4*math.cos(.002)+2*math.sin(.002)])
  self.assertTrue(t.allclose(out.flatten(),expected,rtol=0,atol=1e-6))
 def test_positions_not_shifted_with_padding(self):
  q=t.ones(1,1,3,2);full,_=r.rotary(t,q,q,t.tensor([[0,1,2]]),1000000)
  self.assertAlmostEqual(full[0,0,2,0].item(),math.cos(2)-math.sin(2),places=6)
  self.assertFalse(t.equal(full[0,0,2],full[0,0,0]))
 def test_uniform_causal_and_consecutive_gqa_literal(self):
  q=t.zeros(1,4,2,2,dtype=t.float16);k=t.zeros(1,2,2,2,dtype=t.float16);v=t.tensor([[[[2,6],[4,8]],[[10,20],[14,24]]]],dtype=t.float16)
  out=r.causal_half_attention(t,q,k,v,[1,1],1)
  expected=t.tensor([[[[2,6],[3,7]],[[2,6],[3,7]],[[10,20],[12,22]],[[10,20],[12,22]]]],dtype=t.float16)
  self.assertTrue(t.equal(out,expected))
 def test_left_pad_outputs_zero_and_live_causal_values(self):
  q=t.zeros(1,2,4,2,dtype=t.float16);k=t.zeros(1,1,4,2,dtype=t.float16);v=t.tensor([[[[60000,60000],[60000,60000],[2,2],[4,4]]]],dtype=t.float16)
  out=r.causal_half_attention(t,q,k,v,[0,0,1,1]);self.assertTrue(t.equal(out[:,:,:2],t.zeros_like(out[:,:,:2])));self.assertEqual(out[0,0,:,0].tolist(),[0,0,2,3])
 def test_bad_masks_refuse(self):
  q=t.zeros(1,2,3,2,dtype=t.float16);kv=t.zeros(1,1,3,2,dtype=t.float16)
  for mask in [[0,0,0],[1,0,1],[1,1,0]]:
   with self.subTest(mask=mask),self.assertRaises(ValueError):r.causal_half_attention(t,q,kv,kv,mask)
 def test_nonfinite_attention_refuse(self):
  q=t.zeros(1,2,2,2,dtype=t.float16);kv=t.zeros(1,1,2,2,dtype=t.float16);q[0,0,0,0]=float('nan')
  with self.assertRaisesRegex(ValueError,'nonfinite'):r.causal_half_attention(t,q,kv,kv,[1,1])
 def test_block_bound_and_half_dtype(self):
  q=t.zeros(1,2,2,2,dtype=t.float16);kv=t.zeros(1,1,2,2,dtype=t.float16)
  for block in [0,65]:
   with self.assertRaises(ValueError):r.causal_half_attention(t,q,kv,kv,[1,1],block)
  with self.assertRaises(ValueError):r.causal_half_attention(t,q.float(),kv,kv,[1,1])
 def test_blocking_preserves_full_small_result(self):
  q=t.tensor([[[[.5,1],[1,-1],[2,.25]],[[1,.5],[.25,2],[1,-.5]]]],dtype=t.float16);k=t.tensor([[[[1,1],[2,-1],[-1,.5]]]],dtype=t.float16);v=t.tensor([[[[2,4],[6,8],[10,12]]]],dtype=t.float16)
  self.assertTrue(t.equal(r.causal_half_attention(t,q,k,v,[1,1,1],1),r.causal_half_attention(t,q,k,v,[1,1,1],3)))
 def test_candidate_ids_are_vocabulary_ids(self):
  row=native();row['candidate_positions']=[9999];row['option_mask']=[True];r.validate_native(t,config(),False,row)
  row['candidate_positions']=[10000]
  with self.assertRaises(ValueError):r.validate_native(t,config(),False,row)
 def test_native_focus_positions_and_final_readout(self):
  row=native(True);r.validate_native(t,config(),True,row);row['position_ids']=[0]*512
  with self.assertRaises(ValueError):r.validate_native(t,config(),True,row)
 def test_original_half_weights_all_and_norm_islands(self):
  c=config();weights=state(c);ref=r.HalfReference(t,c,weights,False,'cpu')
  for name,value in ref.weights.items():self.assertEqual(value.dtype,t.float32 if name.endswith(('norm.weight','layernorm.weight')) else t.float16)
  self.assertEqual(len(ref.weights),13);self.assertEqual(tuple(ref.weights['layers.0.self_attn.q_proj.weight'].shape),(4,2))
 def test_original_nan_inf_overflow_refuse(self):
  c=config()
  for key,value in [('norm.weight',float('nan')),('layers.0.self_attn.q_proj.weight',float('inf')),('embed_tokens.weight',70000.)]:
   weights=state(c);weights[key].view(-1)[0]=value
   with self.subTest(key=key),self.assertRaises(ValueError):r.HalfReference(t,c,weights,False,'cpu')
 def test_missing_extra_dtype_architecture_refuse(self):
  c=config();weights=state(c);weights['extra']=t.zeros(1)
  with self.assertRaises(ValueError):r.HalfReference(t,c,weights,False,'cpu')
  weights=state(c);weights['norm.weight']=weights['norm.weight'].half()
  with self.assertRaises(ValueError):r.HalfReference(t,c,weights,False,'cpu')
  for key,value in [('attention_bias',True),('hidden_act','gelu'),('rope_theta',10000)]:
   changed=dict(c);changed[key]=value
   with self.subTest(key=key),self.assertRaises(ValueError):r.HalfReference(t,changed,state(c),False,'cpu')
 def test_complete_tiny_context_fp32_sparse_score_and_mask(self):
  c=config();ref=r.HalfReference(t,c,state(c),False,'cpu');row=native();original=copy.deepcopy(row);out=ref.forward(row)
  expected=(3+8)/math.sqrt(2.5+1e-6)
  self.assertEqual(out.dtype,t.float32);self.assertAlmostEqual(out[0,0].item(),expected,places=5);self.assertEqual(out[0,1].item(),-10000);self.assertEqual(row,original)
 def test_complete_tiny_focus_protected_head(self):
  c=config();weights=state(c);ref=r.HalfReference(t,c,weights,True,'cpu');row=native(True);out=ref.forward(row);den=math.sqrt(2.5+1e-6)
  expected=[(weights['embed_tokens.weight'][token,0].item()+2*weights['embed_tokens.weight'][token,1].item())/den for token in [2152,9693]]
  for value,oracle in zip(out[0].tolist(),expected):self.assertAlmostEqual(value,oracle,places=5)
  self.assertEqual(ref.head.dtype,t.float32);self.assertTrue(t.equal(ref.head,weights['embed_tokens.weight'][[2152,9693]]))
 def test_census_half_transpose_independent_bytes(self):
  c=config();weights=state(c);weights['layers.0.self_attn.q_proj.weight']=t.tensor([[1.,2.],[3.,4.],[5.,6.],[7.,8.]])
  ref=r.HalfReference(t,c,weights,True,'cpu');census=ref.parameter_census();entry=next(x for x in census if x['parameter']=='layers.0.self_attn.q_proj.weight')
  expected=b''.join(struct.pack('<e',v) for v in [1,3,5,7,2,4,6,8]);self.assertEqual(entry['onnx_transposed_sha256'],hashlib.sha256(expected).hexdigest());self.assertTrue(census[-1]['protected_fp32'])
 def test_full_source_and_rounded_layout_census_literal(self):
  c=config();weights=state(c);values=[1.0003,2.0007,3.0008,4.0002,5.001,6.002,7.003,8.001];weights['layers.0.self_attn.q_proj.weight']=t.tensor(values).reshape(4,2)
  ref=r.HalfReference(t,c,weights,False,'cpu');entry=next(x for x in ref.parameter_census() if x['parameter']=='layers.0.self_attn.q_proj.weight');source=weights['layers.0.self_attn.q_proj.weight'].flatten().tolist();transposed=[source[i] for i in [0,2,4,6,1,3,5,7]]
  self.assertEqual(entry['original_fp32_sha256'],hashlib.sha256(struct.pack('<8f',*source)).hexdigest())
  self.assertEqual(entry['original_fp32_onnx_layout_sha256'],hashlib.sha256(struct.pack('<8f',*transposed)).hexdigest())
  self.assertEqual(entry['onnx_transposed_sha256'],hashlib.sha256(struct.pack('<8e',*transposed)).hexdigest())
 def test_untrusted_origin_refuses_before_import_or_load(self):
  with tempfile.TemporaryDirectory() as folder:
   path=pathlib.Path(folder)/'bad.py';path.write_text('raise RuntimeError("must not execute")\n',encoding='utf-8')
   with self.assertRaisesRegex(ValueError,'publisher owner source differs'):r.from_authenticated_original(None,'focus',path,None,'cpu')
 def test_reference_math_flags_without_GPU_context(self):
  self.assertFalse(t.cuda.is_initialized());r.freeze_reference_math(t);self.assertFalse(t.backends.cuda.matmul.allow_tf32);self.assertFalse(t.backends.cuda.matmul.allow_fp16_reduced_precision_reduction);self.assertFalse(t.backends.cuda.matmul.allow_fp16_accumulation);self.assertFalse(t.cuda.is_initialized())


class ActualByteBindingControls(unittest.TestCase):
 def candidate(self,reference,root):
  import onnx
  from onnx import helper as h,TensorProto as T
  initial=[];nodes=[];offset=0;prefix='base' if reference.focus else 'scorer';blob=bytearray()
  for i,record in enumerate(reference.parameter_census()):
   name=record['parameter'];projection=name.endswith('_proj.weight');head=name=='trained_readout_rows_2152_9693';value=reference.head if head else reference.weights[name]
   if projection or head:value=value.transpose(0,1)
   raw=value.contiguous().numpy().tobytes();initializer='linear_'+str(i) if projection or head else ('base.' if reference.focus else 'scorer.text.')+name
   tensor=T();tensor.name=initializer;tensor.data_type=T.FLOAT if record['protected_fp32'] else T.FLOAT16;tensor.dims.extend(list(value.shape));tensor.data_location=1
   for key,val in [('location','weights.bin'),('offset',str(offset)),('length',str(len(raw)))]:entry=tensor.external_data.add();entry.key=key;entry.value=val
   initial.append(tensor);blob.extend(raw);offset+=len(raw)
   if projection:
    parts=name.split('.');route='/'+prefix+'/layers.'+parts[1]+'/'+parts[2]+'/'+parts[3]+'/MatMul';nodes.append(h.make_node('MatMul',['x',initializer],['z'+str(i)],name=route))
   elif head:nodes.append(h.make_node('MatMul',['x',initializer],['head'],name='/MatMul'))
  initial.append(h.make_tensor('/airk_g5/half_zero',T.FLOAT16,[],[0]))
  graph=h.make_model(h.make_graph(nodes,'literal-candidate',[],[],initial));path=root/'model.onnx';path.write_bytes(graph.SerializeToString());(root/'weights.bin').write_bytes(blob)
  return onnx,path,{'weights.bin':str(root/'weights.bin')}
 def test_complete_actual_tiny_initializer_binding(self):
  c=config();weights=state(c);weights['layers.0.self_attn.q_proj.weight']=t.tensor([[1.,2.],[3.,4.],[5.,6.],[7.,8.]])
  for focus in [False,True]:
   with self.subTest(focus=focus),tempfile.TemporaryDirectory() as folder:
    reference=r.HalfReference(t,c,weights,focus,'cpu');onnx,path,files=self.candidate(reference,pathlib.Path(folder));bindings=r._verify_parameter_bytes(reference,path,onnx,files)
    self.assertEqual(len(bindings),14 if focus else 13);self.assertTrue(all(x['actual_initializer_sha256']==x['independent_expected_sha256'] for x in bindings))
 def test_actual_half_byte_mutation_refuses(self):
  reference=r.HalfReference(t,config(),state(config()),False,'cpu')
  with tempfile.TemporaryDirectory() as folder:
   onnx,path,files=self.candidate(reference,pathlib.Path(folder));weights=pathlib.Path(files['weights.bin']);raw=bytearray(weights.read_bytes());raw[0]^=1;weights.write_bytes(raw)
   with self.assertRaisesRegex(ValueError,'actual candidate bytes differ'):r._verify_parameter_bytes(reference,path,onnx,files)


class StrictFocusAbiControls(unittest.TestCase):
 def test_focus_rejects_float_candidate_ids_and_integer_option_masks(self):
  for field,value in [('candidate_positions',[2152.0,9693.0]),('option_mask',[1,1])]:
   row=native(True);row[field]=value
   with self.subTest(field=field),self.assertRaisesRegex(ValueError,'dtype differs'):r.validate_native(t,config(),True,row)

class NestedContextNamespaceControls(unittest.TestCase):
 def fixture(self,root):
  # Independent literal nested original model. Historical R2 retains the actual
  # pinned publisher-wrapper witness; default tests need no private artifacts.
  from onnx import helper as h,TensorProto as T
  import onnx
  c=config();literal=state(c);literal['embed_tokens.weight'][1,0]=1+2**-11;literal['norm.weight'][0]=1.0003
  class Weight(t.nn.Module):
   def __init__(self,value):
    super().__init__();self.weight=t.nn.Parameter(value.clone())
  class Layer(t.nn.Module):
   def __init__(self):
    super().__init__()
    self.input_layernorm=Weight(literal['layers.0.input_layernorm.weight'])
    self.post_attention_layernorm=Weight(literal['layers.0.post_attention_layernorm.weight'])
    self.self_attn=t.nn.Module();self.mlp=t.nn.Module()
    for name in ['q_norm','k_norm','q_proj','k_proj','v_proj','o_proj']:
     self.self_attn.add_module(name,Weight(literal['layers.0.self_attn.'+name+'.weight']))
    for name in ['gate_proj','up_proj','down_proj']:
     self.mlp.add_module(name,Weight(literal['layers.0.mlp.'+name+'.weight']))
  class Text(t.nn.Module):
   def __init__(self):
    super().__init__();self.embed_tokens=Weight(literal['embed_tokens.weight']);self.norm=Weight(literal['norm.weight']);self.layers=t.nn.ModuleList([Layer()])
  class Scorer(t.nn.Module):
   def __init__(self):
    super().__init__();self.text=Text()
  scorer=Scorer()
  original=scorer.text.state_dict();reference=r.HalfReference(t,c,original,False,'cpu')
  initial=[];nodes=[];blob=bytearray()
  # Candidate bytes/names derive from literal original module and independent
  # struct IEEE encoding, NOT reference.weights/parameter_census/exporter report.
  for index,(qualified,value) in enumerate(scorer.named_parameters()):
   self.assertTrue(qualified.startswith('text.'));name=qualified[len('text.'):];projection=name.endswith('_proj.weight');protected=name.endswith(('norm.weight','layernorm.weight'))
   v=value.detach().cpu();v=v.transpose(0,1) if projection else v;values=v.contiguous().flatten().tolist();raw=b''.join(struct.pack('<f' if protected else '<e',x) for x in values)
   initializer='actual_projection_'+str(index) if projection else 'scorer.'+qualified
   tensor=T();tensor.name=initializer;tensor.data_type=T.FLOAT if protected else T.FLOAT16;tensor.dims.extend(list(v.shape));tensor.data_location=1
   for key,val in [('location','weights.bin'),('offset',str(len(blob))),('length',str(len(raw)))]:entry=tensor.external_data.add();entry.key=key;entry.value=val
   initial.append(tensor);blob.extend(raw)
   if projection:
    parts=name.split('.');route='/scorer/layers.'+parts[1]+'/'+parts[2]+'/'+parts[3]+'/MatMul'
    nodes.append(h.make_node('Cast',['literal_fp32'],['literal_half_'+str(index)],name='literal_cast16_'+str(index),to=T.FLOAT16))
    nodes.append(h.make_node('MatMul',['literal_half_'+str(index),initializer],['literal_result16_'+str(index)],name=route))
    nodes.append(h.make_node('Cast',['literal_result16_'+str(index)],['literal_result32_'+str(index)],name='literal_cast32_'+str(index),to=T.FLOAT))
  path=root/'model.onnx';graph=h.make_model(h.make_graph(nodes,'authentic-context-name-only-fixture',[],[],initial));path.write_bytes(graph.SerializeToString());(root/'weights.bin').write_bytes(blob)
  return reference,onnx,path,{'weights.bin':str(root/'weights.bin')},original
 def test_literal_nested_embedding_and_all_norm_classes_bind(self):
  with tempfile.TemporaryDirectory() as folder:
   reference,onnx,path,files,original=self.fixture(pathlib.Path(folder));bindings=r._verify_parameter_bytes(reference,path,onnx,files);by={x['parameter']:x for x in bindings}
   wanted=['embed_tokens.weight','norm.weight','layers.0.input_layernorm.weight','layers.0.post_attention_layernorm.weight','layers.0.self_attn.q_norm.weight','layers.0.self_attn.k_norm.weight']
   for name in wanted:self.assertEqual(by[name]['initializer'],'scorer.text.'+name)
   self.assertEqual(len(bindings),13);self.assertEqual(by['layers.0.self_attn.q_proj.weight']['node'],'/scorer/layers.0/self_attn/q_proj/MatMul')
   self.assertEqual(by['embed_tokens.weight']['actual_initializer_sha256'],hashlib.sha256(b''.join(struct.pack('<e',x) for x in original['embed_tokens.weight'].flatten().tolist())).hexdigest())
   self.assertEqual(by['norm.weight']['actual_initializer_sha256'],hashlib.sha256(b''.join(struct.pack('<f',x) for x in original['norm.weight'].flatten().tolist())).hexdigest())
 def test_missing_or_flattened_context_namespace_refuses(self):
  for missing in [False,True]:
   with self.subTest(missing=missing),tempfile.TemporaryDirectory() as folder:
    reference,onnx,path,files,_=self.fixture(pathlib.Path(folder));graph=onnx.load_model(str(path),load_external_data=False)
    target=next(v for v in graph.graph.initializer if v.name=='scorer.text.embed_tokens.weight')
    if missing:graph.graph.initializer.remove(target)
    else:target.name='scorer.embed_tokens.weight'
    path.write_bytes(graph.SerializeToString())
    with self.assertRaisesRegex(ValueError,'missing/duplicate actual learned initializer'):r._verify_parameter_bytes(reference,path,onnx,files)
 def test_actual_half_protected_dtypes_and_changed_bytes_refuse(self):
  for mutate in ['half_dtype','protected_dtype','bytes']:
   with self.subTest(mutate=mutate),tempfile.TemporaryDirectory() as folder:
    reference,onnx,path,files,_=self.fixture(pathlib.Path(folder));graph=onnx.load_model(str(path),load_external_data=False)
    if mutate=='bytes':
     raw=bytearray(pathlib.Path(files['weights.bin']).read_bytes());raw[0]^=1;pathlib.Path(files['weights.bin']).write_bytes(raw)
    else:
     name='scorer.text.embed_tokens.weight' if mutate=='half_dtype' else 'scorer.text.norm.weight';v=next(v for v in graph.graph.initializer if v.name==name);v.data_type=onnx.TensorProto.FLOAT if mutate=='half_dtype' else onnx.TensorProto.FLOAT16;path.write_bytes(graph.SerializeToString())
    with self.assertRaises(ValueError):r._verify_parameter_bytes(reference,path,onnx,files)

if __name__=='__main__':unittest.main()
