// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {chooseRuntimeUpgradeMode,RUNTIME_UPGRADE_CHOICES,validateRebuildObservation,rebuildNotice,RebuildNotices} from '../runtimeRebuild';

function observation(stage='indexing'){
    const original={binding:'11'.repeat(32),root:'/original/source',data_root:'/preserved/index',collection:'original'};
    const replacement={...original,binding:'22'.repeat(32),data_root:'/fresh/index'};
    return {schema:1,operation_id:'33'.repeat(16),stage,provenance:'native_rebuild_from_saved_configuration',
        candidate:'44'.repeat(32),predecessor:'55'.repeat(32),original_namespace:'66'.repeat(16),
        preserved_state:'/preserved/state',preserved_control:'/preserved/state/control',preserved_index_roots:[original.data_root],
        successor_state:'/fresh/state',successor_control:'/fresh/state/control',successor_namespace:'77'.repeat(16),original_failure:null,failure:null,index_scope:null,
        indexes:[{original,replacement,sources:{docs:'/original/docs'},settings_sha256:'88'.repeat(32),original_paused:false,paused:stage==='paused',watching:false,
            job:stage==='ready'?{id:'99'.repeat(32),project:replacement.binding,kind:'index',state:'succeeded',fencing_generation:1,attempts:1,outcome:{state:'succeeded',revision:null}}:null,
            readiness:stage==='ready'?'ready':stage==='paused'?'paused':stage==='failed'?'failed':stage==='unavailable'?'unavailable':'pending',failure:stage==='failed'?{code:'source_unavailable',message:'Original source is unavailable'}:stage==='unavailable'?{code:'rebuild_observation_unavailable',message:'Original job metadata is unavailable'}:null}],
        future_native_observation:{complete:'retained'}};
}

test('the early upgrade choice forwards only a fixed native mode and cancellation has no choice',async()=>{
    assert.equal(await chooseRuntimeUpgradeMode(async items=>{assert.equal(items,RUNTIME_UPGRADE_CHOICES);return items[1];}),'rebuild');
    assert.equal(await chooseRuntimeUpgradeMode(async items=>items[0]),'migrate');
    assert.equal(await chooseRuntimeUpgradeMode(async()=>undefined),undefined);
    await assert.rejects(chooseRuntimeUpgradeMode(async()=>({...RUNTIME_UPGRADE_CHOICES[1]})),/invalid_runtime_rebuild_choice/);
});

test('native rebuild presentation preserves the complete value and distinguishes rebuild from migration',()=>{
    const value=observation();assert.equal(validateRebuildObservation(value),value);
    assert.deepEqual(validateRebuildObservation(value).future_native_observation,{complete:'retained'});
    assert.match(rebuildNotice(value).message,/rebuilding replacement indexes/);
    assert.match(rebuildNotice(observation('ready')).message,/connections require reconnection/);
    assert.match(rebuildNotice(observation('paused')).message,/Replacement indexing is paused/);
    assert.equal(rebuildNotice(observation('failed')).warning,true);
    assert.throws(()=>validateRebuildObservation({...value,successor_control:value.preserved_control}),/invalid_native_rebuild_observation/);
    const crossed=structuredClone(value);crossed.indexes[0].replacement.root='/foreign/source';
    assert.throws(()=>validateRebuildObservation(crossed),/invalid_native_rebuild_index/);
});

test('rebuild notices persist operation and terminal stage through concurrent reads and restart',async()=>{
    const data=new Map<string,unknown>(),shown:string[]=[];
    const storage={get:<T>(key:string)=>data.get(key) as T|undefined,update:async(key:string,value:unknown)=>{data.set(key,value);}};
    const notices=new RebuildNotices(storage);
    await Promise.all([notices.report(observation(),message=>shown.push(message)),notices.report(observation('preparing'),message=>shown.push(message))]);
    assert.equal(shown.length,1);
    const reopened=new RebuildNotices(storage);
    await reopened.report(observation(),message=>shown.push(message));assert.equal(shown.length,1);
    await reopened.report(observation('paused'),message=>shown.push(message));
    await reopened.report(observation('paused'),message=>shown.push(message));assert.equal(shown.length,2);
    await reopened.report(observation('ready'),message=>shown.push(message));assert.equal(shown.length,3);
});

test('a notification storage failure remains visible and does not consume the notice',async()=>{
    let fail=true,writes=0,shown=0,saved:unknown;
    const notices=new RebuildNotices({get:<T>()=>saved as T|undefined,update:async(_key,value)=>{writes++;if(fail)throw new Error('fixture storage denied');saved=value;}});
    await assert.rejects(notices.report(observation(),()=>shown++),/fixture storage denied/);assert.equal(shown,0);
    fail=false;await notices.report(observation(),()=>shown++);assert.equal(shown,1);assert.equal(writes,2);
});

test('missing historical rebuild observations remain informational and deduplicate without inventing an execution outcome',async()=>{
    const value=observation('unavailable'),notice=rebuildNotice(value);
    assert.equal(validateRebuildObservation(value),value);
    assert.equal(notice.warning,false);
    assert.equal(notice.key,`${value.operation_id}:unavailable`);
    assert.match(notice.message,/cannot verify the historical rebuild outcome/);
    assert.match(notice.message,/current index readiness separately/);
    assert.doesNotMatch(notice.message,/could not finish|rebuilt replacement indexes|is rebuilding/);
    let saved:unknown,shown=0;
    const storage={get:<T>()=>saved as T|undefined,update:async(_key:string,keys:unknown)=>{saved=keys;}};
    await new RebuildNotices(storage).report(value,()=>shown++);
    await new RebuildNotices(storage).report(value,()=>shown++);
    assert.equal(shown,1);
    assert.deepEqual((validateRebuildObservation(value).indexes[0] as any).failure,{code:'rebuild_observation_unavailable',message:'Original job metadata is unavailable'});
});
