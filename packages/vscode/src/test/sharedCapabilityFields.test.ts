// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {sharedCapabilityDigest,SharedCapabilityDisplayCache,verifySharedCapabilityFields} from '../sharedCapabilityFields';
import {materializeCapabilityMember,SHARED_CAPABILITY_REFERENCE} from '../workbenchRender';
import {newSession} from '../workbenchSessions';
import {object,Value} from '../workbenchContracts';

function fixture(){
    const fields={adapters:[{id:'original'}],workbench:{operations:{'10':{type:'object'},'2':{type:'object'}}}},id=sharedCapabilityDigest(fields);
    const body={request_admission:{limit:16},scope_observation:{fingerprint:'11'.repeat(32)},[SHARED_CAPABILITY_REFERENCE]:id,future_inline:{complete:true}};
    const result={schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-capability-fields/1',values:{[id]:fields}},
        members:[{index_id:'idx_'+'12'.repeat(16),result:{content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body,isError:false}}]};
    return {fields,id,body,result};
}
test('shared capability digest matches the native framed lexical-key golden including integer-like keys',()=>{
    assert.equal(sharedCapabilityDigest({workbench:{'10':'a','2':'b'}}),'ba2e0fd072e154161d7a53c77f61ea42c8dc4f8d9f6f4beaf9f7dc0a9824e5e0');
    assert.equal(sharedCapabilityDigest({workbench:{'2':'b','10':'a'}}),sharedCapabilityDigest({workbench:{'10':'a','2':'b'}}));
});
test('host verifies shared native groups once and materializes complete member fields without mutating raw results',()=>{
    const f=fixture(),before=structuredClone(f.result),display=verifySharedCapabilityFields(f.result)!;
    const body=materializeCapabilityMember(f.result.members[0].result,display);
    assert.deepEqual(body,{...f.fields,request_admission:f.body.request_admission,scope_observation:f.body.scope_observation,future_inline:{complete:true}});
    assert.deepEqual(f.result,before);assert.equal(Object.hasOwn(body,SHARED_CAPABILITY_REFERENCE),false);assert.equal(display.state,'verified');
    f.result.members[0].result.structuredContent.future_inline={complete:false};
    assert.equal(object(materializeCapabilityMember(f.result.members[0].result,display).future_inline).complete,false,'distinct native fields remain per member');
});
test('host rejects unresolved, mismatched, colliding, nested and unsafe shared namespaces without source work',()=>{
    const mutations:Array<(f:ReturnType<typeof fixture>)=>void>=[
        f=>{delete (f.result as Value).shared_values;},
        f=>{f.result.shared_values.schema='unknown';},
        f=>{(f.result.shared_values as Value).extra=true;},
        f=>{f.result.shared_values.values[f.id].adapters[0].id='changed';},
        f=>{(f.result.members[0].result.structuredContent as Value).adapters=[];},
        f=>{f.result.members[0].result.structuredContent[SHARED_CAPABILITY_REFERENCE]='ff'.repeat(32);},
        f=>{(f.result.members[0].result.structuredContent as Value).runtime={[SHARED_CAPABILITY_REFERENCE]:f.id};},
        f=>{(f.result.shared_values.values[f.id] as Value).runtime={};},
        f=>{Object.defineProperty(f.result.shared_values.values[f.id],'constructor',{value:'unsafe',enumerable:true});},
        f=>{(f.result.shared_values.values[f.id] as Value).workbench={[SHARED_CAPABILITY_REFERENCE]:f.id};},
        f=>{(f.result.shared_values.values as Record<string,unknown>)['ef'.repeat(32)]={adapters:[],workbench:{operations:{}}};},
    ];
    for(const mutate of mutations){const f=fixture();mutate(f);assert.throws(()=>verifySharedCapabilityFields(f.result));}
    assert.throws(()=>sharedCapabilityDigest({number:0.5}));assert.throws(()=>sharedCapabilityDigest({number:Number.MAX_SAFE_INTEGER+1}));
    assert.throws(()=>sharedCapabilityDigest({value:'\ud800'}));assert.throws(()=>sharedCapabilityDigest({'naïve':true}));
});
test('nonshareable original inline keys survive while malformed tables cannot alter object prototypes',()=>{
    const f=fixture();Object.defineProperty(f.result.members[0].result.structuredContent,'constructor',{value:{native:'inline'},enumerable:true});
    const display=verifySharedCapabilityFields(f.result)!;
    assert.deepEqual(materializeCapabilityMember(f.result.members[0].result,display).constructor,{native:'inline'});
    assert.equal((Object.prototype as Value).native,undefined);
});
test('verified shared capability displays require one to thirty-two distinct typed native index identities',()=>{
    for(const id of [undefined,null,7,'idx_bad','prj_'+'12'.repeat(16),'idx_'+'AB'.repeat(16)]){
        const f=fixture();(f.result.members[0] as Value).index_id=id;const raw=structuredClone(f.result);
        assert.throws(()=>verifySharedCapabilityFields(f.result),/member identity/);assert.deepEqual(f.result,raw,'refusing display validation preserves complete native data');
    }
    const duplicate=fixture();duplicate.result.members.push(structuredClone(duplicate.result.members[0]));assert.throws(()=>verifySharedCapabilityFields(duplicate.result),/duplicate/);
    const empty=fixture();empty.result.members=[];assert.throws(()=>verifySharedCapabilityFields(empty.result),/combined capability result/);
    const tooMany=fixture();tooMany.result.members=Array.from({length:33},(_,i)=>({...structuredClone(tooMany.result.members[0]),index_id:'idx_'+i.toString(16).padStart(32,'0')}));assert.throws(()=>verifySharedCapabilityFields(tooMany.result),/combined capability result/);
    const valid=fixture();valid.result.members.push({...structuredClone(valid.result.members[0]),index_id:'idx_'+'13'.repeat(16)});assert.equal(verifySharedCapabilityFields(valid.result)?.state,'verified');
});
test('shared top-field names obey the same nonempty ASCII byte limit as the native contract',()=>{
    for(const key of ['', 'x'.repeat(129)]){
        const fields={[key]:'native'},id=sharedCapabilityDigest(fields),result={schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-capability-fields/1',values:{[id]:fields}},
            members:[{index_id:'idx_'+'12'.repeat(16),result:{[SHARED_CAPABILITY_REFERENCE]:id}}]};
        assert.throws(()=>verifySharedCapabilityFields(result),/namespace/);
    }
});
test('host bounds expanded member data and caches only verified common tables or explicit unavailable observations',()=>{
    const f=fixture(),cache=new SharedCapabilityDisplayCache(),session=newSession('11'.repeat(32));
    const turn={id:'capabilities',at:1,input:'/capabilities',format:'structured' as const,envelope:{steps:[{id:'capabilities',result:f.result}]}};
    session.turns.push(turn);
    const first=cache.project(session.turns);assert.equal(first[turn.id].capabilities.state,'verified');
    assert.equal(cache.project(session.turns)[turn.id],first[turn.id]);
    assert.ok(!JSON.stringify(first).includes('request_admission'),'per-member expanded bodies are never duplicated into the cache');
    const broken=structuredClone(turn.envelope);object(object((broken.steps[0].result.members[0]).result).structuredContent)[SHARED_CAPABILITY_REFERENCE]='ff'.repeat(32);turn.envelope=broken;
    assert.equal(cache.project(session.turns)[turn.id].capabilities.state,'unavailable');
    const fields={description:'x'.repeat(64000)},id=sharedCapabilityDigest(fields),large={schema:'airk.combined-read/1',shared_values:{schema:'airk.shared-capability-fields/1',values:{[id]:fields}},members:[{index_id:'idx_'+'12'.repeat(16),result:{[SHARED_CAPABILITY_REFERENCE]:id,runtime:'y'.repeat(4096)}}]};
    assert.throws(()=>verifySharedCapabilityFields(large),/byte budget/);
    cache.clear();
});
test('a valid prototype-spelled native step ID remains an own display entry through host message serialization',()=>{
    const f=fixture(),cache=new SharedCapabilityDisplayCache(),turn={id:'turn',at:1,input:'/capabilities',format:'structured' as const,
        envelope:{steps:[{id:'__proto__',result:f.result}]}};
    const original=structuredClone(turn.envelope),details=cache.project([turn]);
    assert.equal(Object.hasOwn(details.turn,'__proto__'),true);assert.equal(details.turn['__proto__'].state,'verified');
    assert.equal(JSON.parse(JSON.stringify(details)).turn['__proto__'].state,'verified');
    assert.deepEqual(turn.envelope,original);assert.equal((Object.prototype as Value).state,undefined);
});
