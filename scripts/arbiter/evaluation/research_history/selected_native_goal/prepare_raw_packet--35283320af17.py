# SPDX-License-Identifier: Apache-2.0
"""Prepare proposed raw CAL packet from actual native artifacts; no model execution."""
import argparse
import hashlib
import json
from pathlib import Path
from run_raw_reviewed import ROOT,OWNER,OWNER_SHA,SCOPES,sha
HERE=Path(__file__).resolve().parent

def cargo_image(raw):
    images=set()
    for line in raw.decode('utf-8').splitlines():
        try:value=json.loads(line)
        except json.JSONDecodeError:continue
        if value.get('reason')=='compiler-artifact'and value.get('target',{}).get('name')=='airk_kernel'and value.get('profile',{}).get('test')is True and value.get('executable'):images.add(value['executable'])
    if len(images)!=1:raise ValueError('One actual Cargo-generated kernel test image required')
    return Path(images.pop()).resolve()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model',choices=SCOPES,required=True)
    for name in ('staged-inputs','preflight','capture','corpus','rubric','source-manifest','capture-release','cargo-json','build-source-seal'):
        parser.add_argument('--'+name,type=Path,required=True);parser.add_argument('--'+name+'-sha256',required=True)
    parser.add_argument('--output-packet',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();scope=SCOPES[args.model]
    if args.output_packet.exists()or args.output.exists()or not args.output.resolve().is_relative_to(ROOT/'artifacts'):raise ValueError('Fresh private packet/result paths required')
    pins={}
    for name in ('staged_inputs','preflight','capture','corpus','rubric','source_manifest','capture_release','cargo_json','build_source_seal'):
        path=getattr(args,name).resolve();expected=getattr(args,name+'_sha256')
        if sha(path)!=expected:raise ValueError('Actual metadata/artifact pin differs: '+name)
        pins[str(path)]=expected
    preflight=json.loads(args.preflight.read_bytes());capture=json.loads(args.capture.read_bytes());release=json.loads(args.capture_release.read_bytes())
    if not(preflight.get('complete')is True and preflight.get('Focus_configuration')=='ON'and preflight.get('human_semantic_gold_count')==0 and preflight.get('qualified_profiles')==[] and capture.get('complete')is True and capture.get('Context_task_NN_invocations')==0 and capture.get('Focus_configuration')=='ON'and release.get('approved')is True and capture['release_sha256']==args.capture_release_sha256):raise ValueError('Actual complete FocusON source/capture release chain required')
    references={}
    for name in ('capture','corpus','rubric','source_manifest'):
        path=getattr(args,name).resolve();expected=getattr(args,name+'_sha256')
        if preflight[name+'_sha256']!=expected:raise ValueError('Independent preflight source pin differs: '+name)
        references[name]={'path':str(path),'sha256':expected}
    inputs=json.loads(args.staged_inputs.read_bytes())
    if len(inputs)!=3 or any(set(row)!={'profile','profile_sha256','assets','fixtures'}for row in inputs):raise ValueError('Exact existing native staged3 Input array required')
    selected=[];models=set()
    for row in inputs:
        path=Path(row['profile']).resolve()
        if sha(path)!=row['profile_sha256']:raise ValueError('Actual native Profile snapshot changed')
        pins[str(path)]=row['profile_sha256'];profile=json.loads(path.read_bytes());models.add(profile['model'])
        if profile['model']==args.model:selected.append((row,path,profile))
    if models!={'arbiter-focus-qwen3-reranker-06b',*SCOPES}or len(selected)!=1:raise ValueError('Fixed selected native3 model roster differs')
    row,profile_path,profile=selected[0]
    expected_fit={'minimum_physical_cores':scope['cpu'],'intra_threads':scope['cpu'],'inter_threads':1,'sequential':True,'spinning':False}
    if profile.get('qualification')is not None or profile.get('cpu_fit')!=expected_fit or profile['memory_reservation_bytes']!=scope['reservation']or profile.get('qualified_targets')or profile.get('downloads'):raise ValueError('Actual unqualified original Profile/resource scope differs')
    assets=Path(row['assets']).resolve()
    for asset in profile['assets']:
        if Path(asset['name']).name!=asset['name']:raise ValueError('Nonflat native staged asset refused')
        path=assets/asset['name']
        if path.is_symlink()or not path.is_file()or path.stat().st_size!=asset['bytes']or sha(path)!=asset['sha256']:raise ValueError('Actual native asset identity/type/length changed')
        pins[str(path.resolve())]=asset['sha256']
    fixture=Path(row['fixtures']).resolve()
    if str(fixture)not in pins:raise ValueError('Native fixture must be authenticated asset')
    binary=cargo_image(args.cargo_json.read_bytes())
    if not binary.is_file()or binary.is_symlink():raise ValueError('Actual Cargo image missing/aliased')
    source_seal=json.loads(args.build_source_seal.read_bytes())
    for source in source_seal['files']:
        path=Path(source['path']).resolve()
        if sha(path)!=source['sha256']:raise ValueError('Actual build source seal differs')
        pins[str(path)]=source['sha256']
    if not source_seal['files']:raise ValueError('Actual consumed build source closure required')
    for path,digest in release['input_pins'].items():
        if sha(path)!=digest:raise ValueError('Upstream capture source/runtime closure drift')
        pins[str(Path(path).resolve())]=digest
    if sha(OWNER)!=OWNER_SHA:raise ValueError('Original process owner changed')
    for path in (OWNER,Path(__file__).resolve(),HERE/'run_raw_reviewed.py',HERE/'test_raw_owner.py',HERE/'test_prepare_raw.py'):pins[str(path.resolve())]=sha(path)
    output=args.output.resolve();env={'CUDA_VISIBLE_DEVICES':'','OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1','PYTHONDONTWRITEBYTECODE':'1','PYTHONNOUSERSITE':'1','AIRK_SELECTED_CONTEXT_PREFLIGHT':str(args.preflight.resolve()),'AIRK_SELECTED_CONTEXT_PREFLIGHT_SHA256':args.preflight_sha256,'AIRK_SELECTED_CONTEXT_PROFILES':str(args.staged_inputs.resolve()),'AIRK_SELECTED_CONTEXT_PROFILES_SHA256':args.staged_inputs_sha256,'AIRK_SELECTED_CONTEXT_CORPUS_SHA256':args.corpus_sha256,'AIRK_SELECTED_CONTEXT_MODEL':args.model,'AIRK_CONTEXT_ACTION_PRIVATE_CONTROL':str(output/'native-control')}
    header={'model':args.model,'profile':profile['manifest'],'profile_source_sha256':row['profile_sha256'],'profiles_sha256':args.staged_inputs_sha256,'preflight_sha256':args.preflight_sha256,'capture_sha256':args.capture_sha256,'corpus_sha256':args.corpus_sha256,'task_policy':preflight['task_policy']}
    packet={'schema':'airk.private-selected-context-cal-raw-packet/1','status':'PREPARED_FOR_ROOT_REVIEW','model':args.model,'runner_sha256':sha(HERE/'run_raw_reviewed.py'),'binary':{'path':str(binary),'sha256':sha(binary),'bytes':binary.stat().st_size},'actual_build_provenance':[{'path':str(args.cargo_json.resolve()),'sha256':args.cargo_json_sha256},{'path':str(args.build_source_seal.resolve()),'sha256':args.build_source_seal_sha256}],'profile_manifest':profile['manifest'],'profile_source_sha256':row['profile_sha256'],'cpu_fit':expected_fit,'candidate_memory_reservation_bytes':scope['reservation'],'limits':{'wall_seconds':scope['seconds'],'host_committed_bytes':scope['host']},'output':str(output),'environment':env,'pins':pins,'reference_inputs':references,'capture_release':{'path':str(args.capture_release.resolve()),'sha256':args.capture_release_sha256},'raw_header':header,'scope':'One original fixed native Context FocusON CAL raw100 capture only; no model selection/fit/qualification; separate actual Root taskNN release required.'}
    args.output_packet.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8');print(json.dumps({'packet':str(args.output_packet),'sha256':sha(args.output_packet),'model':args.model,'pins':len(pins),'NN_executed':False}))

if __name__=='__main__':main()
