# SPDX-License-Identifier: Apache-2.0
"""Private single-U8 before/after DEV diagnostic using unchanged native coverage grader.

No same-index bridge or model-promotion gate is claimed for deliberately changed graphs.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import threading
import time
import types

ROOT=Path(__file__).resolve().parents[4]
MODELS=ROOT/'artifacts/advanced-split/models'
HELPER=MODELS/'visible-prefix-export-source-r1/grade_candidates.py'
HELPER_SHA='f12dffc952b2f53fa2316e2605a5591ac101531ef577202513577821e1091348'
HERE=Path(__file__).resolve().parent

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture',type=Path,required=True)
    parser.add_argument('--capture-sha256',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if any(os.environ.get(k)!='1' for k in ['OMP_NUM_THREADS','MKL_NUM_THREADS','OPENBLAS_NUM_THREADS']):
        raise ValueError('original single-thread grading environment required')
    helper_raw=HELPER.read_bytes()
    if hashlib.sha256(helper_raw).hexdigest()!=HELPER_SHA:raise ValueError('retained grader helper source changed')
    helper=types.ModuleType('_retained_candidate_grade_helpers');helper.__file__=str(HELPER)
    exec(compile(helper_raw,str(HELPER),'exec'),helper.__dict__)
    tools=ROOT/'tools/retrieval-train';clock_path=MODELS/'a1-next-experiment-blind-metadata-r5/audit.py'
    sources={tools/'development_retrieval.py':'8ede0bd19ae3ff96ed534d662832eca32a8bde7408aea86051e542ce1cd34ad0',
        tools/'goal_sources.py':'603f4c64357dd4dfa1d51d1556c19ac17977d4b5ed0fcaa1d2aa44bca43274c7',
        tools/'goal_rubric.py':'3fae6cdf245d9fa0b18f1c4d63e29c135cdfbc873543f08a72f611222022544d',
        tools/'grade_native_retrieval.py':'34995d4431819e910a96b177625fe05c572a79072287ccc21322635fd0a1c0e1',
        clock_path:'9c540c08c2e56d868059ab8425193e52f0d9141f75d16df01f622c679032425b'}
    rows=[]
    for name,path in [('_diagnostic_bounded',clock_path),('goal_rubric',tools/'goal_rubric.py'),
        ('goal_sources',tools/'goal_sources.py'),('grade_native_retrieval',tools/'grade_native_retrieval.py'),
        ('development_retrieval',tools/'development_retrieval.py')]:
        path,raw=helper.captured(path,sources[path]);rows.append((name,path,raw))
    fixture=MODELS/'a1-component-dev-native-fixture-r1';manifest=fixture/'manifest.private.json'
    rubric=fixture/'rubric.private.json';mask=MODELS/'a1-component-dev-evaluation-protocol-r1/quality-mask.json'
    original=MODELS/'candidate-native-r20-r1/visible-prefix-mining-r1-u8/capture.json'
    inputs={**sources,HELPER:HELPER_SHA,manifest:'bf53b30e12a7aaa7b2bd4200ae892a8e2edefc2941dbb84c52483d62705dad2f',
        rubric:'587bb503da85119b9f0f7fbb544a0216d655ac07691a3c6319a2a9b9fc1e3b2a',
        mask:'76842592dab174fd90bc3918542759adbf9cea615080334a308acafd6924727d',
        original:'9aec2ccc46b1d278db426da62183078702496879dd826442f5b3fb8f999caeb9',
        args.capture:args.capture_sha256,Path(__file__):helper.sha(Path(__file__))}
    for path,pin in inputs.items():helper.require(helper.sha(path)==pin,'frozen grading input changed')
    output=args.output.resolve()
    helper.require(not output.exists() and output.is_relative_to(MODELS),'fresh private grading output required')
    for frozen in [fixture,HERE,args.capture.resolve().parent,original.parent]:
        frozen=frozen.resolve();helper.require(not output.is_relative_to(frozen) and not frozen.is_relative_to(output),'grading output overlaps frozen input tree')
    with helper.verified_modules(rows) as modules:
        owner=modules['development_retrieval'];clock=modules['_diagnostic_bounded']
        clock.START=time.monotonic();clock.JOB_STATE=clock.enforce_budget()
        def expire():
            if not clock.done.wait(60):os._exit(124)
        threading.Thread(target=expire,daemon=True).start()
        old=helper.load(original);new=helper.load(args.capture)
        helper.require(new['complete'] is True and new['query_only'] is True and new['indexing_performed'] is False,'complete query-only native capture required')
        for key in ['family','runtime','model_json_sha256','pipeline_digest','encoder_input_sha256','corpus_sha256',
            'quality_mask_sha256','source_manifest_sha256','source_before_sha256','source_after_sha256','source_roster_count','query']:
            helper.require(new[key]==old[key],'original non-graph DEV inputs changed: '+key)
        helper.require([r['id'] for r in new['records']]==[r['id'] for r in old['records']],'same ALL64 original order required')
        metadata=helper.load(manifest);owner.verify_fixture(fixture,metadata)
        cache=modules['goal_sources'].FrozenSourceMetadata(fixture,inputs[manifest])
        tasks=helper.load(rubric)['tasks'];frozen_mask=helper.load(mask)
        before=owner.grade_family(old,tasks,frozen_mask,fixture,inputs[manifest],cache)
        after=owner.grade_family(new,tasks,frozen_mask,fixture,inputs[manifest],cache)
        owner.verify_fixture(fixture,metadata)
        for path,pin in inputs.items():helper.require(helper.sha(path)==pin,'grading input drift')
        output.mkdir()
        clock.atomic_stream(output/'grading.private.json',{'schema':'airk.private-regenerated-graph-DEV-diagnostic/1',
            'before':before,'after':after,'source_inputs':{str(p):s for p,s in inputs.items()},
            'same_index_bridge_claimed':False,'all_model_baseline_control_claimed':False,
            'promotion_or_final_quality_pass':False,'scope':'Known graph correction, same U8 weights/all64DEV requests; independent original20-quality/44refusal mask; no FINAL.'})
        receipt={'elapsed_seconds':time.monotonic()-clock.START,'peak_committed_process_bytes':clock.peak_memory(),
            'hard_seconds':60,'hard_bytes':268435456,'singlethread_BLAS':True,'no_model_or_native_execution':True}
        clock.atomic_stream(output/'operation-receipt.json',receipt);clock.done.set()
        print(json.dumps({'before':before['summary'],'after':after['summary'],'operation':receipt}))

if __name__=='__main__':main()
