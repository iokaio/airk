# SPDX-License-Identifier: Apache-2.0
"""One bounded actual R20 native candidate stage; no retries or final access."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
MODELS=ROOT/'artifacts/advanced-split/models'
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('stage',choices=['numeric','bridge','visible-prefix-mining-r1-u8','visible-prefix-mining-r1-s8']);args=p.parse_args()
    binary=ROOT/'artifacts/advanced-split/native-harnesses/context-r20/airk-kernel.exe'
    candidate=MODELS/'candidate-native-emission-r20/master.json';closed=MODELS/'native-encoder-qualification-input-r1.json';owner=ROOT/'tools/retrieval-train/training_execution.py'
    pins={binary:'0f6ddd264fa8c0ed41c6c2af545b86cf0cf138fb7fc583ccb964784837ed2fd2',candidate:'0b58b97ee107c9c2c3e4adfefa7b6bf215a9c2d59a1966167b81e3f88224f718',closed:'3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81',owner:'d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'}
    descriptor=closed if args.stage=='bridge' else candidate
    rows=json.loads(descriptor.read_bytes());family='original-mixed-u8-ort130-evaluation' if args.stage=='bridge' else args.stage
    selected=rows if args.stage=='numeric' else [r for r in rows if r['family']==family]
    if len(selected)!=(2 if args.stage=='numeric' else 1):raise ValueError('exact reviewed native stage family absent')
    for row in selected:
        directory=Path(row['directory']);manifest=directory/'model.json';pins[manifest]=row['model_json_sha256'];pins[Path(row['oracle'])]=row['oracle_sha256'];metadata=json.loads(manifest.read_bytes())
        for name,key in [('model.onnx','model_sha256'),('tokenizer.json','tokenizer_sha256'),('LICENSE','license_sha256')]:pins[directory/name]=metadata[key]
    corpus=MODELS/'a1-component-development-materialized-r1/corpus.private.json';mask=MODELS/'a1-component-dev-evaluation-protocol-r1/quality-mask.json';manifest=MODELS/'a1-component-dev-native-fixture-r1/manifest.private.json'
    pins.update({corpus:'f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097',mask:'76842592dab174fd90bc3918542759adbf9cea615080334a308acafd6924727d',manifest:'bf53b30e12a7aaa7b2bd4200ae892a8e2edefc2941dbb84c52483d62705dad2f'})
    for path,pin in pins.items():
        if sha(path)!=pin:raise ValueError('native reviewed input changed: '+str(path))
    work=MODELS/'candidate-native-r20-r1'/args.stage;work.mkdir(parents=True,exist_ok=False)
    output=work/'capture.json';environment=dict(os.environ);environment.update({'AIRK_NATIVE_ENCODER_INPUT':str(descriptor),'AIRK_CANDIDATE_ENCODER_INPUT_SHA256':pins[candidate],'OMP_NUM_THREADS':'2','MKL_NUM_THREADS':'2','OPENBLAS_NUM_THREADS':'2'})
    if args.stage=='numeric':
        selector='inference_owner::evaluation_tests::candidate::candidate_two_families_match_independent_native_bucket_oracles';environment['AIRK_NATIVE_ENCODER_OUTPUT']=str(output)
    else:
        selector='inference_owner::evaluation_tests::'+('development::native_dev64_questions_use_current_kernel_with_fixed20_quality_mask' if args.stage=='bridge' else 'candidate::candidate_dev64_questions_use_current_kernel_with_fixed20_quality_mask')
        data=MODELS/'native-dev64-baseline-r10-r1/original-mixed-u8-ort130-evaluation/data' if args.stage=='bridge' else work/'data'
        environment.update({'AIRK_NATIVE_ENCODER_FAMILY':family,'AIRK_NATIVE_DEV64_CORPUS':str(corpus),'AIRK_NATIVE_DEV64_MASK':str(mask),'AIRK_NATIVE_DEV64_SOURCE_MANIFEST':str(manifest),'AIRK_NATIVE_DEV64_SOURCE_MANIFEST_SHA256':pins[manifest],'AIRK_NATIVE_DEV64_DATA':str(data),'AIRK_NATIVE_DEV64_OUTPUT':str(output),'AIRK_NATIVE_DEV64_STOP_FILE':str(work/'STOP')})
    argv=[str(binary),'--exact',selector,'--ignored','--nocapture','--test-threads=1'];request={'argv':argv,'pins':{str(k):v for k,v in pins.items()},'native_environment':{k:v for k,v in environment.items() if k.startswith('AIRK_') or k.endswith('NUM_THREADS')},'limits':{'seconds':1800,'host_bytes':8*1024**3,'max_job_processes':2},'timings':'Concurrent compiler/native work; not latency qualification'}
    (work/'request.json').write_text(json.dumps(request,indent=2)+'\n');(work/'stdin.txt').write_bytes(b'')
    spec=importlib.util.spec_from_file_location('owned',owner);owned=importlib.util.module_from_spec(spec);spec.loader.exec_module(owned);started=time.monotonic()
    with (work/'stdout.log').open('wb') as log,(work/'stdin.txt').open('rb') as stdin:result=owned.run_windows_owned_training(argv,ROOT,log,1800,environment,stdin,8*1024**3)
    drift=[str(path) for path,pin in pins.items() if sha(path)!=pin];receipt={'request':request,'owned':result,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift};(work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if result['exit_code'] or result['timed_out'] or result['owned_job_forced_termination'] or not result['owned_descendants_settled'] or drift:raise ValueError('native stage failed; preserve all evidence and stop')
    capture=json.loads(output.read_bytes())
    if capture.get('complete') is not True:raise ValueError('native output incomplete')
    if args.stage=='numeric':
        if len(capture['families'])!=2 or any(len(r['cases'])!=12 for r in capture['families']):raise ValueError('missing exact native numerical population')
    elif len(capture['records'])!=64:raise ValueError('missing original64 DEV records')
    print(json.dumps({'stage':args.stage,'capture_sha256':sha(output),'elapsed_seconds':receipt['elapsed_seconds'],'owned_natural_close':True,'post_input_drift':drift}),flush=True)
if __name__=='__main__':main()
