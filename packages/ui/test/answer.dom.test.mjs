// SPDX-License-Identifier: Apache-2.0
// Actual Workbench DOM over synthetic retained data; no native parity claim.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
import { browserAvailable, browserHarness } from './browserHarness.mjs';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
let browser;
before(async () => { if (browserAvailable) browser = await browserHarness(); });
after(async t => { try { await browser?.close(); } finally { t.diagnostic(JSON.stringify({browser_ownership: browser?.observation})); } });
test('actual Workbench cited context navigates retained records, discloses gaps and keeps full copy/export',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async t => {
    let stage = 'compile-workbench';
    try {
    const bundle = await build({ absWorkingDir: root, entryPoints: ['src/workbenchWebview/main.tsx'], bundle: true, write: false,
      outdir: 'out/unused-answer-dom', format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact' });
    const id = 'a'.repeat(64), missing = 'b'.repeat(64), turn = 'observed-answer';
    const answer = { schema: 'airk.arbiter.context/2', packet_schema: 'airk.evidence.packet/1', question: 'Who owns the fence?',
      paragraphs: [{ text: 'Retained evidence <script>must remain text</script>', evidence: [id], interpretation: false },
        { text: 'An explicit interpretation.', evidence: [missing], interpretation: true }],
      records: [{ id, scope: { original_identity: 'retained' }, operation: 'read_evidence', role: 'definition',
        canonical_result: { source: 'original/path/é中.rs', publication: 'original-publication', future_native_field: { complete: ['all', 'fields'] } } }],
      gaps: ['counterevidence'], stop: 'budget', candidate_count: 3, omitted_count: 2, work: { calls: 8 }, applications: [], assessment_history: [],
      assessment:{scope:{index_id:'idx_'+'d'.repeat(32),publication:{collection:'fixture',token:'retained-publication'}},termination:'budget',coverage:{status:'unassessed',reason:'disabled'}} };
    stage = 'prepare-document';
    await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.requests=[];window.acquireVsCodeApi=()=>({postMessage:message=>window.requests.push(message)});`);
    stage = 'mount-workbench';
    await browser.evaluate(bundle.outputFiles.find(file => file.path.endsWith('.js')).text);
    stage = 'ready-observed-barrier';
    await browser.observed(`window.requests.some(message=>message.type==='ready')`);
    const project = 'c'.repeat(64);
    stage = 'deliver-retained-native-state';
    await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:${JSON.stringify({ projects: [{ id: project, name: 'Fixture' }], project,
      session: { schema: 1, id: 'fixture-session', project, title: 'Context', createdAt: 1, updatedAt: 1, revision: 1, omitted: 0,
        turns: [{ id: turn, input: 'Who owns the fence?', at: 1, format: 'structured', envelope: answer }] },
      cards: { [turn]: [{id:'record-0',path:'original/path/é中.rs',hash:'c'.repeat(64),start:7,end:9,text:'Original native source',step:id,resultPointer:'/hits/0',publication:'retained-publication',indexId:'idx_'+'d'.repeat(32),sourceRootId:'src_'+'e'.repeat(32),sourceRelativePath:'original/path/é中.rs',projectIds:['prj_'+'f'.repeat(32)]}] }, recent: [], busy: false, save: 'Synthetic DOM fixture', persistence: false, sessionEpoch: 1 })}}}));`);
    stage = 'answer-rendered-barrier';
    await browser.observed(`document.body.textContent.includes('Evidence-grounded context')`);
    assert.equal(await browser.evaluate(`document.body.textContent.includes('counterevidence')&&document.body.textContent.includes('omitted: 2')`), true);
    assert.equal(await browser.evaluate(`document.body.textContent.includes('Unresolved native evidence citation: ${missing}')`), true);
    assert.equal(await browser.evaluate(`document.querySelectorAll('script').length`), 0);
    stage = 'read-evidence-button';
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Read evidence ${id.slice(0, 12)}').click()`);
    await browser.observed(`document.activeElement?.id==='answer-${turn}-${id}'`);
    assert.equal(await browser.evaluate(`document.activeElement.textContent.includes('original/path/é中.rs')&&document.activeElement.closest('details').open`), true);
    stage = 'open-source-locator-button';
    await browser.evaluate(`[...document.querySelectorAll('section[aria-label="Open cited answer sources"] button')].find(button=>button.textContent.includes('original/path/é中.rs:7')).click()`);
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='openAnswerCitation')`),
      [{type:'openAnswerCitation',turn,record:id,hit:0}], 'renderer sends only retained IDs and a bounded hit locator, never a source path/root/hash');
    stage = 'complete-result-copy-export';
    const retained = await browser.evaluate(`JSON.parse([...document.querySelectorAll('details.complete-response pre')][0].textContent)`);
    assert.deepEqual(retained, answer);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='json').click()`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Copy JSON')`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Copy JSON').click();[...document.querySelectorAll('button')].find(button=>button.textContent==='Save export…').click();`);
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='copy'||message.type==='export')`),
      [{ type: 'copy', turn, format: 'json' }, { type: 'export', turn, format: 'json' }]);
    stage = 'answer-question-budget-controls';
    await browser.evaluate(`{const input=document.querySelector('textarea[aria-label="Request indexed evidence"]');input.value='Explicit context question';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Gather cited context'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Gather cited context').click()`);
    assert.deepEqual(await browser.evaluate(`window.requests.find(message=>message.type==='answer')`), { type: 'answer', question: 'Explicit context question' });
    await browser.evaluate(`(()=>{const question=document.querySelector('[aria-label="Context question"]');question.value='é'.repeat(8192);question.dispatchEvent(new Event('input',{bubbles:true}));const budget=document.querySelector('[aria-label="Native context budget"]');budget.value=JSON.stringify({calls:2,rounds:1,fanout:2,input_tokens:512,candidates:4,max_bytes:4096,deadline_ms:1200});budget.dispatchEvent(new Event('input',{bubbles:true}));})()`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Gather with this budget'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Gather with this budget').click()`);
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='answer').at(-1)`),{type:'answer',question:'é'.repeat(8192),budget:{calls:2,rounds:1,fanout:2,input_tokens:512,candidates:4,max_bytes:4096,deadline_ms:1200}});
    } catch (error) { t.diagnostic(JSON.stringify({stage,browser_ownership:browser?.observation})); throw error; }
  });


// Rust cases qualify the rendered shape, not native execution in this DOM host.
test('current /2 actual native assessments retain policies work and complete records in the Workbench DOM',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async t => {
    let stage = 'compile-current-native-view';
    try {
      const native = JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/context_action_cases.json', import.meta.url), 'utf8'));
      const bundle = await build({absWorkingDir:root,entryPoints:['src/workbenchWebview/main.tsx'],bundle:true,write:false,
        outdir:'out/unused-answer-dom',format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
      await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.requests=[];window.acquireVsCodeApi=()=>({postMessage:message=>window.requests.push(message)});`);
      await browser.evaluate(bundle.outputFiles.find(file=>file.path.endsWith('.js')).text);
      await browser.observed(`window.requests.some(message=>message.type==='ready')`);
      const turn='actual-native-case', project='c'.repeat(64);
      const deliver=async answer=>{
        await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:${JSON.stringify({projects:[{id:project,name:'Fixture'}],project,
          session:{schema:1,id:'native-case',project,title:'Actual native shape',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[{id:turn,input:'Native case',at:1,format:'structured',envelope:answer}]},
          cards:{},recent:[],busy:false,save:'Synthetic host / actual Rust case',persistence:false,sessionEpoch:1})}}}));`);
        await browser.observed(`document.body.textContent.includes('Evidence-grounded context')`);
        assert.deepEqual(await browser.evaluate(`JSON.parse(document.querySelector('details.complete-response pre').textContent)`),answer);
      };
      for(const name of ['local_sufficient_with_complete_native_result','overrun_deadline_metadata_retained','generic_read_leaves_need_unclassified']) {
        stage=`native-case-${name}`;
        const row=native.cases.find(row=>row.name===name);assert.equal(row.accepted,true);
        await deliver(row.input);
        const body=await browser.evaluate('document.body.textContent');
        for(const event of row.input.assessment_history) {
          assert.ok(body.includes(JSON.stringify(event,null,2)),'full actual native event is retained');
          if(event.result.advice!==null) {
            assert.ok(body.includes(`Task policy: ${event.result.advice.task_policy}`));
            assert.ok(body.includes(`Reserved input tokens: ${event.result.advice.reserved_input_tokens}`));
          }
        }
        if(name==='generic_read_leaves_need_unclassified')assert.ok(body.includes('Unclassified need (native unsupported)'));
      }
      stage='explicit-historical-unknown-view-control';
      const control=structuredClone(native.cases.find(row=>row.name==='overrun_deadline_metadata_retained').input);
      control.assessment_history[0].applicability='historical_after_configuration_change';
      control.assessment_history[0].result.work_unknown=true;
      control.assessment_history[0].result.advice=null;
      await deliver(control);
      const body=await browser.evaluate('document.body.textContent');
      assert.ok(body.includes('Historical after configuration changed; no current coverage claim'));
      assert.ok(body.includes('Actual optional work may be unknown; no zero-cost or replay claim.'));
      assert.ok(body.includes('Token reservation and observed-token receipt were not returned'));
    } catch(error) {t.diagnostic(JSON.stringify({stage,browser_ownership:browser?.observation}));throw error;}
  });


test('actual Rust combined Answer members remain separate with complete outer export and native per-member history',
  {skip:browserAvailable?false:'Chromium unavailable.'},async t=>{
    let stage='load-native-combined-cases';
    try {
      const native=JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/combined_answer_cases.json',import.meta.url),'utf8'));
      assert.deepEqual(native.cases.map(row=>row.name),['two_complete_members','complete_and_failed_member','complete_and_unqueried_member']);
      const bundle=await build({absWorkingDir:root,entryPoints:['src/workbenchWebview/main.tsx'],bundle:true,write:false,
        outdir:'out/unused-answer-dom',format:'iife',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
      await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.requests=[];window.acquireVsCodeApi=()=>({postMessage:message=>window.requests.push(message)});`);
      await browser.evaluate(bundle.outputFiles.find(file=>file.path.endsWith('.js')).text);
      await browser.observed(`window.requests.some(message=>message.type==='ready')`);
      for(const row of native.cases){
        stage=`combined-${row.name}`;const answer=row.output,project='c'.repeat(64),turn=row.name;
        await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:${JSON.stringify({projects:[{id:project,name:'Fixture'}],project,
          session:{schema:1,id:'combined-case',project,title:'Actual combined shape',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[{id:turn,input:'Native combined case',at:1,format:'structured',envelope:answer}]},
          cards:{},recent:[],busy:false,save:'Synthetic host / actual Rust combined case',persistence:false,sessionEpoch:1})}}}));`);
        await browser.observed(`document.body.textContent.includes('Combined Answer')`);
        assert.equal(await browser.evaluate(`document.querySelectorAll('section[aria-label^="Answer partition "]').length`),answer.members.length);
        assert.deepEqual(await browser.evaluate(`JSON.parse(document.querySelector('details.complete-response pre').textContent)`),answer);
        assert.deepEqual(await browser.evaluate(`[...document.querySelectorAll('section[aria-label^="Answer partition "]')].map(section=>JSON.parse([...section.querySelectorAll('details')].find(detail=>detail.firstElementChild?.textContent==='Complete original member').querySelector('pre').textContent))`),answer.members);
        const body=await browser.evaluate('document.body.textContent');
        for(const [index,member] of answer.members.entries()){
          assert.ok(body.includes(`Index ${member.index_id} · member ${index} · ${member.outcome}`));
          const packet=member.result?.structuredContent??member.result;
          for(const event of packet?.assessment_history??[])if(event.result.advice!==null)assert.ok(body.includes(`Task policy: ${event.result.advice.task_policy}`));
        }
      }
    }catch(error){t.diagnostic(JSON.stringify({stage,browser_ownership:browser?.observation}));throw error;}
  });
