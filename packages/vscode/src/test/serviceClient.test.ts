// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { ControlTransport, NativeControlTransport, ServiceClient, ServiceError, RecordValue } from '../serviceClient';
const project='1'.repeat(64), other='2'.repeat(64), incarnation='3'.repeat(32);
const fixture=String.raw`
let pending=Buffer.alloc(0);
process.stdin.on('data',bytes=>{
 pending=Buffer.concat([pending,bytes]);
 while(pending.length>=4&&pending.length>=4+pending.readUInt32BE()){
  const length=pending.readUInt32BE();const request=JSON.parse(pending.subarray(4,4+length));pending=pending.subarray(4+length);
  if(process.env.AIRK_CONSOLE_TEST_MODE==='stall')continue;
  if(process.env.AIRK_CONSOLE_TEST_MODE==='oversize'){const prefix=Buffer.alloc(4);prefix.writeUInt32BE(65537);process.stdout.write(prefix);continue;}
  const body=Buffer.from(JSON.stringify({id:process.env.AIRK_CONSOLE_TEST_MODE==='crossed'?'f'.repeat(32):request.id,response:{status:'fixture',echo:request.method}}));
  const prefix=Buffer.alloc(4);prefix.writeUInt32BE(body.length);
  process.stdout.write(prefix.subarray(0,2));
  setTimeout(()=>{process.stdout.write(prefix.subarray(2));process.stdout.write(body.subarray(0,3));process.stdout.write(body.subarray(3));},5);
 }
});
`;
function transport(mode='echo',timeout=2000): NativeControlTransport {
    return new NativeControlTransport('unused',[],timeout,()=>spawn(process.execPath,['-e',fixture],
        {stdio:'pipe',windowsHide:true,env:{...process.env,AIRK_CONSOLE_TEST_MODE:mode}}));
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
