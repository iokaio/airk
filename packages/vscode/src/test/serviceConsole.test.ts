// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { ConsoleHost, ServiceConsoleController } from '../serviceConsoleController';
import { consoleAction, diagnosticPreview, ServiceConsoleState } from '../serviceConsoleView';
import { managementView } from '../explorerManagement';

/** What Explorer Management shows for a console state (the Service Console view was removed in 1.5.0). */
const shown = (state: ServiceConsoleState): string => JSON.stringify(managementView(state));
import { RecordValue, ServiceClient, ServiceError } from '../serviceClient';
const project='12'.repeat(32),namespace='34'.repeat(16),instance='56'.repeat(16),job='78'.repeat(32);
test('console workspace registration uses a trusted host action and highlights only native project bindings',async()=>{
    const f=fixture();let registrations=0;
    f.host.registerWorkspace=async()=>{registrations++;};f.host.currentProjects=()=>new Set([project]);
    await f.controller.refresh();const registration=managementView(f.controller.state);
    assert.equal(registration.enabled.registerWorkspace,true,'Management offers workspace registration');
    assert.ok(registration.projects.some(entry=>entry.summary.current),'the open workspace project is marked current');
    const forged={type:'service',revision:f.controller.state.revision,action:'registerWorkspace',root:'/untrusted'};
    await f.controller.action(forged);assert.equal(registrations,0);
    await f.action('registerWorkspace');assert.equal(registrations,1);
    f.setTrusted(false);await f.action('registerWorkspace');assert.equal(registrations,1);f.controller.dispose();
});

test('trusted rebuild commands create a saved guarded preview and never approve or execute it automatically',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),calls:unknown[]=[];
    const plan={id:'de'.repeat(32),preview_digest:'ef'.repeat(32),counts:{selected:1},state:'ready'};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='plan_create'){
            assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.service.intent.')));calls.push(structuredClone(method));return {status:'plan',plan};
        }
        if(name==='plan')return {status:'plan',plan};
        if(name==='plan_items')return {status:'plan_items',page:{plan,items:[{project}],after:null}};
        if(name==='plan_execute')throw new Error('Preview must never execute without separate approval');
        return base.transport.request(method);
    }});
    await f.controller.projectPlan(project,'rebuild');assert.equal(f.controller.state.message,undefined);
    assert.equal(f.controller.state.plan?.value.id,plan.id);assert.equal(calls.length,1);
    assert.equal((calls[0] as any).plan_create.request.operation,'rebuild');assert.deepEqual((calls[0] as any).plan_create.request.selection,{selected:[project]});
    f.setTrusted(false);await f.controller.projectPlan(project,'rebuild');assert.equal(calls.length,1);f.controller.dispose();
});
function fixture(){
    let trusted=true,fail=false;const sent:unknown[]=[],values=new Map<string,unknown>(),copied:string[]=[];
    const row={project:{registration:{binding:project,root:'/sensitive/<source>',data_root:'/private/data',collection:'<script>name</script>'},paused:false,fencing_generation:1},progress:null,inspection:null};
    const binding={installDir:'/install',controlDir:'/control',executable:'/executable',launcher:null,runtimeSha256:'90'.repeat(32),version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const client=new ServiceClient({dispose(){},async request(method){
        sent.push(structuredClone(method));const name=typeof method==='string'?method:Object.keys(method as object)[0];
        switch(name){
            case 'health':return {status:'health',observation:{instance,version:'1.4.0',user:'secret-user'}};
            case 'resources':return {status:'resources',observation:{configuration:{preset:'balanced'}}};
            case 'embedding_status':return {status:'embedding_status',available:false,loads:0};
            case 'source_status':return {status:'source_status',available:false};
            case 'request_epoch':return {status:'request_epoch',namespace,epoch:7};
            case 'console':case 'inactive_console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,projects:[row],after:null}};
            case 'workers':return {status:'workers',observations:[{project,identity:null,restarting:false,automatic_start_blocked:false}],age_ms:1};
            case 'events':assert.ok((method as {events:{limit:number}}).events.limit<=32);return {status:'events',page:{namespace,retained_prefix_lost:false,events:[],next:null,tail:{namespace,project:null,after:0,through:null}}};
            case 'enqueue':case 'configure_project':{
                assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1,'persist before send');
                if(fail)throw new ServiceError('console_transport_closed',true);
                return {status:'job',job:{id:job,project}};
            }
            default:return {status:name};
        }
    }});
    const host:ConsoleHost={version:'1.4.0',authority:'local-test',storage:{keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}},
        runtime:{discover:async()=>({bundle:{main:'/main',helper:'/helper',source:'/bundle',digest:binding.runtimeSha256,version:'1.4.0',platform:'test'},installDir:'/install',installed:true,inspection:{control_dir:'/control'},location:{}}),binding:()=>binding,ensure:async()=>binding,client:()=>client,snapshot:async()=>({}),repair:async()=>({}),prepareConfiguration:async()=>({incarnation:namespace,settings:'ab'.repeat(32),configuration:{operation:'vectors',enabled:false}}),
            reviewTransition:async(_binding,action)=>({action,installDir:binding.installDir,controlDir:binding.controlDir,observedActive:binding.runtimeSha256,expectedActive:binding.runtimeSha256,target:'cd'.repeat(32),version:'1.4.0'}),transition:async()=>({})},
        trusted:()=>trusted,changed(){},confirm:async()=>true,copy:async text=>{copied.push(text);},logs:async()=>{},resources:async()=>undefined,lifecycle:async()=>({status:'lifecycle'})};
    const controller=new ServiceConsoleController(host);
    const action=async(name:string,extra:RecordValue={})=>controller.action({type:'service',revision:controller.state.revision,action:name,...extra});
    return {controller,host,sent,values,copied,action,setTrusted:(value:boolean)=>{trusted=value;},setFail:(value:boolean)=>{fail=value;}};
}
test('console messages reject stale revisions, injected bodies and out-of-scope identities',async()=>{
    const f=fixture();await f.controller.refresh();const state=f.controller.state;
    for(const message of [
        {type:'service',revision:'old',action:'index',project},
        {type:'service',revision:state.revision,action:'index',project,command:'delete'},
        {type:'service',revision:state.revision,action:'index',project:'ff'.repeat(32)},
        {type:'service',revision:state.revision,action:'rpc',body:{shutdown:{}}},
    ])assert.equal(consoleAction(message,state),undefined);
    assert.ok(consoleAction({type:'service',revision:state.revision,action:'index',project},state));
    f.controller.dispose();
});

test('console binds sampled wait reasons and queued cancellation to observed jobs',async()=>{
    const f=fixture();await f.controller.refresh();const state=f.controller.state,row=state.page!.projects[0],worker=(state.workers!.observations as RecordValue[])[0];
    row.pending_job={id:job,project,kind:'index',state:'pending'};
    worker.waiting={job,reason:'extraction_capacity',age_ms:125};
    assert.match(shown(state),/Waiting for extraction capacity/);
    assert.equal(managementView(state).projects[0].pendingJobId,job,'Management offers cancellation for the observed queued job');
    const action={type:'service',revision:state.revision,action:'cancelJob',id:job};
    assert.ok(consoleAction(action,state));
    assert.equal(consoleAction({...action,id:'ff'.repeat(32)},state),undefined);
    assert.equal(consoleAction({...action,action:'retryJob'},state),undefined);
    worker.waiting={job:'ff'.repeat(32),reason:'reader_lease',age_ms:999};
    assert.doesNotMatch(shown(state),/Waiting for active readers/);
    row.pending_paused=true;
    assert.match(shown(state),/Queued work is paused by project policy/);
    row.pending_paused=false;
    const identity={service:instance,worker:namespace,project,fencing_generation:3};
    worker.identity=identity;
    worker.waiting={job,reason:'reader_lease',age_ms:50};
    row.pending_job=null;row.progress={worker:identity,current_fencing_generation:3,job:{id:job,state:'running'},progress:{phase:'semantic'}};
    assert.match(shown(state),/Waiting for active readers/);
    (row.progress as RecordValue).current_fencing_generation=4;
    assert.doesNotMatch(shown(state),/Waiting for active readers/);
    worker.waiting={job,reason:'<script>bad</script>',age_ms:50};
    assert.doesNotMatch(shown(state),/<script>bad/);
    f.controller.dispose();
});

test('project configuration is journaled before send and uncertain retry preserves the complete guarded request',async()=>{
    const f=fixture();let choices=0;
    f.host.configuration=async()=>{choices++;return {operation:'vectors',enabled:false};};
    await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};f.setFail(true);await f.action('configureProject',{project});
    assert.equal(f.controller.state.pending.length,1);
    const original=f.sent.find(value=>value&&typeof value==='object'&&'configure_project' in value);
    assert.ok(original);f.setFail(false);await f.action('retryIntent',{id:f.controller.state.pending[0].id});
    assert.equal(f.controller.state.pending.length,0);assert.equal(choices,1);
    assert.deepEqual(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value),[original,original]);
    f.controller.dispose();
});

test('trust loss during a project setting dialog prevents native preparation and submission',async()=>{
    const f=fixture();let prepared=false;f.host.configuration=async()=>{f.setTrusted(false);return {operation:'vectors',enabled:false};};
    f.host.runtime.prepareConfiguration=async()=>{prepared=true;throw new Error('Must not prepare');};
    await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};const before=f.sent.length;await f.action('configureProject',{project});
    assert.equal(prepared,false);assert.equal(f.sent.length,before);assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
});

test('configuration requires an inspection before opening host choices or preparing work',async()=>{
    const f=fixture();let prompted=false,prepared=false;
    f.host.configuration=async()=>{prompted=true;return {operation:'vectors',enabled:false};};
    f.host.runtime.prepareConfiguration=async()=>{prepared=true;throw new Error('Must inspect first');};
    await f.controller.refresh();await f.action('configureProject',{project});
    assert.equal(prompted,false);assert.equal(prepared,false);assert.equal(f.controller.state.pending.length,0);
    assert.match(f.controller.state.message!,/Inspect this project/);f.controller.dispose();
});

test('unregister binds the prepared project and requires approval before durable dispatch',async()=>{
    for(const approved of [false,true]){
        const f=fixture();let reviewed='';
        f.host.configuration=async()=>({operation:'unregister'});
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{
            assert.equal(selected,project);return {incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft};
        };
        f.host.confirm=async message=>{reviewed=message;return approved;};
        await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};
        await f.action('configureProject',{project});
        assert.match(reviewed,/Unregister/);assert.match(reviewed,/Source: \/sensitive/);assert.match(reviewed,/Sources, settings and index data are retained/);
        assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,approved?1:0);
        assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
    }
    const f=fixture();f.host.configuration=async()=>({operation:'unregister'});
    f.host.runtime.prepareConfiguration=async(_binding,_project,draft)=>({incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft});
    f.host.confirm=async()=>{f.setTrusted(false);return true;};
    await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};
    await f.action('configureProject',{project});
    assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,0);
    assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
});

test('legacy startup controls require a separate reviewed scope and approval, preserve trust and resume only listed receipts',async()=>{
    for(const approval of ['decline','approve','lose-trust'] as const){
        const f=fixture();let removed=0,confirmed='';
        const target={root:'/old-source',data_dir:'/old-data',collection:'old-fixture',executable:'/old/airk'},key='ab'.repeat(32);
        const reviewed:any={installDir:'/install',controlDir:'/control',runtimeSha256:'90'.repeat(32),target,approval:key,value:{approval:key,preview:{spec:{id:'airk-fixture',target},observation:{owned:true,definition_matches:true}},workers:{matching_executable_pids:[123],complete:false,unobserved:1}}};
        f.host.legacy={choose:async()=>target,review:async()=>reviewed,load:async(_binding,id)=>{assert.equal(id,key);return {...reviewed,phase:'definition_removed'};},list:async()=>({items:[{approval:key,service_id:'airk-fixture',phase:'definition_removed'}]}),remove:async(_binding,review)=>{assert.equal(review.approval,key);removed++;return {startup_removed:true};}};
        f.host.confirm=async message=>{confirmed=message;if(approval==='lose-trust')f.setTrusted(false);return approval!=='decline';};
        await f.controller.refresh();
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'removeLegacyStartup'},f.controller.state),undefined);
        await f.action('reviewLegacyStartup');assert.equal(removed,0);assert.ok(f.controller.state.legacyReview);
        assert.equal(managementView(f.controller.state).legacy?.reviewed,true);
        await f.action('removeLegacyStartup');assert.match(confirmed,/Source: \/old-source/);assert.match(confirmed,/project settings and old indexes stay in place/);
        assert.equal(removed,approval==='approve'?1:0);
        f.setTrusted(true);await f.action('legacyStartupReceipts');
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'resumeLegacyStartup',id:'ef'.repeat(32)},f.controller.state),undefined);
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'resumeLegacyStartup',id:key,target:{root:'/injected'}},f.controller.state),undefined);
        f.host.confirm=async()=>false;await f.action('resumeLegacyStartup',{id:key});assert.equal(f.controller.state.legacyReview?.phase,'definition_removed');
        assert.equal(removed,approval==='approve'?1:0);f.controller.dispose();
    }
});

test('inactive cleanup requires a completed exact preview and host approval before journaled dispatch',async()=>{
    for(const approval of ['decline','approve','lose-trust'] as const){
        const f=fixture();let reviewed='';
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{assert.equal(selected,project);return {incarnation:namespace,settings:project,configuration:draft};};
        await f.controller.refresh();
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'deleteDerived',project},f.controller.state),undefined);
        await f.action('inactiveProjects');assert.equal(f.controller.state.inactiveProjects,true);
        assert.ok(f.sent.some(value=>value&&typeof value==='object'&&'inactive_console' in value));
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'index',project},f.controller.state),undefined);
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'repair',scope:'all',operation:'auto'},f.controller.state),undefined);
        await f.action('deleteDerived',{project});assert.equal(f.controller.state.message,'cleanup_preview_required');
        const row=f.controller.state.page!.projects[0];row.configuration={id:job,project,kind:'configure',state:'succeeded'};
        f.controller.state.configurationResults=new Map([[project,{job,jobState:'succeeded',checkedAt:1,value:{active:instance,guard:'ab'.repeat(32),owned_files:9,owned_bytes:123,inventory_complete:false}}]]);
        f.host.confirm=async message=>{reviewed=message;if(approval==='lose-trust')f.setTrusted(false);return approval!=='decline';};
        await f.action('deleteDerived',{project});
        assert.match(reviewed,/Source: \/sensitive/);assert.match(reviewed,/bounded lower bounds/);assert.match(reviewed,/Saved settings and original sources are preserved/);
        const sent=f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value) as any[];
        assert.equal(sent.length,approval==='approve'?1:0);
        if(sent.length)assert.deepEqual(sent[0].configure_project.request.configuration,{operation:'delete_derived',preview:'ab'.repeat(32)});
        assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
    }
});

test('usage configuration waits for native completion and preserves a lost acknowledgement without replay',async()=>{
    for(const lost of [false,true]){
        const f=fixture(),base=f.host.runtime.client({} as any),inspect='ab'.repeat(32);let configured=0;
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{
            assert.equal(selected,project);return {incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft};
        };
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const name=typeof method==='string'?method:Object.keys(method as object)[0];
            if(name==='enqueue')return {status:'job',job:{id:inspect,project,kind:'inspect'}};
            if(name==='configure_project'){
                assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.service.intent.')));configured++;
                if(lost)throw new ServiceError('console_transport_closed',true);
                return {status:'job',job:{id:job,project,kind:'configure'}};
            }
            if(name==='job'){
                const id=(method as any).job.id;return {status:'job',job:{id,project,kind:id===inspect?'inspect':'configure',outcome:{state:'succeeded'}}};
            }
            if(name==='configuration_result')return {status:'configuration',job:{id:job,project,kind:'configure'},result:{cleared:true}};
            return base.transport.request(method);
        }});
        if(lost)await assert.rejects(f.controller.projectConfiguration(project,{operation:'clear_usage'}),ServiceError);
        else await f.controller.projectConfiguration(project,{operation:'clear_usage'});
        assert.equal(configured,1);
        assert.equal(f.controller.state.pending.length,lost?1:0);
        await f.controller.refresh();assert.equal(configured,1,'read-only refresh must not replay a clear request');
        f.controller.dispose();
    }
});

test('closing or changing the usage scope during inspection prevents configuration dispatch',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any);let valid=true,prepared=false,configured=false;
    f.host.runtime.prepareConfiguration=async()=>{prepared=true;throw new Error('Stale selection');};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='enqueue')return {status:'job',job:{id:job,project,kind:'inspect'}};
        if(name==='job'){valid=false;return {status:'job',job:{id:job,project,kind:'inspect',outcome:{state:'succeeded'}}};}
        if(name==='configure_project'){configured=true;throw new Error('Must not dispatch');}
        return base.transport.request(method);
    }});
    await assert.rejects(f.controller.projectConfiguration(project,{operation:'clear_usage'},()=>valid));
    assert.equal(prepared,false);assert.equal(configured,false);f.controller.dispose();
});

test('disconnected snapshots retain bounded configuration results and Copy uses the exact saved value',async()=>{
    const f=fixture(),observation=await f.host.runtime.discover(),binding=await f.host.runtime.ensure();binding.payloadVerified=false;
    const result={summary:{baseline_tokens:42,returned_tokens:12},notice:'Explicit sample counts'};
    const row={project:{registration:{binding:project,collection:'fixture',root:'/source',data_root:'/data'}},
        configuration:{id:job,project,kind:'configure',state:'succeeded'}};
    observation.inspection={control_dir:'/control',payload_verified:false,registry_namespace:namespace,
        console:{revision:1,cached:true,observed_unix_ms:1,projects:[row],after:null},
        configuration_results:[{project,job,result,unavailable:false}]};
    observation.inspection.configuration_results_omitted=true;
    f.host.runtime.discover=async()=>observation;
    f.host.runtime.client=()=>{throw new Error('No selected executable may run');};
    f.host.runtime.snapshot=async()=>observation.inspection!;
    await f.controller.refresh();assert.equal(f.controller.state.connected,false);
    assert.deepEqual(f.controller.state.configurationResults?.get(project)?.value,result);
    await f.action('copyConfiguration',{project});assert.equal(f.copied[0],JSON.stringify(result,null,2));
    const cached=managementView(f.controller.state);
    assert.equal(cached.projects[0].configurationResult,true,'the cached configuration result stays available');
    assert.equal(cached.configurationResultsOmitted,true);
    const report=JSON.parse(diagnosticPreview(f.controller.state,false));
    assert.equal(report.configurationResultsOmitted,true);
    assert.equal(report.projects[0].configuration.id,job);
    assert.equal(report.projects[0].configurationResult.job,job);
    assert.doesNotMatch(JSON.stringify(report),/baseline_tokens|Explicit sample counts/);
    f.controller.dispose();
});
test('console persists before enqueue and exposes exact-key recovery after an uncertain outcome',async()=>{
    const f=fixture();await f.controller.refresh();f.setFail(true);await f.action('index',{project});
    assert.equal(f.controller.state.pending.length,1);const original=f.sent.find(value=>typeof value==='object'&&value!==null&&'enqueue' in value);
    f.setFail(false);await f.action('retryIntent',{id:f.controller.state.pending[0].id});
    assert.equal(f.controller.state.pending.length,0);
    assert.deepEqual(f.sent.filter(value=>typeof value==='object'&&value!==null&&'enqueue' in value),[original,original]);
    f.controller.dispose();
});
test('trust loss refuses native actions while reports remain previewable and redact source paths',async()=>{
    const f=fixture();await f.controller.refresh();const before=f.sent.length;f.setTrusted(false);
    await f.action('index',{project});assert.equal(f.sent.length,before);
    await f.action('previewReport',{includePaths:false});
    assert.ok(f.controller.state.preview);assert.doesNotMatch(f.controller.state.preview!,/sensitive|secret-user|private\/data|script>name/);
    await f.action('copyReport');assert.equal(f.copied.length,1);assert.equal(f.sent.length,before);
    f.controller.dispose();
});
test('an external service manager disables lifecycle actions and the full report keeps paths only on request',async()=>{
    const f=fixture();await f.controller.refresh();f.controller.state.lifecycle='external';
    const view=managementView(f.controller.state);
    assert.deepEqual([view.enabled.start,view.enabled.stop,view.enabled.restart],[false,false,false]);
    assert.equal(view.lifecycle,'external');
    const text=diagnosticPreview(f.controller.state,true);assert.match(text,/sensitive/);assert.match(text,/secret-user/);
    assert.ok(Buffer.byteLength(text)<=65536);f.controller.dispose();
});
test('bounded diagnostic export records truncation without source, credentials or raw errors',()=>{
    const f=fixture(),state=f.controller.state as ServiceConsoleState;
    state.events=Array.from({length:128},(_,sequence)=>({sequence,kind:'x'.repeat(512),code:'y'.repeat(512),query:'private query',token:'bearer-secret'}));
    const report=diagnosticPreview(state,false);assert.ok(Buffer.byteLength(report)<=65536);assert.doesNotMatch(report,/private query|bearer-secret/);
    f.controller.dispose();
});
test('background refresh preserves the exact report reviewed before Copy',async()=>{
    const f=fixture();await f.controller.refresh();await f.action('previewReport',{includePaths:false});
    const preview=f.controller.state.preview;assert.ok(preview);
    await f.controller.refresh();assert.equal(f.controller.state.preview,preview);
    await f.action('copyReport');assert.equal(f.copied[0],preview);f.controller.dispose();
});

test('damaged payload uses cached pages and persists an instance-fenced Stop without launching it',async()=>{
    const f=fixture(),binding=await f.host.runtime.ensure();binding.payloadVerified=false;
    const page=(id:string,after:string|null)=>({revision:3,cached:true,observed_unix_ms:10,
        projects:[{project:{registration:{binding:id,collection:'offline',root:'/missing',data_root:'/saved'},paused:false}}],after});
    const second='ab'.repeat(32),requests:unknown[]=[];
    const lifecycle={recorded_instance:instance,namespace,epoch:7,pending:null,explicitly_stopped:false};
    const observation=await f.host.runtime.discover();
    observation.inspection={schema:1,metadata_only:true,payload_verified:false,control_dir:'/control',lifecycle,registry_namespace:namespace,console:page(project,project)};
    f.host.runtime.discover=async()=>observation;
    f.host.runtime.client=()=>{throw new Error('Damaged payload must never execute');};
    f.host.runtime.snapshot=async(_binding,after,revision)=>{
        requests.push({after,revision});return {...observation.inspection,console:after?page(second,null):page(project,project)};
    };
    f.host.lifecycle=async(_binding,intent)=>{
        assert.equal(f.controller.state.pending.length,1,'durably saved before native Stop');
        assert.equal(intent.lifecycle.action,'stop');assert.equal(intent.lifecycle.expected_instance,instance);
        assert.equal(intent.lifecycle.key.namespace,namespace);assert.equal(intent.lifecycle.key.epoch,7);
        return {status:'lifecycle',record:{phase:'complete',request:{...intent.lifecycle,manager:false}}};
    };
    await f.controller.refresh();assert.equal(f.controller.state.connected,false);
    assert.equal(f.controller.state.page?.projects[0].project&&((f.controller.state.page.projects[0].project as RecordValue).registration as RecordValue).binding,project);
    assert.equal(managementView(f.controller.state).enabled.stop,true,'a recorded instance can still be stopped from cached state');
    await f.action('nextPage');assert.deepEqual(requests.at(-1),{after:project,revision:3});
    assert.equal(((f.controller.state.page!.projects[0].project as RecordValue).registration as RecordValue).binding,second);
    await f.action('stop');assert.equal(f.controller.state.pending.length,0);assert.equal(f.controller.state.message,undefined);
    assert.equal((f.controller.state.bootstrap.value?.lifecycle as RecordValue).explicitly_stopped,true);
    assert.equal(f.sent.length,0);f.controller.dispose();
});

test('a failed health check changes previously live resource and work observations to cached',async()=>{
    const f=fixture();await f.controller.refresh();assert.equal(f.controller.state.resources.state,'live');
    f.host.runtime.client=()=>new ServiceClient({dispose(){},request:async()=>{throw new ServiceError('service_down');}});
    await f.action('connect');assert.equal(f.controller.state.connected,false);
    assert.equal(f.controller.state.resources.state,'cached');assert.equal(f.controller.state.model.state,'cached');
    const html=shown(f.controller.state);assert.match(html,/Cached sample/);assert.doesNotMatch(html,/Current generation/);
    f.controller.dispose();
});

test('runtime repair requires Stop and approves only the native reviewed preview',async()=>{
    const f=fixture(),calls:(string|undefined)[]=[],preview='ab'.repeat(32);
    f.host.runtime.repair=async(_binding,token)=>{calls.push(token);return token?{repaired:true}:{preview,preserved_files:5,preserved_bytes:123,preserved_directory:'/preserved'};};
    await f.controller.refresh();await f.action('repairRuntime');
    assert.equal(f.controller.state.message,'stop_service_before_payload_repair');assert.deepEqual(calls,[]);
    const observation=await f.host.runtime.discover();observation.inspection={control_dir:'/control',lifecycle:{explicitly_stopped:true}};
    f.host.runtime.discover=async()=>observation;
    await f.action('connect');f.host.confirm=async()=>false;await f.action('repairRuntime');assert.deepEqual(calls,[undefined]);
    f.host.confirm=async message=>{assert.match(message,/123 bytes/);assert.match(message,new RegExp(preview));return true;};
    await f.action('repairRuntime');assert.deepEqual(calls,[undefined,undefined,preview]);
    assert.match(f.controller.state.message!,/Runtime payload repaired/);
    assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'repairRuntime',preview},f.controller.state),undefined);
    f.controller.dispose();
});

test('runtime activation reviews the exact target and requires host confirmation before applying it',async()=>{
    const f=fixture();let applied=0;
    const review=await f.host.runtime.reviewTransition(await f.host.runtime.ensure(),'upgrade');
    f.host.runtime.reviewTransition=async()=>review;
    f.host.runtime.transition=async(_binding,actual)=>{assert.equal(actual,review);applied++;return {schema:1,active:review.target};};
    await f.controller.refresh();f.host.confirm=async message=>{assert.match(message,new RegExp(review.target));assert.match(message,/changed selection invalidates/);return false;};
    await f.action('upgradeRuntime');assert.equal(applied,0);
    f.host.confirm=async()=>true;await f.action('upgradeRuntime');assert.equal(applied,1);
    assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'upgradeRuntime',target:review.target},f.controller.state),undefined);
    f.controller.dispose();
});

test('a different native version keeps installer observations without executing its console',async()=>{
    const f=fixture(),binding=await f.host.runtime.ensure();binding.version='1.4.1';
    f.host.runtime.client=()=>{throw new Error('Mismatched console must not execute');};
    f.host.runtime.snapshot=async()=>({selection:{active:binding.runtimeSha256},approval:{version:'1.4.1',verification_basis:'trusted_distribution'},payload_verified:false});
    await f.controller.refresh();assert.equal(f.controller.state.connected,false);
    assert.match(f.controller.state.message!,/pin is preserved/);
    assert.equal(f.controller.state.bootstrap.value?.selectedVersion,'1.4.1');
    assert.equal(managementView(f.controller.state).enabled.upgradeRuntime,true,'Management offers the bundled runtime upgrade review');
    assert.equal(f.sent.length,0);f.controller.dispose();
});

test('trust loss during activation confirmation prevents mutation and unknown activation exposes its saved native phase',async()=>{
    const f=fixture();let applied=0;
    f.host.runtime.transition=async()=>{applied++;throw new ServiceError('native_activation_unknown',true);};
    await f.controller.refresh();f.host.confirm=async()=>{f.setTrusted(false);return true;};
    await f.action('rollbackRuntime');assert.equal(applied,0);
    f.setTrusted(true);f.host.confirm=async()=>true;
    const observation=await f.host.runtime.discover();observation.diagnosis={pending_activation:{candidate:'cd'.repeat(32),phase:'probe'}};
    f.host.runtime.discover=async()=>observation;
    await f.action('rollbackRuntime');assert.equal(applied,1);
    assert.equal(f.controller.state.message,'native_activation_unknown');
    assert.equal(managementView(f.controller.state).enabled.resumeActivation,true,'Management offers the saved activation review');
    assert.equal(JSON.parse(diagnosticPreview(f.controller.state,false)).runtimeActivation.phase,'probe');
    f.controller.dispose();
});

test('trust lost while native repair preview loads prevents approval and mutation',async()=>{
    const f=fixture(),observation=await f.host.runtime.discover();observation.inspection={control_dir:'/control',lifecycle:{explicitly_stopped:true}};
    f.host.runtime.discover=async()=>observation;let calls=0,confirmed=false;
    f.host.confirm=async()=>{confirmed=true;return true;};
    f.host.runtime.repair=async()=>{calls++;f.setTrusted(false);return {preview:'ab'.repeat(32)};};
    await f.controller.refresh();await f.action('repairRuntime');
    assert.equal(calls,1);assert.equal(confirmed,false);assert.equal(f.controller.state.message,'runtime_client_no_longer_authorized');f.controller.dispose();
});
