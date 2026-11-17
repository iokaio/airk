# SPDX-License-Identifier: Apache-2.0
"""One preselected epoch, fixed eligible weak parent bags, reviewed gate required."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import types

def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


ROOT=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'native-parent-bag-candidate-r1'

def sha(raw):return hashlib.sha256(raw).hexdigest()
def sealed(path,pin):
    raw=path.read_bytes()
    if sha(raw)!=pin:raise ValueError('training input drift: '+str(path))
    return raw

def main():
    require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
    for name in ('HERE','PREP','H','MODELS'):
     if name in globals():require_private_root(globals()[name])
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--execution-seal',type=Path,required=True)
    p.add_argument('--execution-seal-sha256',required=True)
    args=p.parse_args()
    owner_path=ROOT/'tools/retrieval-train/training_execution.py'
    owner=types.ModuleType('_sealed_training_owner');owner.__file__=str(owner_path)
    exec(compile(sealed(owner_path,'d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'),str(owner_path),'exec'),owner.__dict__)
    owner.verify_seal(args.execution_seal,args.execution_seal_sha256,wait_for_gate=True)
    import torch
    from safetensors.torch import load as load_tensors
    from transformers import BertConfig,BertModel
    from tokenizers import Tokenizer
    owner.verify_torch_runtime(torch,args.execution_seal)
    recipe_path=HERE/'recipe.json';recipe_raw=recipe_path.read_bytes();recipe=json.loads(recipe_raw)
    master_raw=sealed(HERE/'master.private.json',recipe['master_sha256']);master=json.loads(master_raw)
    units_raw=sealed((Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str(ROOT/'artifacts/advanced-split/models'))).resolve())/'native-parent-bag-units-r4/units.private.json',recipe['native_units_sha256']);native=json.loads(units_raw)
    batches_raw=sealed(HERE/'epoch1-batches.private.json',recipe['epoch1_batches_sha256']);batches=json.loads(batches_raw)
    loss_path=Path(__file__).resolve().parent/'bag_loss.py';loss_module=types.ModuleType('_sealed_bag_loss');loss_module.__file__=str(loss_path)
    exec(compile(sealed(loss_path,recipe['loss_source_sha256']),str(loss_path),'exec'),loss_module.__dict__)
    if master['all_rows']!=315 or master['admitted_rows']!=99 or len(batches)!=13:
        raise ValueError('frozen effective population changed')
    rows={r['ordinal']:r for r in master['rows']};units={u['id']:u for u in native['units']}
    if args.output.exists() or not args.output.resolve().is_relative_to(ROOT/'artifacts'):
        raise ValueError('fresh private output required')
    base=sealed(args.source/'model.safetensors','53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db')
    tok_raw=sealed(args.source/'tokenizer.json','be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037')
    config_raw=sealed(args.source/'config.json',recipe['config_sha256'])
    tokenizer=Tokenizer.from_str(tok_raw.decode());tokenizer.no_padding();tokenizer.no_truncation()
    # All exact native token inputs and all99 actual query inputs checked BEFORE gradients.
    query_inputs={}
    for row in master['rows']:
        if row['status']=='ADMITTED':
            e=tokenizer.encode(row['original_row']['question'])
            if not 2<=len(e.ids)<=256:raise ValueError('query would crop/change')
            query_inputs[row['ordinal']]={'input_ids':e.ids,'attention_mask':e.attention_mask,'token_type_ids':e.type_ids}
    used=set(i for batch in batches for i in batch['unit_ids'])
    for identifier in used:
        unit=units[identifier];e=tokenizer.encode(unit['input']);t=unit['native_input_tensors']
        if unit['embedding_admission']!='embeddable' or e.ids!=t['input_ids'][0] or e.attention_mask!=t['attention_mask'][0] or e.type_ids!=t['token_type_ids'][0]:
            raise ValueError('native actual tokens differ from candidate encoder')
    # Allocation cap is per device allocator; owned Job independently caps aggregate host.
    torch.cuda.set_per_process_memory_fraction((4*1024**3)/torch.cuda.get_device_properties(0).total_memory)
    torch.set_num_threads(2);torch.manual_seed(1908);torch.cuda.manual_seed_all(1908)
    torch.backends.cuda.matmul.allow_tf32=False;torch.backends.cudnn.allow_tf32=False
    torch.use_deterministic_algorithms(True)
    model=BertModel(BertConfig.from_dict(json.loads(config_raw)),add_pooling_layer=False)
    weights={k:v for k,v in load_tensors(base).items() if not k.startswith('pooler.') and k!='embeddings.position_ids'}
    model.load_state_dict(weights,strict=True);del weights,base
    device=torch.device('cuda');model.to(device);model.train()
    optimizer=torch.optim.AdamW(model.parameters(),lr=2e-5,weight_decay=.01)
    def tensor_batch(values):
        width=max(len(v['input_ids']) for v in values)
        return tuple(torch.tensor([v[key]+[0]*(width-len(v[key])) for v in values],device=device)
            for key in ['input_ids','attention_mask','token_type_ids'])
    def encode(ids,masks,types):
        hidden=model(input_ids=ids,attention_mask=masks,token_type_ids=types).last_hidden_state
        pooled=(hidden*masks.unsqueeze(-1)).sum(1)/masks.sum(1,keepdim=True).clamp_min(1)
        return torch.nn.functional.normalize(pooled.float(),p=2,dim=1)
    torch.cuda.reset_peak_memory_stats();losses=[]
    for batch in batches:
        active=[rows[i] for i in batch['rows']]
        if len(active)>8 or any(r['status']!='ADMITTED' for r in active):raise ValueError('batch admission changed')
        ids=batch['unit_ids']
        if len(ids)>27:raise ValueError('precommitted unit graph cap exceeded')
        # Exact embedding-key identity is shared. Collisions are checked, never guessed.
        representatives={};positions={};values=[]
        for identifier in ids:
            unit=units[identifier];key=unit['embedding_key']
            if key in representatives:
                old=units[representatives[key]]
                if old['input']!=unit['input'] or old['native_input_tensors']!=unit['native_input_tensors']:
                    raise ValueError('embedding-key collision')
            else:
                representatives[key]=identifier;positions[key]=len(values)
                values.append({name:value[0] for name,value in unit['native_input_tensors'].items()})
        bags=[[positions[units[i]['embedding_key']] for i in row[key]]
            for key in ['positive_unit_ids','negative_unit_ids'] for row in active]
        parent_ids=[r['original_row'][key] for key in ['positive','distractor'] for r in active]
        mask=torch.tensor([[parent in r['masked_parent_ids'] and j!=i for j,parent in enumerate(parent_ids)] for i,r in enumerate(active)],device=device)
        optimizer.zero_grad(set_to_none=True)
        queries=encode(*tensor_batch([query_inputs[r['ordinal']] for r in active]))
        passages=loss_module.encode_checkpointed(encode,tensor_batch(values),microbatch=8,checkpointed=True)
        value=loss_module.loss(queries,passages,bags,mask,torch.arange(len(active),device=device))
        value.backward();torch.nn.utils.clip_grad_norm_(model.parameters(),1.);optimizer.step()
        losses.append(float(value.detach().cpu()))
    args.output.mkdir(parents=True);checkpoint=args.output/'epoch-1';model.eval();model.save_pretrained(checkpoint,safe_serialization=True)
    (checkpoint/'tokenizer.json').write_bytes(tok_raw)
    owner.verify_seal(args.execution_seal,args.execution_seal_sha256)
    receipt={'schema':'airk.private-native-parent-bag-training/1','epoch':1,'preselected_epoch':1,'rows':315,'eligible_rows':99,'steps':len(losses),'losses':losses,
        'model_sha256':sha((checkpoint/'model.safetensors').read_bytes()),'recipe_sha256':sha(recipe_raw),'master_sha256':sha(master_raw),'native_units_sha256':sha(units_raw),
        'peak_cuda_allocated_bytes':torch.cuda.max_memory_allocated(),'weak_parent_bags':True,'fragment_positive_labels':False,'release_qualified':False}
    (args.output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)

if __name__=='__main__':main()
