# SPDX-License-Identifier: Apache-2.0
"""Prepared synthetic worst-shape resource pilot. Requires separate primary release.

No TRAIN/DEV/CAL/FINAL examples or pretrained weights are decoded. This is not training.
Must run inside the existing reviewed owned-process runtime seal/GO wrapper.
"""
from pathlib import Path
import argparse,hashlib,json,os,time,types
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'encoder-recovery-preparation-r1'

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def sealed_loss_source(expected):
    path=Path(__file__).resolve().parent.parent/'parent_bag_r1/bag_loss.py'
    raw=path.read_bytes()
    if len(expected)!=64 or hashlib.sha256(raw).hexdigest()!=expected:
        raise ValueError('explicit maintained loss source pin differs')
    return path,raw


def main():
 require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
 for name in ('HERE','PREP','H','MODELS'):
  if name in globals():require_private_root(globals()[name])
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--protocol-sha256',required=True);p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);p.add_argument('--cuda-equivalence',type=Path,required=True);p.add_argument('--cuda-equivalence-sha256',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--execution-seal',type=Path,required=True);p.add_argument('--execution-seal-sha256',required=True);p.add_argument('--loss-source-sha256',required=True);a=p.parse_args();self_before=sha(__file__)
 if sha(HERE/'protocol.json')!=a.protocol_sha256 or sha(a.release)!=a.release_sha256:raise ValueError('reviewed protocol/resource release changed')
 packet=json.loads((HERE/'protocol.json').read_bytes());release=json.loads(a.release.read_bytes())
 if release.get('protocol_sha256')!=a.protocol_sha256 or release.get('action')!='synthetic-worst-shape-bert-resource' or release.get('script_sha256')!=self_before:raise ValueError('separate primary resource release required')
 if sha(a.cuda_equivalence)!=a.cuda_equivalence_sha256:raise ValueError('CUDA proof changed')
 proof=json.loads(a.cuda_equivalence.read_bytes())
 if proof.get('protocol_sha256')!=a.protocol_sha256 or proof.get('device')!='cuda' or proof.get('complete') is not True or proof.get('selected')!=6 or proof.get('passed')!=6:raise ValueError('same-protocol actual CUDA equality required')
 if a.output.exists() or a.output.resolve().parent!=HERE:raise ValueError('fresh exact private output required')
 if os.environ.get('AIRK_TRAIN_START_GATE')!='1' or os.environ.get('CUBLAS_WORKSPACE_CONFIG')!=':4096:8':raise ValueError('owned reviewed gate/environment required')
 for file,pin in packet['pins'].items():
  if sha(file)!=pin:raise ValueError('frozen input drift: '+file)
 owner_path=Path(next(file for file in packet['pins'] if file.endswith('training_execution.py')));owner=types.ModuleType('_owned_resource');owner.__file__=str(owner_path);exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__);owner.verify_seal(a.execution_seal,a.execution_seal_sha256,wait_for_gate=True)
 loss_path,loss_raw=sealed_loss_source(a.loss_source_sha256)
 import torch
 owner.verify_torch_runtime(torch,a.execution_seal)
 from transformers import BertConfig,BertModel
 torch.set_num_threads(2);torch.manual_seed(1908);torch.cuda.manual_seed_all(1908);torch.use_deterministic_algorithms(True);torch.backends.cuda.matmul.allow_tf32=False;torch.backends.cudnn.allow_tf32=False
 torch.cuda.set_per_process_memory_fraction((4*1024**3)/torch.cuda.get_device_properties(0).total_memory)
 config_path=Path(packet['retained_recipe']).parent.parent/'minilm-training-source/config.json';config=json.loads(config_path.read_bytes())
 # Random initialization and synthetic token IDs only; exact live architecture and worst admitted dimensions.
 model=BertModel(BertConfig.from_dict(config),add_pooling_layer=False).to('cuda').train();optimizer=torch.optim.AdamW(model.parameters(),lr=2e-5,weight_decay=.01)
 module=types.ModuleType('_retained_parent_bag_loss');exec(compile(loss_raw,str(loss_path),'exec'),module.__dict__)
 def encode(ids,masks,types):
  hidden=model(input_ids=ids,attention_mask=masks,token_type_ids=types).last_hidden_state;pooled=(hidden*masks.unsqueeze(-1)).sum(1)/masks.sum(1,keepdim=True).clamp_min(1);return torch.nn.functional.normalize(pooled.float(),p=2,dim=1)
 def synthetic(count):
  ids=torch.randint(1,config['vocab_size'],(count,256),device='cuda');return ids,torch.ones_like(ids),torch.zeros_like(ids)
 queries=synthetic(8);units=synthetic(27);bags=[[i] for i in range(8)]+[[8+i,16+i,24+i%3] for i in range(8)];mask=torch.zeros((8,16),device='cuda',dtype=torch.bool);targets=torch.arange(8,device='cuda');torch.cuda.reset_peak_memory_stats();started=time.monotonic();losses=[]
 for step in range(2):
  optimizer.zero_grad(set_to_none=True);q=encode(*queries);u=module.encode_checkpointed(encode,units,microbatch=8,checkpointed=True);value=module.loss(q,u,bags,mask,targets)
  if not bool(torch.isfinite(value)):raise ValueError('synthetic nonfinite loss')
  value.backward();torch.nn.utils.clip_grad_norm_(model.parameters(),1.);optimizer.step();torch.cuda.synchronize();losses.append(float(value.detach().cpu()));print(json.dumps({'synthetic_resource_step':step+1,'peak_cuda_allocated_bytes':torch.cuda.max_memory_allocated()}),flush=True)
 peak=torch.cuda.max_memory_allocated()
 if peak>4*1024**3:raise ValueError('original allocator ceiling exceeded')
 owner.verify_seal(a.execution_seal,a.execution_seal_sha256)
 if sha(__file__)!=self_before:raise ValueError('resource source drift')
 result={'schema':'airk.private-parent-bag-resource-pilot/1','complete':True,'maintained_loss_source_sha256':a.loss_source_sha256,'protocol_sha256':a.protocol_sha256,'script_sha256':self_before,'cuda_equivalence_sha256':a.cuda_equivalence_sha256,'synthetic_only':True,'pretrained_weights_or_task_text_decoded':False,'architecture':{'layers':config['num_hidden_layers'],'hidden':config['hidden_size'],'heads':config['num_attention_heads']},'shape':{'queries':8,'units':27,'all_sequence_widths':256,'unit_microbatch':8},'steps':2,'optimizer':'AdamW fp32','losses':losses,'peak_cuda_allocated_bytes':peak,'peak_cuda_reserved_bytes':torch.cuda.max_memory_reserved(),'elapsed_seconds':time.monotonic()-started,'actual_training_peak_still_required':True,'training_or_quality_qualified':False};a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
if __name__=='__main__':main()
