# SPDX-License-Identifier: Apache-2.0
"""Tiny synthetic-only direct/checkpoint math proof; never loads encoder or corpus."""
from pathlib import Path
import argparse,copy,hashlib,json,os,time,types
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
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--protocol-sha256',required=True);p.add_argument('--device',choices=['cpu','cuda'],default='cpu');p.add_argument('--output',type=Path,required=True);p.add_argument('--release',type=Path);p.add_argument('--release-sha256');p.add_argument('--execution-seal',type=Path);p.add_argument('--execution-seal-sha256');p.add_argument('--loss-source-sha256',required=True);args=p.parse_args()
 self_before=sha(__file__)
 protocol=HERE/'protocol.json'
 if sha(protocol)!=args.protocol_sha256:raise ValueError('reviewed protocol changed')
 packet=json.loads(protocol.read_bytes())
 for file,pin in packet['pins'].items():
  if sha(file)!=pin:raise ValueError('frozen input changed: '+file)
 if args.output.exists() or args.output.resolve().parent!=HERE:raise ValueError('fresh exact private result required')
 if args.device=='cuda':
  if not args.release or sha(args.release)!=args.release_sha256:raise ValueError('primary CUDA release required')
  release=json.loads(args.release.read_bytes())
  if release.get('protocol_sha256')!=args.protocol_sha256 or release.get('action')!='tiny-synthetic-cuda-equivalence' or release.get('script_sha256')!=sha(__file__):raise ValueError('wrong primary CUDA release')
  if os.environ.get('CUBLAS_WORKSPACE_CONFIG')!=':4096:8':raise ValueError('reviewed deterministic CUDA environment required')
  if not args.execution_seal or not args.execution_seal_sha256:raise ValueError('fresh owned runtime execution seal required')
  owner_path=Path(next(file for file in packet['pins'] if file.endswith('training_execution.py')));owner=types.ModuleType('_owned_equivalence');owner.__file__=str(owner_path);exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__);owner.verify_seal(args.execution_seal,args.execution_seal_sha256,wait_for_gate=True)
 loss_path,loss_raw=sealed_loss_source(args.loss_source_sha256)
 import torch
 if args.device=='cuda':owner.verify_torch_runtime(torch,args.execution_seal)
 torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
 if args.device=='cuda':
  if not torch.cuda.is_available():raise ValueError('CUDA unavailable')
  torch.backends.cuda.matmul.allow_tf32=False;torch.backends.cudnn.allow_tf32=False
  torch.cuda.set_per_process_memory_fraction((4*1024**3)/torch.cuda.get_device_properties(0).total_memory)
  torch.cuda.reset_peak_memory_stats()
 device=torch.device(args.device);module=types.ModuleType('_retained_parent_bag_loss');exec(compile(loss_raw,str(loss_path),'exec'),module.__dict__)
 started=time.monotonic();checks=[]
 for seed in [789,1908]:
  for size in [9,27]:
   torch.manual_seed(seed);original=torch.nn.Sequential(torch.nn.Linear(6,8),torch.nn.Dropout(.25),torch.nn.Linear(8,4)).to(device);original.register_parameter('intentionally_unused',torch.nn.Parameter(torch.tensor([3.],device=device)))
   query=torch.randn(2,6,device=device);passages=torch.randn(size,6,device=device);bags=[[0,1,2],[2,3,4],[4,5,6],[size-2,size-1]];mask=torch.tensor([[False,False,False,True],[False,False,True,False]],device=device);observed=[]
   for checkpointed in [False,True]:
    model=copy.deepcopy(original)
    def encode(value):return torch.nn.functional.normalize(model(value),dim=1)
    torch.manual_seed(seed+1)
    if args.device=='cuda':torch.cuda.manual_seed_all(seed+1)
    q=encode(query);u=module.encode_checkpointed(encode,(passages,),microbatch=2 if size==9 else 8,checkpointed=checkpointed);value=module.loss(q,u,bags,mask,torch.tensor([0,1],device=device));value.backward()
    winners=[(q.detach()@u.detach()[bag].T).max(dim=1).indices for bag in bags]
    observed.append((value.detach(),[v.grad.clone() if v.grad is not None else None for v in model.parameters()],torch.get_rng_state(),torch.cuda.get_rng_state() if args.device=='cuda' else None,winners))
   a,b=observed;torch.testing.assert_close(a[0],b[0],rtol=0,atol=0)
   for x,y in zip(a[1],b[1]):
    if x is None:assert y is None
    else:torch.testing.assert_close(x,y,rtol=0,atol=0)
   assert torch.equal(a[2],b[2]);assert (a[3] is None and b[3] is None) or torch.equal(a[3],b[3]);assert all(torch.equal(x,y) for x,y in zip(a[4],b[4]));checks.append({'seed':seed,'unique_units':size,'loss_gradient_None_ties_rng_bit_identical':True})
 q=torch.tensor([[1.,0.]],device=device);u=torch.tensor([[1.,0.],[1.,0.],[0.,1.]],device=device,requires_grad=True);actual=module.logits_for_bags(q,u,[[0,1],[2]],temperature=1.);assert actual.tolist()==[[1.,0.]];actual[0,0].backward();assert u.grad.tolist()==[[1.,0.],[0.,0.],[0.,0.]];checks.append({'literal_first_tie_gradient':True})
 try:module.loss(q,u,[[0],[2]],torch.tensor([[False,True]],device=device),torch.tensor([0],device=device));raise AssertionError('masked alternatives must refuse')
 except ValueError:checks.append({'all_alternatives_masked_refused':True})
 if args.device=='cuda':torch.cuda.synchronize()
 result={'schema':'airk.private-parent-bag-equivalence/1','complete':True,'device':args.device,'selected':len(checks),'passed':len(checks),'checks':checks,'maintained_loss_source_sha256':args.loss_source_sha256,'protocol_sha256':args.protocol_sha256,'script_sha256':sha(__file__),'torch':str(torch.__version__),'elapsed_seconds':time.monotonic()-started,'synthetic_only':True,'encoder_weights_or_corpus_decoded_or_used':False,'cuda_peak_allocated_bytes':torch.cuda.max_memory_allocated() if args.device=='cuda' else 0}
 if args.device=='cuda':owner.verify_seal(args.execution_seal,args.execution_seal_sha256)
 if sha(__file__)!=self_before:raise ValueError('equivalence source changed during execution')
 args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
if __name__=='__main__':main()
