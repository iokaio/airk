# SPDX-License-Identifier: Apache-2.0
"""Compare exported compiled-prompt scores to the unmodified official text forward."""
import argparse
import hashlib
import json
from pathlib import Path
import time
import zipfile

import qwen_export


def main():
    import numpy as np
    import torch
    import transformers.models.qwen3_5.modeling_qwen3_5 as reference
    from safetensors.torch import save_file
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',choices=('intern08','intern4','strands2','kev4'),required=True)
    parser.add_argument('--assets',type=Path,default=qwen_export.ROOT/'artifacts/advanced-split/models/candidate-assets')
    parser.add_argument('--fixtures',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(qwen_export.ROOT/'artifacts'):parser.error('fresh private output required')
    source=Path(__file__).read_bytes();loader_source=Path(qwen_export.__file__).read_bytes();fixture_raw=args.fixtures.read_bytes()
    official_path=Path(reference.__file__);official_source=official_path.read_bytes()
    wheel=qwen_export.ROOT/'artifacts/advanced-split/models/training-env/wheels/transformers-5.10.1-py3-none-any.whl'
    with zipfile.ZipFile(wheel) as archive:
        if archive.read('transformers/models/qwen3_5/modeling_qwen3_5.py')!=official_source:raise ValueError('official implementation differs from reviewed wheel')
    torch.set_num_threads(2);torch.backends.cuda.matmul.allow_tf32=False;torch.backends.cudnn.allow_tf32=False
    started=time.monotonic();config,state,head,kind,seals=qwen_export.load_candidate(args.assets,args.candidate)
    output.mkdir(parents=True)
    if kind=='kev':
        save_file({k:v.contiguous() for k,v in head.items()},str(output/'complete-original-head.safetensors'),metadata={'scope':'All original tensor keys preserved from restricted weights_only loader; original temperature not applied; own AIRK calibration required'})
    model=qwen_export.make_wrapper(config,state,head,kind).cuda().eval();del state
    # Restore the actual official chunk/recurrent implementation, not our ONNX
    # lowering. Run the official complete forward including its own causal mask.
    for layer in model.text.layers:
        if hasattr(layer,'linear_attn'):
            layer.linear_attn.chunk_gated_delta_rule=reference.torch_chunk_gated_delta_rule
            layer.linear_attn.recurrent_gated_delta_rule=reference.torch_recurrent_gated_delta_rule
    reports=[]
    for case in json.loads(fixture_raw)['cases']:
        inputs={name:torch.tensor(case[name],device='cuda') for name in ('input_ids','attention_mask','position_ids')}
        with torch.inference_mode():hidden=model.text(**inputs,use_cache=False).last_hidden_state
        values=hidden[0].float().cpu().numpy().astype(np.float64)
        decision=values[case['decision_positions'][0]]
        indices=case['candidate_positions'][0]
        if kind=='intern':
            weights=model.lm_head.weight.detach().cpu().numpy()
            scores=weights[indices].astype(np.float64) @ decision
        else:
            options=values[indices]
            if kind=='strands':
                weight=head['norm.weight'].numpy().astype(np.float64);bias=head['norm.bias'].numpy().astype(np.float64)
                norm=lambda x:((x-x.mean(-1,keepdims=True))/np.sqrt(x.var(-1,keepdims=True)+1e-5))*weight+bias
                decision=norm(decision);options=norm(options)
            query=head['q.weight'].numpy().astype(np.float64) @ decision+head['q.bias'].numpy()
            keys=options @ head['k.weight'].numpy().astype(np.float64).T+head['k.bias'].numpy()
            scores=keys @ query / np.sqrt(query.shape[-1])
        expected=np.asarray(case['raw_scores'])[0]
        error=float(np.max(np.abs(scores-expected)))
        reports.append({'name':case['name'],'tokens':case['tokens'],'official_raw_scores':scores.tolist(),'onnx_raw_scores':expected.tolist(),
            'maximum_absolute_error':error,'argmax_matches':int(np.argmax(scores))==int(np.argmax(expected))})
        if error>1e-3 or not np.isfinite(scores).all():raise ValueError('official full scoring parity failed: '+str(reports[-1]))
    if Path(__file__).read_bytes()!=source or Path(qwen_export.__file__).read_bytes()!=loader_source or args.fixtures.read_bytes()!=fixture_raw or official_path.read_bytes()!=official_source:
        raise ValueError('reference input/source drift')
    if any(qwen_export.digest(path)!=sha for path,sha in seals.items()):raise ValueError('candidate source drift')
    report={'schema':'airk.development-official-scoring-parity/1','candidate':args.candidate,'cases':reports,'source_sha256':hashlib.sha256(source).hexdigest(),
        'loader_sha256':hashlib.sha256(loader_source).hexdigest(),'official_transformers_source_sha256':hashlib.sha256(official_source).hexdigest(),
        'fixtures_sha256':hashlib.sha256(fixture_raw).hexdigest(),'input_seals':seals,'elapsed_seconds':time.monotonic()-started,
        'scope':'Full unmodified official Qwen text forward plus independently evaluated original complete readout algebra, compared with compiled-prompt CPU ONNX scores; developer smoke cases only, not human quality evaluation'}
    (output/'runner.py').write_bytes(source);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'candidate':args.candidate,'selected_cases':len(reports),'max_error':max(r['maximum_absolute_error'] for r in reports),'elapsed_seconds':report['elapsed_seconds']}),flush=True)


if __name__=='__main__':main()
