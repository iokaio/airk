// SPDX-License-Identifier: Apache-2.0
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { phaseRows, selected, associate, scopeBinding, invoke, verifySearch, capturePhase } = require('./phase.cjs');
const ids = Array.from({length: 100}, (_, i) => 'q' + i);
const questions = ids.map(id => ({id, split: 'calibration', question: 'question ' + id}));
const publication = {collection:'a', token:'p'};
const grant = {scope_identity:'a'.repeat(64),catalog_revision:1};
const member = {index_id:'i',source_root_ids:['r'],source_revision:2,policy_epoch:0,policy_digest:null,memberships:[]};
const obs = {schema:1,selection:{owner:'agent',catalog_revision:1}, qualifier:'selected_subset', fingerprint:'1'.repeat(64),
    members:[member]};
const windowObs = {...obs,selection:{...grant,owner:'window'},members:[{...member,
    memberships:[{project_id:'p',membership_revision:1}]}]};
const binding = {instance:'s', index_id:'i', source_root_ids:['r'], publication, scope_observation:obs,
    focus_manifest:'f', context_manifest:'c', pipeline:'sem',window_selection:{project_ids:['p']},files:753,indexed_vectors:767};
const status = {scope_observation:windowObs,files:753,indexed_vectors:767,semantic_readiness:{state:'ready'},
    collection:'a', publication:{token:'p'}};
const template = {question:'orig', budget:{input_tokens:8192,deadline_ms:120000},include_status:true,response_mode:'full'};

test('matches all thirteen frozen phase ranges and refuses membership drift', () => {
    assert.deepEqual(Array.from({length:13}, (_,i)=>phaseRows(questions,ids,i).length), [8,8,8,8,8,8,8,8,8,8,8,8,4]);
    assert.throws(()=>phaseRows(questions,ids.toReversed(),0),/CAL order/);
    assert.throws(()=>phaseRows(questions,ids,13),/phase ordinal/);
});
test('checks native window status against the original publication, member and source policy', () => {
    const bound = scopeBinding(status,binding,grant);
    selected(status,bound);
    assert.throws(()=>scopeBinding({...status,scope_observation:obs},binding,grant),/window/);
    assert.throws(()=>scopeBinding({...status,files:9},binding,grant),/coverage/);
    assert.throws(()=>scopeBinding({...status,collection:'b'},binding,grant),/publication/);
    assert.throws(()=>selected({...status,scope_observation:{...obs,fingerprint:'wrong'}},bound),/selection/);
});
test('requires the full served MiniLM publication on every delivered Search', () => {
    const good = {publication, semantic_readiness:{state:'ready',pipeline_digest:'sem'},
        execution:{served_mode:'hybrid',incomplete:false}};
    verifySearch(good,binding);
    assert.throws(()=>verifySearch({...good,semantic_readiness:{state:'ready',pipeline_digest:'foreign'}},binding),/MiniLM/);
    assert.throws(()=>verifySearch({...good,execution:{served_mode:'lexical',incomplete:false}},binding),/MiniLM/);
    assert.throws(()=>verifySearch({...good,publication:{...publication,token:'new'}},binding),/MiniLM/);
});
test('requires matching admission, strict zero work and original verified probe roster', () => {
    const scope = {publication};
    const records = [{id:1,canonical_result:{value:'original'}}];
    const request = {question:'q',records,read_attempt:{request_id:'1'.repeat(32)},scope,ordinal:2,roster:{basis:'b'}};
    const record = {schema:'airk.private-selected-Context-native-admission-observation/1',request,
        Context_NN_dispatch:false,Focus_configuration:'ON',actual_Context_profile:{manifest:'c'},
        actual_Focus_profile:'f',MiniLM_pipeline:'sem',planned_input_digest:'d',verified_probe_requests:[]};
    const advice = {input_digest:'d',observed_input_tokens:0,work:{input_tokens:0},outcome:{status:'unavailable'}};
    const delivered = {request_id:'2'.repeat(32),records:[...records,{id:2}],assessment:{scope},assessment_history:[{ordinal:2,basis:'b',result:{advice}}]};
    assert.deepEqual(associate(record,delivered,'q',binding),[]);
    assert.throws(()=>associate(record,{...delivered,records:[{id:9}]},'q',binding),/source record association/);
    assert.throws(()=>associate({...record,Context_NN_dispatch:true},delivered,'q',binding),/admission/);
    advice.observed_input_tokens = 1;
    assert.throws(()=>associate(record,delivered,'q',binding),/non-dispatch/);
});
test('deadline refusal never submits or replays an uncertain call', async () => {
    let calls = 0;
    const owner = {checkRemaining(){throw Error('expired')},session:{invoke(){calls++}}};
    await assert.rejects(invoke(owner,binding,'answer',template),/expired/);
    assert.equal(calls,0);
});
test('retains all four refused questions and orphans on the final phase; no probes', async () => {
    const events=[], persisted=[];
    const owner = {
        session:{ hello:{serviceInstance:'s'},async invoke(argv,opts){
            assert.equal(opts.deadlineMs,125000); assert.equal(argv[3],'--args');
            if(argv[2]==='status') return reply({...status,scope_observation:{...windowObs,
                fingerprint:(events.length+1).toString(16).padStart(64,'0')}});
            assert.equal(argv[2],'answer');
            return reply({error:{code:'verification_reply_budget',retryable:false,verification_performed:false}},true);
        }},
        checkRemaining(){},async beginQuestion(){return grant},async endQuestion(){},async retainExchange(argv){events.push(argv[2])},async beginPhase(){},
        async observationCursor(){return 0},async observationsAfter(){return [{orphan:true}]},
        async completeQuestion(row){persisted.push(row)},async closePhase(phase,ids){assert.deepEqual(ids,['q96','q97','q98','q99'])}
    };
    function reply(body,refused=false) {
        return {exitCode:refused?1:0, class:refused?'refused':'success',
            stdout:JSON.stringify({isError:refused,structuredContent:body})};
    }
    await capturePhase(owner,binding,{questions,ids,answer:template},12);
    assert.deepEqual(events,['status','answer','status','answer','status','answer','status','answer']);
    assert.equal(persisted.length,4);
    assert.ok(persisted.every(row=>row.answer.refused && row.probes.length===0 && row.captured[0].orphan));
});
