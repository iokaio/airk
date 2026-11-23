// SPDX-License-Identifier: Apache-2.0
// The shared Answer /3 presentation over the Rust-emitted reply cases; no native execution claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
const {build}=createRequire(new URL('../../vscode/package.json',import.meta.url))('esbuild');
const compile=async file=>{
    const compiled=await build({entryPoints:[fileURLToPath(new URL(file,import.meta.url))],bundle:true,write:false,format:'esm',platform:'node',metafile:true});
    return {module:await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text).toString('base64')}`),text:compiled.outputFiles[0].text};
};
const {module:{answerPresentation,answerNodeMarkdown,answerState,questionSpanText,decisionSourceLabel}}=await compile('../src/answerPresentation.ts');
const cases=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/answer_loop_cases.json',import.meta.url),'utf8'));
const emitted=id=>structuredClone(cases.response_cases.find(row=>row.id===id&&row.accepted).response);
const text=value=>typeof value==='string'?value:value.children.map(text).join('\n');
const nodes=value=>typeof value==='string'?[]:[value,...value.children.flatMap(nodes)];

test('the emitted two-part reply renders every part, sub-need, coverage, unrun read, decision source and record',()=>{
    const answer=emitted('two_parts_valid'),before=structuredClone(answer),view=answerPresentation(answer,'case'),rendered=text(view);
    for(const part of answer.parts){
        assert.ok(rendered.includes(`Part ${part.id}: ${questionSpanText(answer.question,part.span.utf8_start,part.span.utf8_end)}`));
        for(const need of part.sub_needs){
            assert.ok(rendered.includes(`Sub-need ${part.id}.${need.id} (${need.kind.replaceAll('_',' ')}): ${need.text}`));
            for(const read of need.unrun_reads)assert.ok(rendered.includes(`${read.operation.replaceAll('_',' ')} ${read.target} — not run (${read.reason.replaceAll('_',' ')})`));
        }
    }
    for(const decision of answer.decisions)assert.ok(rendered.includes(decisionSourceLabel(decision.source)),decision.source.kind);
    for(const record of answer.records){
        assert.ok(nodes(view).some(node=>node.id===`answer-case-${record.id}`));
        assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children.includes(JSON.stringify(record,null,2))),'each record is also shown whole');
    }
    assert.match(rendered,/Complete within the request budget\./);
    assert.match(rendered,/4 decision\(s\) in the service's log; 4 shown here, 0 omitted/);
    assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children[0].includes('"decision_log"')&&node.children[0].includes('"model_work"')));
    assert.deepEqual(answer,before);
});

test('every decision source kind and an unknown future kind have a plain label',()=>{
    for(const source of [{kind:'model',mode:'rule',rule:2},{kind:'deterministic',rule:'every_proposed_need'},{kind:'abstained',fallback:'top_focus_span',reason:'abstained'},
        {kind:'unavailable',fallback:'top_focus_span',reason:'deadline'},{kind:'not_dispatched',fallback:'first_extracted_term',reason:'token_budget'}])
        assert.ok(decisionSourceLabel(source).length>10,source.kind);
    assert.match(decisionSourceLabel({kind:'from_a_newer_service'}),/not known to this client/);
});

test('partial and queued replies say so, and the reply\'s extra union counts are shown without a client change',()=>{
    const answer=emitted('two_parts_valid');answer.partial={stage:'union',reason:'bytes',part:null,continuation:null};
    answer.union.repeats_merged=3;answer.union.overlaps_trimmed=1;
    const rendered=text(answerPresentation(answer,'p'));
    assert.match(rendered,/Partial: stopped at the union stage \(bytes\)\./);
    assert.match(rendered,/repeats merged: 3\./);assert.match(rendered,/overlaps trimmed: 1\./);
    assert.equal(answerState(answer).kind,'partial');
    const ticket=emitted('ticket_valid'),state=answerState(ticket);
    assert.equal(state.kind,'queued');assert.equal(state.continuation,ticket.ticket.id);
    assert.match(text(answerPresentation(ticket,'t')),/Queued: position/);
});

test('each text once: same_as and overlaps link to the carrying row, also_at places open, and no location is lost',()=>{
    // The row shape of Core "Where the text is" (change log entry 45).
    const answer=emitted('two_parts_valid'),[search,symbols]=answer.records,hash='ab'.repeat(32);
    symbols.canonical_result={projection:'airk.arbiter.loop-evidence/1',spans:[
        {pointer:'/hits/0',path:'src/a.rs',start_line:1,end_line:3,content_hash:hash,truncated:false,text:'shared text',also_at:['src/b.rs:4-6'],
            overlaps:[{record:search.id,pointer:'/hits/1',bytes:40,side:'head'}]},
        {pointer:'/hits/1',path:'src/a.rs',start_line:2,end_line:2,content_hash:hash,truncated:false,same_as:{record:symbols.id,pointer:'/hits/0'},within:true}]};
    search.canonical_result={projection:'airk.arbiter.loop-evidence/1',spans:[
        {pointer:'/hits/0',path:'src/b.rs',start_line:4,end_line:6,content_hash:'cd'.repeat(32),truncated:false,same_as:{record:symbols.id,pointer:'/hits/0'}},
        {pointer:'/hits/1',path:'src/a.rs',start_line:5,end_line:6,content_hash:hash,truncated:false,text:'head'},
        {pointer:'/hits/2',path:'src/c.rs',start_line:9,end_line:9,content_hash:'ef'.repeat(32),truncated:false,same_as:{record:'99'.repeat(32),pointer:'/hits/0'}}]};
    const view=answerPresentation(answer,'s'),rendered=text(view),all=nodes(view);
    const carrier=`answer-s-${symbols.id}-span-0`,trimmed=`answer-s-${search.id}-span-1`;
    assert.ok(all.some(node=>node.id===carrier)&&all.some(node=>node.id===trimmed),'record-list rows are anchors');
    assert.ok(all.some(node=>node.citationTarget===carrier&&text(node)===`src/a.rs:1–3 (record ${symbols.id.slice(0,12)})`),'a same_as row links to the row holding its text');
    assert.match(rendered,/its text lies within/);assert.match(rendered,/same text as/);
    assert.ok(all.some(node=>node.citationTarget===trimmed),'an overlap links to the row that shows the trimmed bytes');assert.match(rendered,/40 bytes at its head are shown in/);
    assert.ok(all.some(node=>node.tag==='button'&&text(node)==='Open src/b.rs:4-6'&&JSON.stringify(node.sourceLocator)===JSON.stringify({record:search.id,span:0})),'an also_at place opens its own row');
    assert.match(rendered,/which this reply does not carry/,'an unresolved carrier is disclosed, not dropped');
    for(const place of ['src/a.rs:1–3','src/a.rs:2','src/b.rs:4–6','src/a.rs:5–6','src/c.rs:9'])assert.ok(rendered.includes(`Open ${place}`),place);
    const markdown=answerNodeMarkdown(view);
    for(const place of ['src/a.rs:1–3','src/a.rs:2','src/b.rs:4–6','src/b.rs:4-6','src/c.rs:9'])assert.ok(markdown.includes(place),`Markdown keeps ${place}`);
});

test('Markdown of the tree keeps its headings, lists and excerpts and escapes evidence text',()=>{
    const answer=emitted('two_parts_valid');answer.paragraphs[0].text='Text with <b>markup</b> and `ticks`';
    const markdown=answerNodeMarkdown(answerPresentation(answer,'m'));
    assert.match(markdown,/^### Answer/);assert.match(markdown,/#### Part 1: /);assert.match(markdown,/##### Sub-need 2\.1 /);
    assert.ok(markdown.includes('&lt;b&gt;markup&lt;/b&gt;'));assert.ok(markdown.includes('\\`ticks\\`'));
    assert.match(markdown,/```\n\{\n {2}"arguments"/);
});

test('read_status is shown whole when the reply carries it and its absence is said, in the tree and its Markdown',()=>{
    // Core "One status per reply" (change log entry 54); the field values here are independent of the client.
    const answer=emitted('two_parts_valid');assert.equal(answer.read_status,undefined);
    assert.match(text(answerPresentation(answer,'n')),/carries no index status/);
    answer.read_status={freshness:{state:'current',observed_unix_ms:1763856000000},semantic_readiness:{state:'ready'}};
    const view=answerPresentation(answer,'r'),markdown=answerNodeMarkdown(view);
    assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children.includes(JSON.stringify(answer.read_status,null,2))),'the whole status object is shown');
    assert.match(text(view),/Index status at the first search: freshness, semantic readiness/);
    assert.ok(markdown.includes('"observed_unix_ms": 1763856000000')&&markdown.includes('"semantic_readiness"'),'Markdown keeps the status');
    assert.ok(nodes(view).some(node=>node.tag==='pre'&&node.children[0].includes('"packet_schema"')),'packet_schema is retained');
});

test('the Answer default budget module inlines only the Rust-emitted default',async()=>{
    const {module:{ANSWER_DEFAULT_BUDGET},text:bundle}=await compile('../src/answerBudget.ts');
    assert.deepEqual(ANSWER_DEFAULT_BUDGET,cases.default_budget);assert.ok(Object.isFrozen(ANSWER_DEFAULT_BUDGET));
    for(const unused of ['refusal_cases','response_cases','budget_cases'])assert.ok(!bundle.includes(unused),unused);
});
