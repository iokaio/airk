# SPDX-License-Identifier: Apache-2.0
"""Owned no-gradient feature pilot over exact TRAIN-only native rows. Not task qualification."""
from pathlib import Path
import argparse
import hashlib
import importlib.metadata
import json
import os
import sys
import time
import types
HERE=Path(__file__).resolve().parent
MODELS=HERE.parent
REPO=HERE.parents[3]

def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def load_module(path, expected, name):
    raw=Path(path).read_bytes()
    if hashlib.sha256(raw).hexdigest()!=expected:raise ValueError('reviewed Python source changed')
    module=types.ModuleType(name);module.__file__=str(path)
    exec(compile(raw,str(path),'exec'),module.__dict__)
    return module

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seal-sha256',required=True)
    args=parser.parse_args();seal_path=HERE/'pilot-seal.json'
    if digest(seal_path)!=args.seal_sha256:raise ValueError('pilot source/input freeze differs')
    seal=json.loads(seal_path.read_bytes())
    if os.environ.get('AIRK_FEATURE_START_GATE')!='1':raise ValueError('owned feature gate required')
    print('AIRK_FEATURE_GATE_READY:'+str(os.getpid()),flush=True)
    if sys.stdin.readline().strip()!='GO '+args.seal_sha256:raise ValueError('parent did not prove exact owned base before release')
    for name,value in seal['environment'].items():
        if os.environ.get(name)!=value:raise ValueError('frozen feature environment differs: '+name)
    for name,expected in seal['pins'].items():
        if digest(name)!=expected:raise ValueError('feature source/input drift: '+name)
    runtime=json.loads(Path(seal['runtime_reference']).read_bytes())
    if sys.version!=runtime['python_version']:raise ValueError('actual base Python version differs')
    if {name:importlib.metadata.version(name) for name in runtime['packages']}!=runtime['packages']:raise ValueError('Python package versions differ')
    # No Torch/model import happens before the owned-process gate and checked inputs.
    guards=load_module(HERE/'guards.py',seal['pins'][str(HERE/'guards.py')],'_feature_guards')
    projection=json.loads(Path(seal['train_projection']).read_bytes())
    rows=projection['per_model']['smallintern'];guards.validate_rows(rows,seal['train_question_order'])
    output=guards.isolated_output(seal['output'],HERE,seal['frozen_roots'])
    import torch
    from safetensors.torch import save_file
    if str(torch.__version__)!=runtime['torch']['version'] or str(torch.version.cuda)!=runtime['torch']['cuda'] or torch.backends.cudnn.version()!=runtime['torch']['cudnn']:raise ValueError('actual Torch/CUDA/cuDNN differ')
    if not torch.cuda.is_available():raise ValueError('reviewed CUDA profile unavailable')
    props=torch.cuda.get_device_properties(0)
    if props.name!=runtime['gpu']['name'] or [props.major,props.minor]!=runtime['gpu']['compute_capability']:raise ValueError('actual GPU architecture differs')
    free,total=torch.cuda.mem_get_info(0);baseline_free,baseline_total=free,total
    guards.admit_gpu(free,total,seal['limits']['cuda_allocator_bytes'],seal['limits']['cuda_context_headroom_bytes'])
    torch.cuda.set_per_process_memory_fraction(seal['limits']['cuda_allocator_bytes']/props.total_memory,0)
    torch.cuda.reset_peak_memory_stats(0)
    torch.backends.cuda.matmul.allow_tf32=False;torch.backends.cudnn.allow_tf32=False
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.manual_seed(7)
    torch.use_deterministic_algorithms(True)
    exporter=load_module(seal['exporter'],seal['pins'][seal['exporter']],'_retained_feature_exporter')
    # This exact retained loader authenticates the complete original assets.
    started=time.monotonic();output.mkdir();features={};observed=[]
    with torch.inference_mode():
        config,state,head,kind,asset_seals=exporter.load_candidate(Path(seal['assets']),'intern08')
        guards.require(kind=='intern','only pinned smallIntern feature pilot released')
        model=exporter.make_wrapper(config,state,head,kind).cuda().eval();del state,head
        for parameter in model.parameters():parameter.requires_grad_(False)
        all_ids=sorted({v for item in rows for v in item['native_row']['compiled']['candidate_positions']})
        initial={'symbol_ids':torch.tensor(all_ids,dtype=torch.int64),'weight':model.lm_head.weight[all_ids].detach().cpu().contiguous()}
        symbol_rows={value:i for i,value in enumerate(all_ids)}
        for ordinal,item in enumerate(rows):
            guards.require(time.monotonic()-started<900,'original feature loop deadline expired')
            free,total=torch.cuda.mem_get_info(0)
            guards.require(free>=seal['limits']['cuda_context_headroom_bytes'],'GPU headroom lost; no context or precision fallback')
            compiled=item['native_row']['compiled'];n=len(compiled['input_ids'])
            ids=torch.tensor([compiled['input_ids']],dtype=torch.long,device='cuda')
            mask=torch.tensor([compiled['attention_mask']],dtype=torch.long,device='cuda')
            positions=torch.tensor([compiled['position_ids']],dtype=torch.long,device='cuda')
            hidden=model.hidden(ids,mask,positions)[0];decision=hidden[compiled['decision_position']]
            candidate_ids=compiled['candidate_positions']
            symbols=torch.tensor(candidate_ids,dtype=torch.long,device='cuda')
            direct=(torch.nn.functional.embedding(symbols,model.lm_head.weight)*decision).sum(-1)
            cached=decision.cpu().contiguous();weights=initial['weight'][[symbol_rows[v] for v in candidate_ids]]
            replay=(weights*cached).sum(-1)
            error=float((direct.cpu()-replay).abs().max())
            guards.require(torch.allclose(direct.cpu(),replay,atol=1e-4,rtol=1e-5),'cached original-head replay differs; no feature manifest emitted')
            features[str(ordinal)+'.decide']=cached
            features[str(ordinal)+'.symbol_ids']=torch.tensor(candidate_ids,dtype=torch.int64)
            native=torch.tensor(item['native_row']['scores'],dtype=torch.float32)
            observed.append({'native_input_digest':compiled['input_digest'],'tokens':n,'candidate_count':len(candidate_ids),
                'cached_FP32_head_max_abs_error':error,'FP32_source_scores':direct.cpu().tolist(),
                'old_native_mixed_INT8_max_abs_drift':float((native-direct.cpu()).abs().max()),
                'old_native_drift_is_diagnostic_not_task_or_export_admission':True})
            print(json.dumps({'phase':'TRAIN_feature_done','ordinal':ordinal+1,'tokens':n,'elapsed_seconds':time.monotonic()-started}),flush=True)
            del hidden,decision,direct,replay,ids,mask,positions,symbols,weights
        peak=torch.cuda.max_memory_allocated(0);reserved=torch.cuda.max_memory_reserved(0)
        guards.require(peak<=seal['limits']['cuda_allocator_bytes'],'declared CUDA allocator cap exceeded')
        del model
    for path,expected in seal['pins'].items():
        if digest(path)!=expected:raise ValueError('feature input drift before completion')
    for path,expected in asset_seals.items():
        if exporter.digest(path)!=expected:raise ValueError('original model asset drift')
    guards.require(time.monotonic()-started<900,'original internal feature deadline expired at completion')
    save_file(features,str(output/'features.safetensors'));save_file(initial,str(output/'initial-head.safetensors'))
    result={'schema':'airk.private-original-head-feature-cache/1','complete':True,'model':'smallintern','kind':'intern',
        'examples':[item['example'] for item in rows],'indices_sha256':seal['original_indices_sha256'],
        'projection_sha256':seal['pins'][seal['train_projection']],'observed':observed,'asset_seals':asset_seals,
        'exporter_sha256':seal['pins'][seal['exporter']],'features_sha256':digest(output/'features.safetensors'),
        'head_sha256':digest(output/'initial-head.safetensors'),'elapsed_seconds':time.monotonic()-started,
        'peak_allocated_gpu_bytes':peak,'peak_reserved_gpu_bytes':reserved,'gpu_baseline_after_context':{'free_bytes':baseline_free,'total_bytes':baseline_total},
        'seal_sha256':args.seal_sha256,'gradients':False,'task_qualified':False,'calibration':False,'FINAL':False,
        'limits':'TRAIN-only weak-reference feature preparation; no semantic role/terminal labels, no gradient/export/activation.'}
    with (output/'features.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print(json.dumps({'complete':True,'feature_manifest_sha256':digest(output/'features.json'),'examples':44,'gradients':False,'task_qualified':False}),flush=True)
if __name__=='__main__':main()
