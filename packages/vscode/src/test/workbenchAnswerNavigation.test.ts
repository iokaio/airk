// SPDX-License-Identifier: Apache-2.0
// Answer /3 citation navigation, copies and exports over the Rust-emitted reply
// cases. The combined controls below are synthetic wrappers around emitted
// replies: no Rust-emitted combined /3 case exists yet.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {readResultBody} from '../sourceVerification';
import {answerPartitions,isAnswerEnvelope} from '../workbenchAnswerNavigation';
import { newSession } from '../workbenchSessions';
import { evidenceCards, evidenceRows, markdown, renderExport } from '../workbenchRender';
import { parseWorkbenchMessage, type Value } from '../workbenchContracts';
import {answerSourceFixture,emittedReply,answerRecord,answerIndex,answerProject,answerRoot,answerHash,answerPublication} from './workbenchAnswerFixture';
const object=(value:unknown):Value=>value&&typeof value==='object'&&!Array.isArray(value)?value as Value:{};

test('answer citation projection resolves the exact record span and its native source association without changing evidence',()=>{
    const {answer}=answerSourceFixture(), before=structuredClone(answer);
    const cards=evidenceCards(answer);
    assert.equal(cards.length,1);
    assert.deepEqual(cards[0],{id:'record-0',path:'api/source.rs',hash:answerHash,start:7,end:9,text:'Canonical source bytes.',location:undefined,step:answerRecord,publication:answerPublication.token,
        indexId:answerIndex,projectIds:[answerProject],sourceRootId:answerRoot,sourceRelativePath:'api/source.rs',resultPointer:'/spans/0'});
    assert.deepEqual(object(evidenceRows(answer)[0].span).future_span,{preserve:true},'the returned span stays whole');
    assert.deepEqual(answer,before);
    assert.deepEqual(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,span:0}),{type:'openAnswerCitation',turn:'turn',record:answerRecord,span:0});
    for(const invalid of [{path:'/renderer/guess'}, {record:'forged'}, {span:-1}, {span:Number.MAX_SAFE_INTEGER+1}, {hit:0}])assert.equal(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,span:0,...invalid}),undefined);
});

test('answer citation projection refuses foreign ambiguous missing or replaced source associations',()=>{
    for(const kind of ['missing','invalid-root','duplicate-record','duplicate-source','duplicate-origin','duplicate-reference','foreign-origin','wrong-reference','wrong-record','missing-reference','wrong-hash','wrong-revision','wrong-origin-publication','not-v3']){
        const {answer}=answerSourceFixture(),records=answer.records as Value[],verification=answer.source_verification as Value,projection=records[1].canonical_result as Value;
        if(kind==='missing')delete answer.source_verification;
        if(kind==='invalid-root')verification.schema='future';
        if(kind==='duplicate-record')records.push(structuredClone(records[1]));
        if(kind==='duplicate-source')(verification.sources as Value[]).push(structuredClone((verification.sources as Value[])[0]));
        if(kind==='duplicate-origin')(verification.origins as Value[]).push(structuredClone((verification.origins as Value[])[0]));
        if(kind==='duplicate-reference')(verification.references as Value[]).push(structuredClone((verification.references as Value[])[0]));
        if(kind==='foreign-origin')(verification.origins as Value[])[0].index_id='idx_'+'99'.repeat(16);
        if(kind==='wrong-reference')(verification.references as Value[])[0].result_pointer='/records/1/canonical_result/spans/1';
        if(kind==='wrong-record')(verification.references as Value[])[0].result_pointer='/records/0/canonical_result/spans/0';
        if(kind==='missing-reference')verification.references=[];
        if(kind==='wrong-hash')(verification.sources as Value[])[0].expected_hash='99'.repeat(32);
        if(kind==='wrong-revision')projection.revision=`airk-r1:${'03'.repeat(16)}:9:${'04'.repeat(32)}`;
        if(kind==='wrong-origin-publication')(verification.origins as Value[])[0].publication=`airk-r1:${'03'.repeat(16)}:9:${'04'.repeat(32)}`;
        if(kind==='not-v3')answer.schema='airk.arbiter.context/2';
        assert.deepEqual(evidenceCards(answer),[],kind);
    }
});

test('a span without a content hash, path or lines is shown but never navigable',()=>{
    for(const drop of ['path','content_hash','start_line','end_line']){
        const {answer}=answerSourceFixture(),span=((answer.records as Value[])[1].canonical_result as Value).spans as Value[];
        delete span[0][drop];
        assert.deepEqual(evidenceCards(answer),[],drop);
    }
});

test('answer source locators retain empty native Project attribution for explicit Open without inventing a Project',()=>{
    const {answer}=answerSourceFixture();
    ((answer.source_verification as Value).origins as Value[])[0].project_ids=[];
    const cards=evidenceCards(answer);
    assert.equal(cards.length,1);assert.deepEqual(cards[0].projectIds,[]);
});

test('answer source navigation keeps every located span of every record without a default count or subset',()=>{
    const {answer}=answerSourceFixture(),records=answer.records as Value[],projection=records[1].canonical_result as Value,first=(projection.spans as Value[])[0];
    projection.spans=Array.from({length:101},(_,index)=>({...structuredClone(first),start_line:index+1,end_line:index+1}));
    const receipt=answer.source_verification as Value,source=(receipt.sources as Value[])[0];
    receipt.references=Array.from({length:101},(_,index)=>({id:`ref-${index}`,source_id:source.id,result_pointer:`/records/1/canonical_result/spans/${index}`}));
    source.reference_ids=Array.from({length:101},(_,index)=>`ref-${index}`);
    const before=structuredClone(answer),cards=evidenceCards(answer);
    assert.equal(cards.length,101);assert.equal(cards[100].resultPointer,'/spans/100');assert.equal(cards[100].start,101);
    assert.deepEqual(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,span:100}),{type:'openAnswerCitation',turn:'turn',record:answerRecord,span:100});
    assert.deepEqual(answer,before,'projection must not trim the native answer');
});

test('a span whose text another span carries (same_as) stays navigable by its own location',()=>{
    const {answer}=answerSourceFixture(),span=(((answer.records as Value[])[1].canonical_result as Value).spans as Value[])[0];
    delete span.text;span.same_as={record:'1f'.repeat(32),pointer:'/hits/0'};span.within=true;
    const cards=evidenceCards(answer);
    assert.equal(cards.length,1);assert.equal(cards[0].text,'');assert.deepEqual(object(evidenceRows(answer)[0].span).same_as,span.same_as);
});

test('Answer /3 copies and exports keep the whole reply and add the readable view grouped by part then sub-need',()=>{
    const {answer}=answerSourceFixture(),turn={id:'native-v3',at:1,input:String(answer.question),format:'structured' as const,
        request:{schema:'airk.arbiter.context.request/3',question:answer.question,budget:{deadline_ms:300000}},envelope:answer};
    const session=newSession('41'.repeat(32));session.turns.push(turn);
    const text=markdown(turn);
    assert.match(text,/Status: Complete within the request budget\./);
    assert.ok(text.includes(`Publication: ${answerPublication.token}`));
    const order=['#### Part 1: Where is the restart guard defined?','##### Sub-need 1\\.1 \\(definition\\): where is the restart guard defined',
        '#### Part 2: Which README section covers search?','##### Sub-need 2\\.1 \\(documentation\\): README section on search',
        'document sections README\\.md — not run \\(not chosen\\)','### Complete response'];
    let at=0;for(const heading of order){const found=text.slice(at).search(new RegExp(heading));assert.ok(found>=0,heading);at+=found;}
    assert.match(text,/Open api\/source\.rs:7–9/);assert.match(text,/Focus 0\.500/);
    assert.match(text,/Model decided \(top score\)/);assert.match(text,/Model abstained \(abstained\); deterministic fallback top focus span/);
    assert.match(text,/Deterministic order \(first extracted term\)/);
    assert.match(text,/The Context model abstained 1 time\(s\)/);
    for(const field of ['decision_log','record_uses','model_work','future_projection','future_native_field','source_verification'])assert.ok(text.includes(`"${field}"`),field);
    assert.deepEqual(JSON.parse(renderExport(session,'json',turn.id)).turns[0].envelope,answer,'the JSON export is the whole reply');
});

test('a partial reply, an omitted citation and a queued ticket are disclosed in copies',()=>{
    const answer=emittedReply();
    answer.partial={stage:'coverage',reason:'deadline',part:2,continuation:'ab'.repeat(32)};
    const parts=answer.parts as Value[],need=(parts[1].sub_needs as Value[])[0];
    need.evidence=[...(need.evidence as string[]),'7a'.repeat(32)];need.evidence_omitted=1;answer.omitted_evidence=['7a'.repeat(32)];
    const turn={id:'partial',at:1,input:'q',format:'structured' as const,envelope:answer},text=markdown(turn);
    assert.match(text,/Status: Partial: stopped at the coverage stage \(deadline\) in part 2\./);
    assert.match(text,new RegExp(`Continuation ${'ab'.repeat(32)}`));
    assert.match(text,/was cited but omitted from this reply to fit its size/);
    const ticket=emittedReply('ticket_valid'),queued=markdown({id:'ticket',at:1,input:'q',format:'structured',envelope:ticket});
    assert.match(queued,/Status: Queued: position \d+/);
    assert.ok(queued.includes(String(object(ticket.ticket).id)));
});

test('a saved /2 Answer is history: shown whole, never navigated or projected as current',()=>{
    const answer=emittedReply();answer.schema='airk.arbiter.context/2';
    assert.equal(isAnswerEnvelope(answer),false);assert.deepEqual(evidenceCards(answer),[]);
    const session=newSession('old');session.turns.push({id:'old',at:1,input:'q',format:'structured',envelope:answer});
    assert.deepEqual(JSON.parse(renderExport(session,'json')).turns[0].envelope,answer);
});

// Synthetic combined wrapper around emitted replies, not a Rust-emitted combined /3 case.
function combinedSourceControl():Value {
    const first=answerSourceFixture().answer,other='idx_'+'77'.repeat(16);
    const second=JSON.parse(JSON.stringify(first).replaceAll(answerIndex,other));
    return {schema:'airk.combined-read/1',context:{schema:'airk.combined-context/2',future_accounting:{all:['retained']}},
        publication_vector:{members:[{index_id:answerIndex,publication:answerPublication},{index_id:other,publication:answerPublication}]},
        members:[{index_id:answerIndex,outcome:'complete',result:{content:[],structuredContent:first}},
            {index_id:other,outcome:'complete',result:{content:[],structuredContent:second}}]};
}
test('combined Answer keeps every member and disambiguates identical record IDs by original member position',()=>{
    const combined=combinedSourceControl(),before=structuredClone(combined),cards=evidenceCards(combined);
    assert.equal(isAnswerEnvelope(combined),true);assert.equal(answerPartitions(combined).length,2);
    assert.equal(cards.length,2);assert.deepEqual(cards.map(card=>card.answerMember),[0,1]);
    assert.equal(cards[0].step,cards[1].step);assert.notEqual(cards[0].indexId,cards[1].indexId);
    assert.deepEqual(cards.map(card=>card.resultPointer),['/spans/0','/spans/0']);
    assert.deepEqual(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,span:0,member:1}),{type:'openAnswerCitation',turn:'turn',record:answerRecord,span:0,member:1});
    for(const member of [-1,1.5,Number.MAX_SAFE_INTEGER+1])assert.equal(parseWorkbenchMessage({type:'openAnswerCitation',turn:'turn',record:answerRecord,span:0,member}),undefined);
    assert.deepEqual(combined,before);
    const session=newSession('combined');session.turns.push({id:'combined',at:1,input:'Question',format:'structured',envelope:combined});
    assert.deepEqual(JSON.parse(renderExport(session,'json')).turns[0].envelope,combined);
    const text=markdown(session.turns[0]);
    assert.ok(text.includes('future_accounting'));
    assert.ok(text.includes(`### Index ${answerIndex.replace('_','\\_')} · member 0 · complete`),'member headings are escaped Markdown');assert.match(text,/### Index idx\\_7777/);
});
test('combined Answer refuses cross-member index publication and duplicate member aliases without losing full raw members',()=>{
    for(const change of ['index','publication','duplicate','failed','unqueried','v2-wrapper']) {
        const combined=combinedSourceControl(),members=combined.members as Value[];
        if(change==='index')members[0].index_id='idx_'+'88'.repeat(16);
        if(change==='publication')((combined.publication_vector as Value).members as Value[])[0].publication={...answerPublication,token:`airk-r1:${'03'.repeat(16)}:8:${'04'.repeat(32)}`};
        if(change==='duplicate')members[1].index_id=members[0].index_id;
        if(change==='failed'||change==='unqueried')members[0].outcome=change;
        if(change==='v2-wrapper')(combined.context as Value).schema='airk.combined-context/1';
        const before=structuredClone(combined),cards=evidenceCards(combined);
        assert.ok(cards.every(card=>card.answerMember!==0),change);
        if(change!=='v2-wrapper')assert.equal(answerPartitions(combined).length,2,'all raw member statuses stay visible');
        assert.deepEqual(combined,before);
    }
});
function combinedWholeSourceControl():Value {
    const combined=combinedSourceControl(),sources:Value[]=[],origins:Value[]=[],references:Value[]=[];
    for(const [position,raw] of (combined.members as Value[]).entries()){
        const packet=readResultBody(raw.result),proof=packet.source_verification as Value;
        const source=(proof.sources as Value[])[0],origin=(proof.origins as Value[])[0],reference=(proof.references as Value[])[0];
        source.id=`source-${position}`;source.origin_id=`origin-${position}`;source.reference_ids=[`ref-${position}`];
        origin.id=`origin-${position}`;reference.id=`ref-${position}`;reference.source_id=source.id;
        reference.result_pointer=`/members/${position}/result/structuredContent/records/1/canonical_result/spans/0`;
        sources.push(source);origins.push(origin);references.push(reference);delete packet.source_verification;
    }
    combined.source_verification={schema:'airk.saved-source-verification/1',method:'sha256-full-saved-source/1',sources,origins,references};
    return combined;
}
test('combined outer verification retains original custody and exact MCP member prefixes',()=>{
    const combined=combinedWholeSourceControl(),before=structuredClone(combined),cards=evidenceCards(combined);
    assert.equal(cards.length,2);assert.deepEqual(cards.map(card=>card.answerMember),[0,1]);
    assert.notEqual(cards[0].indexId,cards[1].indexId);assert.deepEqual(combined,before);
    for(const kind of ['wrong-member','wrong-wrapper','wrong-record','foreign-origin','duplicate-reference','wrong-hash','invalid-root']){
        const control=combinedWholeSourceControl(),proof=control.source_verification as Value;
        if(kind==='wrong-member')(proof.references as Value[])[0].result_pointer='/members/1/result/structuredContent/records/1/canonical_result/spans/0';
        if(kind==='wrong-wrapper')(proof.references as Value[])[0].result_pointer='/members/0/result/records/1/canonical_result/spans/0';
        if(kind==='wrong-record')(proof.references as Value[])[0].result_pointer='/members/0/result/structuredContent/records/0/canonical_result/spans/0';
        if(kind==='foreign-origin')(proof.origins as Value[])[0].index_id='idx_'+'99'.repeat(16);
        if(kind==='duplicate-reference')(proof.references as Value[]).push(structuredClone((proof.references as Value[])[0]));
        if(kind==='wrong-hash')(proof.sources as Value[])[0].expected_hash='99'.repeat(32);
        if(kind==='invalid-root'){proof.schema='future';readResultBody((control.members as Value[])[0].result).source_verification=answerSourceFixture().answer.source_verification;}
        assert.ok(!evidenceCards(control).some(card=>card.answerMember===0),kind);
    }
});
test('text-only error display cannot manufacture an addressable combined source reference',()=>{
    const combined=combinedWholeSourceControl(),members=combined.members as Value[],packet=readResultBody(members[0].result);
    members[0].result={isError:true,content:[{type:'text',text:JSON.stringify(packet)}]};
    const proof=combined.source_verification as Value;
    (proof.references as Value[])[0].result_pointer='/members/0/result/records/1/canonical_result/spans/0';
    assert.ok(!evidenceCards(combined).some(card=>card.answerMember===0));
    packet.source_verification=proof;
    members[0].result={isError:true,content:[{type:'text',text:JSON.stringify(packet)}]};
    assert.ok(!evidenceCards(combined).some(card=>card.answerMember===0));
});
test('emitted replies without a verification receipt never gain guessed source navigation',()=>{
    for(const id of ['two_parts_valid','ticket_valid']){const reply=emittedReply(id),before=structuredClone(reply);assert.deepEqual(evidenceCards(reply),[]);assert.deepEqual(reply,before);}
});
