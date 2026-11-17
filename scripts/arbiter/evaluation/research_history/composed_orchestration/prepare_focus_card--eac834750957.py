# SPDX-License-Identifier: Apache-2.0
"""Prepare only: freeze actual fresh runtime inputs; never launch the lifecycle."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys

BASE=Path(__file__).resolve().parent
ORCHESTRATION=BASE.parent
RECIPE=ORCHESTRATION/'public-focus-recipe-r5'
RECIPE_SHA='e62a129f1e87c02abf916ca8077376837abce6d582d6441e0a1bb91df1622324'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
SERVICE_SHA='6c7d1003d327578b997f60c0c857b899bbeefc861dd86fc0f1ca6b192e47c87e'
FOCUS='04821edaed45ac10aa5b0b7b2c0b7b28bf5d04a06c8b623cfe6d762f77d5e23b'
USER_ROOT=Path(r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z')
PYTHON=Path(r'C:\Users\tdotn\AppData\Local\Programs\Python\Python313\python.exe')

def require(value,message):
    if not value:raise ValueError(message)

def sha(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def successful_build(value,native_group,runtime):
    require(value.get('schema')=='airk-readiness-command/1' and value.get('status')=='completed'
        and value.get('passed') is True and value.get('exit_code')==0 and value.get('runner_exit_code')==0,
        'genuine completed passing runtime build receipt required')
    require(all(value.get(key) is True for key in ('inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged'))
        and value.get('changed_groups')==[] and value['groups']['native']['sha256']==native_group,
        'actual runtime consumed-source identity differs')
    command=value['command']
    require(not any(word=='--target' or word.startswith('--target=') for word in command),'cross-target command is outside this Windows card')
    indices=[i for i,word in enumerate(command) if Path(word).name=='build-service-runtime.py']
    require(len(indices)==1,'actual native runtime builder command required')
    args=command[indices[0]+1:]
    require('--destination' in args and Path(args[args.index('--destination')+1]).resolve()==runtime.resolve(),
        'runtime destination differs from actual build command')
    require('--profile' in args and args[args.index('--profile')+1]=='debug','reviewed debug matched pair required')

def child_argv(repository,runtime,service,source,fixture,focus,output):
    return [str(PYTHON),'-B',str(RECIPE/'run_public_focus_r5.py'),'--repository',str(repository),
        '--runtime-dir',str(runtime),'--service-test-exe',str(service),'--service-test-sha256',SERVICE_SHA,
        '--source-sha256',source,'--fixture',str(fixture),'--focus-assets',str(focus),'--output',str(output),
        '--whole-phase-seconds','1800','--consent-focus-native-journey','--preparation-sha256',RECIPE_SHA]

def prepare(args):
    repository=args.repository.resolve(strict=True);runtime=args.runtime_dir.resolve(strict=True)
    receipt_path=args.runtime_build_receipt.resolve(strict=True)
    require(sha(receipt_path)==args.runtime_build_receipt_sha256,'actual runtime build receipt pin changed')
    receipt=json.loads(receipt_path.read_text(encoding='utf-8'))
    successful_build(receipt,args.expected_native_group,runtime)
    prep=RECIPE/'public-focus-preparation-r5.json';require(sha(prep)==RECIPE_SHA,'frozen R5 preparation changed')
    inputs=json.loads(prep.read_text(encoding='utf-8'))
    for row in inputs['files']+inputs['helper_source_pins']+inputs['maintained_owner_pins']:
        require(sha(row['path'])==row['sha256'],'frozen recipe input changed: '+row['path'])
    clients=load('frozen_focus_card_clients',RECIPE/'public-focus-r5-helper-source/scripts/check-service-clients.py')
    require(clients.original_split_pair_payload(runtime)==runtime,'genuine original pair metadata required')
    build=json.loads((runtime/'airk-build.json').read_text(encoding='utf-8'))
    require(build['source']['sha256']==args.expected_build_source_sha256,'actual full build-source SHA differs')
    require(build['runtime']['version']=='1.8.207' and build['runtime']['os']=='windows'
        and build['runtime']['arch']=='x86_64','reviewed Windows/x64 native version differs')
    # Source group and RuntimeBuild full-source SHA are deliberately distinct facts.
    native=build['runtime'];manifest=json.loads((runtime/'airk-install.json').read_text(encoding='utf-8'))
    producer_inventory=ORCHESTRATION/'b2-native-preservation-r1/native-test-images-b2-r1.json'
    require(sha(producer_inventory)=='3de867c6fda0973f7327c8dfb053c99beb0175e14dc5833ee533c12b5c3e405d','B2 producer inventory changed')
    producer=json.loads(producer_inventory.read_text(encoding='utf-8'))
    rows=[row for row in producer['images'] if row['binary_id']=='airk-service'];require(len(rows)==1,'exact B2 producer required')
    service=Path(rows[0]['path']);require(sha(service)==SERVICE_SHA==rows[0]['sha256'],'B2 typed producer image changed')
    fixture=USER_ROOT/'lifecycle/public-focus-native-r5'
    child_output=ORCHESTRATION.parent/'public-focus-native-r5'
    outer_output=ORCHESTRATION.parent/'public-focus-owned-runtime-r5'
    card_output=args.output.resolve()
    require(card_output.is_relative_to(repository/'artifacts') and card_output.parent.is_dir(),'approved protected fresh card parent required')
    require(all(not p.exists() for p in (card_output,fixture,child_output,outer_output)),'fresh card/journey/fixture output required')
    original_copy=repository/'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-acquired-assets-20261116T071657Z'
    focus=original_copy/FOCUS;copy_receipt=original_copy/'copy-receipt.json'
    copy=json.loads(copy_receipt.read_text(encoding='utf-8'))
    pins={}
    def pin(path,expected=None):
        path=Path(path).resolve(strict=True);actual=sha(path)
        require(expected is None or actual==expected,'actual input bytes differ: '+str(path))
        if str(path) in pins:require(pins[str(path)]==actual,'duplicate input mismatch')
        pins[str(path)]=actual
    for row in inputs['files']+inputs['helper_source_pins']+inputs['maintained_owner_pins']:pin(row['path'],row['sha256'])
    for path in (prep,receipt_path,producer_inventory,service,copy_receipt,Path(__file__),PYTHON,PYTHON.with_name('python313.dll')):pin(path)
    for name,digest in manifest['files'].items():pin(runtime/name,digest)
    pin(runtime/'airk-install.json')
    focus_rows=[row for row in copy['files'] if row['model']=='arbiter-focus-qwen3-reranker-06b']
    require(len(focus_rows)==9,'original nine Focus assets required')
    for row in focus_rows:pin(row['destination'],row['destination_reopened_sha256'])
    card={'schema':'airk.private-public-focus-outer-card/1','repository':str(repository),
        'python':{'path':str(PYTHON),'sha256':sha(PYTHON)},'owner_source':str(RECIPE/'frozen-owner-source/scripts/provision-static-onnx-runtime.py'),
        'owned_entrypoint':'run_owned','whole_phase_seconds':1800,'native_read_ceiling_seconds':120,
        'cleanup_observation_seconds':5,'owner_log_maximum_bytes':67108864,
        'native_identity':{'airk_sha256':native['airk_sha256'],'airkctl_sha256':native['airkctl_sha256'],
            'source_sha256':build['source']['sha256'],'native_source_group':args.expected_native_group,'service_test_sha256':SERVICE_SHA},
        'outer_output':str(outer_output),'child_output':str(child_output),'fixture':str(fixture),
        'argv':child_argv(repository,runtime,service,build['source']['sha256'],fixture,focus,child_output),
        'environment':{'PYTHONDONTWRITEBYTECODE':'1','PYTHONNOUSERSITE':'1','PYTHONPATH':'','PYTHONUTF8':'1',
            'TEMP':str(USER_ROOT/'temp'),'TMP':str(USER_ROOT/'temp'),'TMPDIR':str(USER_ROOT/'temp'),
            'AIRK_PRIVATE_CENSUS_BASE':str(USER_ROOT/'private-census')},
        'pins':[{'path':path,'sha256':value} for path,value in pins.items()],
        'runtime_build_receipt':{'path':str(receipt_path),'sha256':sha(receipt_path)},
        'producer_scope':'Root-authorized unchanged B2 typed I1 control producer only; not current embedded Kernel application evidence',
        'application_scope':'Actual fresh matched service owns application behavior; no B3 label assigned to B2 producer',
        'scope':'isolated U93 functional Windows public control; historical installed206 preserved; no I4 or clean latency/resource/quality qualification',
        'actual_execution_approved':False,'pending':'Root must review exact populated card and release actual lifecycle invocation'}
    # Validate against the actual frozen outer consumer without invoking it.
    outer=load('frozen_focus_card_outer',RECIPE/'run_owned_public_focus_r5.py');outer.validate_card(card)
    card_output.mkdir()
    card_path=card_output/'execution-card.json';card_path.write_text(json.dumps(card,indent=2)+'\n',encoding='utf-8')
    launch=[str(PYTHON),'-B',str(RECIPE/'run_owned_public_focus_r5.py'),'--card',str(card_path),
        '--card-sha256',sha(card_path),'--consent-actual-isolated-public-focus']
    (card_output/'launch-argv.json').write_text(json.dumps(launch,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'card':str(card_path),'sha256':sha(card_path),'launch_argv':launch,'executed':False},indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository',type=Path,default=next(p for p in BASE.parents if (p/'Cargo.toml').exists()))
    for name in ('runtime-dir','runtime-build-receipt','output'):parser.add_argument('--'+name,type=Path,required=True)
    for name in ('runtime-build-receipt-sha256','expected-native-group','expected-build-source-sha256'):parser.add_argument('--'+name,required=True)
    prepare(parser.parse_args())