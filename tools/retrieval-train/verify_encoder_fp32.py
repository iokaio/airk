# SPDX-License-Identifier: Apache-2.0
"""Check all masked shape-bucket FP32 oracles against unexported eager BERT."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]


def main():
    import numpy as np
    from safetensors.torch import load
    import torch
    from transformers import BertConfig,BertModel
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint',type=Path,required=True);parser.add_argument('--exports',type=Path,required=True)
    parser.add_argument('--oracle',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    paths=(Path(__file__),args.checkpoint/'model.safetensors',args.checkpoint/'config.json',args.exports/'receipt.json',args.oracle)
    retained={path:path.read_bytes() for path in paths};sha=lambda raw:hashlib.sha256(raw).hexdigest()
    exported=json.loads(retained[args.exports/'receipt.json']);oracle=json.loads(retained[args.oracle])
    if sha(retained[args.checkpoint/'model.safetensors'])!=exported['source_model_sha256'] or sha(retained[args.exports/'receipt.json'])!=oracle['export_receipt_sha256']:raise ValueError('checkpoint/export/oracle relationship differs')
    matches=[value for name,value in exported['input_seals'].items() if name.replace('\\','/').endswith('/config.json')]
    if len(matches)!=1 or sha(retained[args.checkpoint/'config.json'])!=matches[0]:raise ValueError('source config differs from export input')
    config=BertConfig.from_dict(json.loads(retained[args.checkpoint/'config.json']));config._attn_implementation='eager'
    model=BertModel(config,add_pooling_layer=False);state=load(retained[args.checkpoint/'model.safetensors'])
    excluded=sorted(set(state)-set(model.state_dict()))
    if not set(excluded)<={'pooler.dense.weight','pooler.dense.bias','embeddings.position_ids'}:raise ValueError('unexpected unexported encoder tensor exclusion')
    model.load_state_dict({key:value for key,value in state.items() if key not in excluded},strict=True);model.eval();torch.set_num_threads(1)
    results=[]
    with torch.inference_mode():
        for case in oracle['cases']:
            feed={name:torch.tensor(value,dtype=torch.long) for name,value in case['inputs'].items()}
            hidden=model(**feed).last_hidden_state.numpy();mask=np.asarray(case['inputs']['attention_mask'],dtype=np.int64)
            vector=((hidden*mask[:,:,None]).sum(1)/mask.sum(1,keepdims=True))[0];vector/=np.linalg.norm(vector)
            reference=np.asarray(case['reference_vector']);error=float(np.max(np.abs(vector-reference)))
            if not np.isfinite(vector).all() or error>1e-5:raise ValueError('unexported eager encoder differs from FP32 oracle at '+case['name']+': '+str(error))
            results.append({'name':case['name'],'bucket':case['bucket'],'eager_vector':vector.tolist(),'fp32_onnx_max_component_error':error,
                'quantized_diagnostics':{name:{'cosine_vs_eager':float(vector@np.asarray(value['vector'])),'l2_vs_eager':float(np.linalg.norm(vector-np.asarray(value['vector'])))} for name,value in case['results'].items()}})
    if any(path.read_bytes()!=raw for path,raw in retained.items()):raise ValueError('independent encoder reference source/input drift')
    output.mkdir(parents=True)
    report={'schema':'airk.development-unexported-encoder-reference/1','excluded_unused_publisher_tensors':excluded,'source_implementation':'Official pinned Transformers eager BertModel, separate from ONNX exported graph; CPU FP32, mask-aware mean/L2','input_seals':{str(path):sha(raw) for path,raw in retained.items()},'cases':results,'maximum_fp32_onnx_component_error':max(row['fp32_onnx_max_component_error'] for row in results),'scope':'Independent FP32 export/masked-shape correctness only. Original oracle files/case values unchanged; no human/native/platform quality claim.'}
    (output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n');(output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'cases':len(results),'buckets':sorted({case['bucket'] for case in results}),'maximum_fp32_onnx_component_error':report['maximum_fp32_onnx_component_error']}))


if __name__=='__main__':main()
