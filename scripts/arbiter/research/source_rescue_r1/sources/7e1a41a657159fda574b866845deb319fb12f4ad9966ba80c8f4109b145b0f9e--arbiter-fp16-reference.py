# SPDX-License-Identifier: Apache-2.0
"""Independent publisher-array half recipe. No imports of Torch/ONNX/exporters.
Torch is supplied by the actual original owner, or tiny CPU literal tests.
"""
import hashlib,importlib.util,json,math,pathlib,struct
PUBLISHER_OWNER_SHA={
 'focus':'3b7ae63a8c514188195835f291c0ecce501ca42605354d36405c889788884580',
 'context':'4c6f43d0d81539cfc6bf7b6ddf5ae69273e61508d0db3d22a5e7488c60560b6b'}

def need(ok,message):
 if not ok:raise ValueError(message)

def f32(value):return struct.unpack('<f',struct.pack('<f',value))[0]

def checked_finite(torch,value,label):
 need(bool(torch.isfinite(value).all().item()),'nonfinite '+label);return value

def tensor_digest(value):
 # Bounded rows avoid materializing full transposed multi-GiB matrices or bytes.
 need(value.ndim in (1,2),'parameter digest rank differs')
 value=value.detach();width=value.shape[1] if value.ndim==2 else 1
 rows=max(1,(4*1024*1024)//(width*value.element_size()));hasher=hashlib.sha256()
 for first in range(0,value.shape[0],rows):
  chunk=value[first:first+rows].contiguous().cpu()
  hasher.update(chunk.numpy().tobytes(order='C'))
 return hasher.hexdigest()

def freeze_reference_math(torch):
 torch.use_deterministic_algorithms(True)
 torch.backends.cuda.matmul.allow_tf32=False
 torch.backends.cudnn.allow_tf32=False
 torch.backends.cuda.matmul.allow_fp16_reduced_precision_reduction=False
 # Current original pinned Torch exposes this separate accumulation authority.
 need(hasattr(torch.backends.cuda.matmul,'allow_fp16_accumulation'),'missing FP16 accumulation authority')
 torch.backends.cuda.matmul.allow_fp16_accumulation=False

def rms(torch,value,scale,eps):
 need(value.dtype==torch.float32 and scale.dtype==torch.float32,'RMS island must be FP32')
 return (value*torch.rsqrt(value.pow(2).mean(-1,keepdim=True)+eps))*scale

def rotary(torch,q,k,positions,theta):
 need(q.dtype==k.dtype==torch.float32 and q.shape[-1]%2==0,'RoPE FP32/even head required')
 dim=q.shape[-1]
 inv=1.0/(theta**(torch.arange(0,dim,2,device=q.device,dtype=torch.float32)/dim))
 # Independent mathematical construction from config/native positions, no graph constants.
 freqs=positions.to(torch.float32).unsqueeze(-1)*inv
 angles=torch.cat((freqs,freqs),dim=-1).unsqueeze(1)
 cos=angles.cos();sin=angles.sin()
 def rotate(x):return torch.cat((-x[...,dim//2:],x[...,:dim//2]),dim=-1)
 return q*cos+rotate(q)*sin,k*cos+rotate(k)*sin

def live_start(mask):
 need(isinstance(mask,list) and mask and all(type(x) is int and x in (0,1) for x in mask),'binary mask required')
 need(1 in mask,'all masked input forbidden');start=mask.index(1)
 need(mask==[0]*start+[1]*(len(mask)-start),'right padding/hole forbidden');return start

def causal_half_attention(torch,q,k,v,mask,query_block=64):
 """Independent bounded FP32 causal math over half inputs, half result.
 No native SDPA or ONNX path is called; this cannot prove actual efficient dispatch.
 """
 need(q.dtype==k.dtype==v.dtype==torch.float16,'attention inputs must be half')
 for label,value in [('Q',q),('K',k),('V',v)]:checked_finite(torch,value,'half attention '+label)
 need(q.ndim==k.ndim==v.ndim==4 and q.shape[0]==k.shape[0]==v.shape[0]==1,'batch-one attention required')
 need(k.shape==v.shape and q.shape[2:]==k.shape[2:] and q.shape[1]%k.shape[1]==0,'GQA geometry differs')
 need(type(query_block) is int and 0<query_block<=64 and len(mask)==q.shape[2],'bounded query block required')
 start=live_start(mask);heads=q.shape[1];ratio=heads//k.shape[1]
 # Consecutive repeat, not alternating/tiled KV ordering.
 key=k[:,:,start:,:].repeat_interleave(ratio,dim=1).float()
 value=v[:,:,start:,:].repeat_interleave(ratio,dim=1).float()
 output=torch.zeros_like(q) # Explicit zero for all undefined padded queries.
 live=q.shape[2]-start;key_axis=torch.arange(live,device=q.device)
 scale=f32(q.shape[-1]**-0.5)
 for first in range(0,live,query_block):
  last=min(live,first+query_block);query=q[:,:,start+first:start+last,:].float()
  scores=torch.matmul(query,key.transpose(-1,-2))*scale
  allowed=key_axis.unsqueeze(0)<=torch.arange(first,last,device=q.device).unsqueeze(1)
  scores=scores.masked_fill(~allowed,float('-inf'))
  mass=torch.softmax(scores,dim=-1,dtype=torch.float32)
  result=torch.matmul(mass,value).to(torch.float16)
  output[:,:,start+first:start+last,:]=checked_finite(torch,result,'attention output')
 return output

def validate_native(torch,config,focus,row):
 # Consume native fields only; do not construct, tokenize or recompute a native DTO/digest.
 need(type(row['decision_position']) is int,'native decision dtype differs')
 ids=row['input_ids'];mask=row['attention_mask'];positions=row['position_ids'];candidates=row['candidate_positions'];options=row['option_mask']
 need(isinstance(ids,list) and ids and len(ids)==len(mask)==len(positions),'native tensor lengths differ')
 need(all(type(x) is int and 0<=x<config['vocab_size'] for x in ids),'vocabulary ID differs')
 need(positions==list(range(len(ids))) and all(type(x) is int for x in positions),'native positions differ')
 start=live_start(mask)
 if focus:
  need(all(type(x) is int for x in candidates) and all(type(x) is bool for x in options),'native Focus candidate/option dtype differs')
  need(len(ids)==512 and row['decision_position']==511 and candidates==[2152,9693] and options==[True,True],'native Focus512/readout differs')
 else:
  need(start==0 and len(ids)<=8192 and row['decision_position']==len(ids)-1,'Context all-live/final decision differs')
  need(0<len(candidates)<=8 and len(candidates)==len(options) and any(options) and all(type(x) is bool for x in options),'native option mask differs')
  need(all(type(x) is int and 0<=x<config['vocab_size'] for x in candidates),'candidate vocabulary ID differs')
 return ids,mask,positions,candidates,options

class HalfReference:
 def __init__(self,torch,config,publisher_state,focus,device):
  self.torch=torch;self.config=dict(config);self.focus=focus;self.device=device
  c=self.config;self.layers=c['num_hidden_layers'];self.hidden=c['hidden_size'];self.qheads=c['num_attention_heads'];self.kvheads=c['num_key_value_heads'];self.dim=c['head_dim'];self.intermediate=c['intermediate_size'];self.eps=c['rms_norm_eps']
  need(c['model_type']=='qwen3' and c['hidden_act']=='silu' and c['tie_word_embeddings'] is True and not c.get('attention_bias') and not c.get('use_sliding_window') and not c.get('rope_scaling'),'unreviewed publisher architecture')
  need(self.qheads>0 and self.kvheads>0 and self.qheads%self.kvheads==0 and self.dim>0 and self.dim%2==0 and self.eps==1e-6 and c['rope_theta']==1000000,'Qwen3 math/geometry differs')
  expected={'embed_tokens.weight':(c['vocab_size'],self.hidden),'norm.weight':(self.hidden,)}
  for i in range(self.layers):
   root=f'layers.{i}.'
   expected.update({root+'input_layernorm.weight':(self.hidden,),root+'post_attention_layernorm.weight':(self.hidden,),root+'self_attn.q_norm.weight':(self.dim,),root+'self_attn.k_norm.weight':(self.dim,)})
   for name,shape in {'self_attn.q_proj':(self.qheads*self.dim,self.hidden),'self_attn.k_proj':(self.kvheads*self.dim,self.hidden),'self_attn.v_proj':(self.kvheads*self.dim,self.hidden),'self_attn.o_proj':(self.hidden,self.qheads*self.dim),'mlp.gate_proj':(self.intermediate,self.hidden),'mlp.up_proj':(self.intermediate,self.hidden),'mlp.down_proj':(self.hidden,self.intermediate)}.items():expected[root+name+'.weight']=shape
  need(set(publisher_state)==set(expected),'missing/extra original publisher arrays')
  self.weights={};self.census=[]
  for name,value in publisher_state.items():
   need(value.dtype==torch.float32 and tuple(value.shape)==expected[name],'original logical FP32 tensor shape/type differs: '+name)
   checked_finite(torch,value,'original '+name)
   protected=name.endswith(('norm.weight','layernorm.weight'))
   rounded=value.detach().to(device=device,dtype=torch.float32 if protected else torch.float16).contiguous()
   self.weights[name]=checked_finite(torch,rounded,'rounded '+name)
   info={'parameter':name,'shape':list(value.shape),'protected_fp32':protected,'dtype':'float32' if protected else 'float16','original_fp32_sha256':tensor_digest(value)}
   if name.endswith('_proj.weight'):info['original_fp32_onnx_layout_sha256']=tensor_digest(value.transpose(0,1))
   self.census.append(info)
  self.head=None
  if focus:
   need(c['vocab_size']>9693,'trained Focus rows unavailable')
   self.head=checked_finite(torch,publisher_state['embed_tokens.weight'][[2152,9693]].detach().to(device=device,dtype=torch.float32).contiguous(),'trained Focus readout')
 def linear(self,value,name):
  t=self.torch;need(value.dtype==t.float32,'projection source island must be FP32')
  half=value.to(t.float16);checked_finite(t,half,'half projection input')
  return checked_finite(t,t.nn.functional.linear(half,self.weights[name]),'half projection output').float()
 def forward(self,row):
  t=self.torch;c=self.config;ids,mask,positions,candidates,options=validate_native(t,c,self.focus,row)
  with t.inference_mode(),t.autocast(device_type=t.device(self.device).type,enabled=False):
   token=t.tensor([ids],dtype=t.int64,device=self.device);position=t.tensor([positions],dtype=t.int64,device=self.device)
   hidden=t.nn.functional.embedding(token,self.weights['embed_tokens.weight']).float()
   for i in range(self.layers):
    root=f'layers.{i}.';residual=hidden;normal=rms(t,hidden,self.weights[root+'input_layernorm.weight'],self.eps)
    q=self.linear(normal,root+'self_attn.q_proj.weight').reshape(1,len(ids),self.qheads,self.dim)
    k=self.linear(normal,root+'self_attn.k_proj.weight').reshape(1,len(ids),self.kvheads,self.dim)
    v=self.linear(normal,root+'self_attn.v_proj.weight').reshape(1,len(ids),self.kvheads,self.dim).transpose(1,2)
    q=rms(t,q,self.weights[root+'self_attn.q_norm.weight'],self.eps).transpose(1,2)
    k=rms(t,k,self.weights[root+'self_attn.k_norm.weight'],self.eps).transpose(1,2)
    q,k=rotary(t,q,k,position,c['rope_theta'])
    attention=causal_half_attention(t,q.to(t.float16),k.to(t.float16),v.to(t.float16),mask).float().transpose(1,2).reshape(1,len(ids),self.qheads*self.dim)
    hidden=residual+self.linear(attention,root+'self_attn.o_proj.weight')
    residual=hidden;normal=rms(t,hidden,self.weights[root+'post_attention_layernorm.weight'],self.eps)
    gate=self.linear(normal,root+'mlp.gate_proj.weight');up=self.linear(normal,root+'mlp.up_proj.weight')
    # Independent explicit FP32 SiLU island followed by half down projection.
    activated=gate*t.sigmoid(gate);hidden=residual+self.linear(activated*up,root+'mlp.down_proj.weight')
    checked_finite(t,hidden,'FP32 residual')
   hidden=rms(t,hidden,self.weights['norm.weight'],self.eps)
   if self.focus:scores=t.nn.functional.linear(hidden[:,-1,:],self.head)
   else:
    decision=hidden[:,row['decision_position'],:]
    rows=t.nn.functional.embedding(t.tensor([candidates],dtype=t.int64,device=self.device),self.weights['embed_tokens.weight']).float()
    scores=(rows*decision[:,None,:]).sum(-1).masked_fill(~t.tensor([options],dtype=t.bool,device=self.device),-1e4)
   checked_finite(t,scores,'FP32 scores');need(scores.dtype==t.float32,'output ABI differs')
   return scores
 def parameter_census(self):
  # Invoke ONLY during an authorized actual reference run. Expected values derive
  # exclusively from original publisher arrays, never candidate ONNX outputs.
  rows=[]
  for info in self.census:
   value=self.weights[info['parameter']];row=dict(info);row['rounded_sha256']=tensor_digest(value)
   if info['parameter'].endswith('_proj.weight'):row['onnx_transposed_sha256']=tensor_digest(value.transpose(0,1))
   rows.append(row)
  if self.head is not None:
   value=self.head;rows.append({'parameter':'trained_readout_rows_2152_9693','shape':list(value.shape),'dtype':'float32','protected_fp32':True,'original_fp32_sha256':tensor_digest(value),'original_fp32_onnx_layout_sha256':tensor_digest(value.transpose(0,1)),'rounded_sha256':tensor_digest(value),'onnx_transposed_sha256':tensor_digest(value.transpose(0,1))})
  return rows

def from_authenticated_original(torch,model,owner_path,asset_root,device):
 """Actual source adapter, held until Root permits publisher reads/reference work.
 Original owners authenticate COMPLETE original checkpoints and publisher revision.
 """
 owner_path=pathlib.Path(owner_path);raw=owner_path.read_bytes();kind='focus' if model=='focus' else 'context'
 need(hashlib.sha256(raw).hexdigest()==PUBLISHER_OWNER_SHA[kind],'original publisher owner source differs')
 freeze_reference_math(torch)
 spec=importlib.util.spec_from_file_location('original_qwen_publisher',owner_path);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
 if kind=='focus':
  owner.verify_assets();official=owner.load_official(torch,'cpu');config=official.config.to_dict();state=official.model.state_dict()
 else:
  need(model in ('compact','large'),'selected original model differs')
  original,config=owner.load_authenticated(asset_root,'Qwen/Qwen3-1.7B' if model=='compact' else 'Qwen/Qwen3-4B');state=original.text.state_dict()
 result=HalfReference(torch,config,state,kind=='focus',device)
 return result,{'publisher_owner':{'path':str(owner_path),'sha256':PUBLISHER_OWNER_SHA[kind]},'model':model,'candidate_ONNX_used_for_expected_values':False,'logical_source_dtype':'float32','publisher_storage_dtype':'original authenticated publisher (Context BF16 expanded to FP32)','qualification':False}


def verify_actual_half_parameters(reference,graph_path,onnx,files):
 """Independent expected bytes -> ACTUAL candidate ranges; no inference.
 Native owner first authenticates/holds graph+external file pins and graph math.
 Never trust exporter per-range hashes as observations of candidate contents.
 """
 graph_path=pathlib.Path(graph_path);graph=onnx.load_model(str(graph_path),load_external_data=False)
 tensors={v.name:v for v in graph.graph.initializer};nodes={n.name:n for n in graph.graph.node}
 need(len(tensors)==len(graph.graph.initializer) and len(nodes)==len(graph.graph.node),'ambiguous actual graph')
 census=reference.parameter_census();prefix='base' if reference.focus else 'scorer';used=set();ranges={};bindings=[]
 for record in census:
  name=record['parameter'];projection=name.endswith('_proj.weight');head=name=='trained_readout_rows_2152_9693'
  if projection:
   pieces=name.split('.');route='/'+prefix+'/layers.'+pieces[1]+'/'+pieces[2]+'/'+pieces[3]+'/MatMul'
   need(route in nodes and nodes[route].op_type=='MatMul' and len(nodes[route].input)==2,'actual learned projection route differs')
   initializer=nodes[route].input[1]
  elif head:
   route='/MatMul';need(route in nodes and nodes[route].op_type=='MatMul' and len(nodes[route].input)==2,'actual trained head differs');initializer=nodes[route].input[1]
  else:route=None;initializer=('base.' if reference.focus else 'scorer.text.')+name
  need(initializer in tensors and initializer not in used,'missing/duplicate actual learned initializer');used.add(initializer);tensor=tensors[initializer]
  expected_shape=list(reversed(record['shape'])) if projection or head else record['shape']
  protected=record['protected_fp32'];expected_dtype=1 if protected else 10
  need(list(tensor.dims)==expected_shape and tensor.data_type==expected_dtype,'actual half/protected tensor shape/dtype differs')
  length=math.prod(expected_shape)*(4 if protected else 2)
  if tensor.data_location==1:
   entries={v.key:v.value for v in tensor.external_data};need(len(entries)==len(tensor.external_data) and set(entries)<= {'location','offset','length','checksum'},'external half fields differ')
   location=entries.get('location','');need(location not in ('','.','..') and len(pathlib.Path(location).parts)==1 and ':' not in location and '\\' not in location,'unsafe actual half location')
   path=graph_path.parent/location;need(location in files and pathlib.Path(files[location]).resolve()==path.resolve() and not path.is_symlink() and not path.is_junction(),'actual half file escaped native closure')
   first=int(entries.get('offset','0'));declared=int(entries.get('length',str(length)));need(first>=0 and declared==length and first+length<=path.stat().st_size,'actual half range differs')
   ranges.setdefault(path,[]).append((first,first+length));hasher=hashlib.sha256()
   with path.open('rb') as source:
    source.seek(first);remaining=length
    while remaining:
     raw=source.read(min(4*1024*1024,remaining));need(raw,'short actual half range');hasher.update(raw);remaining-=len(raw)
   actual=hasher.hexdigest()
  else:
   raw=onnx.numpy_helper.to_array(tensor).tobytes(order='C');need(len(raw)==length,'inline half byte count differs');actual=hashlib.sha256(raw).hexdigest()
  expected=record['onnx_transposed_sha256'] if projection or head else record['rounded_sha256']
  need(actual==expected,'actual candidate bytes differ from independent publisher half recipe: '+name)
  bindings.append({'parameter':name,'initializer':initializer,'node':route,'transposed':projection or head,'actual_initializer_sha256':actual,'independent_expected_sha256':expected,'original_fp32_sha256':record['original_fp32_sha256'],'shape':expected_shape,'dtype':record['dtype']})
 for path,spans in ranges.items():
  spans.sort();need(all(a[1]<=b[0] for a,b in zip(spans,spans[1:])),'overlapping actual half learned ranges')
 # Every floating initializer is accounted for. The one exporter half-zero is a
 # literal constant, not an extra embedding/projection or a hidden FP32 copy.
 floating={v.name for v in graph.graph.initializer if v.data_type in (1,10)};extra=floating-used
 if extra:
  need(extra=={'/airk_g5/half_zero'},'unmapped floating actual initializer')
  zero=tensors['/airk_g5/half_zero'];need(zero.data_type==10 and list(zero.dims)==[] and onnx.numpy_helper.to_array(zero).tobytes()==b'\x00\x00','half zero constant differs')
 need(len(bindings)==len(census),'incomplete actual learned mapping')
 return bindings
