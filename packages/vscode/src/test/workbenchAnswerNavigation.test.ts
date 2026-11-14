// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import {readResultBody} from '../sourceVerification';
import {answerPartitions,isAnswerEnvelope} from '../workbenchAnswerNavigation';
import { newSession } from '../workbenchSessions';
import { evidenceCards, evidenceRows, markdown, renderExport } from '../workbenchRender';
import { parseWorkbenchMessage, type Value } from '../workbenchContracts';
import {answerSourceFixture,answerRecord,answerIndex,answerProject,answerRoot,answerHash,answerPublication} from './workbenchAnswerFixture';

test('answer citation projection resolves exact record hit and original native source association without changing evidence',()=>{
    const {answer}=answerSourceFixture(), before=structuredClone(answer);
    const cards=evidenceCards(answer);
    assert.equal(cards.length,1);
    assert.deepEqual(cards[0],{id:'record-0',path:'api/source.rs',hash:answerHash,start:7,end:9,text:'Canonical source bytes.',location:{Lines:{start:7,end:9}},step:answerRecord,publication:answerPublication.token,
        indexId:answerIndex,projectIds:[answerProject],sourceRootId:answerRoot,sourceRelativePath:'api/source.rs',resultPointer:'/hits/0'});
    assert.equal(evidenceRows(answer)[0].future_hit!==undefined,true);
    assert.deepEqual(answer,before);
    assert.deepEqual(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:0}),{type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:0});
    for(const invalid of [{path:'/renderer/guess'}, {record:'forged'}, {hit:-1}, {hit:Number.MAX_SAFE_INTEGER+1}])assert.equal(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:0,...invalid}),undefined);
});

test('answer citation projection refuses foreign ambiguous missing or replaced source associations',()=>{
    for(const kind of ['missing','duplicate-record','duplicate-source','duplicate-origin','duplicate-reference','foreign-origin','foreign-root','wrong-reference','wrong-publication','wrong-authorization','wrong-membership']){
        const {answer}=answerSourceFixture(),records=answer.records as Value[],body=records[0].canonical_result as Value,verification=body.source_verification as Value;
        if(kind==='missing')delete body.source_verification;
        if(kind==='duplicate-record')records.push(structuredClone(records[0]));
        if(kind==='duplicate-source')(verification.sources as Value[]).push(structuredClone((verification.sources as Value[])[0]));
        if(kind==='duplicate-origin')(verification.origins as Value[]).push(structuredClone((verification.origins as Value[])[0]));
        if(kind==='duplicate-reference')(verification.references as Value[]).push(structuredClone((verification.references as Value[])[0]));
        if(kind==='foreign-origin')(verification.origins as Value[])[0].index_id='idx_'+'99'.repeat(16);
        if(kind==='foreign-root')(verification.sources as Value[])[0].source_root_id='src_'+'99'.repeat(16);
        if(kind==='wrong-reference')(verification.references as Value[])[0].result_pointer='/hits/1/citation';
        if(kind==='wrong-publication')body.publication={...answerPublication,token:`airk-r1:${'36'.repeat(16)}:5:${'37'.repeat(32)}`};
        if(kind==='wrong-authorization')(body.scope_observation as Value).fingerprint='99'.repeat(32);
        if(kind==='wrong-membership')((body.scope_observation as Value).members as Value[])[0].memberships=[];
        assert.deepEqual(evidenceCards(answer),[],kind);
    }
});

test('answer source locators retain empty native Project attribution for explicit Open without inventing a Project',()=>{
    const {answer}=answerSourceFixture(),body=(answer.records as Value[])[0].canonical_result as Value;
    ((body.source_verification as Value).origins as Value[])[0].project_ids=[];
    ((body.scope_observation as Value).members as Value[])[0].memberships=[];
    const cards=evidenceCards(answer);
    assert.equal(cards.length,1);assert.deepEqual(cards[0].projectIds,[]);
});

test('answer source navigation preserves every locator across more than the default32 native records',()=>{
    const {answer}=answerSourceFixture(),original=(answer.records as Value[])[0];
    answer.records=Array.from({length:40},(_,index)=>({...structuredClone(original),id:(index+1).toString(16).padStart(64,'0')}));
    const before=structuredClone(answer),cards=evidenceCards(answer);
    assert.equal(cards.length,40);
    assert.deepEqual(cards.map(card=>({record:card.step,hit:card.resultPointer})),(answer.records as Value[]).map(record=>({record:record.id,hit:'/hits/0'})));
    assert.deepEqual(cards.map(card=>card.id),Array.from({length:40},(_,index)=>`record-${index}`));
    for(const card of cards){assert.equal(card.indexId,answerIndex);assert.equal(card.publication,answerPublication.token);assert.equal(card.hash,answerHash);assert.equal(card.sourceRootId,answerRoot);}
    assert.deepEqual(answer,before,'projection must not trim the full native answer');
});

test('answer source hit locators do not impose a default hit count or silently select a subset',()=>{
    const {answer}=answerSourceFixture(),body=(answer.records as Value[])[0].canonical_result as Value,first=(body.hits as Value[])[0];
    body.hits=Array.from({length:101},()=>structuredClone(first));
    const receipt=body.source_verification as Value,source=(receipt.sources as Value[])[0];
    receipt.references=Array.from({length:101},(_,index)=>({id:`ref-${index}`,source_id:source.id,result_pointer:`/hits/${index}/citation`}));
    source.reference_ids=Array.from({length:101},(_,index)=>`ref-${index}`);
    const cards=evidenceCards(answer);
    assert.equal(cards.length,101);assert.equal(cards[100].resultPointer,'/hits/100');
    assert.deepEqual(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:100}),{type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:100});
});

test('answer source navigation explicitly supports hit receipts and preserves unsupported native result kinds without guessing their locators',()=>{
    for(const operation of ['search','context','read_evidence']){
        const {answer}=answerSourceFixture();(answer.records as Value[])[0].operation=operation;
        assert.equal(evidenceCards(answer).length,1,operation);
    }
    for(const operation of ['definitions','references','document_sections']){
        const {answer}=answerSourceFixture();(answer.records as Value[])[0].operation=operation;
        const before=structuredClone(answer);assert.deepEqual(evidenceCards(answer),[],operation);assert.deepEqual(answer,before);
    }
});

test('actual Core current /2 answer copies retain complete assessment provenance and report native stop and publication honestly',()=>{
    const native=JSON.parse(readFileSync(path.resolve(__dirname,'../../../../fixtures/split/arbiter/context_action_cases.json'),'utf8'));
    const row=native.cases.find((value:{name:string})=>value.name==='local_sufficient_with_complete_native_result');
    assert.equal(row.accepted,true);
    const answer=structuredClone(row.input),turn={id:'native-current-context',at:1,input:answer.question,format:'structured' as const,envelope:answer};
    const session=newSession('41'.repeat(32));session.turns.push(turn);
    const text=markdown(turn);
    assert.match(text,/Status: covered/);assert.ok(text.includes(`Publication: ${answer.assessment.scope.publication.token}`));
    for(const field of ['assessment_history','assessment','reserved_input_tokens','observed_input_tokens'])assert.ok(text.includes(`"${field}"`),field);
    assert.deepEqual(JSON.parse(renderExport(session,'json',turn.id)).turns[0].envelope,answer);
});


// Synthetic association controls, not a native combined-result fixture or parity claim.
function combinedSourceControl():Value {
    const first=answerSourceFixture().answer,other='idx_'+'77'.repeat(16);
    const second=JSON.parse(JSON.stringify(first).replaceAll(answerIndex,other));
    return {schema:'airk.combined-read/1',context:{schema:'airk.combined-context/1',future_accounting:{all:['retained']}},
        publication_vector:{members:[{index_id:answerIndex,publication:answerPublication},{index_id:other,publication:answerPublication}]},
        members:[{index_id:answerIndex,outcome:'complete',result:{content:[],structuredContent:first}},
            {index_id:other,outcome:'complete',result:{content:[],structuredContent:second}}]};
}
test('combined Answer keeps every member and disambiguates identical record IDs using the original member position',()=>{
    const combined=combinedSourceControl(),before=structuredClone(combined),cards=evidenceCards(combined);
    assert.equal(isAnswerEnvelope(combined),true);assert.equal(answerPartitions(combined).length,2);
    assert.equal(cards.length,2);assert.deepEqual(cards.map(card=>card.answerMember),[0,1]);
    assert.equal(cards[0].step,cards[1].step);assert.notEqual(cards[0].indexId,cards[1].indexId);
    assert.deepEqual(cards.map(card=>card.resultPointer),['/hits/0','/hits/0']);
    assert.deepEqual(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:0,member:1}),{type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:0,member:1});
    for(const member of [-1,1.5,Number.MAX_SAFE_INTEGER+1])assert.equal(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,hit:0,member}),undefined);
    assert.deepEqual(combined,before);
    const session=newSession('combined');session.turns.push({id:'combined',at:1,input:'Question',format:'structured',envelope:combined});
    assert.deepEqual(JSON.parse(renderExport(session,'json')).turns[0].envelope,combined);
    assert.ok(markdown(session.turns[0]).includes('future_accounting'));
});
test('combined Answer refuses cross-member index publication and duplicate member aliases without losing full raw members',()=>{
    for(const change of ['index','publication','duplicate','failed','unqueried']) {
        const combined=combinedSourceControl(),members=combined.members as Value[];
        if(change==='index')members[0].index_id='idx_'+'88'.repeat(16);
        if(change==='publication')((combined.publication_vector as Value).members as Value[])[0].publication={...answerPublication,token:`airk-r1:${'36'.repeat(16)}:8:${'37'.repeat(32)}`};
        if(change==='duplicate')members[1].index_id=members[0].index_id;
        if(change==='failed'||change==='unqueried')members[0].outcome=change;
        const before=structuredClone(combined),cards=evidenceCards(combined);
        assert.ok(cards.every(card=>card.answerMember!==0),change);
        assert.equal(answerPartitions(combined).length,2,'all raw member statuses stay visible');assert.deepEqual(combined,before);
    }
});


test('actual Rust combined Answer cases retain every outer member packet and failed or unqueried outcome without a first-member projection',()=>{
    const native=JSON.parse(readFileSync(path.resolve(__dirname,'../../../../fixtures/split/arbiter/combined_answer_cases.json'),'utf8'));
    assert.deepEqual(native.cases.map((row:{name:string})=>row.name),['two_complete_members','complete_and_failed_member','complete_and_unqueried_member']);
    for(const row of native.cases){
        const outer=row.output,before=structuredClone(outer),partitions=answerPartitions(outer);
        if(row.name==='two_complete_members')assert.deepEqual(outer.members.map((member:Value)=>member.result),row.native_member_results,'actual composer member values equal the native precomposition inputs');
        if(row.name==='complete_and_failed_member')assert.deepEqual(outer.members[1].result,row.native_failure,'complete native failure and custom retry detail remains unchanged');
        assert.equal(isAnswerEnvelope(outer),true);assert.equal(partitions.length,outer.members.length);
        for(const [index,member] of outer.members.entries()){
            assert.equal(partitions[index].member,index);assert.equal(partitions[index].indexId,member.index_id);assert.equal(partitions[index].outcome,member.outcome);
            assert.deepEqual(partitions[index].raw,member);
            const body=readResultBody(member.result);
            if(body.schema==='airk.arbiter.context/2')assert.deepEqual(partitions[index].packet,body,'complete native local packet stays attached to its exact member');
            else assert.equal(partitions[index].packet,undefined,'failed/unqueried member never becomes a fabricated local packet');
        }
        const session=newSession('actual-combined');session.turns.push({id:row.name,at:1,input:'Actual Rust case',format:'structured',envelope:outer});
        assert.deepEqual(JSON.parse(renderExport(session,'json')).turns[0].envelope,outer,'full outer fields and all native member values are exported unchanged');
        assert.deepEqual(outer,before);
    }
});


test('Answer source scope equality rejects paired foreign authorization and unknown scope fields while ignoring object key order',()=>{
    for(const combined of [false,true])for(const field of ['paired-authorization','unknown-scope-field','array-scope-field']){
        const envelope=combined?combinedSourceControl():answerSourceFixture().answer;
        const packet=combined?readResultBody((envelope.members as Value[])[0].result):envelope;
        const record=(packet.records as Value[])[0],scope=record.scope as Value,body=record.canonical_result as Value;
        if(field==='paired-authorization'){scope.authorization='99'.repeat(32);(body.scope_observation as Value).fingerprint='99'.repeat(32);}
        if(field==='unknown-scope-field')scope.future_native_scope={all:['preserved','different']};
        if(field==='array-scope-field')scope.future_native_scope=['different','order'];
        const before=structuredClone(envelope),cards=evidenceCards(envelope);
        assert.ok(cards.every(card=>combined?card.answerMember!==0:false),`${combined?'combined':'local'} ${field}`);
        assert.deepEqual(envelope,before,'scope checks do not discard or rewrite unknown native fields');
    }
    const {answer}=answerSourceFixture(),record=(answer.records as Value[])[0],scope=record.scope as Value;
    record.scope=Object.fromEntries(Object.entries(scope).reverse());
    assert.equal(evidenceCards(answer).length,1,'JSON object ordering alone does not change scope identity');
});
