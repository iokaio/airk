// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import {createHash,randomUUID} from 'node:crypto';
import {PrivateIntentStore,validateScopeRetirementCapture,PRIVATE_INTENT_PHYSICAL_READ_BYTES,PRIVATE_INTENT_CAPTURE_FILES} from '../privateIntentStore';
import {IntentJournal,IntentStorage,INTENT_PREFIX,SavedIntent,type ConsoleIntent} from '../serviceIntents';
import {acceptedIntentSlots} from '../intentAcceptedEvidence';
import {validateSplitRecover} from '../indexingSplit';
import {splitFrozenFixture} from './fixtures/split';

class Memory implements IntentStorage{
    values=new Map<string,unknown>();
    keys():string[]{return [...this.values.keys()].filter(key=>this.values.get(key)!==undefined);}
    get<T>(key:string):T|undefined{return this.values.get(key) as T|undefined;}
    async update(key:string,value:unknown):Promise<void>{if(value===undefined)this.values.delete(key);else this.values.set(key,JSON.parse(JSON.stringify(value)));}
}
const namespace='ab'.repeat(16),control=path.resolve('/fictional-control');
const scopeFor=(controlDir=control,ns=namespace)=>createHash('sha256').update(JSON.stringify([controlDir,ns])).digest('hex');
function saved(index:number,scope=scopeFor(),ns=namespace):{key:string;value:SavedIntent}{
    const nonce=index.toString(16).padStart(32,'0');
    return {key:INTENT_PREFIX+scope+'.'+nonce,value:{schema:1,scope,title:'Index fixture',
        intent:{enqueue:{project:'cd'.repeat(32),caller:'ef'.repeat(16),kind:'index',key:{namespace:ns,epoch:7,nonce}}}}};
}
async function fixture(legacy=new Memory(),valid=()=>true){
    const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'airk-private-intent-'))),directory=path.join(root,'journal');
    const store=new PrivateIntentStore(directory,legacy,valid);await store.initialize(scopeFor());
    return {root,directory,legacy,store,close:()=>fs.promises.rm(root,{recursive:true,force:true})};
}
function catalog(index:number){
    const row=saved(index),key={namespace,epoch:7,nonce:index.toString(16).padStart(32,'0')},digest='12'.repeat(32),caller='ef'.repeat(16);
    const request={caller,key,expected_catalog_revision:1,operation:{operation:'rename_project',id:'prj_'+'34'.repeat(16),expected_definition_revision:1,name:'Reviewed'}};
    row.value.intent={catalog_apply:{request,preview_digest:digest},request_digest:digest} as ConsoleIntent;
    const response={status:'catalog_applied',receipt:{schema:1,operation_id:'56'.repeat(16),caller,key,request_digest:digest,catalog_revision:2,authority_fence_required:false},future_native:{all:['original','kept']}};
    return {...row,response};
}

test('private requests survive Memento erasure and never mirror new intents into preferences',async t=>{
    const f=await fixture();t.after(f.close);
    const journal=new IntentJournal(f.store,control,namespace),item=saved(1);
    await journal.prepare(item.value.title,item.value.intent);assert.equal(journal.list().length,1);
    assert.equal(f.legacy.keys().filter(key=>key.startsWith(INTENT_PREFIX)).length,0);
    f.legacy.values.clear();assert.deepEqual(journal.list()[0].value.intent,item.value.intent);
    await f.store.update('airk.preference','kept');assert.equal(f.legacy.get('airk.preference'),'kept');
    await assert.rejects(f.store.update(item.key,item.value),/private_intent_generic_update_forbidden/);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());assert.deepEqual(reloaded.get(item.key),journal.list()[0].value);
});

test('legacy import is once only; late known snapshots stay retired and unknown or changed rows block',async t=>{
    const legacy=new Memory(),item=saved(1);legacy.values.set(item.key,item.value);
    const f=await fixture(legacy);t.after(f.close);assert.deepEqual(f.store.get(item.key),item.value);
    await f.store.retire(item.key);legacy.values.set(item.key,structuredClone(item.value));
    const reloaded=new PrivateIntentStore(f.directory,legacy);await reloaded.initialize(scopeFor());
    assert.equal(reloaded.get(item.key),undefined);assert.equal(reloaded.keys().includes(item.key),false);
    const foreign=saved(2);legacy.values.set(foreign.key,foreign.value);assert.throws(()=>reloaded.keys(),/legacy_outcome_requires_review/);
    legacy.values.delete(foreign.key);legacy.values.set(item.key,{...item.value,title:'changed immutable request'});
    assert.throws(()=>reloaded.keys(),/legacy_outcome_requires_review/);
});

test('immutable base survives observation replacement and imported mutable fields can be deleted',async t=>{
    const legacy=new Memory(),item=saved(1);item.value.nativeRefreshValidationError='old diagnostic';legacy.values.set(item.key,item.value);
    const f=await fixture(legacy);t.after(f.close);const base=path.join(f.directory,fs.readdirSync(f.directory).find(name=>name.startsWith('b-'))!),bytes=fs.readFileSync(base);
    const next={...item.value,nativeRefreshValidationError:undefined,nativeObservation:{status:'known'}};
    await f.store.replace(item.key,next);assert.deepEqual(fs.readFileSync(base),bytes);
    assert.equal(f.store.get<SavedIntent>(item.key)!.nativeRefreshValidationError,undefined);
    assert.deepEqual(f.store.get<SavedIntent>(item.key)!.nativeObservation,{status:'known'});
    await assert.rejects(f.store.replace(item.key,{...next,title:'different request'}),/private_intent_request_changed/);
});

for(const when of ['prepare','before','after'] as const)test(`observation publication ${when} rename cannot resurrect a retired base`,async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1);await f.store.create(item.key,item.value);
    const other=new PrivateIntentStore(f.directory,f.legacy);await other.initialize(scopeFor());
    const native=require('node:fs') as typeof fs,rename=native.renameSync;let retired:Promise<void>|undefined;
    if(when==='prepare'){
        const sync=native.fsyncSync;let armed=true;
        t.mock.method(native,'fsyncSync',(fd:number)=>{sync(fd);if(armed){armed=false;retired=other.retire(item.key);}});
    }
    t.mock.method(native,'renameSync',function(from:fs.PathLike,to:fs.PathLike){
        if(String(to).includes(path.sep+'o-')){
            if(when==='before')retired=other.retire(item.key);
            rename(from,to);
            if(when==='after')retired=other.retire(item.key);
            return;
        }
        rename(from,to);
    });
    await assert.rejects(f.store.replace(item.key,{...item.value,nativeObservation:{status:'known'}}),/private_intent_retirement_in_progress/);
    await retired;assert.equal(f.store.get(item.key),undefined);assert.equal(f.store.keys().includes(item.key),false);
    assert.equal(fs.readdirSync(f.directory).filter(name=>/^[bo]-/.test(name)).length,0,'retired observations must not accumulate invisible artifacts');
    await assert.rejects(f.store.replace(item.key,item.value),/private_intent_not_found/);
});

test('successful observe and forget repeatedly retire all private request and observation artifacts',async t=>{
    const f=await fixture();t.after(f.close);const footprint=fs.readdirSync(f.directory).sort();
    for(let index=1;index<=6;index++){
        const item=saved(index);await f.store.create(item.key,item.value);
        await f.store.replace(item.key,{...item.value,nativeObservation:{known:index}});
        await f.store.retire(item.key);
        assert.equal(f.store.get(item.key),undefined);
        assert.deepEqual(fs.readdirSync(f.directory).filter(name=>/^[bo]-/.test(name)),[]);
        assert.deepEqual(fs.readdirSync(f.directory).sort(),footprint,'completed requests cannot accumulate per-nonce retirement artifacts');
    }
});

test('ordinary retirement interrupted after base unlink recovers only its exact observed request',async t=>{
    const f=await fixture();t.after(f.close);
    const current=saved(2),foreignScope=scopeFor(path.resolve('/foreign-control'),'bc'.repeat(16)),foreign=saved(3,foreignScope,'bc'.repeat(16));
    await f.store.create(current.key,current.value);await f.store.replace(current.key,{...current.value,nativeObservation:{untouched:'current'}});
    await f.store.initialize(foreignScope);await f.store.create(foreign.key,foreign.value);await f.store.replace(foreign.key,{...foreign.value,nativeObservation:{untouched:'foreign'}});
    const footprint=fs.readdirSync(f.directory).sort(),currentValue=f.store.get(current.key),foreignValue=f.store.get(foreign.key),item=saved(1);
    await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:{known:true}});
    const native=require('node:fs') as typeof fs,unlink=native.unlinkSync;let interrupted=false;
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){
        unlink(file);
        if(!interrupted&&path.basename(String(file))===`b-${scopeFor()}-${'1'.padStart(32,'0')}.json`){interrupted=true;throw new Error('fixture death after base unlink');}
    });
    await assert.rejects(f.store.retire(item.key),/fixture death after base unlink/);t.mock.restoreAll();assert.equal(interrupted,true);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());
    assert.equal(reloaded.get(item.key),undefined);assert.deepEqual(reloaded.get(current.key),currentValue);assert.deepEqual(reloaded.get(foreign.key),foreignValue);
    assert.deepEqual(fs.readdirSync(f.directory).sort(),footprint,'recovered retirement leaves no per-nonce proof or observation behind');
});

for(const boundary of ['before_base','after_base','before_overlay','after_overlay','before_anchor','after_anchor'] as const)test(`retirement recovery completes exact producer boundary ${boundary}`,async t=>{
    const f=await fixture();t.after(f.close);const footprint=fs.readdirSync(f.directory).sort(),item=catalog(1);
    await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:item.response});
    const native=require('node:fs') as typeof fs,unlink=native.unlinkSync,kind=boundary.endsWith('base')?'b':boundary.endsWith('anchor')?'a':'o';let armed=true;
    const target=`${kind}-${scopeFor()}-${'1'.padStart(32,'0')}${kind==='a'?'-catalog_metadata_applied':''}.json`;
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){
        if(armed&&path.basename(String(file))===target){armed=false;if(boundary.startsWith('before'))throw new Error('fixture producer interruption');unlink(file);throw new Error('fixture producer interruption');}
        unlink(file);
    });
    await assert.rejects(f.store.retire(item.key),/fixture producer interruption/);t.mock.restoreAll();assert.equal(armed,false);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());assert.equal(reloaded.get(item.key),undefined);
    assert.deepEqual(fs.readdirSync(f.directory).sort(),footprint);
});

for(const changed of ['overlay','generation'] as const)test(`interrupted retirement preserves changed foreign ${changed}`,async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1);await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:{known:true}});
    const stem=scopeFor()+'-'+ '1'.padStart(32,'0'),base=path.join(f.directory,'b-'+stem+'.json'),overlay=path.join(f.directory,'o-'+stem+'.json'),original=JSON.parse(fs.readFileSync(base,'utf8'));
    const native=require('node:fs') as typeof fs,unlink=native.unlinkSync;let armed=true;
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){unlink(file);if(armed&&String(file)===base){armed=false;throw new Error('fixture death');}});
    await assert.rejects(f.store.retire(item.key),/fixture death/);t.mock.restoreAll();
    const foreign=changed==='overlay'?overlay:base;
    fs.writeFileSync(foreign,JSON.stringify(changed==='overlay'?{...JSON.parse(fs.readFileSync(overlay,'utf8')),changes:{nativeObservation:{foreign:true}}}:{...original,generation:randomUUID(),value:{...original.value,title:'foreign generation'}}),{mode:0o600});
    const bytes=fs.readFileSync(foreign),reloaded=new PrivateIntentStore(f.directory,f.legacy);
    await assert.rejects(reloaded.initialize(scopeFor()),/private_intent_retirement_changed/);assert.deepEqual(fs.readFileSync(foreign),bytes);
});

test('a full legal legacy record retains its semantic capacity independently of private envelopes',async t=>{
    const legacy=new Memory(),item=saved(1);item.value.nativeObservation={padding:''};
    item.value.nativeObservation.padding='x'.repeat(65536-Buffer.byteLength(JSON.stringify(item.value)));assert.equal(Buffer.byteLength(JSON.stringify(item.value)),65536);
    legacy.values.set(item.key,item.value);const f=await fixture(legacy);t.after(f.close);assert.deepEqual(f.store.get(item.key),item.value);
    await f.store.replace(item.key,{...item.value,nativeObservation:{current:'small'}});assert.deepEqual(f.store.get<SavedIntent>(item.key)!.nativeObservation,{current:'small'});
});

test('immutable accepted catalog authority survives a stale unknown writer and reload',async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1);await f.store.create(item.key,item.value);
    const other=new PrivateIntentStore(f.directory,f.legacy);await other.initialize(scopeFor());const stale=other.get<SavedIntent>(item.key)!;
    await f.store.replace(item.key,{...item.value,nativeObservation:item.response});
    const unknown={status:'catalog_receipt',receipt:null,future_unknown:{kept:['entire','reply']}};
    await other.replace(item.key,{...stale,nativeObservation:unknown});
    const result=other.get<SavedIntent>(item.key)!;assert.deepEqual(result.nativeObservation,unknown);assert.deepEqual(result.nativeCatalogObservation,item.response);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());assert.deepEqual(reloaded.get(item.key),result);
    const capture=await reloaded.captureScope(control,namespace);assert.ok(capture.files.some(file=>file.name.includes('-catalog_metadata_applied.json')));
    await reloaded.retire(item.key);assert.equal(fs.readdirSync(f.directory).some(name=>/^[boat]-/.test(name)),false);
});

test('accepted slot collisions preserve the first full DTO and refuse contradictory authority',async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1);await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:item.response});
    const anchorName=fs.readdirSync(f.directory).find(name=>name.startsWith('a-'))!,anchorBytes=fs.readFileSync(path.join(f.directory,anchorName));
    const later={...item.response,future_native:{all:['later-compatible']}};await f.store.replace(item.key,{...item.value,nativeObservation:later});
    assert.deepEqual(f.store.get<SavedIntent>(item.key)!.nativeObservation,later);
    assert.deepEqual(fs.readFileSync(path.join(f.directory,anchorName)),anchorBytes);
    await f.store.replace(item.key,{...item.value,nativeObservation:{status:'catalog_receipt',receipt:null}});
    assert.deepEqual(f.store.get<SavedIntent>(item.key)!.nativeCatalogObservation,item.response);
    const reload=new PrivateIntentStore(f.directory,f.legacy);await reload.initialize(scopeFor());
    assert.deepEqual(reload.get<SavedIntent>(item.key),f.store.get<SavedIntent>(item.key));
    assert.deepEqual(fs.readFileSync(path.join(f.directory,anchorName)),anchorBytes);
    const before=f.store.get(item.key),files=fs.readdirSync(f.directory).sort();
    const changed={...item.response,receipt:{...item.response.receipt,operation_id:'ff'.repeat(16)}};
    await assert.rejects(f.store.replace(item.key,{...item.value,nativeObservation:changed}),/private_intent_accepted_evidence_conflict/);
    assert.deepEqual(f.store.get(item.key),before);assert.deepEqual(fs.readdirSync(f.directory).sort(),files);
});

test('projected fallback capacity refuses before publishing accepted authority or a lookup',async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1);item.value.nativeObservation={status:'failure',code:'fictional',padding:'x'.repeat(63000)};
    await f.store.create(item.key,item.value);const before=f.store.get(item.key),files=fs.readdirSync(f.directory).sort();
    const response={...item.response,future_native:{all:['x'.repeat(8000)]}};
    await assert.rejects(f.store.replace(item.key,{...item.value,nativeObservation:response}),/local_intent_record_invalid/);
    assert.deepEqual(f.store.get(item.key),before);assert.deepEqual(fs.readdirSync(f.directory).sort(),files);
});

test('an interrupted accepted-anchor publication retains the original accepted base for recovery',async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1),native=require('node:fs') as typeof fs,link=native.linkSync;
    const accepted={...item.value,nativeObservation:item.response};
    t.mock.method(native,'linkSync',function(from:fs.PathLike,to:fs.PathLike){if(path.basename(String(to)).startsWith('a-'))throw new Error('fixture accepted publication interrupted');link(from,to);});
    await assert.rejects(f.store.create(item.key,accepted),/fixture accepted publication interrupted/);t.mock.restoreAll();
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());
    assert.deepEqual(reloaded.get<SavedIntent>(item.key)!.nativeObservation,item.response);
    await reloaded.replace(item.key,{...item.value,nativeObservation:{status:'catalog_receipt',receipt:null}});
    assert.deepEqual(reloaded.get<SavedIntent>(item.key)!.nativeCatalogObservation,item.response);
});

test('accepted-anchor disappearance during a read preserves the full accepted current DTO',async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1);await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:item.response});
    const native=require('node:fs') as typeof fs,lstat=native.lstatSync;let armed=true;
    t.mock.method(native,'lstatSync',function(file:fs.PathLike,options?:unknown){
        if(armed&&path.basename(String(file)).startsWith('a-')){armed=false;const error=new Error('fixture concurrently retired anchor') as NodeJS.ErrnoException;error.code='ENOENT';throw error;}
        return (lstat as (...args:any[])=>any)(file,options);
    });
    // The original accepted current DTO is also durable in this record's
    // lookup, so one disappeared duplicate must not turn it into corruption.
    const value=f.store.get<SavedIntent>(item.key)!;assert.deepEqual(value.nativeObservation,item.response);assert.equal(armed,false);
});

test('captured-scope recovery validates all changed preimages before deleting a known orphan',async t=>{
    const f=await fixture();t.after(f.close);for(const index of [1,2]){const item=saved(index);await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:{known:index}});}
    const capture=await f.store.captureScope(control,namespace),native=require('node:fs') as typeof fs,unlink=native.unlinkSync;let armed=true;
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){unlink(file);if(armed&&path.basename(String(file))===`b-${scopeFor()}-${'1'.padStart(32,'0')}.json`){armed=false;throw new Error('fixture scope interruption');}});
    await assert.rejects(f.store.retireCapturedScope(capture,'12'.repeat(16)),/fixture scope interruption/);t.mock.restoreAll();
    const changed=path.join(f.directory,`b-${scopeFor()}-${'2'.padStart(32,'0')}.json`),orphan=path.join(f.directory,`o-${scopeFor()}-${'1'.padStart(32,'0')}.json`);
    const original=JSON.parse(fs.readFileSync(changed,'utf8'));fs.writeFileSync(changed,JSON.stringify({...original,generation:randomUUID(),value:{...original.value,title:'foreign'}}));
    const bytes=fs.readFileSync(changed),knownOrphan=fs.readFileSync(orphan),reloaded=new PrivateIntentStore(f.directory,f.legacy);
    await assert.rejects(reloaded.initialize(scopeFor()),/private_intent_retirement_changed/);
    assert.deepEqual(fs.readFileSync(changed),bytes);assert.deepEqual(fs.readFileSync(orphan),knownOrphan);
});

test('an interrupted two-link publication remains explicit unsupported evidence rather than an unsafe repair',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1),native=require('node:fs') as typeof fs,unlink=native.unlinkSync;let armed=true;
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){if(armed&&path.basename(String(file)).startsWith('.publish-')){armed=false;throw new Error('fixture death before stage unlink');}unlink(file);});
    await assert.rejects(f.store.create(item.key,item.value));t.mock.restoreAll();
    const base=fs.readdirSync(f.directory).find(name=>name.startsWith('b-'))!,stage=fs.readdirSync(f.directory).find(name=>name.startsWith('.publish-'))!;
    assert.equal(fs.statSync(path.join(f.directory,base)).nlink,2);assert.equal(fs.statSync(path.join(f.directory,stage)).nlink,2);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await assert.rejects(reloaded.initialize(scopeFor()),/private_intent_file_unsafe/);
    assert.ok(fs.existsSync(path.join(f.directory,base)));assert.ok(fs.existsSync(path.join(f.directory,stage)));
});

test('unsafe hardlinks and corrupt records are refused without deleting evidence',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1);await f.store.create(item.key,item.value);
    const name=fs.readdirSync(f.directory).find(name=>name.startsWith('b-'))!,base=path.join(f.directory,name),linked=path.join(f.root,'linked');
    fs.linkSync(base,linked);assert.throws(()=>f.store.keys(),/private_intent_file_unsafe/);fs.unlinkSync(linked);
    fs.writeFileSync(base,'{"broken":true}');assert.throws(()=>f.store.keys(),/private_intent_base_invalid/);assert.ok(fs.existsSync(base));
});

test('descriptor growth is capped before parsing and a census shares its actual byte allowance',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1);await f.store.create(item.key,item.value);
    const name=fs.readdirSync(f.directory).find(name=>name.startsWith('b-'))!,base=path.join(f.directory,name),length=fs.statSync(base).size;
    const native=require('node:fs') as typeof fs,read=native.readSync;let changed=false,maxRequested=0;
    t.mock.method(native,'readSync',function(fd:number,buffer:NodeJS.ArrayBufferView,offset:number,count:number,position:fs.ReadPosition|null){
        maxRequested=Math.max(maxRequested,count);
        if(!changed&&count===length+1){changed=true;fs.appendFileSync(base,' '.repeat(10000));}
        return read(fd,buffer,offset,count,position);
    });
    assert.throws(()=>f.store.get(item.key),/private_intent_file_changed/);assert.ok(changed);assert.ok(maxRequested<=length+1);
    t.mock.restoreAll();
    const bounded=new PrivateIntentStore(f.directory,f.legacy,()=>true,async()=>{},2*1024*1024+65536);await bounded.initialize(scopeFor());
    assert.throws(()=>new PrivateIntentStore(f.directory,f.legacy,()=>true,async()=>{},PRIVATE_INTENT_PHYSICAL_READ_BYTES+1),/private_intent_read_budget_invalid/);
    // Other windows' publication stages confer no request authority, but their
    // bytes still consume the same bounded descriptor census allowance.
    for(let index=0;index<3;index++)fs.writeFileSync(path.join(f.directory,'.publish-'+randomUUID()),JSON.stringify({padding:'x'.repeat(800000)}),{mode:0o600});
    assert.throws(()=>bounded.keys(),/private_intent_read_budget/);
});

test('an unknown orphan observation remains explicit blocked evidence',async t=>{
    const f=await fixture();t.after(f.close);
    const file=path.join(f.directory,`o-${scopeFor()}-${'ff'.repeat(16)}.json`);
    fs.writeFileSync(file,JSON.stringify({schema:1,base_sha256:'aa'.repeat(32),changes:{nativeObservation:{unknown:true}},removed:[]}),{mode:0o600});
    assert.throws(()=>f.store.keys(),/private_intent_orphan_requires_review/);assert.ok(fs.existsSync(file));
});

for(const slot of ['unknown_future_slot','source_begin'] as const)test(`an unowned ${slot} anchor is refused before admission or capture`,async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1);await f.store.create(item.key,item.value);
    const name=`a-${scopeFor()}-${'1'.padStart(32,'0')}-${slot}.json`,file=path.join(f.directory,name);
    fs.writeFileSync(file,JSON.stringify({schema:1,key:item.key,base_sha256:'aa'.repeat(32),generation:randomUUID(),slot,evidence:{foreign:true}}),{mode:0o600});
    const bytes=fs.readFileSync(file);assert.throws(()=>f.store.keys(),/private_intent_accepted_evidence_changed/);
    await assert.rejects(f.store.captureScope(control,namespace),/private_intent_accepted_evidence_changed/);assert.deepEqual(fs.readFileSync(file),bytes);
});

test('capture validates a newly introduced anchor in its own capture pass',async t=>{
    const f=await fixture();t.after(f.close);const item=catalog(1);await f.store.create(item.key,item.value);
    const initialize=f.store.initialize.bind(f.store),slot='source_begin',file=path.join(f.directory,`a-${scopeFor()}-${'1'.padStart(32,'0')}-${slot}.json`);
    t.mock.method(f.store,'initialize',async(scope:string)=>{
        await initialize(scope);
        fs.writeFileSync(file,JSON.stringify({schema:1,key:item.key,base_sha256:'aa'.repeat(32),generation:randomUUID(),slot,evidence:{foreign:true}}),{mode:0o600});
    });
    await assert.rejects(f.store.captureScope(control,namespace),/private_intent_accepted_evidence_changed/);assert.ok(fs.existsSync(file));
});

test('other namespaces consume global admission while all existing pending rows remain readable',async t=>{
    const legacy=new Memory(),foreignScope=scopeFor(path.resolve('/foreign-control'),'bc'.repeat(16));
    for(let index=1;index<=64;index++){const item=saved(index,foreignScope,'bc'.repeat(16));legacy.values.set(item.key,item.value);}
    const f=await fixture(legacy);t.after(f.close);const item=saved(100);
    assert.equal(f.store.keys().filter(key=>key.startsWith(INTENT_PREFIX)).length,64);
    await assert.rejects(f.store.create(item.key,item.value),/private_intent_global_capacity/);
    const extra=saved(65,foreignScope,'bc'.repeat(16));legacy.values.set(extra.key,extra.value);
    assert.equal(f.store.keys().filter(key=>key.startsWith(INTENT_PREFIX)).length,65);
    assert.deepEqual(f.store.get(extra.key),extra.value);
});

test('near-cap valid census reads each body once and direct get does not scan unrelated requests',async t=>{
    const f=await fixture();t.after(f.close);
    for(let index=1;index<=32;index++){const item=saved(index);item.value.nativeObservation={padding:'x'.repeat(63000)};await f.store.create(item.key,item.value);}
    const native=require('node:fs') as typeof fs,read=native.readSync;let requested=0;
    t.mock.method(native,'readSync',function(fd:number,buffer:NodeJS.ArrayBufferView,offset:number,count:number,position:fs.ReadPosition|null){requested+=count;return read(fd,buffer,offset,count,position);});
    assert.equal(f.store.keys().filter(key=>key.startsWith(INTENT_PREFIX)).length,32);assert.ok(requested<2*1024*1024+65536);
    requested=0;assert.ok(f.store.get(saved(1).key));assert.ok(requested<70000);
});

test('scope cleanup requires an accepted operation identity and preserves current and foreign records',async t=>{
    const f=await fixture();t.after(f.close);const old=saved(1);await f.store.create(old.key,old.value);
    const currentScope=scopeFor(path.resolve('/current-control'),'bc'.repeat(16)),current=saved(2,currentScope,'bc'.repeat(16));
    await f.store.initialize(currentScope);await f.store.create(current.key,current.value);
    const foreign=saved(3,scopeFor(path.resolve('/foreign-control'),'cd'.repeat(16)),'cd'.repeat(16));f.legacy.values.set(foreign.key,foreign.value);
    const capture=await f.store.captureScope(control,namespace);assert.deepEqual(validateScopeRetirementCapture(capture),capture);
    const before=fs.readdirSync(f.directory).sort();await assert.rejects(f.store.retireCapturedScope(capture,''),/private_intent_scope_capture_crossed/);assert.deepEqual(fs.readdirSync(f.directory).sort(),before);
    const receipt=await f.store.retireCapturedScope(capture,'12'.repeat(16));assert.equal(receipt.complete,true);assert.equal(f.store.get(old.key),undefined);
    assert.deepEqual(f.store.get(current.key),current.value);assert.deepEqual(f.store.get(foreign.key),foreign.value);
    await assert.rejects(f.store.create(old.key,old.value),/private_intent_scope_retired/);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());await assert.rejects(reloaded.create(old.key,old.value),/private_intent_scope_retired/);
});

test('cold scope retirement revokes late old Memento nonces without adopting or charging them',async t=>{
    const f=await fixture();t.after(f.close);const capture=await f.store.captureScope(control,namespace);
    for(let index=1;index<=64;index++){const item=saved(index);f.legacy.values.set(item.key,item.value);}
    const foreignScope=scopeFor(path.resolve('/foreign-control'),'bc'.repeat(16)),foreign=saved(100,foreignScope,'bc'.repeat(16));f.legacy.values.set(foreign.key,foreign.value);
    assert.throws(()=>f.store.keys(),/private_intent_legacy_outcome_requires_review/);
    const cold=new PrivateIntentStore(f.directory,f.legacy),result=await cold.retireCapturedScope(capture,'12'.repeat(16));
    assert.equal(result.complete,true);assert.equal(result.late_observed_legacy.length,64);assert.deepEqual(result.legacy_remaining,[]);assert.deepEqual(cold.get(foreign.key),foreign.value);
    // A queued old main-storage snapshot can reappear even after removal ACK.
    // Its authenticated retired namespace remains logically revoked.
    for(let index=1;index<=64;index++){const item=saved(index);f.legacy.values.set(item.key,item.value);}
    const currentScope=scopeFor(path.resolve('/current-control'),'cd'.repeat(16)),current=saved(101,currentScope,'cd'.repeat(16));
    await cold.initialize(currentScope);await cold.create(current.key,current.value);
    assert.equal(cold.keys().filter(key=>key.startsWith(INTENT_PREFIX)).length,2);assert.equal(cold.get(saved(1).key),undefined);
    assert.deepEqual(cold.get(current.key),current.value);assert.deepEqual(cold.get(foreign.key),foreign.value);
});

for(const invalid of ['malformed','wrong_namespace'] as const)test(`revoked scope preserves and blocks ${invalid} old-hash legacy rows`,async t=>{
    const f=await fixture();t.after(f.close);const capture=await f.store.captureScope(control,namespace),item=saved(1);
    const value=invalid==='malformed'?{schema:1,scope:scopeFor(),title:'Unproven',intent:{unexpected:true}}:saved(1,scopeFor(),'bc'.repeat(16)).value;
    f.legacy.values.set(item.key,value);const cold=new PrivateIntentStore(f.directory,f.legacy),result=await cold.retireCapturedScope(capture,'12'.repeat(16));
    assert.equal(result.complete,false);assert.ok(result.blocked.length);assert.ok(result.legacy_remaining.includes(item.key));assert.deepEqual(f.legacy.get(item.key),value);
    assert.throws(()=>cold.keys());
});

test('scope proof syntax retains the full bounded mixed-family file roster',async t=>{
    const f=await fixture();t.after(f.close);const capture=await f.store.captureScope(control,namespace);
    const physical={dev:'1',ino:'2',born:'3'},proof=(name:string)=>({name,sha256:'ab'.repeat(32),physical});
    const files:typeof capture.files=[];
    // Syntax/size fixture only: no such file is adopted as authority. Ten
    // reserved split records plus54 small setup records fit the slot catalog.
    for(let index=1;index<=64;index++){
        const stem=scopeFor()+'-'+index.toString(16).padStart(32,'0');files.push(proof('b-'+stem+'.json'),proof('o-'+stem+'.json'));
        const slots=index<=10?acceptedIntentSlots(validateSplitRecover(splitFrozenFixture().intent)):['setup_reserved','setup_prepared','setup_ready'];
        for(const slot of slots)files.push(proof('a-'+stem+'-'+slot+'.json'));
    }
    assert.ok(files.length>128);assert.ok(files.length<=PRIVATE_INTENT_CAPTURE_FILES);
    const value={...capture,files};assert.deepEqual(validateScopeRetirementCapture(value).files,files);
    const wrong={...value,files:[...files,{...files[0],name:'a-'+scopeFor()+'-'+ 'ff'.repeat(16)+'-phase.json'}]};
    assert.throws(()=>validateScopeRetirementCapture(wrong),'65 distinct request IDs are not a supported captured scope');
});

test('cold captured retirement never creates or adopts a missing or replaced private root',async t=>{
    const f=await fixture();t.after(f.close);const capture=await f.store.captureScope(control,namespace),moved=f.directory+'-original';
    fs.renameSync(f.directory,moved);const cold=new PrivateIntentStore(f.directory,f.legacy);
    await assert.rejects(cold.retireCapturedScope(capture,'12'.repeat(16)));assert.equal(fs.existsSync(f.directory),false);
    fs.mkdirSync(f.directory,{mode:0o700});fs.writeFileSync(path.join(f.directory,'foreign'),'preserve');
    await assert.rejects(cold.retireCapturedScope(capture,'12'.repeat(16)),/private_intent_scope_capture_crossed/);
    assert.equal(fs.readFileSync(path.join(f.directory,'foreign'),'utf8'),'preserve');assert.ok(fs.existsSync(path.join(moved,'owned.json')));
});

test('changed or newly introduced predecessor records block cleanup without erasing them',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1);await f.store.create(item.key,item.value);
    const capture=await f.store.captureScope(control,namespace),before=fs.readdirSync(f.directory).filter(name=>name.startsWith('b-'));
    await f.store.replace(item.key,{...item.value,nativeObservation:{changed:true}});
    const extra=saved(2);await f.store.create(extra.key,extra.value);
    const result=await f.store.retireCapturedScope(capture,'12'.repeat(16));assert.equal(result.complete,false);assert.ok(result.blocked.length);
    assert.ok(f.store.get(item.key));assert.ok(f.store.get(extra.key));assert.ok(fs.existsSync(path.join(f.directory,before[0])));
});

test('interrupted predecessor cleanup resumes from its same immutable operation fence',async t=>{
    const f=await fixture();t.after(f.close);for(const index of [1,2]){const item=saved(index);await f.store.create(item.key,item.value);await f.store.replace(item.key,{...item.value,nativeObservation:{known:index}});}
    const foreignScope=scopeFor(path.resolve('/foreign-control'),'bc'.repeat(16)),foreign=saved(3,foreignScope,'bc'.repeat(16));
    await f.store.initialize(foreignScope);await f.store.create(foreign.key,foreign.value);
    const capture=await f.store.captureScope(control,namespace),native=require('node:fs') as typeof fs,unlink=native.unlinkSync;let removed=0;
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){unlink(file);if(String(file).includes(path.sep+'b-')&&++removed===1)throw new Error('fixture interruption');});
    await assert.rejects(f.store.retireCapturedScope(capture,'12'.repeat(16)),/fixture interruption/);t.mock.restoreAll();
    assert.equal(fs.readdirSync(f.directory).filter(name=>name.startsWith('b-'+scopeFor()+'-')).length,1);
    assert.equal(fs.readdirSync(f.directory).filter(name=>name.startsWith('o-'+scopeFor()+'-')).length,2);
    const reloaded=new PrivateIntentStore(f.directory,f.legacy);await reloaded.initialize(scopeFor());
    await assert.rejects(reloaded.retireCapturedScope(capture,'34'.repeat(16)),/private_intent_scope_retirement_conflict/);
    const receipt=await reloaded.retireCapturedScope(capture,'12'.repeat(16));assert.equal(receipt.complete,true);assert.equal(receipt.removed_files.length,4);
    assert.deepEqual(reloaded.get(foreign.key),foreign.value);
});

test('a scoped live publisher blocks retirement completion and rolls back only its unacknowledged base',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1),capture=await f.store.captureScope(control,namespace);
    const other=new PrivateIntentStore(f.directory,f.legacy);await other.initialize(scopeFor());
    const native=require('node:fs') as typeof fs,link=native.linkSync;let intercepted=false,retirement:ReturnType<PrivateIntentStore['retireCapturedScope']>|undefined;
    t.mock.method(native,'linkSync',function(existing:fs.PathLike,destination:fs.PathLike){
        if(!intercepted&&String(destination).includes(path.sep+'b-')){
            intercepted=true;
            // This is another window's complete public cleanup call while the
            // first writer's real stage exists, immediately before base link.
            retirement=other.retireCapturedScope(capture,'12'.repeat(16));
        }
        link(existing,destination);
    });
    await assert.rejects(f.store.create(item.key,item.value),/private_intent_scope_retired/);
    assert.ok(intercepted);const receipt=await retirement;assert.ok(receipt);
    assert.equal(receipt.complete,false,'a live old-scope stage cannot outlive a complete cleanup receipt');
    assert.equal(fs.readdirSync(f.directory).some(name=>name.startsWith('b-')),false,'failed pre-ACK publication must remove only its own fresh base');
    const resumed=await other.retireCapturedScope(capture,'12'.repeat(16));assert.equal(resumed.complete,true);
});

test('failed pre-ACK creator preserves another writer new generation and observation at the same key',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1),other=new PrivateIntentStore(f.directory,f.legacy);await other.initialize(scopeFor());
    const native=require('node:fs') as typeof fs,unlink=native.unlinkSync;let armed=true;
    let observed:Promise<void>|undefined;
    const foreign={...item.value,title:'Another reviewed generation',nativeObservation:{generation:'other'}};
    t.mock.method(native,'unlinkSync',function(file:fs.PathLike){
        unlink(file);
        if(armed&&path.basename(String(file)).startsWith('.publish-'+scopeFor()+'-')){
            armed=false;
            // The first base is linked and its temporary is now gone, while
            // its creator has not yet acknowledged the original publication.
            const base=path.join(f.directory,`b-${scopeFor()}-${'1'.padStart(32,'0')}.json`),replacement=path.join(f.root,'peer-generation.json');
            const original=JSON.parse(fs.readFileSync(base,'utf8'));
            fs.writeFileSync(replacement,JSON.stringify({...original,generation:randomUUID(),value:foreign}),{mode:0o600});
            // A completed peer generation is atomically published at the OS
            // boundary; it must never be erased by the first creator's rollback.
            fs.renameSync(replacement,base);
            observed=other.replace(item.key,{...foreign,nativeObservation:{generation:'other',known:true}});
        }
    });
    await assert.rejects(f.store.create(item.key,item.value),/private_intent_create_unconfirmed/);
    await observed;assert.equal(armed,false);
    assert.deepEqual(f.store.get(item.key),{...foreign,nativeObservation:{generation:'other',known:true}});
    assert.equal(fs.readdirSync(f.directory).filter(name=>/^[bo]-/.test(name)).length,2);
});

test('a scoped observation publisher cannot outlive cleanup or discard the captured pending base',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1);await f.store.create(item.key,item.value);
    const capture=await f.store.captureScope(control,namespace),other=new PrivateIntentStore(f.directory,f.legacy);await other.initialize(scopeFor());
    const native=require('node:fs') as typeof fs,rename=native.renameSync;let retirement:ReturnType<PrivateIntentStore['retireCapturedScope']>|undefined;
    t.mock.method(native,'renameSync',function(from:fs.PathLike,to:fs.PathLike){
        if(path.basename(String(to)).startsWith('o-'+scopeFor()+'-'))retirement=other.retireCapturedScope(capture,'12'.repeat(16));
        rename(from,to);
    });
    await assert.rejects(f.store.replace(item.key,{...item.value,nativeObservation:{known:true}}),/private_intent_scope_retired/);
    const result=await retirement;assert.ok(result);assert.equal(result.complete,false);
    assert.deepEqual(f.store.get(item.key),item.value);assert.equal(fs.readdirSync(f.directory).some(name=>name.startsWith('o-')),false);
    const resumed=await other.retireCapturedScope(capture,'12'.repeat(16));assert.equal(resumed.complete,true);
});

test('an actual foreign-scoped publisher is preserved while predecessor cleanup completes',async t=>{
    const f=await fixture();t.after(f.close);const capture=await f.store.captureScope(control,namespace);
    const foreignScope=scopeFor(path.resolve('/foreign-control'),'bc'.repeat(16)),item=saved(1,foreignScope,'bc'.repeat(16));
    const other=new PrivateIntentStore(f.directory,f.legacy);await other.initialize(foreignScope);
    const native=require('node:fs') as typeof fs,link=native.linkSync;let retirement:ReturnType<PrivateIntentStore['retireCapturedScope']>|undefined,armed=true;
    t.mock.method(native,'linkSync',function(existing:fs.PathLike,destination:fs.PathLike){
        if(armed&&path.basename(String(destination)).startsWith('b-'+foreignScope+'-')){armed=false;retirement=f.store.retireCapturedScope(capture,'12'.repeat(16));}
        link(existing,destination);
    });
    await other.create(item.key,item.value);const result=await retirement;assert.ok(result);assert.equal(result.complete,true);
    assert.deepEqual(other.get(item.key),item.value);
});

test('an unattributed stage claiming foreign metadata never receives a scope cleanup exemption',async t=>{
    const f=await fixture();t.after(f.close);const capture=await f.store.captureScope(control,namespace),stage=path.join(f.directory,'.publish-'+randomUUID());
    fs.writeFileSync(stage,JSON.stringify({scope:scopeFor(path.resolve('/foreign-control'),'bc'.repeat(16))}),{mode:0o600});
    const result=await f.store.retireCapturedScope(capture,'12'.repeat(16));assert.equal(result.complete,false);
    assert.ok(result.blocked.some(value=>value.includes('publisher_in_flight')));assert.ok(fs.existsSync(stage));
});

test('failed stage writes remove only their own temporary and never acknowledge a request',async t=>{
    const f=await fixture();t.after(f.close);const item=saved(1),native=require('node:fs') as typeof fs;
    t.mock.method(native,'fsyncSync',()=>{throw new Error('fixture fsync failure');});
    await assert.rejects(f.store.create(item.key,item.value),/fixture fsync failure/);t.mock.restoreAll();
    assert.equal(fs.readdirSync(f.directory).some(name=>name.startsWith('.publish-')),false);assert.equal(f.store.get(item.key),undefined);
});

test('interrupted setup removes only its own still-empty root and can initialize after trust returns',async t=>{
    const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'airk-private-intent-'))),directory=path.join(root,'journal');t.after(()=>fs.promises.rm(root,{recursive:true,force:true}));
    let trusted=true;const store=new PrivateIntentStore(directory,new Memory(),()=>trusted);
    const acl=require('../privateDirectory');
    t.mock.method(acl,'restrictWindowsDirectory',async()=>{trusted=false;throw new Error('fixture interrupted ACL');});
    await assert.rejects(store.initialize(scopeFor()),/fixture interrupted ACL/);assert.equal(fs.existsSync(directory),false);
    trusted=true;t.mock.restoreAll();await store.initialize(scopeFor());assert.deepEqual(store.keys(),[]);
});

test('setup interruption preserves a replacement directory and its foreign contents',async t=>{
    const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'airk-private-intent-'))),directory=path.join(root,'journal');t.after(()=>fs.promises.rm(root,{recursive:true,force:true}));
    const store=new PrivateIntentStore(directory,new Memory()),acl=require('../privateDirectory');
    t.mock.method(acl,'restrictWindowsDirectory',async()=>{fs.renameSync(directory,directory+'-original');fs.mkdirSync(directory);fs.writeFileSync(path.join(directory,'foreign'),'keep');throw new Error('fixture swapped root');});
    await assert.rejects(store.initialize(scopeFor()),/fixture swapped root/);assert.equal(fs.readFileSync(path.join(directory,'foreign'),'utf8'),'keep');
});
