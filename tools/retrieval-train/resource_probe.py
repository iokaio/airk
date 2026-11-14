# SPDX-License-Identifier: Apache-2.0
"""Measure one complete model's bounded CPU prefill; no native-resource claim."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import time

from compile_reference import compile_case,FOLDERS,ROOT


def physical_affinity(count):
    import ctypes
    import os
    import psutil
    if os.name!='nt' or psutil.cpu_count()>64:raise ValueError('physical affinity requires verified single-group Windows topology')
    class Payload(ctypes.Union):
        _fields_=[('flags',ctypes.c_ubyte),('reserved',ctypes.c_ulonglong*2)]
    class Processor(ctypes.Structure):
        _fields_=[('mask',ctypes.c_size_t),('relationship',ctypes.c_int),('payload',Payload)]
    function=ctypes.windll.kernel32.GetLogicalProcessorInformation
    function.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_ulong)];function.restype=ctypes.c_bool
    length=ctypes.c_ulong();function(None,ctypes.byref(length))
    buffer=ctypes.create_string_buffer(length.value)
    if not function(buffer,ctypes.byref(length)) or length.value%ctypes.sizeof(Processor):raise ValueError('physical topology query failed')
    rows=ctypes.cast(buffer,ctypes.POINTER(Processor));masks=[int(rows[i].mask) for i in range(length.value//ctypes.sizeof(Processor)) if rows[i].relationship==0]
    allowed=set(psutil.Process().cpu_affinity());chosen=[]
    for mask in masks:
        available=[index for index in allowed if mask&(1<<index)]
        if available:chosen.append(min(available))
    if len(chosen)<count:raise ValueError('insufficient independently verified physical cores')
    selected=chosen[:count];psutil.Process().cpu_affinity(selected)
    if set(psutil.Process().cpu_affinity())!=set(selected):raise ValueError('physical affinity assignment differs')
    return {'method':'GetLogicalProcessorInformation physical-core masks; one logical processor per selected physical core','physical_core_masks':masks,'physical_cores_observed':len(masks),'selected_logical_processors':selected}


def main():
    import numpy as np
    import onnxruntime as ort
    import psutil
    from transformers import AutoTokenizer
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',choices=FOLDERS,required=True)
    parser.add_argument('--model',type=Path,required=True)
    parser.add_argument('--data-name',choices=('weights.data','model.onnx.data'),default='weights.data')
    parser.add_argument('--threads',type=int,default=1)
    parser.add_argument('--physical-cores',type=int,choices=(0,8),default=0)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts') or not 1<=args.threads<=16:parser.error('fresh private output and bounded threads required')
    if args.physical_cores and args.threads!=args.physical_cores:parser.error('explicit physical tier and thread count must match')
    affinity=physical_affinity(args.physical_cores) if args.physical_cores else None
    source=Path(__file__).read_bytes()
    def digest(path):
        with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
    paths=[Path(__file__),Path(__file__).with_name('compile_reference.py'),args.model,args.model.parent/args.data_name]
    if args.physical_cores:paths.append(Path(__file__).with_name('resource-cpu8-proposal.json'))
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
    options.add_session_config_entry('session.intra_op.allow_spinning','0')
    options.add_session_config_entry('session.inter_op.allow_spinning','0')
    def frequency():
        try:
            value=psutil.cpu_freq();return value._asdict() if value else None
        except (OSError,NotImplementedError):return None
    hardware={'os':platform.platform(),'architecture':platform.machine(),'processor':platform.processor(),'physical_cores':psutil.cpu_count(logical=False),'logical_processors':psutil.cpu_count(),'os_reported_clock_before':frequency()}
    process=psutil.Process();before=process.memory_info()._asdict();available=psutil.virtual_memory().available
    loading=time.monotonic();session=ort.InferenceSession(str(args.model),options,providers=['CPUExecutionProvider']);load_seconds=time.monotonic()-loading
    feed={v.name:np.asarray(case[v.name],dtype=np.bool_ if v.type=='tensor(bool)' else np.int64) for v in session.get_inputs()}
    start=time.monotonic();scores=session.run(['scores'],feed)[0];elapsed=time.monotonic()-start
    after=process.memory_info()._asdict()
    hardware['os_reported_clock_after']=frequency()
    if not np.isfinite(scores).all():raise ValueError('invalid full-model score')
    if any(path.stat().st_size!=pins[str(path.resolve())]['bytes'] or digest(path)!=pins[str(path.resolve())]['sha256'] for path in paths):raise ValueError('probe source/model drift')
    output.mkdir(parents=True)
    report={'schema':'airk.development-full-model-resource-probe/1','candidate':args.candidate,'model':str(args.model.resolve()),'threads':args.threads,
        'tokens':case['tokens'],'candidates':8,'load_seconds':load_seconds,'prefill_seconds':elapsed,'raw_scores':scores.tolist(),'physical_affinity':affinity,'hardware':hardware,
        'session_config':{'intra_threads':args.threads,'inter_threads':1,'sequential':True,'spinning':False,'cpu_arena':False,'memory_pattern':False},
        'memory_before':before,'memory_after':after,'system_available_before':available,'source_sha256':hashlib.sha256(source).hexdigest(),'input_seals':pins,
        'scope':'One developer complete-prefill observation, not p50/p95/p99, task quality or native qualification. Pre-run hashing warms the filesystem cache: no cold-load claim. Filepath loading can consume less peak memory than production verified-file-memory binding; actual native resource gate remains required.'}
    (output/'runner.py').write_bytes(source);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)


if __name__=='__main__':main()
