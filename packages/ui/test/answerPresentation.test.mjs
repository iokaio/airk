// SPDX-License-Identifier: Apache-2.0
// Shared /2 presentation over actual Rust semantic fixtures; no native execution claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
const {build}=createRequire(new URL('../../vscode/package.json',import.meta.url))('esbuild');
const compiled=await build({entryPoints:[fileURLToPath(new URL('../src/answerPresentation.ts',import.meta.url))],bundle:true,write:false,format:'esm',platform:'node'});
const {answerPresentation}=await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text).toString('base64')}`);
const native=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/context_action_cases.json',import.meta.url),'utf8'));
const rows=native.cases;
const text=value=>typeof value==='string'?value:value.children.map(text).join('\n');
const nodes=value=>typeof value==='string'?[]:[value,...value.children.flatMap(nodes)];

test('current /2 shared answer renders actual native whole-card role gap action source-unit and complete provenance',()=>{
    const row=rows.find(row=>row.name==='local_sufficient_with_complete_native_result');assert.equal(row.accepted,true);
    const answer=structuredClone(row.input),before=structuredClone(answer),view=answerPresentation(answer,'native-case'),rendered=text(view);
    assert.match(rendered,/Local stop: covered/);assert.match(rendered,/Local partition assessment/);
    assert.match(rendered,/Current local assessment/);assert.match(rendered,/Recorded action: sufficient/);
    assert.match(rendered,/Role hypothesis: definition/);assert.match(rendered,/Relevance hypothesis: useful/);
    assert.match(rendered,/UTF-8 bytes 0–13/);assert.match(rendered,/not exhaustive coverage/);
    assert.match(rendered,/does not establish coverage for other authorized indexes/);
    assert.ok(nodes(view).some(node=>node.citationTarget===`answer-native-case-${answer.records[0].id}`));
    for(const event of answer.assessment_history)assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children.includes(JSON.stringify(event,null,2))));
    for(const record of answer.records)assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children.includes(JSON.stringify(record,null,2))));
    assert.deepEqual(answer,before);
});

test('current /2 native deadline and known overrun work remain unassessed without a false success or zero-cost claim',()=>{
    const row=rows.find(row=>row.name==='overrun_deadline_metadata_retained');assert.equal(row.accepted,true);
    const view=answerPresentation(row.input,'deadline'),rendered=text(view);
    assert.match(rendered,/Local stop: deadline/);assert.match(rendered,/Coverage unassessed: deadline/);
    assert.match(rendered,/Assessment not applied: deadline/);assert.match(rendered,/Known measured optional work is retained/);
    assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children.includes(JSON.stringify(row.input.assessment_history[0],null,2))));
});

test('current /2 presentation keeps every sparse historical and basis-omitted event separate from live coverage',()=>{
    const answer=structuredClone(rows.find(row=>row.accepted).input);
    const first=structuredClone(answer.assessment_history[0]);first.ordinal=1;first.applicability='historical_after_read';
    const second=structuredClone(first);second.ordinal=3;second.applicability='basis_omitted';
    answer.assessment_history=[first,second];answer.assessment.coverage={status:'unassessed',reason:'basis_omitted'};answer.assessment.termination='budget';answer.stop='budget';
    const rendered=text(answerPresentation(answer,'history'));
    assert.match(rendered,/Assessment 1/);assert.match(rendered,/Assessment 3/);
    assert.match(rendered,/Historical after a later read; no current coverage claim/);
    assert.match(rendered,/Basis omitted from this packet; no current coverage claim/);
    assert.match(rendered,/Coverage unassessed: basis omitted/);assert.doesNotMatch(rendered,/Current local assessment/);
    const unavailable=structuredClone(rows.find(row=>row.name==='overrun_deadline_metadata_retained').input);
    unavailable.assessment_history[0].result.work_unknown=true;
    assert.match(text(answerPresentation(unavailable,'unknown')),/Actual optional work may be unknown; no zero-cost or replay claim/);
});

test('current /2 advice shows exact reservation observation and measured zero without treating null as zero',()=>{
    const nativeCovered=rows.find(row=>row.name==='local_sufficient_with_complete_native_result');assert.equal(nativeCovered.accepted,true);
    const covered=structuredClone(nativeCovered.input),advice=covered.assessment_history[0].result.advice;
    const rendered=text(answerPresentation(covered,'reserved'));
    assert.match(rendered,new RegExp(`Reserved input tokens: ${advice.reserved_input_tokens}`));
    assert.match(rendered,new RegExp(`Observed input tokens: ${advice.observed_input_tokens}`));
    assert.match(rendered,/compile 0; inference 0/,'actual measured zero remains distinct from unobserved');
    const unknown=structuredClone(rows.find(row=>row.name==='overrun_deadline_metadata_retained').input);
    unknown.assessment_history[0].result.advice.observed_input_tokens=null;
    unknown.assessment_history[0].result.advice.work.input_tokens=0;
    unknown.assessment_history[0].result.work.input_tokens=0;
    unknown.assessment_history[0].result.work_unknown=true;
    const uncertain=text(answerPresentation(unknown,'unknown-native-advice'));
    assert.match(uncertain,/observed unknown/);assert.match(uncertain,/unknown; the known lower bound does not establish zero actual use/);
    assert.match(uncertain,/work unknown/);assert.match(uncertain,/no zero-cost or replay claim/);
    assert.doesNotMatch(uncertain,/Recorded action:/,'unavailable advice never projects a learned card');
});

test('actual Rust /2 failure advice and over-reservation measurements remain complete and cannot become a learned success',()=>{
    const row=rows.find(row=>row.name==='failed_token_measurement_retained');assert.equal(row.accepted,true);
    const answer=structuredClone(row.input),result=answer.assessment_history[0].result;
    assert.equal(result.status,'unavailable');assert.notEqual(result.advice,null);
    const view=answerPresentation(answer,'native-token-failure'),rendered=text(view);
    assert.match(rendered,/Reserved input tokens: 2048/);assert.match(rendered,/Observed input tokens: 2050/);
    assert.match(rendered,/Recorded input-token lower bound: 2050/);assert.match(rendered,/Assessment not applied: token budget/);
    assert.doesNotMatch(rendered,/Recorded action:/);
    assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children.includes(JSON.stringify(result.advice,null,2))));
    const lost=structuredClone(answer);lost.assessment_history[0].result.advice=null;lost.assessment_history[0].result.work_unknown=true;
    const absent=text(answerPresentation(lost,'lost-reply'));
    assert.match(absent,/No actual native advice reply was retained/);
    assert.match(absent,/Token reservation and observed-token receipt were not returned/);
    assert.doesNotMatch(absent,/Reserved input tokens: 0/);assert.doesNotMatch(absent,/Observed input tokens: 0/);
});

test('current /2 native task policy stays distinct from profile policy and configuration history is not mislabeled omitted',()=>{
    const row=rows.find(row=>row.name==='local_sufficient_with_complete_native_result');assert.equal(row.accepted,true);
    const advice=row.input.assessment_history[0].result.advice;
    const rendered=text(answerPresentation(row.input,'policy'));
    assert.ok(rendered.includes(`Profile policy: ${advice.policy}. Task policy: ${advice.task_policy}.`));
    assert.equal(typeof advice.task_policy,'string');assert.equal(advice.task_policy.length,64);
    const historical=structuredClone(row.input);historical.assessment_history[0].applicability='historical_after_configuration_change';
    historical.assessment.coverage={status:'unassessed',reason:'unavailable'};historical.assessment.termination='budget';historical.stop='budget';
    const history=text(answerPresentation(historical,'config-history'));
    assert.match(history,/Historical after configuration changed; no current coverage claim/);
    assert.doesNotMatch(history,/Basis omitted from this packet/);assert.doesNotMatch(history,/Current local assessment/);
});

test('actual Core generic read leaves semantic need unclassified without a fabricated definition or terminal claim',()=>{
    const row=rows.find(row=>row.name==='generic_read_leaves_need_unclassified');assert.equal(row.accepted,true);
    const rendered=text(answerPresentation(row.input,'generic-read'));
    assert.match(rendered,/Recorded action: read/);assert.match(rendered,/Unclassified need \(native unsupported\)/);
    assert.match(rendered,/additional evidence may help/);assert.match(rendered,/Role hypothesis: unknown/);assert.match(rendered,/Relevance hypothesis: uncertain/);
    assert.doesNotMatch(rendered,/Recorded action: sufficient/);assert.doesNotMatch(rendered,/Recorded action: no justified read/);
    assert.doesNotMatch(rendered,/Gap: definition/);assert.doesNotMatch(rendered,/Gap: constraint/);
});
