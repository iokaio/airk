// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { IntentJournal, IntentStorage } from '../serviceIntents';
import { JobIntent, MutationIntent, ServiceClient, ServiceError, validateConfigurationRequest } from '../serviceClient';
import {isLocalTrackingFailure,isNativeReplyValidationFailure} from '../nativeOutcome';
class Memory implements IntentStorage {
    values=new Map<string,unknown>(); fail=false;
    keys():string[]{return [...this.values.keys()];}
    get<T>(key:string):T|undefined{return this.values.get(key) as T|undefined;}
    async update(key:string,value:unknown):Promise<void>{
        if(this.fail)throw new Error('Persistence failed');
        if(value===undefined)this.values.delete(key);else this.values.set(key,JSON.parse(JSON.stringify(value)));
    }
}
const namespace='ab'.repeat(16),project='cd'.repeat(32),caller='ef'.repeat(16);
const intent=(index:number):MutationIntent=>({enqueue:{project,caller,kind:'index',key:{namespace,epoch:7,nonce:index.toString(16).padStart(32,'0')}}});

for(const phase of ['callback','retire'] as const)test(`job acknowledgment keeps the complete original reply after local ${phase} failure`,async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace),id=await journal.prepare('Index',intent(1)),cause=new ServiceError('fixture_callback_failed');let dispatched=0;
    const reply={status:'job',job:{id:'12'.repeat(32),project,kind:'index'},future_native:{all_fields:['preserved',null]}};
    const client=new ServiceClient({dispose(){},async request(){dispatched++;return reply;}});
    await assert.rejects(journal.submit(client,id,async()=>{if(phase==='callback')throw cause;storage.fail=true;}),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.deepEqual(error.nativeResponse,reply);assert.equal(isLocalTrackingFailure(error),true);if(phase==='callback')assert.equal(error.cause,cause);return true;});
    assert.equal(dispatched,1);assert.equal(journal.list().length,1);
});

test('receipt validation failure retains the full raw frame without labeling it accepted or a local write failure',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);await journal.prepare('Index',intent(1));
    const reply={status:'request_receipt',job:{id:17,project},future_native:{unaccepted:true}},client=new ServiceClient({dispose(){},async request(){return reply;}});
    await assert.rejects(journal.reconcile(client,async()=>{assert.fail('malformed reply cannot reach receipt consumer');}),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.deepEqual(error.nativeResponse,reply);assert.equal(isNativeReplyValidationFailure(error),true);assert.equal(isLocalTrackingFailure(error),false);return true;});assert.equal(journal.list().length,1);
});

test('a local generic receipt failure retains the original RPC envelope rather than its job callback projection',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);await journal.prepare('Index',intent(1));
    const reply={status:'request_receipt',expired:false,job:{id:'12'.repeat(32),project},future_native:{outer_envelope:true}},client=new ServiceClient({dispose(){},async request(){return reply;}}),cause=new ServiceError('fixture_receipt_save_failed');
    await assert.rejects(journal.reconcile(client,async projected=>{assert.deepEqual(projected,{status:'job',job:reply.job});throw cause;}),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.deepEqual(error.nativeResponse,reply);assert.equal(error.cause,cause);return true;});
});

for(const returned of ['stronger_history','current_reply','unknown_field','immutable_request','contradictory_authority'] as const)test(`journal readback permits only canonical historical normalization: ${returned}`,async()=>{
    const storage:Memory&{replace?:(key:string,value:unknown)=>Promise<void>}=new Memory(),journal=new IntentJournal(storage,'/control',namespace),digest='43'.repeat(32);
    const request={caller,key:{namespace,epoch:7,nonce:'25'.repeat(16)},expected_catalog_revision:1,operation:{operation:'rename_project',id:'prj_'+'26'.repeat(16),expected_definition_revision:1,name:'Reviewed'}};
    const applied={status:'catalog_applied',receipt:{schema:1,operation_id:'27'.repeat(16),caller,key:request.key,request_digest:digest,catalog_revision:2,authority_fence_required:false},future_original:{complete:true}};
    const id=await journal.prepare('Reviewed',{catalog_apply:{request,preview_digest:digest},request_digest:digest}),lookup={status:'catalog_receipt',receipt:null,future_lookup:{complete:true}};
    storage.replace=async(key,value)=>{
        const changed=structuredClone(value) as Record<string,any>;
        if(returned==='stronger_history')changed.nativeCatalogObservation=applied;
        if(returned==='current_reply')changed.nativeObservation={...lookup,future_lookup:{changed:true}};
        if(returned==='unknown_field')changed.unexpected_stored_field='changed';
        if(returned==='immutable_request')changed.intent.catalog_apply.request.operation.name='Crossed';
        if(returned==='contradictory_authority')changed.nativeCatalogObservation={...applied,receipt:{...applied.receipt,caller:'28'.repeat(16)}};
        storage.values.set(key,changed);
    };
    if(returned==='stronger_history'){
        await journal.observeCatalog(id,lookup);assert.deepEqual(journal.list()[0].value.nativeObservation,lookup);assert.deepEqual(journal.list()[0].value.nativeCatalogObservation,applied);
    }else await assert.rejects(journal.observeCatalog(id,lookup),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.deepEqual(error.nativeResponse,lookup);assert.equal(error.outcomeUnknown,true);return true;});
});

for(const phase of ['prepare','observe','retire'] as const)test(`journal refuses an acknowledged ${phase} whose exact record readback disagrees`,async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace),digest='12'.repeat(32);
    const request={caller,key:{namespace,epoch:7,nonce:'13'.repeat(16)},expected_catalog_revision:1,operation:{operation:'rename_project',id:'prj_'+'14'.repeat(16),expected_definition_revision:1,name:'Reviewed'}};
    const original=storage.update.bind(storage);let corrupt=false;
    storage.update=async(key,value)=>{
        const prior=storage.values.get(key);await original(key,value);
        if(!corrupt)return;
        if(phase==='retire')storage.values.set(key,prior);else if(phase==='prepare')storage.values.delete(key);else storage.values.set(key,prior);
    };
    if(phase==='prepare'){
        corrupt=true;await assert.rejects(journal.prepare('Reviewed',{catalog_apply:{request,preview_digest:digest},request_digest:digest}),/local_intent_write_unconfirmed/);assert.deepEqual(journal.list(),[]);
    }else{
        const id=await journal.prepare('Reviewed',{catalog_apply:{request,preview_digest:digest},request_digest:digest});corrupt=true;
        if(phase==='observe')await assert.rejects(journal.observeCatalog(id,{status:'catalog_receipt',receipt:null}),/local_intent_write_unconfirmed/);
        else await assert.rejects(journal.forget(id),/local_intent_retirement_unconfirmed/);
        assert.equal(journal.list().length,1,'a failed acknowledgement never discards the unknown original intent');
    }
});
test('exhausted native metadata contention never restarts a read or mutation budget',async()=>{
    let calls=0;const client=new ServiceClient({dispose(){},async request(){calls++;return calls<3?{status:'failure',code:'registry_busy'}:{status:'job',job:{id:'12'.repeat(32),project}};}});
    await assert.rejects(client.rpc({job:{id:'12'.repeat(32)}},'job'),/registry_busy/);assert.equal(calls,1);
    calls=0;await assert.rejects(client.submitMutation(intent(1)),/registry_busy/);assert.equal(calls,1);
});
test('lost acknowledgement reconciles by read-only receipt and never resubmits a missing request',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    await journal.prepare('Index',intent(1));let found=false,receipts=0;const sent:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(value){sent.push(value);return {status:'request_receipt',expired:!found,job:found?{id:'12'.repeat(32),project,kind:'index'}:null};}});
    await journal.reconcile(client,async()=>{receipts++;});assert.equal(journal.list().length,1);
    found=true;await journal.reconcile(client,async()=>{receipts++;});
    assert.equal(receipts,1);assert.equal(journal.list().length,0);
    assert.deepEqual(sent,[{request_receipt:{caller,project,key:(intent(1) as {enqueue:JobIntent}).enqueue.key}},{request_receipt:{caller,project,key:(intent(1) as {enqueue:JobIntent}).enqueue.key}}]);
});

test('bounded receipt checks rotate past missing and failed records without replaying them',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace),sent:string[]=[],failures:string[]=[];
    for(let index=1;index<=10;index++)await journal.prepare('Index',intent(index));
    const before=structuredClone([...storage.values]);let receipts=0;
    const client=new ServiceClient({dispose(){},async request(value){
        const key=(value as {request_receipt:{key:{nonce:string}}}).request_receipt.key;
        sent.push(key.nonce);
        if(key.nonce==='1'.padStart(32,'0'))throw new ServiceError('invalid_service_failure');
        return {status:'request_receipt',job:key.nonce==='a'.padStart(32,'0')?{id:'12'.repeat(32),project}:null};
    }});
    await journal.reconcile(client,async()=>{receipts++;},undefined,id=>{failures.push(id);});
    assert.equal(sent.length,8);assert.equal(receipts,0);
    await journal.reconcile(client,async()=>{receipts++;},undefined,id=>{failures.push(id);});
    assert.equal(sent.length,16);assert.equal(receipts,1);
    assert.equal(journal.list().length,9);
    assert.ok(failures.length>0);
    for(const [key,value] of before)if(!key.endsWith('a'.padStart(32,'0')))assert.deepEqual(storage.values.get(key),value);
});
test('configuration journal rejects raw input fields, stale body reuse and oversized configuration',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    const request={incarnation:namespace,settings:'12'.repeat(32),configuration:{operation:'vectors',enabled:false}};
    const input:MutationIntent={configure_project:{project,caller,key:{namespace,epoch:7,nonce:'01'.repeat(16)},request}};
    const id=await journal.prepare('Disable vectors',input);
    const changed=structuredClone(input);changed.configure_project.request.configuration.enabled=true;
    await assert.rejects(journal.prepare('Enable vectors',changed),/local_intent_key_conflict/);
    assert.equal(journal.list()[0].id,id);
    assert.throws(()=>validateConfigurationRequest({...request,configuration:{...request.configuration,baseline_text:'Private source text'}}));
    assert.throws(()=>validateConfigurationRequest({...request,configuration:{operation:'install_model',directory:'x'.repeat(20000),manifest_sha256:'12'.repeat(32)}}));
});
test('prepared requests survive reload and uncertain submission uses the exact original intent',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    const original=intent(1),id=await journal.prepare('Index project',original);
    (original as {enqueue:JobIntent}).enqueue.kind='recover';
    let failed=true;const sent:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(value){
        sent.push(structuredClone(value));if(failed)throw new ServiceError('console_transport_closed',true);
        return {status:'job',job:{id:'12'.repeat(32),project}};
    }});
    await assert.rejects(journal.submit(client,id));
    const reloaded=new IntentJournal(storage,'/control',namespace);
    assert.equal(reloaded.list().length,1);failed=false;
    await reloaded.submit(client,id);
    assert.deepEqual(sent,[intent(1),intent(1)]);assert.deepEqual(reloaded.list(),[]);
});
test('persist failure prevents sending and acknowledgement cleanup failure preserves the request',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    storage.fail=true;await assert.rejects(journal.prepare('Index',intent(2)));assert.equal(journal.list().length,0);
    storage.fail=false;const id=await journal.prepare('Index',intent(2));storage.fail=true;
    const client=new ServiceClient({dispose(){},async request(){return {status:'job',job:{id:'12'.repeat(32),project}};}});
    await assert.rejects(journal.submit(client,id));assert.equal(journal.list().length,1);
});
test('scope, corruption, capacity and conflicting reuse preserve unknown records',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    const id=await journal.prepare('Index',intent(1));
    assert.deepEqual(new IntentJournal(storage,'/elsewhere',namespace).list(),[]);
    assert.deepEqual(new IntentJournal(storage,'/control','99'.repeat(16)).list(),[]);
    assert.throws(()=>journal.prepare('Foreign',{enqueue:{...(intent(2) as {enqueue:JobIntent}).enqueue,key:{namespace:'99'.repeat(16),epoch:7,nonce:'03'.repeat(16)}}}));
    const conflict=intent(1);if('enqueue' in conflict)conflict.enqueue.kind='recover';
    await assert.rejects(journal.prepare('Different',conflict),/local_intent_key_conflict/);
    for(let index=2;index<=64;index++)await journal.prepare('Index',intent(index));
    await assert.rejects(journal.prepare('Index',intent(65)),/capacity/);assert.equal(journal.list().length,64);
    const key=storage.keys()[0],before=storage.get(key);
    storage.values.set(key,{schema:1,scope:journal.scope,title:'bad',intent:{shutdown:{}}});
    assert.throws(()=>journal.list());assert.deepEqual(storage.get(key),{schema:1,scope:journal.scope,title:'bad',intent:{shutdown:{}}});
    storage.values.set(key,before);await journal.forget(id);assert.equal(journal.list().length,63);
});
test('separate windows persist distinct keys without losing each others requests',async()=>{
    const storage=new Memory(),a=new IntentJournal(storage,'/control',namespace),b=new IntentJournal(storage,'/control',namespace);
    await Promise.all([a.prepare('One',intent(1)),b.prepare('Two',intent(2))]);
    assert.equal(a.list().length,2);await a.forget('1'.padStart(32,'0'));assert.equal(b.list().length,1);
});

test('a concurrently retired key is absent while every present malformed request remains a failure',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);await journal.prepare('One',intent(1));
    const missing='airk.service.intent.1.'+journal.scope+'.'+'2'.padStart(32,'0'),keys=storage.keys.bind(storage);storage.keys=()=>[...new Set([...keys(),missing])];
    assert.equal(journal.list().length,1);const malformed={schema:1,scope:journal.scope,title:'Preserved malformed',intent:{shutdown:{}}};storage.values.set(missing,malformed);
    assert.throws(()=>journal.list());assert.deepEqual(storage.values.get(missing),malformed);
});
test('the complete 256-project selection fits the durable local journal',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    const members=Array.from({length:256},(_,index)=>index.toString(16).padStart(64,'0'));
    const selection:MutationIntent={plan_create:{caller,key:{namespace,epoch:7,nonce:'09'.repeat(16)},request:{selection:{selected:members},operation:'verify'}}};
    assert.ok(Buffer.byteLength(JSON.stringify(selection))>8192);
    await journal.prepare('Verify selected projects',selection);
    assert.deepEqual(journal.list()[0].value.intent,selection);
});


test('authoritative project removal clears only matching saved requests without replay or invented job outcomes',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    await journal.prepare('Index',intent(1));let removed='12'.repeat(32),receipts=0;const sent:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(value){sent.push(value);return {status:'request_receipt',expired:true,job:null,removed_project:removed};}});
    await assert.rejects(journal.reconcile(client,async()=>{receipts++;}),/crossed_removed_project_receipt/);
    assert.equal(journal.list().length,1);
    removed=project;await journal.reconcile(client,async()=>{receipts++;});
    assert.equal(journal.list().length,0);assert.equal(receipts,0);
    assert.ok(sent.every(value=>!!(value as any).request_receipt),'removal never resubmits the old indexing request');
});
