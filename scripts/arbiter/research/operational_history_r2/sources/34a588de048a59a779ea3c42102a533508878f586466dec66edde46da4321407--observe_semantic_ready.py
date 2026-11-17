# SPDX-License-Identifier: Apache-2.0
"""Root-only actual metadata observation, using the existing command owner.

One original setup deadline; successful status observations, then one diagnostics
call. No model-scoring probe, command resubmission, sleep or failed-call retry.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import time
from prepare_post_index import ROOT,pin,verify,json_lines,setup_index,semantic_ready,same_physical_path
EVENT_HELPER=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102/registry_events.py'

def remaining(deadline_ns,clock=time.monotonic_ns):
    value=(deadline_ns-clock())/1_000_000_000
    if value<=0:raise TimeoutError('original index/setup deadline exhausted; no observation launched')
    return value

def canonical(log):
    values=[]
    for row in json_lines(log):
        content=row.get('content')
        if not isinstance(content,list):continue
        if row.get('isError') or len(content)!=1 or content[0].get('type')!='text':
            raise ValueError('actual canonical metadata response refused/malformed')
        value=json.loads(content[0]['text'])
        if not isinstance(value,dict):raise ValueError('actual canonical metadata document not object')
        values.append(value)
    if len(values)!=1:raise ValueError('exactly one original canonical metadata envelope required')
    return values[0]

def phase(value):
    if value.get('index_state')!='available':raise ValueError('actual published index unavailable')
    state=value['semantic_readiness']['state'];discovery=value['discovery_readiness']['state']
    if state not in ('pending','partial','ready') or discovery not in ('pending','partial','ready'):
        raise ValueError('native semantic/discovery failed/disabled/unknown; no retry')
    return state=='ready' and discovery=='ready'

def observe(args):
    execution=json.loads(Path(args.setup_execution).read_text(encoding='utf-8'))
    setup=json.loads(Path(args.setup_packet).read_text(encoding='utf-8'))
    fixture=json.loads(Path(args.fixture_packet).read_text(encoding='utf-8'))
    if execution.get('complete') is not True or execution['phase']!='index' or execution['packet_sha256']!=pin(args.setup_packet)['sha256']:
        raise ValueError('completed original setup execution/packet required')
    deadline=execution['original_deadline_monotonic_ns'];remaining(deadline)
    if setup['fixture_packet']['sha256']!=pin(args.fixture_packet)['sha256'] or not fixture.get('semantic_plan'):
        raise ValueError('original fixture/native semantic plan association absent')
    refs=[fixture['image'],fixture['owner'],fixture['semantic_plan'],setup['data_bindings'],*setup['runtime_images'],*setup['runtime_metadata']]
    for ref in refs:verify(ref);remaining(deadline)
    data=json.loads(Path(setup['data_bindings']['path']).read_text(encoding='utf-8'))['partitions']['CAL']
    selected=[x for x in execution['commands'] if x['label']=='actual-index-setup']
    if len(selected)!=1:raise ValueError('unique actual native index setup record required')
    verify(selected[0]['log'])
    index=setup_index(selected[0]['log']['path'],source_root=data['source_root'],collection='selected-context-cal-r1')
    pipeline=json.loads(Path(fixture['semantic_plan']['path']).read_text(encoding='utf-8'))['object_plan']['pipeline_digest']
    ready=json.loads(Path(args.ready).read_text(encoding='utf-8'));work=Path(fixture['work'])
    if not same_physical_path(args.ready,work/'ready.json') or not same_physical_path(ready['control_root'],work/'control') or ready['inputs_sha256']!=fixture['inputs']['sha256'] or ready['health']['executable_sha256']!=fixture['image']['sha256']:
        raise ValueError('actual original fixture Ready changed')
    ctl=[x['path'] for x in setup['runtime_images'] if Path(x['path']).name=='airkctl.exe']
    if len(ctl)!=1:raise ValueError('actual matched client absent')
    output=Path(args.output).resolve()
    if not output.is_relative_to(ROOT/'artifacts') or not output.parent.is_dir():raise ValueError('fresh private artifacts output under existing parent required')
    output.mkdir(exist_ok=False)
    spec=importlib.util.spec_from_file_location('existing_owned_primitive',fixture['owner']['path'])
    owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
    calls=[];receipt={'schema':'airk.private-selected-cal-semantic-observation/1','complete':False,'qualification_granted':False,
        'setup_execution':pin(args.setup_execution),'setup_packet':pin(args.setup_packet),'fixture_packet':pin(args.fixture_packet),
        'ready':pin(args.ready),'source_script':pin(__file__),'original_deadline_monotonic_ns':deadline,'index_id':index,
        'native_pipeline_digest':pipeline,'calls':calls,'event_helper':pin(EVENT_HELPER),'event_waits':[],'event_handle_closed':False}
    def call(tool,label):
        budget=remaining(deadline)
        argv=[ctl[0],'--service-runtime-dir',str(work/'control'),'tools','call',tool,'--index',index,'--args','{"response_mode":"full"}']
        log=output/(label+'.log');began=time.time_ns()//1_000_000
        result=owner.run_owned(argv,ROOT,log,min(budget,30),dict(os.environ))
        calls.append({'tool':tool,'argv':argv,'started_unix_ms':began,'completed_unix_ms':time.time_ns()//1_000_000,
                      'owned':result,'log':pin(log)})
        if not owner.successful(result):raise ValueError('actual metadata command failed/unsettled; no retry')
        remaining(deadline)
        return canonical(log),log
    events=None
    try:
        event_spec=importlib.util.spec_from_file_location('reviewed_registry_events',EVENT_HELPER)
        event_module=importlib.util.module_from_spec(event_spec);event_spec.loader.exec_module(event_module)
        events=event_module.RegistryEvents(work/'control')
        for ordinal in range(args.maximum_observations):
            value,_=call('status',f'{ordinal:03d}-status')
            if value.get('collection')!='selected-context-cal-r1':raise ValueError('actual status collection changed')
            members=value['scope_observation']['members']
            if len(members)!=1 or members[0]['index_id']!=index:raise ValueError('actual status original index changed')
            if phase(value):break
            if ordinal+1<args.maximum_observations:
                remaining(deadline)
                began=time.time_ns()//1_000_000
                events.wait(deadline)
                receipt['event_waits'].append({'started_unix_ms':began,'completed_unix_ms':time.time_ns()//1_000_000,
                    'authority':False,'meaning':'Actual registry directory change wakeup only; next native canonical status remains authority.'})
        else:raise ValueError('bounded successful metadata observation roster exhausted')
        full,diagnostic=call('diagnostics','full-diagnostics')
        semantic_ready(diagnostic,index,pipeline,'selected-context-cal-r1')
        if full['files']!=data['source_files'] or full['indexed_vectors']<=0 or full['vectors_enabled'] is not True:
            raise ValueError('actual indexed source census/vector publication differs from complete CAL fixture')
        receipt['full_diagnostics']=pin(diagnostic)
        for ref in refs:verify(ref);remaining(deadline)
        verify(receipt['event_helper']);remaining(deadline)
        receipt['complete']=True
    finally:
        try:
            if events is not None:
                events.close();receipt['event_handle_closed']=True
        except BaseException:
            receipt['complete']=False
            raise
        finally:
            receipt['remaining_original_seconds']=max(0,(deadline-time.monotonic_ns())/1_000_000_000)
            if receipt['remaining_original_seconds']<=0:receipt['complete']=False
            (output/'semantic-observation.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
        remaining(deadline)
    print(json.dumps(receipt))

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    for name in ('setup-execution','setup-packet','fixture-packet','ready','output'):parser.add_argument('--'+name,required=True)
    parser.add_argument('--maximum-observations',type=int,default=512)
    args=parser.parse_args()
    if not 1<=args.maximum_observations<=1024:raise ValueError('bounded original status observer required')
    observe(args)
