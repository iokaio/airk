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
        runtime:{discover:async()=>({bundle:{main:'/main',helper:'/helper',source:'/bundle',digest:binding.runtimeSha256,version:'1.4.0',platform:'test'},installDir:'/install',installed:true,inspection:{},location:{}}),binding:()=>binding,ensure:async()=>binding,client:()=>client},
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
