// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {nativeVerification,selectedVerification,verificationLabel} from '../sourceVerification';
import {discoveryDetails,evidenceCards,markdown,renderExport} from '../workbenchRender';
import {Value,WorkbenchSession,WorkbenchTurn,toolFormArguments,toolFormFields} from '../workbenchContracts';

function receipt():Value {
    const fixtures=JSON.parse(readFileSync(resolve(__dirname,'../../../../docs/research/contracts-v190/fixtures.json'),'utf8'));
    return structuredClone(fixtures.receipt_cases.find((entry:any)=>entry.name==='single_index_match_and_supplement').value);
}
function turn(check:Value):WorkbenchTurn {
    const source=(check.sources as Value[])[0],origin=(check.origins as Value[])[0];
    const evidence=[{step:'query',publication:origin.publication,citation:{path:source.path,content_hash:source.expected_hash,start_line:1,end_line:2},text:'Original indexed evidence'}];
    return {id:'checked',at:1000,input:'Read saved evidence',format:'structured',envelope:{status:'complete',executed:true,publication:origin.publication,
        plan:{path_prefix:''},evidence,steps:[{id:'query',tool:'search',status:'complete',result:{hits:evidence}}],source_verification:check}};
}

test('dated verification display keeps zero, partial, malformed and historical observations distinct',()=>{
    const check=receipt();
    assert.match(verificationLabel(check),/verified at 1970-01-01T00:00:01.010Z: 1 referenced source/);
    for(const changed of [{status:'partial'},{source_rows_omitted:1},{inventory_complete:false},{publication_fence:'expired'},{requirements_satisfied:false},{observed_to_unix_ms:-1},{origins:[]},{counts:{...check.counts as Value,references:0}}]){
        assert.match(verificationLabel({...check,...changed}),/verification incomplete/);
    }
    const forged=structuredClone(check);(forged.sources as Value[])[0].observed_hash='b'.repeat(64);
    assert.match(verificationLabel(forged),/verification incomplete/);
    assert.equal(nativeVerification({schema:'unknown'}),undefined);
    const empty={...check,status:'not_applicable',counts:{references:0,requested_sources:0,matched_sources:0},sources:[],references:[]};
    assert.match(verificationLabel(empty),/No returned saved-source references/);
    // Rendering and later reopening keep the original date, never Date.now().
    assert.equal(verificationLabel(check),verificationLabel(structuredClone(check)));
});

test('selected exports rebind every association and preserve original failed-request uncertainty',()=>{
    const check=receipt();check.status='partial';check.requirements_satisfied=false;
    const original=turn(check),before=structuredClone(original),cards=evidenceCards(original.envelope!);
    const selected=selectedVerification(original.envelope!,cards);
    assert.equal(selected.observations[0].original_status,'partial');
    assert.equal(selected.observations[0].verification_performed,false);
    assert.equal(selected.accounting.with_retained_observations,1);
    const session:WorkbenchSession={schema:1,id:'session',project:'a'.repeat(64),title:'Saved',createdAt:0,updatedAt:0,revision:0,omitted:0,turns:[original]};
    const exported=JSON.parse(renderExport(session,'json',original.id,['record-0'])).turns[0].envelope;
    assert.equal(exported.source_verification,undefined,'a full-response receipt cannot keep dangling original pointers in a subset');
    const observation=exported.source_verification_observations[0];
    assert.equal(observation.original_status,'partial');
    assert.equal(observation.original_requirements_satisfied,false);
    assert.equal(observation.sources[0].reference_ids.length,1);
    assert.equal(observation.references[0].result_pointer,'/evidence/0/citation');
    assert.equal(observation.references[0].source_id,observation.sources[0].id);
    assert.equal(observation.sources[0].origin_id,observation.origins[0].id);
    assert.equal(exported.evidence[0].citation.path,observation.sources[0].path);
    assert.equal(observation.sources[0].observed_to_unix_ms,(check.sources as Value[])[0].observed_to_unix_ms);
    assert.match(markdown(original,new Set(['record-0'])),/Selected evidence and dated source observations/);
    assert.match(markdown(original,new Set(['record-0'])),/"original_status": "partial"/);
    assert.deepEqual(original,before);
    const unavailable=turn(check);(unavailable.envelope!.evidence as any[])[0].citation.content_hash='b'.repeat(64);
    assert.deepEqual(selectedVerification(unavailable.envelope!,evidenceCards(unavailable.envelope!)).accounting.without_retained_observations,['record-0']);
    const full=JSON.parse(renderExport(session,'json')).turns[0].envelope;
    assert.deepEqual(full.source_verification,check,'complete exports preserve every native receipt field');
});

test('typed error receipts and per-step metadata survive structured details and form/session export',()=>{
    const check=receipt(),original=turn(check);
    original.envelope!.source_verification=undefined;
    (original.envelope!.steps as Value[])[0].result={isError:true,content:[],structuredContent:{source_verification:check}};
    assert.deepEqual(discoveryDetails(original.envelope!)[0].sourceVerification,check);
    const schema:Value={type:'object',properties:{verification:{type:'string',enum:['off','report','require']},verification_max_sources:{type:'integer',minimum:1,maximum:256},verification_max_bytes:{type:'integer',minimum:1,maximum:1073741824},verification_deadline_ms:{type:'integer',minimum:1,maximum:30000}}};
    assert.equal(toolFormFields(schema)[0].label,'Saved-source verification');
    assert.deepEqual(toolFormArguments(schema,{verification:'require',verification_max_sources:'25',verification_max_bytes:'16777216',verification_deadline_ms:'2000'}),
        {verification:'require',verification_max_sources:25,verification_max_bytes:16777216,verification_deadline_ms:2000});
    assert.throws(()=>toolFormArguments(schema,{verification_max_sources:'257'}));
    assert.match(markdown(original),/"source_verification"/);
});

test('actual response-wide combined verification receipts follow native member bindings into selected exports',()=>{
    const check=receipt(),source=(check.sources as Value[])[0],nativeOrigin=(check.origins as Value[])[0];
    const index=(number:number)=>`idx_${String(number).repeat(32)}`,root=(number:number)=>`src_${String(number).repeat(32)}`,project=(number:number)=>`prj_${String(number).repeat(32)}`;
    const scope='ab'.repeat(32),publication=String(nativeOrigin.publication);
    check.status='partial';check.requirements_satisfied=false;
    source.source_root_id=root(1);source.project_ids=[project(1)];source.reference_ids=['ref-0'];
    nativeOrigin.index_id=index(1);nativeOrigin.project_ids=[project(1)];
    check.references=[{id:'ref-0',source_id:source.id,result_pointer:'/steps/0/result/citation',kind:'citation'}];
    check.counts={...objectValue(check.counts),references:1};
    const evidence=[1,2].map(number=>({step:'query',publication,citation:{path:source.path,content_hash:source.expected_hash,start_line:1,end_line:2},text:`member ${number}`,
        origin:{index_id:index(number),source_root_id:root(number),project_ids:[project(number)],source_relative_path:source.source_relative_path,publication:{token:publication}}}));
    const own={steps:[{result:{citation:structuredClone(evidence[0].citation),publication}}],source_verification:check};
    const envelope:Value={status:'verification_failed',executed:true,scope_identity:scope,evidence,steps:[],
        member_verification:{schema:'airk.combined-verification/1',scope_identity:scope,partial:true,error:{code:'member_verification_failed',retryable:false},members:[
            {index_id:index(1),reference_bindings:[{member_pointer:'/steps/0/result',result_pointer:'/evidence/0'}],result:own},
            {index_id:index(2),reference_bindings:[],result:{error:{code:'verification_deadline_exhausted',retryable:false},future_private_payload:'UNSELECTED_FINAL_PAYLOAD'}},
        ]}};
    const cards=evidenceCards(envelope),selected=selectedVerification(envelope,cards);
    assert.equal(selected.requested,true);assert.equal(selected.observations.length,1);assert.equal(selected.accounting.with_retained_observations,1);
    assert.deepEqual(selected.accounting.without_retained_observations,['record-1']);
    const observation=selected.observations[0];
    assert.equal(observation.index_id,index(1));assert.equal(observation.original_member_scope_identity,scope);
    assert.equal(observation.original_status,'partial');assert.equal(observation.original_requirements_satisfied,false);
    assert.deepEqual(observation.original_reference_associations,[{selected_reference_id:'ref-0',reference_id:'ref-0',member_pointer:'/steps/0/result/citation',result_pointer:'/evidence/0/citation'}]);
    assert.equal((observation.references as Value[])[0].result_pointer,'/evidence/0/citation');
    const value:WorkbenchSession={schema:1,id:'session',project:'a'.repeat(64),title:'Saved',createdAt:0,updatedAt:0,revision:0,omitted:0,
        turns:[{id:'final',at:1000,input:'Combined',format:'structured',envelope}]};
    const exported=JSON.parse(renderExport(value,'json','final',['record-0'])).turns[0].envelope;
    assert.equal(exported.source_verification_observations[0].index_id,index(1));
    assert.equal(exported.source_verification_observations[0].observed_to_unix_ms,check.observed_to_unix_ms);
    assert.equal(JSON.stringify(exported).includes('UNSELECTED_FINAL_PAYLOAD'),false);
    assert.match(renderExport(value,'markdown','final',['record-0']),/Selected evidence and dated source observations/);
    assert.match(renderExport(value,'markdown','final',['record-0']),/original_member_scope_identity/);
    for(const change of ['index','scope','binding','target','missing_binding'] as const){
        const altered=structuredClone(envelope),final=objectValue(altered.member_verification),members=final.members as Value[];
        if(change==='index')members[0].index_id=index(2);
        if(change==='scope')final.scope_identity='cd'.repeat(32);
        if(change==='binding')(members[0].reference_bindings as Value[])[0].result_pointer='/evidence/1';
        if(change==='target')((altered.evidence as Value[])[0].citation as Value).start_line=2;
        if(change==='missing_binding')members[0].reference_bindings=[];
        const invalid=selectedVerification(altered,evidenceCards(altered));
        assert.equal(invalid.requested,true);assert.equal(invalid.observations.length,0,change);
    }
    assert.equal(selectedVerification(envelope,[cards[1]]).observations.length,0,'identical paths/hashes/publications in another index cannot acquire this receipt');
});

function objectValue(value:unknown):Value{return value&&typeof value==='object'&&!Array.isArray(value)?value as Value:{};}
