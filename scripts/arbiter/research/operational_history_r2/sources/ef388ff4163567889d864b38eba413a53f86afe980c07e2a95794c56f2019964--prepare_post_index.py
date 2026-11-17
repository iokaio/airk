# SPDX-License-Identifier: Apache-2.0
"""Prepare only: original native setup/Ready -> existing owned continuation.

No subprocess, native DTO construction, model/CLI invocation or source edit.
"""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[4]
DRIVER=ROOT/'artifacts/advanced-split/actual-composed-service-preparation-r1'

def pin(path):
    path=Path(path).resolve(strict=True)
    with path.open('rb') as stream: sha=hashlib.file_digest(stream,'sha256').hexdigest()
    return {'path':str(path),'sha256':sha,'bytes':path.stat().st_size}

def verify(reference):
    actual=pin(reference['path'])
    if any(actual[k]!=reference[k] for k in ('sha256','bytes') if k in reference):
        raise ValueError('original pinned input changed: '+reference['path'])
    return actual

def same_physical_path(actual,expected):
    try:return Path(actual).samefile(expected)
    except (OSError,ValueError):return False

def json_lines(path):
    text=Path(path).read_text(encoding='utf-8')
    try:
        whole=json.loads(text)
        if isinstance(whole,dict):return [whole]
    except ValueError:pass
    rows=[]
    for line in text.splitlines():
        try: value=json.loads(line)
        except ValueError: continue
        if isinstance(value,dict): rows.append(value)
    return rows

def setup_index(log, *, source_root, collection):
    rows=[x['receipt'] for x in json_lines(log) if x.get('status')=='catalog_index_setup_applied']
    if len(rows)!=1: raise ValueError('exactly one original native setup applied receipt required')
    value=rows[0]
    if value['stage']!='ready' or value.get('completion') is None:
        raise ValueError('native index metadata setup not complete')
    registration=value['request']['registration']
    if not same_physical_path(registration['root'],source_root) or registration['collection']!=collection:
        raise ValueError('native setup belongs to different original source/collection')
    return value['index_id']

def catalog_singleton(project_log,membership_log,index_log,setup_receipt):
    documents=[]
    for path,kind in [(project_log,'project'),(membership_log,'membership'),(index_log,'index')]:
        pages=[x['page'] for x in json_lines(path) if x.get('status')=='catalog_page']
        if len(pages)!=1 or pages[0].get('continuation') is not None:
            raise ValueError('one complete original catalog page required for '+kind)
        page=pages[0]
        if any(x.get('kind')!=kind for x in page['items']):raise ValueError('foreign entity in native catalog census')
        documents.append(page)
    clocks={(x['namespace'],x['revision']) for x in documents}
    if len(clocks)!=1 or documents[0]['namespace']!=setup_receipt['request']['key']['namespace']:
        raise ValueError('native catalog census snapshot association differs')
    projects=[x['value'] for x in documents[0]['items']]
    memberships=[x['value'] for x in documents[1]['items']]
    indexes=[x['value'] for x in documents[2]['items']]
    index=setup_receipt['index_id']
    selected=[x for x in indexes if x['id']==index]
    members=[x for x in memberships if x['index_id']==index]
    if len(selected)!=1 or len(members)!=1:
        raise ValueError('actual index must already have exactly one current membership; never create a redundant set')
    definition=selected[0];member=members[0]
    chosen=[x for x in projects if x['id']==member['project_id']]
    if len(chosen)!=1:raise ValueError('actual singleton membership project absent/ambiguous')
    project=chosen[0];registration=setup_receipt['request']['registration']
    if definition['tombstone'] or definition['source_import']!='complete' or not same_physical_path(definition['backing']['root'],registration['root']):
        raise ValueError('actual current index differs from original setup')
    if project['name']!=setup_receipt['request']['name'] or any(x!=index for x in project['default_indexes']) or sum(x['project_id']==project['id'] for x in memberships)!=1:
        raise ValueError('actual setup-owned singleton project identity/membership differs')
    roots=setup_receipt['source_ids'];atoms=member['scope']
    if len(atoms)!=len(roots) or {x['source_root_id'] for x in atoms}!=set(roots) or any(x['kind']!='subtree' or x['path']!='' for x in atoms):
        raise ValueError('actual singleton membership is not exactly whole original source roots')
    return {'namespace':documents[0]['namespace'],'revision':documents[0]['revision'],'index':definition,'project':project,'membership':member}

def semantic_ready(log,index,pipeline,collection):
    candidates=[]
    for row in json_lines(log):
        content=row.get('content')
        if row.get('isError') or not isinstance(content,list) or len(content)!=1 or content[0].get('type')!='text':continue
        value=json.loads(content[0]['text'])
        if isinstance(value,dict):candidates.append(value)
    if len(candidates)!=1:raise ValueError('one original full canonical semantic status envelope required')
    value=candidates[0]; semantic=value['semantic_readiness']
    if semantic['state']!='ready' or semantic['pipeline_digest']!=pipeline or semantic.get('failure') is not None or value.get('local_model_ready') is not True or value['discovery_readiness']['state']!='ready':
        raise ValueError('actual complete semantic publication for original pipeline absent')
    members=value['scope_observation']['members']
    if len(members)!=1 or members[0]['index_id']!=index or value['collection']!=collection or not value.get('publication'):
        raise ValueError('actual semantic status source/publication identity differs')
    return value

def prepare(args):
    fixture=json.loads(Path(args.fixture_packet).read_text(encoding='utf-8'))
    setup=json.loads(Path(args.setup_packet).read_text(encoding='utf-8'))
    execution=json.loads(Path(args.setup_execution).read_text(encoding='utf-8'))
    if execution['schema']!='airk.private-selected-cal-setup-execution/1' or execution.get('complete') is not True or execution['phase']!='index':
        raise ValueError('complete actual original index setup execution required')
    if execution['packet_sha256']!=pin(args.setup_packet)['sha256'] or setup['fixture_packet']['sha256']!=pin(args.fixture_packet)['sha256']:
        raise ValueError('actual setup and fixture packet association differs')
    if fixture.get('pipeline_pending_native_plan') or not fixture.get('semantic_plan'):
        raise ValueError('actual native semantic plan must precede fixture')
    if fixture.get('context_cal_observer') is not True:
        raise ValueError('actual CAL capture observer must be enabled before fixture startup')
    files=[args.fixture_packet,args.setup_packet,args.setup_execution,args.semantic_ready_log,args.ready,args.catalog_projects,args.catalog_memberships,args.catalog_indexes]
    for row in [fixture['image'],fixture['inputs'],fixture['semantic_plan'],setup['data_bindings'],*setup['runtime_images'],*setup['runtime_metadata']]:
        verify(row);files.append(row['path'])
    semantic_plan=json.loads(Path(fixture['semantic_plan']['path']).read_text(encoding='utf-8'))
    pipeline=semantic_plan['object_plan']['pipeline_digest']
    data=json.loads(Path(setup['data_bindings']['path']).read_text(encoding='utf-8'))['partitions']['CAL']
    source=data['source_root']; scopes=verify(data['question_scopes']);files.append(scopes['path'])
    actual=[x for x in execution['commands'] if x['label']=='actual-index-setup']
    if len(actual)!=1 or actual[0]['owned']['exit_code']!=0 or actual[0]['owned']['owned_descendants_settled'] is not True or actual[0]['owned']['owned_job_forced_termination'] is not False:
        raise ValueError('original native setup command did not settle successfully')
    setup_log=verify(actual[0]['log']);files.append(setup_log['path'])
    collection='selected-context-cal-r1'
    index=setup_index(setup_log['path'],source_root=source,collection=collection)
    setup_receipt=[x['receipt'] for x in json_lines(setup_log['path']) if x.get('status')=='catalog_index_setup_applied'][0]
    singleton=catalog_singleton(args.catalog_projects,args.catalog_memberships,args.catalog_indexes,setup_receipt)
    semantic_ready(args.semantic_ready_log,index,pipeline,collection)
    ready=json.loads(Path(args.ready).read_text(encoding='utf-8'));work=Path(fixture['work'])
    if not same_physical_path(args.ready,work/'ready.json') or ready['schema']!='airk.private-arbiter-fixture/1' or ready.get('evaluation') is not True or ready.get('release_qualified') is not False:
        raise ValueError('actual private fixture Ready identity differs')
    if ready['inputs_sha256']!=fixture['inputs']['sha256'] or ready['health']['executable_sha256']!=fixture['image']['sha256'] or not same_physical_path(ready['control_root'],work/'control'):
        raise ValueError('actual native Ready image/input/control association differs')
    profiles={x['model']:x for x in ready['profiles']}
    wanted={'arbiter-focus-qwen3-reranker-06b','arbiter-context-qwen3-17b','arbiter-context-qwen3-4b'}
    if len(ready['profiles'])!=3 or set(profiles)!=wanted:raise ValueError('actual three selected profiles absent')
    for row in profiles.values():
        snapshot=work/row['snapshot']; observed=pin(snapshot)
        if observed['sha256']!=row['profile_sha256']:raise ValueError('actual native profile snapshot changed')
        value=json.loads(snapshot.read_text(encoding='utf-8'))
        if value['manifest']!=row['manifest'] or value['model']!=row['model']:raise ValueError('actual profile manifest/model differs')
        files.append(snapshot)
    ctl=[x['path'] for x in setup['runtime_images'] if Path(x['path']).name=='airkctl.exe']
    if len(ctl)!=1:raise ValueError('actual matched client image absent')
    output=Path(args.output).resolve()
    if not output.is_relative_to(ROOT/'artifacts') or not output.parent.is_dir():raise ValueError('fresh private artifacts output under existing parent required')
    output.mkdir(exist_ok=False)
    helper=ROOT/'artifacts/advanced-split/models/selected-native-goal-driver-r1/source/native_eval_session.cjs'
    library=ROOT/'packages/vscode/out/airkctl/session.js'
    dependencies=[library.with_name('runner.js'),library.with_name('adminChunks.js')]
    controller=DRIVER/'session_controller.cjs'
    controller_packet={'session_helper':str(helper),'session_library':str(library),'client_dependencies':list(map(str,dependencies)),
                       'pins':{str(x):pin(x)['sha256'] for x in [helper,library,*dependencies]}}
    controller_path=output/'controller-packet.json';controller_path.write_text(json.dumps(controller_packet,indent=2)+'\n',encoding='utf-8')
    files.extend([args.node,helper,library,*dependencies,controller,controller_path,DRIVER/'run_composed.py',DRIVER/'os_measurement.py',Path(__file__)])
    steps=[];common=[ctl[0],'--service-runtime-dir',str(work/'control')]
    def command(argv,label,environment=None):
        steps.append({'kind':'native_command','case':'CAL-'+label,'argv':argv,'deadline_seconds':60,
                      **({'environment':environment} if environment else {})});return len(steps)-1
    selector='service_runtime::evaluation_fixture_tests::emit_private_arbiter_configuration'
    def emit(prefix,stage,manifest):
        for ending in ('configure.json','bindings.json','receipt.json'):
            if (work/f'{prefix}.{ending}').exists():raise ValueError('fresh original native control prefix required')
        env={'AIRK_ARBITER_EVALUATION_WORK':str(work),'AIRK_ARBITER_EVALUATION_SCOPES':scopes['path'],
             'AIRK_ARBITER_EVALUATION_SCOPES_SHA256':scopes['sha256'],'AIRK_ARBITER_EVALUATION_CONTROL_PREFIX':prefix,
             'AIRK_ARBITER_EVALUATION_PROFILE':manifest,'AIRK_ARBITER_EVALUATION_STAGE':stage,'AIRK_ARBITER_EVALUATION_ENABLED':'true'}
        command([fixture['image']['path'],'--ignored','--exact',selector,'--nocapture'],prefix,env)
    for stage,model in [('focus','arbiter-focus-qwen3-reranker-06b'),('context','arbiter-context-qwen3-17b')]:
        manifest=profiles[model]['manifest'];prefix=f'cal-{stage}-warm-r102'
        emit(prefix,stage,manifest)
        idle=command(common+['arbiter','status'],stage+'-prior-idle')
        configure=command(common+['arbiter','configure','--file',str(work/f'{prefix}.configure.json')],stage+'-configure')
        steps.append({'kind':'await_ready','case':'CAL-'+stage+'-Ready','stage':stage,'profile_manifest':manifest,
                      'argv':common+['arbiter','status'],'prior_idle_observation_step':idle,
                      'configuration_observation_step':configure,'deadline_seconds':args.warm_seconds})
    emit('cal-live','context',profiles['arbiter-context-qwen3-17b']['manifest'])
    command(common+['arbiter','status'],'final-native-stage-status')
    steps.append({'kind':'controller','case':'CAL-control-close','request':{'kind':'finish'}})
    pins={str(Path(x).resolve()):pin(x)['sha256'] for x in files}
    plan={'schema':'airk.private.composed-execution-plan/1','whole_phase_seconds':args.whole_seconds,
          'node':str(Path(args.node).resolve()),'controller':str(controller),'controller_packet':str(controller_path),
          'pins':pins,'per_step_pins':{},'allowed_native_images':[ctl[0],fixture['image']['path']],'steps':steps}
    plan_path=output/'continuation-plan.json';plan_path.write_text(json.dumps(plan,indent=2)+'\n',encoding='utf-8')
    handoff={'schema':'airk.private-selected-cal-post-index-preparation/1','prepared_only':True,'qualification_granted':False,
             'driver':pin(DRIVER/'run_composed.py'),'plan':pin(plan_path),'actual_setup_index_id':index,'reused_actual_singleton':singleton,
             'fixture_ready':pin(args.ready),'native_scopes':scopes,'native_pipeline_digest':pipeline,
             'Focus_profile':profiles['arbiter-focus-qwen3-reranker-06b']['manifest'],
             'Context_bootstrap_profile':profiles['arbiter-context-qwen3-17b']['manifest'],
             'cal_live_bindings_pending':str(work/'cal-live.bindings.json'),'cal_live_receipt_pending':str(work/'cal-live.receipt.json'),
             'cal_live_configure_is_not_applied':True,'owner':'Existing provision-static-onnx-runtime.py::run_owned must own run_composed execution after Root release.',
             'argv':['python','-B',str(DRIVER/'run_composed.py'),str(plan_path),str(output/'execution')],
             'next_owner':'QueryReview prepares actual CAL capture release only after genuine completed plan/Ready/current cal-live bindings.'}
    (output/'preparation.json').write_text(json.dumps(handoff,indent=2)+'\n',encoding='utf-8')
    return handoff

if __name__=='__main__':
    p=argparse.ArgumentParser()
    for arg in ('fixture-packet','setup-packet','setup-execution','semantic-ready-log','ready','node','output','catalog-projects','catalog-memberships','catalog-indexes'):p.add_argument('--'+arg,required=True)
    p.add_argument('--warm-seconds',type=int,default=30);p.add_argument('--whole-seconds',type=int,default=600)
    a=p.parse_args()
    if not 1<=a.warm_seconds<=a.whole_seconds<=1800:raise ValueError('invalid original reviewed continuation deadlines')
    print(json.dumps(prepare(a)))
