# SPDX-License-Identifier: Apache-2.0
"""Reviewed-plan-only full publisher LM F1 scorer/F2 LoRA worker. No imports run NN."""
import argparse,hashlib,importlib.metadata,importlib.util,json,math,os,struct,sys,time
from pathlib import Path
import worker_logic as logic
from workflow import continuity_key,load_owned

def digest(path):
    with Path(path).open('rb')as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pinned(pin):
    p=Path(pin['path']);m=p.lstat()
    if not p.is_absolute()or p.is_symlink()or getattr(m,'st_file_attributes',0)&0x400 or not p.is_file()or digest(p)!=pin['sha256']:raise ValueError('sealed input changed: '+str(p))
    if 'bytes'in pin and m.st_size!=pin['bytes']:raise ValueError('sealed input size differs')
    return p

def load_module(pin,name):
    p=pinned(pin);spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);sys.modules[name]=m;spec.loader.exec_module(m);return m

def verify_plan(plan):
    if plan['schema']!='airk.private.F1-F2-owned-plan/1'or plan['mode']not in('score','probe','train')or plan['human_sid']!='S-1-5-21-3678370870-3858642417-826968105-1002':raise ValueError('reviewed F1/F2 scope required')
    limit=plan['limits']
    if not 0<limit['wall_seconds']<=600 or limit['host_committed_bytes']>16*1024**3 or limit['cuda_allocator_bytes']>8*1024**3:raise ValueError('fixed resource cap exceeded')
    if plan['mode']=='probe'and(limit['wall_seconds']>120 or plan['max_optimizer_steps']!=1):raise ValueError('bounded one-update probe required')
    if plan['mode']=='train'and(not plan.get('measured_probe')or plan['max_optimizer_steps']!=14):raise ValueError('measured probe before oneepoch max14steps')
    if plan['continuity_key']!=continuity_key(plan):raise ValueError('immutable model/runtime/data/source/owner/caps continuity differs')
    if not {'torch','transformers','safetensors','numpy'}<=set(plan['versions'])or not plan['input_seals']:raise ValueError('complete source/runtime/checkpoint seal required')
    for key,value in plan['environment'].items():
        if os.environ.get(key)!=value:raise ValueError('sealed environment changed: '+key)
    for name,expected in plan['versions'].items():
        if importlib.metadata.version(name)!=expected:raise ValueError('runtime version changed: '+name)
    for path,sha in plan['input_seals'].items():pinned({'path':path,'sha256':sha})

def inject_lora(torch,model,rank=16):
    class LoRALinear(torch.nn.Module):
        def __init__(self,base):
            super().__init__();self.base=base
            for p in base.parameters():p.requires_grad_(False)
            self.lora_A=torch.nn.Parameter(torch.empty(rank,base.in_features,device=base.weight.device,dtype=torch.float32));self.lora_B=torch.nn.Parameter(torch.zeros(base.out_features,rank,device=base.weight.device,dtype=torch.float32));torch.nn.init.kaiming_uniform_(self.lora_A,a=math.sqrt(5));self.scale=1.0
        def forward(self,x):
            return self.base(x)+torch.nn.functional.linear(torch.nn.functional.linear(x,self.lora_A),self.lora_B)*self.scale
    targets=[]
    for name,module in list(model.named_modules()):
        if name.endswith(('.self_attn.q_proj','.self_attn.k_proj','.self_attn.v_proj','.self_attn.o_proj')):
            if not isinstance(module,torch.nn.Linear)or module.weight.dtype!=torch.float32:raise ValueError('full original FP32 attention projection required')
            parent_name,leaf=name.rsplit('.',1);parent=model.get_submodule(parent_name);setattr(parent,leaf,LoRALinear(module));targets.append(name)
    if len(targets)!=4*model.config.num_hidden_layers:raise ValueError('exact all-layer q/k/v/o LoRA roster required')
    return targets

def merge_lora(torch,model,targets):
    with torch.no_grad():
        for name in targets:
            parent_name,leaf=name.rsplit('.',1);parent=model.get_submodule(parent_name);layer=getattr(parent,leaf)
            delta=(layer.lora_B@layer.lora_A)*layer.scale
            if not torch.isfinite(delta).all():raise ValueError('nonfinite merged LoRA delta')
            layer.base.weight.add_(delta);setattr(parent,leaf,layer.base)
    if model.lm_head.weight.data_ptr()!=model.model.embed_tokens.weight.data_ptr():raise ValueError('merged full tied head changed')

def response_loss(torch,logits,label,vocab_size):
    if logits.ndim!=2 or logits.shape!=(1,vocab_size)or vocab_size<=9693 or not torch.isfinite(logits).all():raise ValueError('complete finite full trained vocabulary required')
    expected=torch.tensor([logic.target(label)],dtype=torch.int64,device=logits.device)
    return torch.nn.functional.cross_entropy(logits,expected)

def run(plan,plan_sha):
    if os.environ.get('AIRK_F2_START_GATE')!='1':raise ValueError('actual owned pre-Torch gate required')
    print('AIRK_F2_GATE_READY:'+str(os.getpid()),flush=True)
    message=sys.stdin.readline().strip().split()
    if len(message)!=3 or message[:2]!=['GO',plan_sha]:raise ValueError('owned GO/source binding absent')
    deadline=float(message[2]);started=time.monotonic()
    def time_left():
        if time.monotonic()>=deadline:raise TimeoutError('original F1/F2 owned deadline expired; no replay')
    time_left();verify_plan(plan);time_left()
    reference=load_module(plan['reference_worker'],'reviewed_focus_reference_worker')
    if reference.windows_sid()!=plan['human_sid']:raise ValueError('actual human SID differs')
    def audit(event,args):
        if event in('subprocess.Popen','os.system','os.posix_spawn','os.fork','socket.connect'):raise RuntimeError('worker cannot spawn another process or use network')
    sys.addaudithook(audit)
    data=json.loads(pinned(plan['native_census']).read_bytes());roster=json.loads(pinned(plan['cohort']).read_bytes());replay=json.loads(pinned(plan['replay']).read_bytes());rows=logic.native_rows(data,roster,replay)
    recipe=plan.get('selected_instruction','publisher')
    if recipe not in('publisher','software_evidence_v1'):raise ValueError('only native two-instruction catalog')
    if plan['mode']!='score':
        selected=json.loads(pinned(plan['selection']).read_bytes());scores=json.loads(pinned(selected['score_report']).read_bytes())
        choice=logic.choose_instruction(scores,rows)
        actual_score=load_owned(selected['owned_score_receipt'],'score')
        if actual_score['continuity_key']!=continuity_key(plan)or selected['continuity_key']!=actual_score['continuity_key']or selected['score_report']['sha256']!=actual_score['execution']['worker_result_sha256']or logic.choose_instruction(actual_score['worker'],rows)!=choice:raise ValueError('actual owned SCORE/model/runtime/source continuity differs')
        if scores['native_census_sha256']!=plan['native_census']['sha256']:raise ValueError('DEV score consumed another census')
        if choice['selected']!=recipe or selected['choice']!=choice or selected['native_census_sha256']!=plan['native_census']['sha256']:raise ValueError('genuine fixedDEV choice differs')
        if plan['mode']=='train':
            actual_probe=load_owned(plan['measured_probe'],'probe')
            if actual_probe['continuity_key']!=continuity_key(plan)or actual_probe['plan']['selected_instruction']!=recipe:raise ValueError('actual probe model/runtime/source/instruction continuity differs')
        schedule=logic.training_schedule(rows,recipe)
    import torch
    torch.manual_seed(1908);torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.allow_tf32=False;torch.backends.cudnn.allow_tf32=False;torch.backends.cudnn.benchmark=False;torch.backends.cudnn.deterministic=True
    if not torch.cuda.is_available():raise ValueError('approved GPU unavailable; no fallback')
    prop=torch.cuda.get_device_properties(0);free,total=torch.cuda.mem_get_info()
    if prop.name!=plan['gpu']['name']or str(prop.uuid)!=plan['gpu']['uuid']or free<plan['limits']['minimum_cuda_free_bytes']:raise ValueError('actual GPU identity or reserve differs')
    torch.cuda.set_per_process_memory_fraction(plan['limits']['cuda_allocator_bytes']/total,0);torch.cuda.reset_peak_memory_stats();time_left()
    official_source=load_module(plan['reference_module'],'reviewed_full_focus_source');official_source.verify_assets();model=official_source.load_official(torch,'cuda')
    for p in model.parameters():p.requires_grad_(False)
    model.eval();time_left();reference.verify_loaded_runtime(plan['reference_plan'])
    output=Path(plan['worker_output']).resolve()
    if output.exists()or not output.is_relative_to(Path(plan['log_dir']).resolve()):raise ValueError('fresh owned worker output required')
    output.mkdir();report_path=output/'result.json'
    result={'schema':'airk.private.F1-F2-worker-result/1','mode':plan['mode'],'plan_sha256':plan_sha,'native_census_sha256':plan['native_census']['sha256'],'complete':False,'rows':[],'updates':[],'forward_attempts':0,'forward_calls':0,'backward_attempts':0,'backwards_completed':0,'optimizer_attempts':0,'gradient_steps':0,'qualification':False,'promotion':False,'actual_model':'original full publisher LM, FP32; no exported2row head'}
    def retain():
        raw=json.dumps(result,indent=2).encode()
        if len(raw)>4*1024*1024:raise ValueError('bounded worker receipt exceeded')
        report_path.write_bytes(raw+b'\n')
    def tensors(input):
        logic.check_input(input)
        return {k:torch.tensor([input[k]],dtype=torch.int64,device='cuda')for k in('input_ids','attention_mask','position_ids')}
    def last_logits(input):
        time_left();result['forward_attempts']+=1
        hidden=model.model(**tensors(input),use_cache=False,return_dict=True).last_hidden_state[:,input['decision_position'],:]
        logits=model.lm_head(hidden)
        if logits.shape!=(1,model.config.vocab_size)or not torch.isfinite(logits).all():raise ValueError('full finite trained vocabulary head required')
        result['forward_calls']+=1;return logits
    def raw_bits(x):return struct.unpack('<I',struct.pack('<f',x))[0]
    try:
        if plan['mode']=='score':
            dev=sorted((r for r in rows.values()if r['pair']['role']=='DEV'),key=lambda r:r['pair']['id'])
            if len(dev)!=85:raise ValueError('entire85DEV required')
            with torch.inference_mode():
                for row in dev:
                    observation={'id':row['pair']['id'],'label':row['pair']['label'],'component':row['component']}
                    for instruction in('publisher','software_evidence_v1'):
                        native=row[instruction]
                        if native['kind']=='refused':observation[instruction]={'status':'native_refused','reason':native['reason']};continue
                        start=time.monotonic();logits=last_logits(native['input']);torch.cuda.synchronize();time_left();no,yes=logits[0,[2152,9693]].detach().float().cpu().tolist()
                        observation[instruction]={'status':'scored','input_digest':native['input']['input_digest'],'no_logit':no,'yes_logit':yes,'no_bits':raw_bits(no),'yes_bits':raw_bits(yes),'elapsed_seconds':time.monotonic()-start}
                    result['rows'].append(observation);retain()
        else:
            targets=inject_lora(torch,model);result['lora_targets']=targets;model.gradient_checkpointing_enable();model.enable_input_require_grads();model.train()
            # Norm/head/RoPE and all base weights stay FP32/frozen, dropout=0.
            for module in model.modules():
                if isinstance(module,torch.nn.Dropout):module.p=0.0
            parameters=[p for p in model.parameters()if p.requires_grad]
            if not parameters or any(p.dtype!=torch.float32 for p in parameters):raise ValueError('exact FP32 LoRA trainable parameters required')
            optimizer=torch.optim.AdamW(parameters,lr=5e-5,weight_decay=0.0)
            groups=logic.batches(schedule,32)
            if len(groups)!=14:raise ValueError('one445draw epoch requires14updates')
            for step,group in enumerate(groups[:plan['max_optimizer_steps']],1):
                optimizer.zero_grad(set_to_none=True);losses=[]
                for row in group:
                    logits=last_logits(row[logic.effective_recipe(row,recipe)]['input'])
                    loss=response_loss(torch,logits,row['pair']['label'],model.config.vocab_size)
                    if not torch.isfinite(loss):raise ValueError('nonfinite full-vocabulary loss')
                    result['backward_attempts']+=1
                    (loss/len(group)).backward();torch.cuda.synchronize();time_left();result['backwards_completed']+=1;losses.append(float(loss.detach().cpu()))
                if any(p.grad is None or not torch.isfinite(p.grad).all()for p in parameters):raise ValueError('missing/nonfinite LoRA gradients')
                result['optimizer_attempts']+=1
                optimizer.step();torch.cuda.synchronize();time_left()
                if any(not torch.isfinite(p).all()for p in parameters):raise ValueError('nonfinite LoRA parameter')
                result['gradient_steps']+=1;result['updates'].append({'step':step,'draws':len(group),'ids':[r['pair']['id']for r in group],'native_recipes':[logic.effective_recipe(r,recipe)for r in group],'loss_sum':sum(losses),'mean_full_vocab_ce':sum(losses)/len(losses)});retain()
            if plan['mode']=='train':
                merge_lora(torch,model,targets);model.eval();time_left();model.save_pretrained(output/'merged-full-checkpoint',safe_serialization=True,max_shard_size='3GB');time_left();result['merged_full_checkpoint']=True
            else:result['probe_adapter_discarded']=True
        torch.cuda.synchronize();time_left();reference.verify_loaded_runtime(plan['reference_plan'])
        for path,sha in plan['input_seals'].items():
            time_left()
            if digest(path)!=sha:raise ValueError('consumed source/runtime/checkpoint changed')
        time_left()
        if torch.cuda.max_memory_allocated()>plan['limits']['cuda_allocator_bytes']or torch.cuda.max_memory_reserved()>plan['limits']['cuda_allocator_bytes']:raise ValueError('CUDA peak exceeded exact allocator cap')
        if plan['mode']=='train'and result['gradient_steps']!=14:raise ValueError('incomplete oneepoch training')
        if plan['mode']=='probe'and result['gradient_steps']!=1:raise ValueError('incomplete measured one-update probe')
        result['complete']=True
    except BaseException as error:
        result['failure']={'type':type(error).__name__,'message':str(error)};raise
    finally:
        result['elapsed_seconds_after_GO']=time.monotonic()-started;result['peak_cuda_allocated_bytes']=torch.cuda.max_memory_allocated();result['peak_cuda_reserved_bytes']=torch.cuda.max_memory_reserved();retain()
    return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--plan',type=Path,required=True);p.add_argument('--plan-sha256',required=True);args=p.parse_args()
    if digest(args.plan)!=args.plan_sha256:raise ValueError('reviewed plan differs')
    run(json.loads(args.plan.read_bytes()),args.plan_sha256)
if __name__=='__main__':main()
