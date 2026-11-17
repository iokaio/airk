# SPDX-License-Identifier: Apache-2.0
"""Exact DEV-only candidate grading after retained/new same-index control proof."""
from pathlib import Path
import argparse
import contextlib
import copy
from fractions import Fraction
import hashlib
import json
import os
import sys
import threading
import time
import types
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
MODELS=ROOT/'artifacts/advanced-split/models'
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def load(path):return json.loads(Path(path).read_bytes())
def require(value,message):
    if not value:raise ValueError(message)
def captured(path,pin):
    raw=Path(path).read_bytes()
    require(hashlib.sha256(raw).hexdigest()==pin,'verified source bytes changed: '+str(path))
    return Path(path),raw
@contextlib.contextmanager
def verified_modules(rows):
    absent=object();saved={name:sys.modules.get(name,absent) for name,_,_ in rows};modules={}
    try:
        for name,path,_ in rows:
            module=types.ModuleType(name);module.__file__=str(path);module.__package__='';modules[name]=module;sys.modules[name]=module
        for name,path,raw in rows:
            exec(compile(raw,str(path),'exec'),modules[name].__dict__)
        yield modules
    finally:
        for name,value in saved.items():
            if value is absent:sys.modules.pop(name,None)
            else:sys.modules[name]=value
def bridge(old,new):
    for key in ('family','model_json_sha256','pipeline_digest','runtime','encoder_input_sha256','corpus_sha256','quality_mask_sha256','source_manifest_sha256','source_before_sha256','source_after_sha256','source_roster_count','query'):
        require(old[key]==new[key],'same-index control metadata differs: '+key)
    require(old['complete'] is True and new['complete'] is True,'incomplete control')
    require(len(old['records'])==len(new['records'])==64,'exact64 raw control records required')
    a={r['id']:r for r in old['records']};b={r['id']:r for r in new['records']}
    require(len(a)==len(b)==64 and a.keys()==b.keys(),'control ID population differs')
    for identifier in a:
        require(Path(a[identifier]['source_root']).samefile(b[identifier]['source_root']),'control scope differs')
        require(a[identifier]['primary_vector']==b[identifier]['primary_vector'],'full native primary_vector differs; all8 matched-image recapture required')
    require(isinstance(old.get('collections'),list) and len(old['collections'])==len(new.get('collections',[])) and old['collections'],'exact nonempty collection census required')
    for old_collection,new_collection in zip(old['collections'],new['collections']):
        require(old_collection.get('reused') is False and new_collection.get('reused') is True,'only observed previously published false-to-true reuse accounting is allowed')
    def normalized(value):
        result=copy.deepcopy(value);result.pop('binary_sha256')
        for record in result['records']:
            require(type(record.get('query_us')) is int and record['query_us']>=0,'missing/invalid native query timing')
            record['query_us']=0
        for collection in result['collections']:collection['reused']=False
        return result
    require(normalized(old)==normalized(new),'full control capture differs beyond explicit binary/query timing/reused accounting identities; all8 matched-image recapture required')
    return {'all64_full_primary_vector_equal':True,'complete_capture_equal_after_documented_accounting':True,'allowed_differences':['binary_sha256','records[].query_us','collections[].reused:false-to-true-only'],'same_publication_required':True}
def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('encoders','new-control','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--encoders-sha256',required=True);p.add_argument('--captures',type=Path,nargs=2,required=True)
    args=p.parse_args()
    require(all(os.environ.get(k)=='1' for k in ('OPENBLAS_NUM_THREADS','OMP_NUM_THREADS','MKL_NUM_THREADS')),'singlethread BLAS environment required before startup')
    helper=ROOT/'scripts/arbiter/export/minilm/support/blind-metadata-r5/audit.py'
    tools=ROOT/'tools/retrieval-train'
    sources={tools/'development_retrieval.py':'8ede0bd19ae3ff96ed534d662832eca32a8bde7408aea86051e542ce1cd34ad0',tools/'goal_sources.py':'603f4c64357dd4dfa1d51d1556c19ac17977d4b5ed0fcaa1d2aa44bca43274c7',tools/'goal_rubric.py':'3fae6cdf245d9fa0b18f1c4d63e29c135cdfbc873543f08a72f611222022544d',tools/'grade_native_retrieval.py':'34995d4431819e910a96b177625fe05c572a79072287ccc21322635fd0a1c0e1',helper:'a1687757532d766baac52f9c78547f30c1d3b340f9370cfe961d7817b237ad77'}
    rows=[]
    for name,path in [('_candidate_bounded',helper),('goal_rubric',tools/'goal_rubric.py'),('goal_sources',tools/'goal_sources.py'),('grade_native_retrieval',tools/'grade_native_retrieval.py'),('development_retrieval',tools/'development_retrieval.py')]:
        path,raw=captured(path,sources[path]);rows.append((name,path,raw))
    with verified_modules(rows) as modules:
        grade(args,modules,sources)
def grade(args,modules,sources):
    clock=modules['_candidate_bounded']
    clock.START=time.monotonic();clock.JOB_STATE=clock.enforce_budget()
    def expire():
        if not clock.done.wait(60):os._exit(124)
    threading.Thread(target=expire,daemon=True).start()
    owner=modules['development_retrieval'];FrozenSourceMetadata=modules['goal_sources'].FrozenSourceMetadata
    fixture=MODELS/'a1-component-dev-native-fixture-r1';manifest=fixture/'manifest.private.json';rubric=fixture/'rubric.private.json';mask=MODELS/'a1-component-dev-evaluation-protocol-r1/quality-mask.json'
    old_control=MODELS/'native-dev64-baseline-r10-r1/original-mixed-u8-ort130-evaluation/capture.json'
    old_grade=MODELS/'native-dev64-baseline-r10-r1/grading-r2/grading.private.json'
    required={manifest:'bf53b30e12a7aaa7b2bd4200ae892a8e2edefc2941dbb84c52483d62705dad2f',rubric:'587bb503da85119b9f0f7fbb544a0216d655ac07691a3c6319a2a9b9fc1e3b2a',mask:owner.MASK_SHA,old_control:'52f79109450dd509401d851df8efeea55dcea0a6a3bf963765653943d9cefc19',old_grade:'1ef850f8bd24836cd12541f9e0789b0ff3f4aaf5838da06f251e429edf4219f4',args.encoders:args.encoders_sha256}
    for path,pin in required.items():require(sha(path)==pin,'DEV input changed: '+str(path))
    inputs={**sources,**required,**{p:sha(p) for p in args.captures+[args.new_control,Path(__file__)]}}
    new_control=load(args.new_control);proof=bridge(load(old_control),new_control)
    encoders={r['family']:r for r in load(args.encoders)}
    require(set(encoders)=={'visible-prefix-mining-r1-u8','visible-prefix-mining-r1-s8'},'exact separate candidate two required')
    require(not args.output.exists() and args.output.resolve().is_relative_to(ROOT/'artifacts'),'fresh private output required')
    require(not args.output.resolve().is_relative_to(fixture.resolve()),'grading output inside read-only source fixture refused')
    metadata=load(manifest);owner.verify_fixture(fixture,metadata);cache=FrozenSourceMetadata(fixture,required[manifest]);tasks=load(rubric)['tasks'];frozen_mask=load(mask)
    results=[];seen=set()
    for path in args.captures:
        family=load(path);name=family['family'];require(name in encoders and name not in seen,'foreign/duplicate candidate');seen.add(name)
        for key in ('binary_sha256','runtime','corpus_sha256','quality_mask_sha256','source_manifest_sha256','source_roster_count','source_before_sha256','source_after_sha256','query'):
            require(family[key]==new_control[key],'candidate differs from matched native/DEV closure: '+key)
        require(family['model_json_sha256']==encoders[name]['model_json_sha256'] and family['encoder_input_sha256']==args.encoders_sha256 and family['development_protocol_sha256']==owner.PROTOCOL_SHA,'candidate model/descriptor/protocol differs')
        results.append(owner.grade_family(family,tasks,frozen_mask,fixture,required[manifest],cache))
    baselines=load(old_grade)['families'];comparisons=[]
    for candidate in results:
        for control in baselines:
            a=candidate['summary'];b=control['summary'];gain=Fraction(a['complete_count']-b['complete_count'],20);delta=Fraction(*a['mean_evidence_recall_fraction'])-Fraction(*b['mean_evidence_recall_fraction'])
            comparisons.append({'candidate':candidate['family'],'control':control['family'],'complete_gain_fraction':[gain.numerator,gain.denominator],'mean_delta_fraction':[delta.numerator,delta.denominator],'final_or_promotion_pass':False})
    owner.verify_fixture(fixture,metadata)
    for path,pin in inputs.items():require(sha(path)==pin,'source/input drift during grade')
    args.output.mkdir(parents=True)
    clock.atomic_stream(args.output/'grading.private.json',{'schema':'airk.private-candidate-DEV64-grading/1','development_only':True,'promotion_or_final_quality_pass':False,'bridge':proof,'families':results,'comparisons_to_all6':comparisons,'source_inputs':{str(k):v for k,v in inputs.items()}})
    receipt={'elapsed_seconds':time.monotonic()-clock.START,'peak_committed_process_bytes':clock.peak_memory(),'hard_seconds':60,'hard_bytes':268435456,'singlethread_BLAS':True,'no_model_or_native_execution':True}
    clock.atomic_stream(args.output/'operation-receipt.json',receipt);clock.done.set()
    print(json.dumps({'summaries':[dict(family=f['family'],**f['summary']) for f in results],'operation':receipt}),flush=True)
if __name__=='__main__':main()
