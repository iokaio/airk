# SPDX-License-Identifier: Apache-2.0
"""Measure one complete model's bounded CPU prefill; no native-resource claim."""
import argparse
import hashlib
import json
from pathlib import Path
import time

from compile_reference import compile_case,FOLDERS,ROOT


def main():
    import numpy as np
    import onnxruntime as ort
    import psutil
    from transformers import AutoTokenizer
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',choices=FOLDERS,required=True)
    parser.add_argument('--model',type=Path,required=True)
    parser.add_argument('--threads',type=int,default=1)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts') or not 1<=args.threads<=16:parser.error('fresh private output and bounded threads required')
    source=Path(__file__).read_bytes()
    def digest(path):
        with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
    paths=[Path(__file__),Path(__file__).with_name('compile_reference.py'),args.model,args.model.parent/'weights.data']
    pins={str(path.resolve()):{'bytes':path.stat().st_size,'sha256':digest(path)} for path in paths}
    tokenizer=AutoTokenizer.from_pretrained(ROOT/'artifacts/advanced-split/models/candidate-assets'/FOLDERS[args.candidate],local_files_only=True,trust_remote_code=False)
    candidates=[{'id':'record-'+str(i),'text':'Verified original source record '+str(i)+'.'} for i in range(8)]
    question='Which permitted record contributes evidence to this information need?'
    phrase='A bounded permitted read preserves the original document location and indexed revision. '
    lo,hi=0,512;case=None
    while lo<=hi:
        middle=(lo+hi)//2
        try:observed=compile_case(tokenizer,args.candidate,phrase*middle,question,candidates)
        except ValueError as error:
            if 'context limit' not in str(error):raise
            hi=middle-1
        else:case=observed;lo=middle+1
    if case is None:raise ValueError('no admitted prefill')
    options=ort.SessionOptions();options.intra_op_num_threads=args.threads;options.inter_op_num_threads=1
    options.execution_mode=ort.ExecutionMode.ORT_SEQUENTIAL;options.enable_mem_pattern=False;options.enable_cpu_mem_arena=False
    options.graph_optimization_level=ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    process=psutil.Process();before=process.memory_info()._asdict();available=psutil.virtual_memory().available
    loading=time.monotonic();session=ort.InferenceSession(str(args.model),options,providers=['CPUExecutionProvider']);load_seconds=time.monotonic()-loading
    feed={v.name:np.asarray(case[v.name],dtype=np.bool_ if v.type=='tensor(bool)' else np.int64) for v in session.get_inputs()}
    start=time.monotonic();scores=session.run(['scores'],feed)[0];elapsed=time.monotonic()-start
    after=process.memory_info()._asdict()
    if not np.isfinite(scores).all():raise ValueError('invalid full-model score')
    if any(path.stat().st_size!=pins[str(path.resolve())]['bytes'] or digest(path)!=pins[str(path.resolve())]['sha256'] for path in paths):raise ValueError('probe source/model drift')
    output.mkdir(parents=True)
    report={'schema':'airk.development-full-model-resource-probe/1','candidate':args.candidate,'model':str(args.model.resolve()),'threads':args.threads,
        'tokens':case['tokens'],'candidates':8,'load_seconds':load_seconds,'prefill_seconds':elapsed,'raw_scores':scores.tolist(),
        'memory_before':before,'memory_after':after,'system_available_before':available,'source_sha256':hashlib.sha256(source).hexdigest(),'input_seals':pins,
        'scope':'One developer complete-prefill observation, not p50/p95/p99, task quality or native qualification. Pre-run hashing warms the filesystem cache: no cold-load claim. Filepath loading can consume less peak memory than production verified-file-memory binding; actual native resource gate remains required.'}
    (output/'runner.py').write_bytes(source);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)


if __name__=='__main__':main()
