// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { ConsoleHost, ServiceConsoleController } from '../serviceConsoleController';
import { consoleAction, diagnosticPreview, renderConsole, ServiceConsoleState } from '../serviceConsoleView';
import { RecordValue, ServiceClient, ServiceError } from '../serviceClient';
const project='12'.repeat(32),namespace='34'.repeat(16),instance='56'.repeat(16),job='78'.repeat(32);
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
            case 'console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,projects:[row],after:null}};
            case 'workers':return {status:'workers',observations:[{project,identity:null,restarting:false,automatic_start_blocked:false}],age_ms:1};
            case 'events':assert.ok((method as {events:{limit:number}}).events.limit<=32);return {status:'events',page:{namespace,retained_prefix_lost:false,events:[],next:null,tail:{namespace,project:null,after:0,through:null}}};
            case 'enqueue':{
                assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1,'persist before send');
                if(fail)throw new ServiceError('console_transport_closed',true);
                return {status:'job',job:{id:job,project}};
            }
            default:return {status:name};
        }
    }});
    const host:ConsoleHost={version:'1.4.0',authority:'local-test',storage:{keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}},
        runtime:{discover:async()=>({bundle:{main:'/main',helper:'/helper',source:'/bundle',digest:binding.runtimeSha256,version:'1.4.0',platform:'test'},installDir:'/install',installed:true,inspection:{control_dir:'/control'},location:{}}),binding:()=>binding,ensure:async()=>binding,client:()=>client,snapshot:async()=>({}),repair:async()=>({})},
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
test('rendering escapes project content, isolates script execution and disables external lifecycle actions',async()=>{
    const f=fixture();await f.controller.refresh();f.controller.state.lifecycle='external';
    const html=renderConsole(f.controller.state);
    assert.match(html,/default-src 'none'/);assert.doesNotMatch(html,/<script>name<\/script>/);
    assert.match(html,/&lt;script&gt;name&lt;\/script&gt;/);
    assert.match(html,/data-action="stop"[^>]*disabled/);assert.match(html,/external service manager/);
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
    assert.doesNotMatch(renderConsole(f.controller.state),/data-action="stop"[^>]*disabled/);
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
    const html=renderConsole(f.controller.state);assert.match(html,/Cached sample/);assert.doesNotMatch(html,/Current generation/);
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

test('trust lost while native repair preview loads prevents approval and mutation',async()=>{
    const f=fixture(),observation=await f.host.runtime.discover();observation.inspection={control_dir:'/control',lifecycle:{explicitly_stopped:true}};
    f.host.runtime.discover=async()=>observation;let calls=0,confirmed=false;
    f.host.confirm=async()=>{confirmed=true;return true;};
    f.host.runtime.repair=async()=>{calls++;f.setTrusted(false);return {preview:'ab'.repeat(32)};};
    await f.controller.refresh();await f.action('repairRuntime');
    assert.equal(calls,1);assert.equal(confirmed,false);assert.equal(f.controller.state.message,'runtime_client_no_longer_authorized');f.controller.dispose();
});
