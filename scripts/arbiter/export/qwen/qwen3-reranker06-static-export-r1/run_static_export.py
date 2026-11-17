# SPDX-License-Identifier: Apache-2.0
"""One root-released static512 full trained Focus reference/export and ORT controls."""
import gc
import importlib.metadata
import json
from pathlib import Path
import sys
import time
from graph_closure import graph_closure,digest
from focus_model_view import model_view

HERE=Path(__file__).resolve().parent
REFERENCE=HERE.parent/'qwen3-reranker06-experiment-r1'
sys.path.insert(0,str(REFERENCE))
import run_focus_export as reference

def run(output, *, source, case_path):
    output=Path(output).resolve()
    if output.exists() or not output.is_relative_to(reference.ROOT/'artifacts'):raise ValueError('Fresh local private export output required')
    source=Path(source).resolve()
    case_path=Path(case_path).resolve()
    if not source.is_relative_to(reference.ROOT/'artifacts') or not case_path.is_relative_to(reference.ROOT/'artifacts'):raise ValueError('Private explicit reference and cases required')
    prior=json.loads((source/'receipt.json').read_bytes())
    if prior.get('complete') is not True or prior['phase']!='reference' or prior['model']!=reference.MODEL or prior['revision']!=reference.REVISION or len(prior['rows'])!=6:
        raise ValueError('Complete exact selected publisher reference required')
    pins=reference.verify_assets()
    pins.update(prior['input_pins'])
    pins[str(case_path)]=digest(case_path)
    case_rows=json.loads(case_path.read_bytes())
    if len(case_rows)!=12 or len({row['name']for row in case_rows})!=12:raise ValueError('Exact independently authored twelve512 controls required')
    for row in case_rows:
        if 'original_query' in row and (row['query']!=model_view(row['original_query']) or row['document']!=model_view(row['original_document'])):
            raise ValueError('Approved model-view transform differs from original input disclosure')
    pins[str(source/'receipt.json')]=digest(source/'receipt.json')
    for item in prior['assets']:
        if Path(item['name']).name!=item['name']:raise ValueError('Reference asset is not local')
        path=source/item['name']
        if path.is_symlink() or path.stat().st_size!=item['bytes'] or digest(path)!=item['sha256']:raise ValueError('Reference exact asset changed')
        pins[str(path)]=item['sha256']
    def verify():
        if any(digest(path)!=expected for path,expected in pins.items()):raise ValueError('Frozen reference or artifact input changed')
    verify();output.mkdir();started=time.monotonic()
    receipt={'schema':1,'phase':'static512-fp32-reference-export','complete':False,'qualification':False,'input_pins':pins,'reference_rows':[],'rows':[]}
    try:
        import os
        if os.environ.get('CUDA_VISIBLE_DEVICES')!='':raise ValueError('Export must be explicitly CPU-only')
        import torch
        import numpy as np
        from types import SimpleNamespace
        reference.prepare_torch(torch,SimpleNamespace(device='cpu',threads=1))
        official=reference.load_official(torch,'cpu');wrapper=reference.wrapper_for(torch,official)
        from transformers import AutoTokenizer
        publisher_tokenizer=AutoTokenizer.from_pretrained(reference.ASSETS,local_files_only=True,trust_remote_code=False,padding_side='left')
        tokenizer=reference.local_tokenizer();archive={}
        for i,row in enumerate(case_rows):
            query,document=row['query'],row['document']
            pair=reference.compile_pair(tokenizer,query,document,512)
            values=reference.tensors(torch,pair,'cpu')
            body=f'<Instruct>: {reference.INSTRUCTION}\n<Query>: {query}\n<Document>: {document}'
            publisher_ids=(publisher_tokenizer.encode(reference.PREFIX,add_special_tokens=False)
                +publisher_tokenizer(body,padding=False,truncation=False,return_attention_mask=False)['input_ids']
                +publisher_tokenizer.encode(reference.SUFFIX,add_special_tokens=False))
            if list(pair.input_ids)[-len(publisher_ids):]!=publisher_ids or pair.observed_tokens!=len(publisher_ids):raise ValueError('Publisher512 tokenizer/compiler tensors differ')
            if row['name']=='long-maintenance-synthetic' and not 480<=pair.observed_tokens<=512:raise ValueError('Long512 reference no longer exercises near-cap source')
            with torch.inference_mode():
                expected=official(input_ids=values[0],attention_mask=values[1],use_cache=False,return_dict=True).logits[:,-1,[reference.FALSE_TOKEN,reference.TRUE_TOKEN]].numpy().copy()
                actual=wrapper(*values).numpy().copy()
            if expected.shape!=(1,2) or actual.shape!=(1,2) or not np.isfinite(actual).all() or not np.isfinite(expected).all():raise ValueError('Invalid512 publisher/reference readout')
            raw,score=reference.errors(np,actual,expected);argmax=bool(np.argmax(actual)==np.argmax(expected))
            receipt['reference_rows'].append(dict(name=row['name'],kind=row['kind'],observed_tokens=pair.observed_tokens,padded_tokens=512,official_logits=expected.tolist(),wrapper_logits=actual.tolist(),raw_max_error=raw,relevance_error=score,argmax_matches=argmax))
            for name,value in zip(['input_ids','attention_mask','position_ids'],values):archive[f'{name}_{i}']=value.numpy().copy()
            archive[f'expected_{i}']=expected
            if raw>reference.RAW_TOLERANCE or score>reference.SCORE_TOLERANCE or not argmax:raise ValueError('Frozen512 publisher/wrapper numerical tolerances exceeded')
        np.savez_compressed(output/'reference512-cases.npz',**archive)
        cases=archive
        values=tuple(torch.from_numpy(cases[f'{name}_0'])for name in ['input_ids','attention_mask','position_ids'])
        with torch.inference_mode():
            torch.onnx.export(wrapper,values,str(output/'model.onnx'),input_names=['input_ids','attention_mask','position_ids'],output_names=['logits'],opset_version=18,dynamo=False,do_constant_folding=True,external_data=True)
        del values,wrapper,official
        gc.collect()
        closure=graph_closure(output,expected_extra=['reference512-cases.npz'])
        # Separate ORT session after dropping the full HF model/trace tensors.
        import onnxruntime as ort
        session=reference.ort_session(ort,output/'model.onnx',1)
        for i,row in enumerate(case_rows):
            actual=session.run(['logits'],{name:cases[f'{name}_{i}']for name in ['input_ids','attention_mask','position_ids']})[0]
            expected=cases[f'expected_{i}']
            if actual.shape!=(1,2) or not np.isfinite(actual).all():raise ValueError('Invalid full static ONNX readout')
            raw,score=reference.errors(np,actual,expected);argmax=bool(np.argmax(actual)==np.argmax(expected))
            receipt['rows'].append(dict(name=row['name'],actual_logits=actual.tolist(),reference_logits=expected.tolist(),raw_max_error=raw,relevance_error=score,argmax_matches=argmax))
            if raw>reference.RAW_TOLERANCE or score>reference.SCORE_TOLERANCE or not argmax:raise ValueError('Frozen FP32 numerical tolerances exceeded')
        del session
        if graph_closure(output,expected_extra=['reference512-cases.npz'])!=closure:raise ValueError('Graph/sidecars changed during ORT controls')
        case_asset=dict(name='reference512-cases.npz',bytes=(output/'reference512-cases.npz').stat().st_size,sha256=digest(output/'reference512-cases.npz'))
        verify();receipt.update(complete=True,graph_closure=closure,precision='fp32',abi='static B1T512 threeInt64inputs twoFloat32logits',assets=closure['files']+[case_asset],raw_control_delimiters_are_not_native_escaping_qualification=True)
    except BaseException as error:
        receipt.update(error_type=type(error).__name__,error=str(error));raise
    finally:
        receipt['elapsed_seconds']=time.monotonic()-started
        receipt['versions']={name:importlib.metadata.version(name)for name in ['torch','transformers','onnx','onnxruntime','numpy']}
        (output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8')
        print(json.dumps({'complete':receipt['complete'],'rows':len(receipt['rows']),'elapsed_seconds':receipt['elapsed_seconds'],'error':receipt.get('error')}),flush=True)


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Owned static Focus export; requires a fresh reviewed execution seal")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--cases", type=Path, required=True)
    args = parser.parse_args()
    run(args.output, source=args.source, case_path=args.cases)
