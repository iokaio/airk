# SPDX-License-Identifier: Apache-2.0
"""Private reviewed query-only DEV stage; consumes actual compiler/artifact pins, never builds."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time
import types

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
BASE=HERE.parent/'ann-regeneration-preparation-r1/run_reviewed.py'
BASE_SHA='4bf63c0f091565aafbcb727c64bb02edd82c71fb2c7d022c21a54d652cb26f9a'

def geometry_proofs(log, selector):
    messages=[]
    prefix='test '+selector+' ... '
    for line in log.splitlines():
        if line.startswith(prefix):line=line[len(prefix):]
        if line.startswith('{'):
            try:row=json.loads(line)
            except json.JSONDecodeError:continue
            if row.get('preflight') is True:messages.append(row)
    return messages

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=['geometry','query'],required=True)
    parser.add_argument('--bound-packet',type=Path,required=True)
    parser.add_argument('--bound-packet-sha256',required=True)
    parser.add_argument('--reviewed-runner-sha256',required=True)
    parser.add_argument('--execute-reviewed',action='store_true')
    parser.add_argument('--geometry-receipt',type=Path)
    args=parser.parse_args()
    raw=BASE.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=BASE_SHA:raise ValueError('retained common evidence helper changed')
    common=types.ModuleType('_retained_regeneration_evidence_helpers');common.__file__=str(BASE)
    exec(compile(raw,str(BASE),'exec'),common.__dict__)
    common.require(args.bound_packet,args.bound_packet_sha256)
    bound=json.loads(args.bound_packet.read_bytes())
    packet_path=Path(bound['preparation']['path']);common.require(packet_path,bound['preparation']['sha256'])
    packet=json.loads(packet_path.read_bytes())
    binary=Path(bound['binary']['path'])
    pins={Path(p):sha for p,sha in packet['source_and_input_seals'].items()}
    pins.update({BASE:BASE_SHA,Path(__file__).resolve():args.reviewed_runner_sha256,
                 args.bound_packet:args.bound_packet_sha256,packet_path:bound['preparation']['sha256'],
                 binary:bound['binary']['sha256']})
    for artifact in [bound['compile_receipt'],bound['kernel_depinfo'],bound['retrieval_depinfo'],*bound['artifacts']]:
        path=Path(artifact['path']);sha=artifact['sha256']
        if path in pins and pins[path]!=sha:raise ValueError('conflicting actual artifact seals')
        pins[path]=sha
    for path,sha in pins.items():common.require(path,sha)
    compile_receipt=json.loads(Path(bound['compile_receipt']['path']).read_bytes())
    if compile_receipt['exit_code']!=0 or not bound['artifacts']:raise ValueError('actual compiler closure missing')
    native=compile_receipt['groups']['native']['files']
    for path,sha in pins.items():
        if path.suffix=='.rs' and path.is_relative_to(ROOT/'crates'):
            if native.get(path.relative_to(ROOT).as_posix())!=sha:raise ValueError('compiler source snapshot differs')
    dep=Path(bound['kernel_depinfo']['path']).read_text().replace('\\','/')
    if 'crates/airk-kernel/src/inference_owner/evaluation_tests/candidate/regenerated_queries.rs' not in dep:
        raise ValueError('owning compiler did not consume query-only module')
    selector=packet['preflight_selector' if args.mode=='geometry' else 'query_selector']
    listed=subprocess.run([str(binary),'--exact',selector,'--ignored','--list'],cwd=ROOT,capture_output=True,text=True,timeout=10)
    if listed.returncode!=0 or listed.stdout.count(selector+': test')!=1:raise ValueError('exactly one native selector required')
    if not args.execute_reviewed:
        print(json.dumps({'preflight_passed':True,'selected':1,'mode':args.mode,'pins':len(pins),'stage_started':False}));return
    if os.name!='nt':raise ValueError('reviewed Windows owned-process stage required')
    work=HERE/('execution-r1-'+args.mode)
    if work.exists():raise ValueError('no overwrite/retry of a previous stage')
    environment=dict(os.environ);environment.update(packet['environment'])
    capture=Path(environment['AIRK_NATIVE_DEV64_OUTPUT'])
    if capture.exists():raise ValueError('native capture already exists; no replay')
    if args.mode=='query':
        geometry_path=args.geometry_receipt or HERE/'execution-r1-geometry/execution.json'
        geometry=json.loads(geometry_path.read_bytes())
        if args.geometry_receipt:
            original=HERE/'execution-r1-geometry/execution.json'
            log=HERE/'execution-r1-geometry/native.log'
            correction=geometry['parser_correction']
            common.require(original,correction['original_execution_sha256'])
            common.require(log,correction['native_log_sha256'])
            retained=json.loads(original.read_bytes())
            if any(geometry[k]!=v for k,v in retained.items() if k!='complete'):raise ValueError('geometry evidence was changed')
            if not common.natural(retained['owned']) or retained['post_input_drift']:raise ValueError('natural geometry closure required')
            if geometry_proofs(log.read_text(),packet['preflight_selector'])!=[correction['proof']]:raise ValueError('geometry proof differs')
        if geometry['complete'] is not True or geometry['bound_packet_sha256']!=args.bound_packet_sha256:
            raise ValueError('same-identity actual geometry proof required before NN')
    owner_path=ROOT/'tools/retrieval-train/training_execution.py'
    owner_raw=owner_path.read_bytes()
    if hashlib.sha256(owner_raw).hexdigest()!=pins[owner_path]:raise ValueError('actual owned-process helper changed')
    owner=types.ModuleType('_reviewed_query_owned_process');owner.__file__=str(owner_path)
    exec(compile(owner_raw,str(owner_path),'exec'),owner.__dict__)
    seconds,host_bytes=(120,2147483648) if args.mode=='geometry' else (packet['limits']['seconds'],packet['limits']['host_bytes'])
    work.mkdir();(work/'stdin.txt').write_bytes(b'')
    argv=[str(binary),'--exact',selector,'--ignored','--nocapture','--test-threads=1']
    request={'argv':argv,'mode':args.mode,'bound_packet_sha256':args.bound_packet_sha256,'pre_input_seals':{str(p):s for p,s in pins.items()},
             'environment':packet['environment'],'limits':{'seconds':seconds,'host_bytes':host_bytes,'max_job_processes':2},'selector_list':listed.stdout}
    (work/'request.json').write_text(json.dumps(request,indent=2)+'\n',encoding='utf-8')
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError('single exact native process required')
        image=owner.image_proof(api,info.process,bound['binary'])
        if time.monotonic()>=deadline:raise ValueError('original stage deadline expired before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    started=time.monotonic()
    with (work/'native.log').open('xb') as log,(work/'stdin.txt').open('rb') as stdin:
        owned=owner.run_windows_owned_training(argv,ROOT,log,seconds,environment,stdin,host_bytes,before_resume=suspended)
    drift=[str(path) for path,sha in pins.items() if common.sha(path)!=sha]
    result={'owned':owned,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,
            'bound_packet_sha256':args.bound_packet_sha256,'complete':False,'quality_or_release_qualified':False}
    if args.mode=='geometry':
        proofs=geometry_proofs((work/'native.log').read_text(encoding='utf-8',errors='replace'),packet['preflight_selector'])
        result['complete']=common.natural(owned) and not drift and len(proofs)==1 and proofs[0].get('query_count')==64 and proofs[0].get('neural_execution') is False
    elif capture.is_file():
        native=json.loads(capture.read_bytes())
        ids=[r['id'] for r in native['records']]
        result['capture_sha256']=common.sha(capture)
        result['complete']=(common.natural(owned) and not drift and native.get('complete') is True
            and native.get('query_only') is True and native.get('indexing_performed') is False
            and len(ids)==64 and len(set(ids))==64 and native['quality_denominator']==20 and native['alignment_refusals']==44)
    (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result))
    if not result['complete']:raise SystemExit(1)

if __name__=='__main__':main()
