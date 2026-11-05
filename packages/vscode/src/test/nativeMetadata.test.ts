// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {materializeNativeMetadata,NativeMetadataProjectionCache,nativeHeaderTemplateDigest,NATIVE_HEADER_REFERENCE} from '../nativeMetadata';
import {object,Value} from '../workbenchContracts';
import {CurrentTurnProjections,evidenceCards,renderExport} from '../workbenchRender';
import {newSession} from '../workbenchSessions';
const idx=(i:number)=>'idx_'+i.toString(16).padStart(32,'0');
const pub=(n:number)=>({collection:'fixture',incarnation:'12'.repeat(16),generation:n,revision:'ab'.repeat(32),token:`airk-r1:${'12'.repeat(16)}:${n}:${'ab'.repeat(32)}`});
function fixture(count=2):Value {
 const template={fields:{request_id:null,scope_observation:{index_id:null,observed_unix_ms:null},publication:{member:null},execution:{incomplete:null}},slots:[
  {path:['request_id'],kind:'string'},{path:['scope_observation','index_id'],kind:'string'},{path:['scope_observation','observed_unix_ms'],kind:'integer'},
  {path:['publication','member'],kind:'integer'},{path:['execution','incomplete'],kind:'boolean'}]};
 const id=nativeHeaderTemplateDigest(template);
 return {schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-native-header-templates/1',values:{[id]:template}},delivery_observation:{digest:'original-only'},
 publication_vector:{authority:'ab'.repeat(32),members:Array.from({length:count},(_,i)=>({index_id:idx(i),publication:pub(i+1)}))},
 members:Array.from({length:count},(_,i)=>({index_id:idx(i),outcome:'complete',result:{[NATIVE_HEADER_REFERENCE]:{id,values:['request-'+i,idx(i),100+i,i,i%2===0]},hits:[{text:'hit '+i,citation:{path:'same.md',start_line:1,end_line:1,content_hash:'cd'.repeat(32)}}]}})),
 ranking:Array.from({length:count},(_,i)=>({index_id:idx(i),result_pointer:`/members/${i}/result/hits/0`,local_rank:0,rank_contribution:1/61,origin:{index_id:idx(i),project_ids:['prj_'+'01'.repeat(16)],source_root_id:'src_'+'02'.repeat(16),source_relative_path:'same.md',publication_ref:{member:i}}}))};
}
const member=(v:Value,i=0)=>object(object((v.members as Value[])[i]).result);
test('native header slots and exact index publication refs restore all32 distinct logical headers without changing raw rows',()=>{
 const raw=fixture(32),before=structuredClone(raw),logical=materializeNativeMetadata(raw);
 assert.deepEqual(raw,before);assert.equal(logical.delivery_observation,undefined);
 for(let i=0;i<32;i++){
  const body=member(logical,i);assert.equal(body.request_id,'request-'+i);assert.deepEqual(body.scope_observation,{index_id:idx(i),observed_unix_ms:100+i});
  assert.deepEqual(body.publication,pub(i+1));assert.deepEqual(body.execution,{incomplete:i%2===0});assert.deepEqual(body.hits,member(raw,i).hits);
  assert.deepEqual(object((logical.ranking as Value[])[i].origin).publication,pub(i+1));
 }
 assert.equal(evidenceCards(logical).length,32);
});
test('header validation refuses malformed definitions, typed slots, identities and cross-index publication references',()=>{
 const mutations:Array<(v:Value)=>void>=[
 v=>{member(v)[NATIVE_HEADER_REFERENCE]={id:'ff'.repeat(32),values:[]};},
 v=>{object(member(v)[NATIVE_HEADER_REFERENCE]).values=[];},
 v=>{(object(member(v)[NATIVE_HEADER_REFERENCE]).values as unknown[])[2]='100';},
 v=>{(object(member(v)[NATIVE_HEADER_REFERENCE]).values as unknown[])[0]='x'.repeat(8193);},
 v=>{(object(member(v)[NATIVE_HEADER_REFERENCE]).values as unknown[])[3]=1;},
 v=>{member(v).request_id='collision';},
 v=>{object((v.ranking as Value[])[0].origin).source_relative_path='C:bad';},
 v=>{object((v.ranking as Value[])[0].origin).source_relative_path='bad'+String.fromCharCode(0)+'name';},
 v=>{object((v.members as Value[])[1]).index_id=idx(0);},
 v=>{object((v.ranking as Value[])[0].origin).publication_ref={member:1};},
 v=>{object((v.ranking as Value[])[0].origin).publication_ref={member:0,extra:true};},
 v=>{object(object((object(v.publication_vector).members as Value[])[0]).publication).token='invalid';},
 v=>{const t=Object.values(object(object(v.shared_values).values))[0] as Value;object(t.fields).unknown=true;},
 v=>{const t=Object.values(object(object(v.shared_values).values))[0] as Value;(t.slots as Value[]).push((t.slots as Value[])[0]);},
 v=>{const t=Object.values(object(object(v.shared_values).values))[0] as Value;object((t.slots as Value[])[0]).path=['missing'];},
 ];
 for(const mutate of mutations){const v=fixture();mutate(v);const before=structuredClone(v);assert.throws(()=>materializeNativeMetadata(v));assert.deepEqual(v,before);assert.ok(new NativeMetadataProjectionCache().view(v).error);}
});
test('publication-only frames and legacy inline frames share the exact logical contract',()=>{
 const raw=fixture();delete raw.shared_values;for(const row of raw.members as Value[]){const body=object(row.result);delete body[NATIVE_HEADER_REFERENCE];body.publication={member:0};}object((raw.members as Value[])[1].result).publication={member:1};
 const logical=materializeNativeMetadata(raw);assert.deepEqual(member(logical,1).publication,pub(2));
 const inline={schema:'airk.combined-read/1',members:[{index_id:idx(0),result:{publication:pub(1)}}]};assert.equal(materializeNativeMetadata(inline),inline);
});
test('Workbench display/cache/export adapter uses derived headers but retains raw templates, typed slots and receipt only in full original response',()=>{
 const raw=fixture(),envelope={status:'complete',delivery_observation:{digest:'outer-raw'},steps:[{id:'__proto__',tool:'search',status:'complete',result:raw}]};
 const cache=new NativeMetadataProjectionCache(),session=newSession('ab'.repeat(32));session.turns=[{id:'turn',at:1,input:'query',format:'structured',envelope}];
 const view=cache.view(envelope);assert.ok(view.payload);assert.equal(view.payload.delivery_observation,undefined);
 const display=new CurrentTurnProjections().project(session.turns,()=>view.payload);assert.equal(display.cards.turn.length,2);assert.equal(display.cards.turn[1].publication,pub(2).token);
 const full=JSON.parse(renderExport(session,'json',undefined,undefined,()=>view.payload));assert.deepEqual(full.turns[0].envelope,envelope);
 const selected=JSON.parse(renderExport(session,'json','turn',['record-0'],()=>view.payload)).turns[0].envelope;
 assert.equal(selected.delivery_observation,undefined);assert.deepEqual(selected.evidence[0].origin.publication,pub(1));
 assert.deepEqual(selected.original_native_metadata[0].shared_values,raw.shared_values);
 assert.deepEqual(selected.original_native_metadata[0].members[0][NATIVE_HEADER_REFERENCE],member(raw)[NATIVE_HEADER_REFERENCE]);
 assert.ok(renderExport(session,'markdown',undefined,undefined,()=>view.payload).includes('outer-raw'));
 assert.equal(cache.view(envelope),view);cache.clear();assert.notEqual(cache.view(envelope),view);
});

test('header digest matches the native canonical golden',()=>{assert.equal(nativeHeaderTemplateDigest({fields:{freshness:{observed_unix_ms:null,state:'unavailable'}},slots:[{kind:'integer',path:['freshness','observed_unix_ms']}]}),'3c2b1d950550b1d7514895aa1645c730a1221132ba50b864215167c9d49f3bc9');});

test('native header table enforces static object and key bounds before interpretation',()=>{for(const fields of [{freshness:Object.fromEntries(Array.from({length:1025},(_,i)=>['k'+i,null]))},{freshness:{['x'.repeat(129)]:null}}]){const raw=fixture(),template={fields,slots:[]},id=nativeHeaderTemplateDigest(template);raw.shared_values={schema:'airk.shared-native-header-templates/1',values:{[id]:template}};for(const row of raw.members as Value[])object(row.result)[NATIVE_HEADER_REFERENCE]={id,values:[]};assert.throws(()=>materializeNativeMetadata(raw));}});
