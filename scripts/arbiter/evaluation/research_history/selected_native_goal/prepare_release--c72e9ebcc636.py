# SPDX-License-Identifier: Apache-2.0
"""Bind actual post-Ready native metadata into an UNAPPROVED CAL release."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
ROOT=Path(__file__).resolve().parents[5]
DRIVER=Path(__file__).resolve().parent.parent

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def pin(path):
    path=Path(path).resolve(strict=True)
    return {'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size}

def need(value,message):
    if not value:raise ValueError(message)

def add(pins,path,digest):
    key=str(Path(path).resolve())
    need(key not in pins or pins[key]==digest,'conflicting provenance pin: '+key)
    pins[key]=digest

def same_path(left,right):
    left,right=Path(left),Path(right)
    try:return left.samefile(right)
    except FileNotFoundError:return left.resolve()==right.resolve()


def postready(ready,bindings,status,namespace,control):
    need(ready.get('schema')=='airk.private-arbiter-fixture/1'and ready.get('evaluation')is True and ready.get('release_qualified')is False,'actualprivateReady required')
    need(bindings.get('schema')=='airk.private-arbiter-index-bindings/1'and bindings.get('evaluation')is True and bindings.get('release_qualified')is False,'actualnativebindings required')
    need(bindings['instance']==ready['health']['instance']and bindings['namespace']==namespace and same_path(bindings['control_root'],control) and same_path(ready['control_root'],control),'nativeinstance/catalog/control mismatch')
    need(status['schema']=='airk.arbiter.status/2'and status['configuration']['catalog']==namespace,'nativeStatus2 namespace mismatch')
    for stage in ('focus','context'):
        config=status['configuration'][stage];state=status[stage]
        need(config['enabled']is True and state['availability']=='ready'and config['selected_profile']==state['selected_profile']==state['active_profile'],'actualselectedwarmReady stage required')
    return status['focus']['selected_profile'],status['context']['selected_profile']

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('template','data-bindings','fixture-packet','bindings','native-plan','stage-status','current-plan','runtime-seal'):
        parser.add_argument('--'+name,type=Path,required=True);parser.add_argument('--'+name+'-sha256',required=True)
    parser.add_argument('--runtime-pair',type=Path,required=True);parser.add_argument('--work',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();supplied={}
    for name in ('template','data_bindings','fixture_packet','bindings','native_plan','stage_status','current_plan','runtime_seal'):
        path=getattr(args,name).resolve(strict=True);expected=getattr(args,name+'_sha256');need(sha(path)==expected,'exactmetadata pin differs: '+name);supplied[name]=json.loads(path.read_bytes())if name!='current_plan'else None
    release=supplied['template'];need(release['schema']=='airk.private-selected-Context-FocusON-CAL-capture-release/1'and release.get('approved')is not True,'unapprovedsource template required')
    cal=supplied['data_bindings']['partitions']['CAL'];fixture_packet=supplied['fixture_packet'];work=args.work.resolve(strict=True);control=work/'control';pair=args.runtime_pair.resolve(strict=True)
    need(Path(fixture_packet['work']).resolve()==work and fixture_packet['context_cal_observer']is True,'actualCALobserver fixturepacket mismatch')
    for key in ('image','inputs','build_receipt','cargo_json','owner'):
        row=fixture_packet[key];need(sha(row['path'])==row['sha256'],'actualfixturepacket evidence changed: '+key)
    build=json.loads(Path(fixture_packet['build_receipt']['path']).read_bytes());need(build['exit_code']==0 and build['inputs_unchanged']is True and build['groups']['native']['sha256']==fixture_packet['native_source_group'],'consumednativebuild failed/drifted')
    need(fixture_packet.get('semantic_plan') and fixture_packet['semantic_plan']['sha256']==args.native_plan_sha256,'startupobserver must bind sameactualnativepipeline plan')
    native={str((ROOT/name).resolve()):digest for name,digest in build['groups']['native']['files'].items()}
    old_source=dict(release['source_pins']);old_inputs=dict(release['input_pins']);pins={};source_pins={};refreshed=[];plan_key=str(args.current_plan.resolve())
    for target,source in ((source_pins,old_source),(pins,old_inputs)):
        for name,digest in source.items():
            path=str(Path(name).resolve())
            if path==plan_key:
                replacement=args.current_plan_sha256
            elif path in native:
                replacement=native[path]
            else:replacement=digest
            need(sha(path)==replacement,'original frozen source/data drift: '+path)
            add(target,path,replacement)
            if replacement!=digest:refreshed.append({'path':path,'historical_unapproved_sha256':digest,'actual_consumed_sha256':replacement})
    for name,digest in native.items():need(sha(name)==digest,'actualconsumednative source differs');add(pins,name,digest)
    for row in supplied['runtime_seal']['files']:need(sha(row['path'])==row['sha256'],'frozenPythonruntime drift');add(pins,row['path'],row['sha256'])
    for name in ('template','data_bindings','fixture_packet','bindings','native_plan','stage_status','current_plan','runtime_seal'):add(pins,getattr(args,name),getattr(args,name+'_sha256'))
    for key in ('image','inputs','build_receipt','cargo_json','owner'):
        row=fixture_packet[key];add(pins,row['path'],row['sha256'])
    ready_path=work/'ready.json';ready=json.loads(ready_path.read_bytes());ready_pin=pin(ready_path);add(pins,ready_path,ready_pin['sha256'])
    need(ready['health']['executable_sha256']==fixture_packet['image']['sha256'],'Readyactualnativeimage differs')
    actual_input=work/'inputs.json';need(sha(actual_input)==ready['inputs_sha256']==fixture_packet['inputs']['sha256'],'actualnativeInputarray differs');add(pins,actual_input,ready['inputs_sha256'])
    profiles={};models=set()
    for row in ready['profiles']:
        need(Path(row['snapshot']).name==row['snapshot'],'foreignprofile snapshot')
        path=work/row['snapshot'];need(sha(path)==row['profile_sha256'],'nativeProfile snapshot drift');value=json.loads(path.read_bytes());need(value['manifest']==row['manifest']and value['model']==row['model'],'nativeProfile substitution');profiles[row['manifest']]=value;models.add(row['model']);add(pins,path,row['profile_sha256'])
    need(len(profiles)==3 and models=={'arbiter-focus-qwen3-reranker-06b','arbiter-context-qwen3-17b','arbiter-context-qwen3-4b'},'fixedselected3roster differs')
    status=supplied['stage_status'];need(status.get('schema')=='airk.arbiter.status/2','raw native CLI Status2 required');namespace=ready['health']['catalog_clock']['namespace'];focus,context=postready(ready,supplied['bindings'],status,namespace,control)
    need(focus in profiles and context in profiles and profiles[focus]['model']=='arbiter-focus-qwen3-reranker-06b'and profiles[context]['model']in ('arbiter-context-qwen3-17b','arbiter-context-qwen3-4b'),'selectedstage Profile identity differs')
    policy=profiles[context]['task_policies']['airk.context_action.v1'];need(all(value['task_policies'].get('airk.context_action.v1')==policy for value in profiles.values()if value['model']!='arbiter-focus-qwen3-reranker-06b'),'Contexttaskpolicy differs')
    binding=supplied['bindings'];need(binding['scopes_sha256']==cal['question_scopes']['sha256']and len(binding['bindings'])==1 and same_path(binding['bindings'][0]['source_root'],cal['source_root']),'actualglobalCALroot/scope differs')
    actual_index=binding['bindings'][0]['index_id']
    for stage,manifest in (('focus',focus),('context',context)):
        enrollments=status['configuration'][stage]['indexes']
        need(len(enrollments)==1 and enrollments[0]['index_id']==actual_index and set(profiles[manifest]['tasks']).issubset(set(enrollments[0]['tasks'])),'actualglobalCAL index must be explicitly enrolled for both selectedstages')
    plan=supplied['native_plan'];need(plan['schema']=='airk.private-selected-native-semantic-setup/1'and plan.get('NN_invocations')==0,'actualnativeMiniLMplan required');pipeline=plan['object_plan']['pipeline_digest']
    need(len(pipeline)==64 and all(c in '0123456789abcdef'for c in pipeline),'nativepipeline digest invalid')
    catalog=plan['actual_native_catalog'];selected=catalog['selection']['profile_id'];need(selected in ('minilm-l6-v2-u8-v3','minilm-l6-v2-s8-v3'),'originalv3retained baseline required')
    chosen=[row for row in catalog['catalog']['profiles']if row['id']==selected];need(len(chosen)==1,'ambiguousnativeMiniLMselection');model_directory=Path(plan['source_directory']).resolve(strict=True)
    definition=model_directory/'model.json';need(sha(definition)==plan['model_definition_sha256'],'nativeMiniLMdefinition drift');add(pins,definition,plan['model_definition_sha256'])
    for asset in chosen[0]['assets']:
        need(Path(asset['name']).name==asset['name'],'nonflatnativeMiniLMasset');path=model_directory/asset['name'];need(sha(path)==asset['sha256']and path.stat().st_size==asset['bytes'],'actualselectedMiniLMasset drift');add(pins,path,asset['sha256'])
    deferred=[]
    for row in json.loads(actual_input.read_bytes()):
        value=json.loads(Path(row['profile']).read_bytes());need(sha(row['profile'])==row['profile_sha256'],'nativeoriginalProfile changed');add(pins,row['profile'],row['profile_sha256'])
        for asset in value['assets']:
            need(Path(asset['name']).name==asset['name'],'nonflatnativeArbiterasset');path=Path(row['assets'])/asset['name'];need(path.is_file()and not path.is_symlink()and path.stat().st_size==asset['bytes'],'nativeassettype/length drift');add(pins,path,asset['sha256']);deferred.append(str(path.resolve()))
    for row in cal['source_text_files']:add(pins,row['path'],row['sha256'])
    for row in (cal['corpus'],cal['source_manifest'],cal['question_scopes'],cal['rubric']):add(pins,row['path'],row['sha256'])
    runtime=json.loads((pair/'airk-runtime.json').read_bytes());build_pair=json.loads((pair/'airk-build.json').read_bytes())
    for name,key in (('airk.exe','airk_sha256'),('airkctl.exe','airkctl_sha256')):
        path=pair/name;need(sha(path)==runtime[key]==build_pair['runtime'][key],'actualmatchedpair image differs');add(pins,path,runtime[key])
    for name in ('airk-runtime.json','airk-build.json'):add(pins,pair/name,sha(pair/name))
    node=shutil.which('node');need(node,'actualNodeclient runtime absent');add(pins,node,sha(node))
    for name in ('session.js','runner.js','adminChunks.js'):
        path=ROOT/'packages/vscode/out/airkctl'/name;add(pins,path,sha(path))
    for name in ('context-request.json',):path=work/name;add(pins,path,sha(path))
    add(pins,__file__,sha(__file__))
    test_file=Path(__file__).with_name('test_prepare_release.py');add(pins,test_file,sha(test_file))
    out=args.output.resolve();need(not out.exists()and out.is_relative_to(ROOT/'artifacts')and not out.is_relative_to(Path(cal['fixture']).resolve())and not out.is_relative_to(Path(cal['corpus']['path']).parent.resolve()),'freshprivate output outside immutablesources required')
    release.update(approved=None,status='ACTUAL_POST_READY_METADATA_BOUND_ROOT_CAPTURE_RELEASE_REQUIRED',source_pins=source_pins,input_pins=pins,Focus_profile=focus,Context_bootstrap_profile=context,MiniLM_pipeline=pipeline,task_policy=policy,source_manifest_sha256=cal['source_manifest']['sha256'],corpus_sha256=cal['corpus']['sha256'])
    release['paths']={'runtime_pair':str(pair),'control_root':str(control),'fixture_ready':str(ready_path),'index_bindings':str(args.bindings.resolve()),'request_template':str(work/'context-request.json'),'question_scopes':cal['question_scopes']['path'],'fixture':cal['fixture'],'admission_directory':str(work/'context-cal-admissions')}
    release['actual_binding_evidence']={'fixture_packet':pin(args.fixture_packet),'native_plan':pin(args.native_plan),'stage_status':pin(args.stage_status),'Ready':ready_pin,'native_source_group':fixture_packet['native_source_group'],'refreshed_unapproved_metadata_pins':refreshed,'deferred_large_asset_sha_checks':'All nativeassetSHAs are in input_pins and will be rechecked BEFORE/AFTER capture by unchangedcollector; producer checks type/length only after genuine native seed.'}
    release['required_unresolved']=['Rootindependent source/resource/actualpipeline/owned-window review and separate approvedexactrelease; native actualprocess/catalog/Ready/source liveness is rechecked by unchangedcollector.']
    release['frozen_configuration']=status['configuration'];release['capture_python']=supplied['runtime_seal']['runtime_executable'];release['post_capture_required']='Ownedcapture controller must reconcile actualcapturereport stage_before/stage_after configuration exactly to this frozen nativeconfiguration; unchangedcollector also checks realcurrentprocess/source/profile and its ownbefore/afterconfig. No metadata snapshot alone grants execution.'
    release['current_plan']=pin(args.current_plan);release['note']='UNAPPROVED metadata only. No nativeDTOorprocess/NN launch; historicalexecutedprotocols/sourcebytes preserved. ActualWarmReady/PID/lifetime/resource/pipeline are not qualified merely by metadata.'
    with out.open('x',encoding='utf-8')as stream:json.dump(release,stream,indent=2);stream.write('\n')
    print(json.dumps({'approved':None,'release':str(out),'sha256':sha(out),'input_pins':len(pins),'Focus_profile':focus,'Context_bootstrap_profile':context,'MiniLM_pipeline':pipeline,'NN_launched':False}))

if __name__=='__main__':main()
