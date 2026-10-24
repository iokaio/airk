// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { IntentJournal, IntentStorage } from '../serviceIntents';
import { JobIntent, MutationIntent, ServiceClient, ServiceError } from '../serviceClient';
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
test('the complete 256-project selection fits the durable local journal',async()=>{
    const storage=new Memory(),journal=new IntentJournal(storage,'/control',namespace);
    const members=Array.from({length:256},(_,index)=>index.toString(16).padStart(64,'0'));
    const selection:MutationIntent={plan_create:{caller,key:{namespace,epoch:7,nonce:'09'.repeat(16)},request:{selection:{selected:members},operation:'verify'}}};
    assert.ok(Buffer.byteLength(JSON.stringify(selection))>8192);
    await journal.prepare('Verify selected projects',selection);
    assert.deepEqual(journal.list()[0].value.intent,selection);
});
