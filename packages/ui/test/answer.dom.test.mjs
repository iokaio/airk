// SPDX-License-Identifier: Apache-2.0
// Actual Workbench DOM over synthetic retained data; no native parity claim.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { browserAvailable, browserHarness } from './browserHarness.mjs';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
let browser;
before(async () => { if (browserAvailable) browser = await browserHarness(); });
after(async () => { await browser?.close(); });
test('actual Workbench cited context navigates retained records, discloses gaps and keeps full copy/export',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async () => {
    const bundle = await build({ absWorkingDir: root, entryPoints: ['src/workbenchWebview/main.tsx'], bundle: true, write: false,
      outdir: 'out/unused-answer-dom', format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact' });
    const id = 'a'.repeat(64), missing = 'b'.repeat(64), turn = 'observed-answer';
    const answer = { schema: 'airk.arbiter.context/1', packet_schema: 'airk.evidence.packet/1', question: 'Who owns the fence?',
      paragraphs: [{ text: 'Retained evidence <script>must remain text</script>', evidence: [id], interpretation: false },
        { text: 'An explicit interpretation.', evidence: [missing], interpretation: true }],
      records: [{ id, scope: { original_identity: 'retained' }, operation: 'read_evidence', role: 'definition',
        canonical_result: { source: 'original/path/é中.rs', publication: 'original-publication', future_native_field: { complete: ['all', 'fields'] } } }],
      gaps: ['counterevidence'], stop: 'budget', candidate_count: 3, omitted_count: 2, work: { calls: 8 }, applications: [], advice: [] };
    await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.requests=[];window.acquireVsCodeApi=()=>({postMessage:message=>window.requests.push(message)});`);
    await browser.evaluate(bundle.outputFiles.find(file => file.path.endsWith('.js')).text);
    await browser.observed(`window.requests.some(message=>message.type==='ready')`);
    const project = 'c'.repeat(64);
    await browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:${JSON.stringify({ projects: [{ id: project, name: 'Fixture' }], project,
      session: { schema: 1, id: 'fixture-session', project, title: 'Context', createdAt: 1, updatedAt: 1, revision: 1, omitted: 0,
        turns: [{ id: turn, input: 'Who owns the fence?', at: 1, format: 'structured', envelope: answer }] },
      cards: { [turn]: [] }, recent: [], busy: false, save: 'Synthetic DOM fixture', persistence: false, sessionEpoch: 1 })}}}));`);
    await browser.observed(`document.body.textContent.includes('Evidence-grounded context')`);
    assert.equal(await browser.evaluate(`document.body.textContent.includes('counterevidence')&&document.body.textContent.includes('omitted: 2')`), true);
    assert.equal(await browser.evaluate(`document.body.textContent.includes('Unresolved native evidence citation: ${missing}')`), true);
    assert.equal(await browser.evaluate(`document.querySelectorAll('script').length`), 0);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Read evidence ${id.slice(0, 12)}').click()`);
    await browser.observed(`document.activeElement?.id==='answer-${turn}-${id}'`);
    assert.equal(await browser.evaluate(`document.activeElement.textContent.includes('original/path/é中.rs')&&document.activeElement.closest('details').open`), true);
    const retained = await browser.evaluate(`JSON.parse([...document.querySelectorAll('details.complete-response pre')][0].textContent)`);
    assert.deepEqual(retained, answer);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='json').click()`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Copy JSON')`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Copy JSON').click();[...document.querySelectorAll('button')].find(button=>button.textContent==='Save export…').click();`);
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='copy'||message.type==='export')`),
      [{ type: 'copy', turn, format: 'json' }, { type: 'export', turn, format: 'json' }]);
    await browser.evaluate(`{const input=document.querySelector('textarea[aria-label="Request indexed evidence"]');input.value='Explicit context question';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Gather cited context'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Gather cited context').click()`);
    assert.deepEqual(await browser.evaluate(`window.requests.find(message=>message.type==='answer')`), { type: 'answer', question: 'Explicit context question' });
    await browser.evaluate(`(()=>{const question=document.querySelector('[aria-label="Context question"]');question.value='é'.repeat(8192);question.dispatchEvent(new Event('input',{bubbles:true}));const budget=document.querySelector('[aria-label="Native context budget"]');budget.value=JSON.stringify({calls:2,rounds:1,fanout:2,input_tokens:512,candidates:4,max_bytes:4096,deadline_ms:1200});budget.dispatchEvent(new Event('input',{bubbles:true}));})()`);
    await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Gather with this budget'&&!button.disabled)`);
    await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Gather with this budget').click()`);
    assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='answer').at(-1)`),{type:'answer',question:'é'.repeat(8192),budget:{calls:2,rounds:1,fanout:2,input_tokens:512,candidates:4,max_bytes:4096,deadline_ms:1200}});
  });
