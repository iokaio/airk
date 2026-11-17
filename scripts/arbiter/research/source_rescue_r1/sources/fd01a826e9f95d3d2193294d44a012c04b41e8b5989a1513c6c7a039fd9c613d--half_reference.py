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
   self.census.append({'parameter':name,'shape':list(value.shape),'protected_fp32':protected,'dtype':'float32' if protected else 'float16'})
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
   value=self.weights[info['parameter']].detach().cpu().contiguous();raw=value.numpy().tobytes(order='C');row=dict(info);row['rounded_sha256']=hashlib.sha256(raw).hexdigest()
   if info['parameter'].endswith('_proj.weight'):row['onnx_transposed_sha256']=hashlib.sha256(value.transpose(0,1).contiguous().numpy().tobytes(order='C')).hexdigest()
   rows.append(row)
  if self.head is not None:
   value=self.head.detach().cpu().contiguous();rows.append({'parameter':'trained_readout_rows_2152_9693','shape':list(value.shape),'dtype':'float32','protected_fp32':True,'rounded_sha256':hashlib.sha256(value.numpy().tobytes(order='C')).hexdigest(),'onnx_transposed_sha256':hashlib.sha256(value.transpose(0,1).contiguous().numpy().tobytes(order='C')).hexdigest()})
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
