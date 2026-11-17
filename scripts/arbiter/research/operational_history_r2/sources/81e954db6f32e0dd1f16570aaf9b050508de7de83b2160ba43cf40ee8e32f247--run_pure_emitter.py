# SPDX-License-Identifier: Apache-2.0
"""Root-only execution after review. Reuses the existing owned command primitive.

This script never launches a live service, CLI read, model load or forward.
Synthetic output is forbidden on R97, whose source lacks that selector.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time

ROOT=Path(__file__).resolve().parents[3]

def digest(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
    return h.hexdigest()

def run(bindings,mode):
    began=time.monotonic()
    packet=json.loads(Path(bindings).read_text(encoding='utf-8'))
    if packet['schema']!='airk.private.composed-current-bindings/1':raise ValueError('wrong original binding schema')
    runtime=packet.get('service_with_synthetic') or (packet['service_r97'] if mode=='vector' else None)
    if not runtime:raise ValueError('Actual synthetic-bearing compiled service image absent; R97 selector is absent')
    if mode=='vector' and not packet['selected_minilm']:
        raise ValueError('actual selected MiniLM gate/identity missing')
    image=runtime['image']
    if digest(image['path'])!=image['sha256']:raise ValueError('actual service image changed')
    paths=packet['future_paths'];anchor=Path(paths['private_anchor'])
    if not (anchor/'protection.private.json').is_file():raise ValueError('actual protected parent proof missing')
    environment=dict(os.environ)
    if mode=='vector':
        if runtime.get('vector_selector_present') is not True or not packet['selected_minilm']:
            raise ValueError('actual selected MiniLM gate/identity missing')
        selected=packet['selected_minilm']
        definition=Path(selected['directory'])/'model.json'
        if digest(definition)!=selected['model_definition_sha256']:raise ValueError('actual selected MiniLM definition changed')
        selector='emit_private_selected_vector_configuration'
        environment.update(AIRK_SELECTED_MINILM_DIRECTORY=selected['directory'],AIRK_SELECTED_VECTOR_DRAFT_OUTPUT=paths['vector_drafts'])
        output=Path(paths['vector_drafts'])
    else:
        if runtime.get('synthetic_selector_present') is not True:raise ValueError('synthetic selector not authenticated in actual image')
        selector='emit_private_selected_synthetic_fixture';output=Path(paths[mode])
        environment['AIRK_SELECTED_SYNTHETIC_OUTPUT']=str(output)
    if output.exists():raise ValueError('fresh pure emitter output required; no replay')
    spec=importlib.util.spec_from_file_location('airk_existing_owned',ROOT/'scripts/provision-static-onnx-runtime.py')
    owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
    argv=[image['path'],'--ignored','--exact','service_runtime::evaluation_fixture_tests::'+selector,'--nocapture']
    remaining=60-(time.monotonic()-began)
    if remaining<=0:raise TimeoutError('original whole pure-emitter phase deadline exhausted before launch')
    result=owner.run_owned(argv,ROOT,anchor/'logs'/f'{mode}-pure-emitter.log',remaining,environment)
    receipt={'schema':'airk.private.composed-pure-emitter-run/1','mode':mode,'bindings_sha256':digest(bindings),
             'runtime_image':image,'argv':argv,'owned':result,'elapsed_seconds':time.monotonic()-began,
             'state':'command_settled_pending_postchecks',
             'scope':'Pure native emitter only; no service/CLI/model session/NN, no composed qualification.'}
    (anchor/'logs'/f'{mode}-pure-emitter.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    if result['exit_code']!=0 or result['owned_descendants_settled'] is not True or result['owned_job_forced_termination'] is not False or time.monotonic()-began>=60:
        raise ValueError('pure native emitter failed/unsettled/deadline; preserve original outputs')
    log=(anchor/'logs'/f'{mode}-pure-emitter.log').read_text(encoding='utf-8')
    if '1 passed;' not in log:raise ValueError('exact one native ignored test pass not observed')
    if digest(image['path'])!=image['sha256']:raise ValueError('actual service image changed after emitter')
    if time.monotonic()-began>=60:raise TimeoutError('whole pure-emitter phase deadline exhausted after pins')
    receipt.update(state='pure_emitter_passed_not_qualified',elapsed_seconds=time.monotonic()-began)
    (anchor/'logs'/f'{mode}-pure-emitter.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(receipt))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--bindings',type=Path,required=True)
    parser.add_argument('--mode',choices=['vector','included','excluded','fault'],required=True)
    args=parser.parse_args();run(args.bindings,args.mode)
