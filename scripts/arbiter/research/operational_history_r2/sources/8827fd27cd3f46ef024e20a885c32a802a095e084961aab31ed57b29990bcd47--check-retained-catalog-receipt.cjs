// SPDX-License-Identifier: Apache-2.0
// Read an existing retained FencedReceipt through stdin only. Never persist its payload.
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const assert=require('node:assert/strict');
const {test}=require('node:test');
const root=path.resolve(__dirname,'../../..');
const {ServiceClient}=require(path.join(root,'packages/vscode/out/serviceClient.js'));
const {IntentJournal}=require(path.join(root,'packages/vscode/out/serviceIntents.js'));
test('retained durable native membership receipt passes real client and journal boundaries',async()=>{
    const raw=fs.readFileSync(0);assert.ok(raw.length>0&&raw.length<=65536,'bounded receipt input');
    const full=JSON.parse(raw.toString('utf8'));
    const envelope={status:'catalog_applied',receipt:full.metadata,fenced_receipt:full};
    const intent={catalog_apply:{request:full.request,preview_digest:full.preview_digest},request_digest:full.metadata.request_digest};
    const result={qualification:'canonical_handler_envelope_reconstructed_from_retained_durable_record',bytes:raw.length,sha256:crypto.createHash('sha256').update(raw).digest('hex'),fields:Object.keys(full).sort(),stages:[]};
    let dispatches=0;const values=new Map();
    const storage={keys:()=>[...values.keys()],get:key=>values.get(key),update:async(key,value)=>{await Promise.resolve();if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));await Promise.resolve();}};
    const journal=new IntentJournal(storage,'/retained-proof-in-memory',full.request.key.namespace);
    const client=new ServiceClient({dispose(){},async request(method){dispatches++;assert.deepEqual(method,{catalog_apply:intent.catalog_apply});assert.equal(journal.list().length,1);return structuredClone(envelope);}});
    try{
        assert.equal(full.request.operation.operation,'set_membership');assert.equal(full.metadata.catalog_revision,7);
        const id=await journal.prepare('Retained membership boundary',intent);result.stages.push('prepare');
        const response=await client.submitCatalog(intent);result.stages.push('client_submit_validation');
        assert.equal(client.validateCatalogObservation(response,full.request,intent.request_digest,full.preview_digest),true);result.stages.push('client_observation_validation');
        await journal.observeCatalog(id,response);result.stages.push('journal_observe');
        const saved=journal.list();assert.equal(saved.length,1);assert.deepEqual(saved[0].value.nativeObservation,envelope);assert.deepEqual(saved[0].value.nativeFenceObservation,envelope);
        await journal.forget(id);assert.deepEqual(journal.list(),[]);assert.equal(dispatches,1);result.stages.push('journal_forget');
        console.log(JSON.stringify(result));
    }catch(error){
        console.log(JSON.stringify({...result,failure:{name:error?.name,code:error?.code,message:typeof error?.message==='string'?error.message.slice(0,200):'unknown'}}));
        // Avoid node:test printing native data from assertion actual/expected.
        throw new Error(`retained boundary failed after ${result.stages.join(',')}: ${error?.code||error?.name||'unknown'}`);
    }finally{client.dispose();}
});
