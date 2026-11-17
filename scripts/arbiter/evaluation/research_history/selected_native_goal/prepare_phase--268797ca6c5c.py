# SPDX-License-Identifier: Apache-2.0
"""Prepare exact no-NN certificate or native fitter packet from actual owned inputs."""
import argparse,json,sys,os
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/'native-cal-owned-r1'))
from run_raw_reviewed import ROOT,OWNER,OWNER_SHA,sha,capture_execution,verify_absences,source_pin,natural
from prepare_raw_packet import cargo_image,retained_cargo_image

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--phase',choices=('certificate','fit'),required=True)
    for name in ('capture','capture-execution','capture-release','data-bindings','runtime-seal','current-plan'):
        p.add_argument('--'+name,type=Path,required=True);p.add_argument('--'+name+'-sha256',required=True)
    for name in ('preflight','staged-inputs','cargo-json','build-source-seal','retained-images'):
        p.add_argument('--'+name,type=Path);p.add_argument('--'+name+'-sha256')
    p.add_argument('--raw-execution',type=Path,action='append',default=[]);p.add_argument('--raw-execution-sha256',action='append',default=[])
    p.add_argument('--output',type=Path,required=True);p.add_argument('--output-packet',type=Path,required=True)
    a=p.parse_args();pins={};absences={}
    def add(path,digest):source_pin(pins,absences,path,digest)
    def load(name):
        path=getattr(a,name);digest=getattr(a,name+'_sha256')
        if path is None or not digest:raise ValueError('Required actual metadata: '+name)
        add(path,digest);return json.loads(path.read_bytes())
    capture=load('capture');release=load('capture_release');data=load('data_bindings');runtime=load('runtime_seal');add(a.current_plan,a.current_plan_sha256)
    pins.update(capture_execution(a.capture_execution,a.capture_execution_sha256,{'path':str(a.capture.resolve()),'sha256':a.capture_sha256},{'path':str(a.capture_release.resolve()),'sha256':a.capture_release_sha256}))
    if not(capture['complete']is True and len(capture['rows'])==100 and len({r['id']for r in capture['rows']})==100 and capture['Focus_configuration']=='ON'and capture['Context_task_NN_invocations']==0 and capture['release_sha256']==a.capture_release_sha256 and release['approved']is True):raise ValueError('Exact complete actual FocusON CAL100 capture required')
    cal=data['partitions']['CAL']
    if capture['corpus_sha256']!=cal['corpus']['sha256']or capture['source_manifest_sha256']!=cal['source_manifest']['sha256']:raise ValueError('Actual CAL source association differs')
    for name in ('corpus','source_manifest','rubric','question_scopes'):
        row=cal[name];add(row['path'],row['sha256'])
    for row in cal['source_text_files']:add(row['path'],row['sha256'])
    for path,sentinel in release.get('absent_build_inputs',{}).items():source_pin(pins,absences,path,sentinel)
    for row in runtime['files']:add(row['path'],row['sha256'])
    for name in ('prepare_phase.py','run_phase_reviewed.py','test_phase.py'):path=HERE/name;add(path,sha(path))
    for name in ('run_raw_reviewed.py','prepare_raw_packet.py'):path=HERE.parent/'native-cal-owned-r1'/name;add(path,sha(path))
    add(OWNER,OWNER_SHA)
    work=a.output.resolve()
    if work.exists()or a.output_packet.exists()or not work.is_relative_to(ROOT/'artifacts')or work.is_relative_to(Path(cal['fixture']).resolve()):raise ValueError('Fresh isolated private outputs required')
    env={key:'1'for key in ('OMP_NUM_THREADS','OPENBLAS_NUM_THREADS','MKL_NUM_THREADS','NUMEXPR_NUM_THREADS','VECLIB_MAXIMUM_THREADS')};env.update(CUDA_VISIBLE_DEVICES='',PYTHONDONTWRITEBYTECODE='1',PYTHONNOUSERSITE='1')
    if a.phase=='certificate':
        if a.raw_execution:raise ValueError('Certificate must precede all raw task scores')
        source=HERE.parent/'source'
        for name in ('prepare_selected_cal_reference_certificates.py','goal_sources.py','goal_rubric.py'):add(source/name,sha(source/name))
        artifact=work/'preflight.private.json';argv=[runtime['runtime_executable'],'-B',str(source/'prepare_selected_cal_reference_certificates.py')]
        for name,row in (('capture',{'path':str(a.capture.resolve()),'sha256':a.capture_sha256}),('corpus',cal['corpus']),('rubric',cal['rubric']),('source-manifest',cal['source_manifest'])):argv+=['--'+name,row['path'],'--'+name+'-sha256',row['sha256']]
        argv+=['--task-policy',capture['task_policy'],'--output',str(artifact)];schema='airk.private-selected-Context-CAL-reference-preflight/1'
    else:
        preflight=load('preflight');staged=load('staged_inputs')
        if preflight['capture_sha256']!=a.capture_sha256 or preflight['corpus_sha256']!=cal['corpus']['sha256']or preflight['complete']is not True or preflight['qualified_profiles']!=[]:raise ValueError('Exact independent preflight required')
        if len(a.raw_execution)!=2 or len(a.raw_execution_sha256)!=2:raise ValueError('Exactly two actual raw owned executions required')
        if a.cargo_json is None or not a.cargo_json_sha256:raise ValueError('Actual fitter kernel Cargo artifact required')
        add(a.cargo_json,a.cargo_json_sha256);image=cargo_image(a.cargo_json.read_bytes())
        if a.retained_images is not None:image=retained_cargo_image(a.cargo_json.read_bytes(),load('retained_images'))
        add(image,sha(image));seal=load('build_source_seal')
        for row in seal['files']:source_pin(pins,absences,row['path'],row['sha256'])
        models=[];seen=set()
        for path,digest in zip(a.raw_execution,a.raw_execution_sha256):
            add(path,digest);execution=json.loads(path.read_bytes());raw_path=path.parent/'capture.json';raw_digest=execution.get('capture_sha256');add(raw_path,raw_digest)
            if not(execution['schema']=='airk.private-selected-context-cal-raw-owned-execution/1'and execution['complete']is True and execution['post_input_drift']==[]and execution['whole_phase_expired']is False and execution['raw_header']['preflight_sha256']==a.preflight_sha256 and execution['raw_header']['capture_sha256']==a.capture_sha256):raise ValueError('Actual complete raw100 owned receipt required')
            binary=next(({'path':name,'sha256':h}for name,h in execution['input_seals'].items()if h==json.loads(raw_path.read_bytes())['test_binary_sha256']),None)
            if binary is None or not natural(execution['owned'],binary):raise ValueError('Actual raw job/image natural closure absent')
            for name,h in execution['input_seals'].items():add(name,h)
            for name,sentinel in execution.get('absent_build_inputs',{}).items():source_pin(pins,absences,name,sentinel)
            candidates=[row for row in staged if row['profile_sha256']==execution['raw_header']['profile_source_sha256']]
            if len(candidates)!=1 or execution['model']in seen:raise ValueError('Exact fixed two native profiles required')
            row=candidates[0];seen.add(execution['model']);models.append({'profile':row['profile'],'profile_sha256':row['profile_sha256'],'raw':str(raw_path.resolve()),'raw_sha256':raw_digest,'base_assets':row['assets']})
        if seen!={'arbiter-context-qwen3-17b','arbiter-context-qwen3-4b'}:raise ValueError('Exact Compact+Large required')
        # This is a native-owned file-reference input, never a Profile/request mirror.
        descriptor=a.output_packet.with_name(a.output_packet.stem+'-fit-input.json')
        if descriptor.exists():raise ValueError('Fresh native fit descriptor required')
        descriptor.write_text(json.dumps({'preflight':str(a.preflight.resolve()),'preflight_sha256':a.preflight_sha256,'models':models},indent=2)+'\n',encoding='utf-8');add(descriptor,sha(descriptor))
        env.update(AIRK_CONTEXT_ACTION_FIT_INPUT=str(descriptor.resolve()),AIRK_CONTEXT_ACTION_FIT_INPUT_SHA256=sha(descriptor),AIRK_CONTEXT_ACTION_FIT_OUTPUT=str(work/'fitted'),AIRK_SELECTED_CONTEXT_PROFILES_SHA256=a.staged_inputs_sha256,AIRK_SELECTED_CONTEXT_CORPUS_SHA256=cal['corpus']['sha256'])
        argv=[str(image),'--ignored','--exact','inference_owner::context_action_calibration_tests::selected::fit_selected_context_generic_read_calibration','--nocapture','--test-threads=1'];artifact=work/'fitted'/'fit-report.private.json';schema='airk.private-selected-Context-generic-read-CAL-fit/1'
    packet={'schema':'airk.private-selected-cal-completion-packet/1','phase':a.phase,'approved':None,'pins':pins,'absent_build_inputs':absences,'argv':argv,'image':{'path':argv[0],'sha256':sha(argv[0])},'environment':env,'output':str(work),'artifact':str(artifact),'artifact_schema':schema,'limits':{'wall_seconds':60,'host_committed_bytes':256*1024**2},'neural_execution':False,'qualification_granted':False,'human_semantic_gold_count':0}
    a.output_packet.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8');print(json.dumps({'packet':str(a.output_packet),'sha256':sha(a.output_packet),'phase':a.phase,'NN_executed':False}))
if __name__=='__main__':main()
