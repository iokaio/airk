// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {chooseRuntimeUpgradeMode,RUNTIME_UPGRADE_CHOICES,validateRebuildObservation,rebuildNotice,RebuildNotices,runtimeCleanBreakRequired,validateCleanBreakPolicy,cleanBreakSuccessorPublished} from '../runtimeRebuild';

test('discarded predecessor request authority follows published native Genesis independently of index cleanup',()=>{
    const observed={...observation(),stage:'indexing' as const,mode:'clean_break',indexes_complete:true,expected_indexes:1,cleanup:{complete:false,state:'waiting_for_indexes',failure:null,observation:null}},binding={runtimeSha256:observed.candidate,controlDir:observed.successor_control};
    assert.equal(cleanBreakSuccessorPublished(observed,binding,'linux'),true);
    assert.equal(cleanBreakSuccessorPublished({...observed,stage:'preparing'},binding,'linux'),true,'an activated registry may be published before jobs start');
    assert.equal(cleanBreakSuccessorPublished({...observed,stage:'failed'},binding,'linux'),true,'later index failure cannot restore predecessor request authority');
    for(const changed of [{stage:'accepted'},{indexes_complete:false},{mode:undefined},{successor_namespace:null},{successor_namespace:observed.original_namespace},{candidate:'ff'.repeat(32)},{successor_control:'/foreign/control'},{preserved_control:binding.controlDir}]){
        assert.equal(cleanBreakSuccessorPublished({...observed,...changed} as Parameters<typeof cleanBreakSuccessorPublished>[0],binding,'linux'),false);
    }
    assert.equal(cleanBreakSuccessorPublished({...observed,successor_control:'\\\\?\\D:\\AIRK\\fresh\\control',preserved_control:'D:\\AIRK\\old\\control'},{...binding,controlDir:'D:\\AIRK\\fresh\\control'},'win32'),true);
});

test('clean break presentation follows the baseline and native state contract without accepting a version label as proof',()=>{
    const current={required:false,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:{schema:1},future_policy:{retained:true}};
    assert.equal(validateCleanBreakPolicy(current),current);
    for(const previous of ['1.7.6','1.8.11','1.8.15'])assert.equal(runtimeCleanBreakRequired(previous,'1.8.16'),true);
    assert.equal(runtimeCleanBreakRequired('1.8.16','1.8.16'),true,'missing native contract cannot be treated as a completed installation');
    assert.equal(runtimeCleanBreakRequired('1.8.16','1.9.0',current),false);
    assert.equal(runtimeCleanBreakRequired('1.8.16','1.9.0',{...current,required:true}),true);
    assert.equal(runtimeCleanBreakRequired('1.9.0','1.8.16'),false,'presentation must preserve a newer installed pin');
    assert.equal(runtimeCleanBreakRequired('1.4.0','1.4.1'),false,'historical fixtures retain their historical behavior');
    assert.throws(()=>runtimeCleanBreakRequired('1.8.15','1.8.16',current),/invalid_native_clean_break_policy/);
    for(const version of ['01.8.16','1.8.16-beta','1.8','v1.8.16'])assert.throws(()=>runtimeCleanBreakRequired(version,'1.9.0'),/invalid_runtime_version/);
    assert.throws(()=>runtimeCleanBreakRequired('1.8.16','1.9.0',{...current,state_contract:'future-state'}),/invalid_native_clean_break_policy/);
});

function observation(stage='indexing'){
    const original={binding:'11'.repeat(32),root:'/original/source',data_root:'/preserved/index',collection:'original'};
    const replacement={...original,binding:'22'.repeat(32),data_root:'/fresh/index'};
    return {schema:1 as const,operation_id:'33'.repeat(16),stage,provenance:'native_rebuild_from_saved_configuration',
        candidate:'44'.repeat(32),predecessor:'55'.repeat(32),original_namespace:'66'.repeat(16),
        preserved_state:'/preserved/state',preserved_control:'/preserved/state/control',preserved_index_roots:[original.data_root],
        successor_state:'/fresh/state',successor_control:'/fresh/state/control',successor_namespace:'77'.repeat(16),original_failure:null,failure:null,index_scope:null,
        indexes:[{original,replacement,sources:{docs:'/original/docs'},settings_sha256:'88'.repeat(32),original_paused:false,paused:stage==='paused',watching:false,
            job:stage==='ready'?{id:'99'.repeat(32),project:replacement.binding,kind:'index',state:'succeeded',fencing_generation:1,attempts:1,outcome:{state:'succeeded',revision:null}}:null,
            readiness:stage==='ready'?'ready':stage==='paused'?'paused':stage==='failed'?'failed':stage==='unavailable'?'unavailable':'pending',failure:stage==='failed'?{code:'source_unavailable',message:'Original source is unavailable'}:stage==='unavailable'?{code:'rebuild_observation_unavailable',message:'Original job metadata is unavailable'}:null}],
        future_native_observation:{complete:'retained'}};
}

test('clean break progress preserves all index observations and distinguishes ready indexes from completed cleanup',()=>{
    const value={...observation('ready'),mode:'clean_break',indexes_complete:true,expected_indexes:1,cleanup:{complete:false,state:'removing',failure:null,observation:null,future_receipt:{retained:true}}};
    assert.equal(validateRebuildObservation(value),value);
    assert.match(rebuildNotice(value).message,/cleanup is pending/);
    assert.notEqual(rebuildNotice(value).key,rebuildNotice({...value,cleanup:{complete:true,state:'complete'}}).key);
    assert.match(rebuildNotice({...value,cleanup:{complete:true,state:'complete'}}).message,/owned legacy native artifacts were removed/);
    assert.doesNotMatch(rebuildNotice({...value,cleanup:{complete:true,state:'complete'}}).message,/clean installation is ready/,'native readiness cannot assert editor client cleanup');
    assert.equal(validateRebuildObservation({...value,indexes:[],expected_indexes:0}).indexes.length,0);
    assert.throws(()=>validateRebuildObservation({...value,indexes:[],indexes_complete:false}),/invalid_native_clean_break_roster/);
    assert.equal(validateRebuildObservation({...value,stage:'accepted',indexes:[],indexes_complete:false,successor_namespace:null}).indexes.length,0);
    assert.throws(()=>validateRebuildObservation({...observation(),indexes:[]}),/invalid_native_rebuild_observation/);
    assert.throws(()=>validateRebuildObservation({...value,cleanup:undefined}),/invalid_native_clean_break_observation/);
    assert.doesNotMatch(rebuildNotice({...value,stage:'indexing'}).message,/Previous state is preserved/);
});

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
