// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { evidenceCards, evidenceRows } from '../workbenchRender';
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
