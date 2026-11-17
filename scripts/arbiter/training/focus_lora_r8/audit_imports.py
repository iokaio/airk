# SPDX-License-Identifier: Apache-2.0
"""Metadata-only exact no-site import-closure audit. No model/tensor/device calls."""
import argparse,hashlib,json,os,sys,traceback
from pathlib import Path
from focus_worker import pinned,load_module


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--plan',type=Path,required=True);parser.add_argument('--plan-sha256',required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    plan=json.loads(pinned({'path':str(args.plan.resolve()),'sha256':args.plan_sha256}).read_bytes())
    if args.output.exists():raise ValueError('create-new import-audit output required')
    report={'schema':'airk.private.F1-no-site-import-audit/1','passed':False,'model_calls':0,'probe_tensor_calls':0,'CUDA_device_or_availability_calls':0,'failure':None,'imports':[]}
    try:
        if sys.flags.no_site!=1 or not sys.dont_write_bytecode or sys.pycache_prefix!=plan['environment']['PYTHONPYCACHEPREFIX']or Path(sys.pycache_prefix).exists():raise ValueError('no-site/fresh-unwritten-cache bootstrap differs')
        if 'torch'in sys.modules:raise ValueError('Torch unexpectedly preimported')
        import torch,numpy,safetensors,transformers
        if torch.cuda.is_initialized():raise ValueError('CUDA initialized during import')
        from transformers.models.qwen3.modeling_qwen3 import Qwen3ForCausalLM
        reference=load_module(plan['reference_worker'],'audit_actual_reference_worker')
        official=load_module(plan['reference_module'],'audit_actual_official_source')
        import gpu_identity,worker_logic,workflow
        if torch.cuda.is_initialized():raise ValueError('CUDA initialized by architecture/reference import')
        runtime=json.loads(pinned(plan['reference_plan']['runtime_manifest']).read_bytes())
        for name,module in list(sys.modules.items()):
            filename=getattr(module,'__dict__',{}).get('__file__')
            if not isinstance(filename,str):continue
            path=Path(filename).resolve()
            if not path.is_file():continue
            expected=runtime['files'].get(str(path))or plan['input_seals'].get(str(path))
            with path.open('rb')as stream:actual=hashlib.file_digest(stream,'sha256').hexdigest()
            report['imports'].append({'module':name,'path':str(path),'expected_sha256':expected,'actual_sha256':actual,'matched':expected==actual})
        reference.verify_loaded_runtime(plan['reference_plan'])
        if any(not x['matched']for x in report['imports']):raise ValueError('unsealed import')
        report.update({'passed':True,'actual_no_site':sys.flags.no_site,'dont_write_bytecode':sys.dont_write_bytecode,'pycache_prefix':sys.pycache_prefix,'prefix_absent':not Path(sys.pycache_prefix).exists(),'CUDA_initialized':torch.cuda.is_initialized(),'versions':{'torch':torch.__version__,'transformers':transformers.__version__,'numpy':numpy.__version__},'model_class_imported_only':Qwen3ForCausalLM.__name__,'sys_path':sys.path,'sitecustomize_imported':'sitecustomize'in sys.modules,'usercustomize_imported':'usercustomize'in sys.modules,'pywin32_bootstrap_imported':'pywin32_bootstrap'in sys.modules})
    except BaseException as error:report['failure']={'type':type(error).__name__,'message':str(error),'traceback':traceback.format_exc()}
    with args.output.open('x',encoding='utf-8')as stream:json.dump(report,stream,indent=2);stream.write('\n')
    print(json.dumps({'passed':report['passed'],'imported_files':len(report['imports']),'failure':report['failure'],'CUDA_initialized':report.get('CUDA_initialized')}))
    return 0 if report['passed']else 1

if __name__=='__main__':raise SystemExit(main())
