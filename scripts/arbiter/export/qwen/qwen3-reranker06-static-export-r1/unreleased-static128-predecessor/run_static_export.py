# SPDX-License-Identifier: Apache-2.0
"""One root-released CPU-only static full trained Focus export and six ORT controls."""
import gc
import importlib.metadata
import json
from pathlib import Path
import sys
import time
from graph_closure import graph_closure,digest

HERE=Path(__file__).resolve().parent
REFERENCE=HERE.parent/'qwen3-reranker06-experiment-r1'
sys.path.insert(0,str(REFERENCE))
import run_focus_export as reference

def run(output):
    output=Path(output).resolve()
    if output.exists() or not output.is_relative_to(HERE):raise ValueError('Fresh local private export output required')
    source=REFERENCE/'reference-r1'
    prior=json.loads((source/'receipt.json').read_bytes())
    if prior.get('complete') is not True or prior['phase']!='reference' or prior['model']!=reference.MODEL or prior['revision']!=reference.REVISION or len(prior['rows'])!=6:
        raise ValueError('Complete exact selected publisher reference required')
    pins=reference.verify_assets()
    pins.update(prior['input_pins'])
    pins[str(source/'receipt.json')]=digest(source/'receipt.json')
    for item in prior['assets']:
        if Path(item['name']).name!=item['name']:raise ValueError('Reference asset is not local')
        path=source/item['name']
        if path.is_symlink() or path.stat().st_size!=item['bytes'] or digest(path)!=item['sha256']:raise ValueError('Reference exact asset changed')
        pins[str(path)]=item['sha256']
    def verify():
        if any(digest(path)!=expected for path,expected in pins.items()):raise ValueError('Frozen reference or artifact input changed')
    verify();output.mkdir();started=time.monotonic()
    receipt={'schema':1,'phase':'static-fp32-export','complete':False,'qualification':False,'input_pins':pins,'rows':[]}
    try:
        import os
        if os.environ.get('CUDA_VISIBLE_DEVICES')!='':raise ValueError('Export must be explicitly CPU-only')
        import torch
        import numpy as np
        from types import SimpleNamespace
        reference.prepare_torch(torch,SimpleNamespace(device='cpu',threads=1))
        cases=np.load(source/'cases.npz')
        official=reference.load_official(torch,'cpu');wrapper=reference.wrapper_for(torch,official)
        values=tuple(torch.from_numpy(cases[f'{name}_0'])for name in ['input_ids','attention_mask','position_ids'])
        with torch.inference_mode():
            torch.onnx.export(wrapper,values,str(output/'model.onnx'),input_names=['input_ids','attention_mask','position_ids'],output_names=['logits'],opset_version=18,dynamo=False,do_constant_folding=True,external_data=True)
        del values,wrapper,official
        gc.collect()
        closure=graph_closure(output)
        # Separate ORT session after dropping the full HF model/trace tensors.
        import onnxruntime as ort
        session=reference.ort_session(ort,output/'model.onnx',1)
        for i,row in enumerate(prior['rows']):
            actual=session.run(['logits'],{name:cases[f'{name}_{i}']for name in ['input_ids','attention_mask','position_ids']})[0]
            expected=cases[f'expected_{i}']
            if actual.shape!=(1,2) or not np.isfinite(actual).all():raise ValueError('Invalid full static ONNX readout')
            raw,score=reference.errors(np,actual,expected);argmax=bool(np.argmax(actual)==np.argmax(expected))
            receipt['rows'].append(dict(name=row['name'],actual_logits=actual.tolist(),reference_logits=expected.tolist(),raw_max_error=raw,relevance_error=score,argmax_matches=argmax))
            if raw>reference.RAW_TOLERANCE or score>reference.SCORE_TOLERANCE or not argmax:raise ValueError('Frozen FP32 numerical tolerances exceeded')
        del session
        if graph_closure(output)!=closure:raise ValueError('Graph/sidecars changed during ORT controls')
        verify();receipt.update(complete=True,graph_closure=closure,precision='fp32',abi='static B1T128 threeInt64inputs twoFloat32logits',assets=closure['files'])
    except BaseException as error:
        receipt.update(error_type=type(error).__name__,error=str(error));raise
    finally:
        receipt['elapsed_seconds']=time.monotonic()-started
        receipt['versions']={name:importlib.metadata.version(name)for name in ['torch','transformers','onnx','onnxruntime','numpy']}
        (output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8')
        print(json.dumps({'complete':receipt['complete'],'rows':len(receipt['rows']),'elapsed_seconds':receipt['elapsed_seconds'],'error':receipt.get('error')}),flush=True)
