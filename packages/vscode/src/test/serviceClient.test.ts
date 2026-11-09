// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { ControlTransport, NativeControlTransport, ServiceClient, ServiceError, RecordValue } from '../serviceClient';
import {isNativeReplyValidationFailure,isLocalTrackingFailure} from '../nativeOutcome';
const project='1'.repeat(64), other='2'.repeat(64), incarnation='3'.repeat(32);

for(const family of ['job','catalog'] as const)test(`malformed ${family} mutation acknowledgment keeps its raw frame strictly diagnostic`,async()=>{
    const key={namespace:incarnation,epoch:1,nonce:'4'.repeat(32)},caller='5'.repeat(32),digest='6'.repeat(64);
    const reply:RecordValue=family==='job'?{status:'job',job:{id:17,project},future_native:{all:'retained'}}:{status:'catalog_applied',receipt:{schema:1,operation_id:17,caller,key,request_digest:digest,catalog_revision:2,authority_fence_required:false},future_native:{all:'retained'}};
    let calls=0;const client=new ServiceClient({dispose(){},async request(){calls++;return reply;}});
    const work=family==='job'?client.submitMutation({enqueue:{project,caller,key,kind:'index'}}):client.submitCatalog({catalog_apply:{request:{caller,key,expected_catalog_revision:1,operation:{operation:'rename_project',id:'prj_'+'7'.repeat(32),expected_definition_revision:1,name:'Reviewed'}},preview_digest:digest},request_digest:digest});
    await assert.rejects(work,(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.equal(isNativeReplyValidationFailure(error),true);assert.equal(isLocalTrackingFailure(error),false);assert.deepEqual(error.nativeResponse,reply);assert.ok(error.cause instanceof Error);return true;});assert.equal(calls,1);
});
const fixture=String.raw`
let pending=Buffer.alloc(0);
process.stdin.on('data',bytes=>{
 pending=Buffer.concat([pending,bytes]);
 while(pending.length>=4&&pending.length>=4+pending.readUInt32BE()){
  const length=pending.readUInt32BE();const request=JSON.parse(pending.subarray(4,4+length));pending=pending.subarray(4+length);
  if(process.env.AIRK_CONSOLE_TEST_MODE==='stall')continue;
  if(process.env.AIRK_CONSOLE_TEST_MODE==='oversize'){const prefix=Buffer.alloc(4);prefix.writeUInt32BE(65537);process.stdout.write(prefix);continue;}
  const response=process.env.AIRK_CONSOLE_TEST_REPLY?JSON.parse(process.env.AIRK_CONSOLE_TEST_REPLY):{status:'fixture',echo:request.method};
  const body=Buffer.from(JSON.stringify({id:process.env.AIRK_CONSOLE_TEST_MODE==='crossed'?'f'.repeat(32):request.id,response}));
  const prefix=Buffer.alloc(4);prefix.writeUInt32BE(body.length);
  process.stdout.write(prefix.subarray(0,2));
  setTimeout(()=>{process.stdout.write(prefix.subarray(2));process.stdout.write(body.subarray(0,3));process.stdout.write(body.subarray(3));},5);
 }
});
`;
function transport(mode='echo',timeout=2000,reply?:RecordValue): NativeControlTransport {
    return new NativeControlTransport('unused',[],timeout,()=>spawn(process.execPath,['-e',fixture],
        {stdio:'pipe',windowsHide:true,env:{...process.env,AIRK_CONSOLE_TEST_MODE:mode,...(reply?{AIRK_CONSOLE_TEST_REPLY:JSON.stringify(reply)}:{})}}));
}
test('native console frames retain request identity across split headers and bodies',async()=>{
    const pipe=transport();
    try {
        const [first,second]=await Promise.all([pipe.request('health'),pipe.request({workers:{projects:[project]}})]);
        assert.equal(first.echo,'health');assert.deepEqual(second.echo,{workers:{projects:[project]}});
    } finally {pipe.dispose();}
});
test('crossed identities and oversized response headers close the console without resubmission',async()=>{
    for(const mode of ['crossed','oversize']){
        const pipe=transport(mode);
        try{
            await assert.rejects(pipe.request('health'),error=>error instanceof ServiceError&&error.code==='invalid_console_response'&&error.outcomeUnknown);
            await assert.rejects(pipe.request('health'),error=>error instanceof ServiceError&&error.code==='console_transport_closed'&&!error.outcomeUnknown);
        } finally {pipe.dispose();}
    }
});
test('console queue and timeout distinguish uncertain in-flight work from unsent requests',async()=>{
    const pipe=transport('stall',100);
    try{
        const requests=Array.from({length:8},()=>pipe.request('health'));
        const completed=Promise.allSettled(requests);
        await assert.rejects(pipe.request('health'),error=>error instanceof ServiceError&&error.code==='console_queue_full'&&!error.outcomeUnknown);
        const results=await completed;
        results.forEach((result,index)=>{assert.equal(result.status,'rejected');if(result.status==='rejected'){
            assert.ok(result.reason instanceof ServiceError);assert.equal(result.reason.outcomeUnknown,index===0);
        }});
    } finally {pipe.dispose();}
});
test('console input bounds count UTF-8 bytes before sending any request',async()=>{
    const pipe=transport();
    try{
        await assert.rejects(pipe.request({value:'é'.repeat(32768)}),error=>error instanceof ServiceError&&error.code==='console_request_oversize'&&!error.outcomeUnknown);
        assert.equal((await pipe.request('health')).echo,'health');
    } finally {pipe.dispose();}
});
class FakeTransport implements ControlTransport {
    calls:unknown[]=[]; reply:RecordValue={status:'request_epoch',namespace:incarnation,epoch:1};
    async request(method:unknown):Promise<RecordValue>{this.calls.push(structuredClone(method));return this.reply;}
    dispose():void{}
}

test('management adapter retains complete failures without multiplying native retry budgets',async()=>{
    const method={catalog_source_mutation_draft:{selection:{index_id:'idx_'+'ab'.repeat(16),key:{nonce:'cd'.repeat(16)}}}};
    for(const request of [method,'request_epoch',{catalog_apply:{request:{same:'saved'}}}]){
        for(const code of ['registry_busy','invalid_registry_request','registry_response_unknown_retry_with_key']){
            const calls:unknown[]=[],response={status:'failure',code,native_detail:{retained:true}};
            const client=new ServiceClient({request:async value=>{calls.push(value);return response;},dispose(){}});
            await assert.rejects(client.rpc(request),error=>error instanceof ServiceError&&error.nativeResponse===response);
            assert.deepEqual(calls,[request]);assert.equal(calls[0],request,'Native budget exhaustion preserves the exact original request');
        }
    }
});
test('prepared durable mutations preserve the exact key after an unknown registry outcome',async()=>{
    const fake=new FakeTransport(),client=new ServiceClient(fake);
    const intent=await client.prepareJob(project,'index');
    fake.reply={status:'failure',code:'registry_response_unknown_retry_with_key'};
    await assert.rejects(client.submitJob(intent),error=>error instanceof ServiceError&&error.outcomeUnknown);
    const failed=fake.calls.at(-1);
    fake.reply={status:'job',job:{id:other,project,kind:'index'}};
    await client.submitJob(intent);
    assert.deepEqual(fake.calls.at(-1),failed);
    assert.equal(fake.calls.filter(method=>method==='request_epoch').length,1,'uncertainty must not manufacture a new key');
});
test('repair and retry intents are reviewable before submission and enforce scoped selections',async()=>{
    const fake=new FakeTransport(),client=new ServiceClient(fake);
    const retry=await client.prepareRetry(other),plan=await client.preparePlan([project,other],'verify');
    assert.ok('retry'in retry);assert.ok('plan_create'in plan);
    assert.ok(fake.calls.every(value=>value==='request_epoch'),'preparation must not submit mutations');
    await assert.rejects(client.preparePlan([],'auto'));
    await assert.rejects(client.preparePlan([project,project],'auto'));
    assert.ok('plan_create'in await client.preparePlan('all','auto'));
});
test('bulk cached worker observations cannot cross project identity or accept duplicate scope',async()=>{
    const fake=new FakeTransport(),client=new ServiceClient(fake);
    await assert.rejects(client.workers([project,project]));assert.equal(fake.calls.length,0);
    fake.reply={status:'workers',age_ms:10,observations:[{project:other,identity:null,restarting:false,automatic_start_blocked:false}]};
    await assert.rejects(client.workers([project]));
    fake.reply={status:'workers',age_ms:10,observations:[{project,identity:{project:other,service:incarnation,worker:incarnation,fencing_generation:1},restarting:false,automatic_start_blocked:false}]};
    await assert.rejects(client.workers([project]));
});

const busyIndex='idx_'+'71'.repeat(16),busySource='src_'+'72'.repeat(16);
function sourceBusy():RecordValue{return {status:'source_change_busy',code:'source_change_jobs_pending',busy:{index_id:busyIndex,pending_jobs:1,running_jobs:0,future_job_detail:{retained:true}},future_native_busy:{retained:true}};}
function sourceMethods():unknown[]{
    const changes=[{operation:'add_root',alias:'extra',path:'/extra'},{operation:'remove_root',alias:'extra'},
        {operation:'trim_path',source_root_id:busySource,kind:'subtree',path:'generated'},{operation:'unblock_path',rule_id:'deny_'+'73'.repeat(16)},{operation:'retire_index'}];
    return [...changes.map(change=>({catalog_source_mutation_draft:{selection:{index_id:busyIndex,change}}})),
        ...['catalog_preview','catalog_apply'].flatMap(name=>['begin_index_source_change','change_index_policy','retire_index'].map(operation=>({[name]:{request:{operation:{operation,id:busyIndex}}}})))];
}

test('all native source draft/preview/apply busy responses are exact-index known refusals without retry',async()=>{
    for(const method of sourceMethods()){
        const fake=new FakeTransport(),client=new ServiceClient(fake);fake.reply=sourceBusy();
        await assert.rejects(client.rpc(method,Object.keys(method as object)[0]),error=>{
            assert.ok(error instanceof ServiceError);assert.equal(error.code,'source_change_jobs_pending');assert.equal(error.outcomeUnknown,false);
            assert.match(error.message,/Finish the current indexing work/);assert.match(error.message,/1 queued, 0 running/);assert.match(error.message,/Management/);
            assert.equal(error.nativeResponse,fake.reply);assert.deepEqual(error.nativeResponse,sourceBusy());return true;
        });
        assert.deepEqual(fake.calls,[method],'the native deferral must not poll or replay the request');
    }
});

test('source busy receipts refuse malformed identities/counts and unrelated methods while retaining complete native detail',async()=>{
    const method=sourceMethods()[0],invalid:RecordValue[]=[];
    for(const patch of [{index_id:'idx_'+'74'.repeat(16)},{index_id:'bad'},{pending_jobs:-1},{running_jobs:0.5},{pending_jobs:'1'},
        {pending_jobs:4294967296},{running_jobs:4294967296},{pending_jobs:0,running_jobs:0},{pending_jobs:undefined},{running_jobs:null}]){
        const response=sourceBusy();response.busy={...(response.busy as RecordValue),...patch};invalid.push(response);
    }
    invalid.push({...sourceBusy(),code:'registry_busy'},{...sourceBusy(),busy:null});
    for(const response of invalid){
        const fake=new FakeTransport(),client=new ServiceClient(fake);fake.reply=response;
        await assert.rejects(client.rpc(method,'catalog_source_mutation_draft'),error=>{
            assert.ok(error instanceof ServiceError);assert.equal(error.code,'crossed_source_change_busy');assert.equal(error.outcomeUnknown,true);assert.equal(error.nativeResponse,response);return true;
        });assert.equal(fake.calls.length,1);
    }
    for(const unrelated of ['health',{catalog_policy_status:{index_id:busyIndex}},{catalog_apply:{request:{operation:{operation:'rename_index',id:busyIndex}}}},
        {catalog_preview:{request:{operation:{operation:'rename_project',id:project}}}},{catalog_apply:{request:{operation:{operation:'complete_index_source_change',id:busyIndex}}}},
        {catalog_apply:{request:{operation:{operation:'retire_index',id:busyIndex}}},health:{}},
        {catalog_source_mutation_draft:{selection:{index_id:busyIndex,change:{operation:'invented'}}}}]){
        const fake=new FakeTransport(),client=new ServiceClient(fake);fake.reply=sourceBusy();
        await assert.rejects(client.rpc(unrelated),error=>error instanceof ServiceError&&error.code==='crossed_source_change_busy'&&error.outcomeUnknown&&error.nativeResponse===fake.reply);
        assert.equal(fake.calls.length,1);
    }
});

test('actual native control framing retains complete source-busy payload and classifies it before expected-tag validation',async()=>{
    const response=sourceBusy(),pipe=transport('busy',2000,response),client=new ServiceClient(pipe);
    try{
        await assert.rejects(client.catalogSourceDraft({key:{namespace:incarnation,epoch:1,nonce:'75'.repeat(16)},expected_catalog_revision:1,index_id:busyIndex,change:{operation:'trim_path',source_root_id:busySource,kind:'subtree',path:'private'}}),error=>{
            assert.ok(error instanceof ServiceError);assert.equal(error.code,'source_change_jobs_pending');assert.equal(error.outcomeUnknown,false);
            assert.deepEqual(error.nativeResponse,response);return true;
        });
    }finally{client.dispose();}
});
