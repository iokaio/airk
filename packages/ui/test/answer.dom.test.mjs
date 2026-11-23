// SPDX-License-Identifier: Apache-2.0
// Actual Workbench DOM over the Rust-emitted Answer /3 reply cases with a
// synthetic host; no native execution or transport parity claim.
import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
import { browserAvailable, browserHarness } from './browserHarness.mjs';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
const cases = JSON.parse(await readFile(new URL('../../../fixtures/split/arbiter/answer_loop_cases.json', import.meta.url), 'utf8'));
const emitted = id => structuredClone(cases.response_cases.find(row => row.id === id && row.accepted).response);
let browser;
before(async () => { if (browserAvailable) browser = await browserHarness(); });
after(async t => { try { await browser?.close(); } finally { t.diagnostic(JSON.stringify({browser_ownership: browser?.observation})); } });
async function mount() {
  const bundle = await build({ absWorkingDir: root, entryPoints: ['src/workbenchWebview/main.tsx'], bundle: true, write: false,
    outdir: 'out/unused-answer-dom', format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact' });
  await browser.evaluate(`document.body.innerHTML='<div id="root"></div>';window.requests=[];window.acquireVsCodeApi=()=>({postMessage:message=>window.requests.push(message)});`);
  await browser.evaluate(bundle.outputFiles.find(file => file.path.endsWith('.js')).text);
  await browser.observed(`window.requests.some(message=>message.type==='ready')`);
}
const project = 'c'.repeat(64);
const deliver = (turn, answer, cards = [], busy = false, request) => browser.evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:${JSON.stringify({ projects: [{ id: project, name: 'Fixture' }], project,
  session: { schema: 1, id: 'fixture-session', project, title: 'Context', createdAt: 1, updatedAt: 1, revision: 1, omitted: 0,
    turns: [{ id: turn, input: 'Who owns the fence?', at: 1, format: 'structured', ...(request ? { request } : {}), ...(answer ? { envelope: answer } : {}) }] },
  cards: { [turn]: cards }, recent: [], busy, save: 'Synthetic host', persistence: false, sessionEpoch: 1 })}}}));`);

test('actual Workbench Answer /3 groups parts and sub-needs, opens cited spans by handle and keeps full copy/export',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async t => {
    let stage = 'compile-workbench';
    try {
      await mount();
      const turn = 'observed-answer', answer = emitted('two_parts_valid'), cited = answer.records[1].id, omitted = '7a'.repeat(32);
      answer.records[1].canonical_result = { projection: 'airk.arbiter.loop-evidence/1', revision: answer.scope.publication.token, hits_total: 1,
        spans: [{ pointer: '/hits/0', path: 'original/path/é中.rs', start_line: 7, end_line: 9, content_hash: 'd'.repeat(64), text: 'Original <script>must remain text</script>', truncated: false, focus: 0.75 }] };
      answer.paragraphs[0].text = 'Retained evidence <script>must remain text</script>';
      answer.parts[1].sub_needs[0].evidence.push(omitted); answer.parts[1].sub_needs[0].evidence_omitted = 1; answer.omitted_evidence = [omitted];
      const cards = [{ id: 'record-0', path: 'original/path/é中.rs', hash: 'd'.repeat(64), start: 7, end: 9, text: 'Original native source', step: cited, resultPointer: '/spans/0',
        publication: answer.scope.publication.token, indexId: answer.scope.index_id, sourceRootId: 'src_' + 'e'.repeat(32), sourceRelativePath: 'original/path/é中.rs', projectIds: [] }];
      stage = 'deliver-retained-native-state';
      await deliver(turn, answer, cards);
      stage = 'answer-rendered-barrier';
      await browser.observed(`document.body.textContent.includes('Part 2: Which README section covers search?')`);
      const body = await browser.evaluate('document.body.textContent');
      for (const text of ['Complete within the request budget.', 'Part 1: Where is the restart guard defined?', 'Sub-need 1.1 (definition): where is the restart guard defined', 'Covered.',
        'Sub-need 2.1 (documentation): README section on search', 'Partly covered.', 'document sections README.md — not run (not chosen)', 'was cited but omitted from this reply to fit its size',
        'The Context model abstained 1 time(s)', 'Model decided (top score)', 'Model abstained (abstained); deterministic fallback top focus span', 'Deterministic order (explicit breaks)', 'Focus 0.750'])
        assert.ok(body.includes(text), text);
      assert.ok(body.indexOf('Sub-need 1.1') < body.indexOf('Part 2:') && body.indexOf('Part 2:') < body.indexOf('Sub-need 2.1'), 'grouped by part then sub-need');
      assert.equal(await browser.evaluate(`document.querySelectorAll('script').length`), 0);
      stage = 'read-evidence-button';
      await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent.startsWith('symbols ${cited.slice(0, 12)}')).click()`);
      await browser.observed(`document.activeElement?.id==='answer-${turn}-${cited}'`);
      assert.equal(await browser.evaluate(`document.activeElement.textContent.includes('original/path/é中.rs')&&document.activeElement.closest('details').open`), true);
      stage = 'open-source-locator-button';
      await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Open original/path/é中.rs:7–9'&&!button.disabled).click()`);
      assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='openAnswerCitation')`),
        [{ type: 'openAnswerCitation', turn, record: cited, span: 0 }], 'renderer sends only retained IDs and a bounded span locator, never a source path/root/hash');
      stage = 'unbound-span-is-disabled';
      await deliver(turn, answer, []);
      await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Open original/path/é中.rs:7–9'&&button.disabled)`);
      await deliver(turn, answer, cards);
      stage = 'complete-result-copy-export';
      await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Open original/path/é中.rs:7–9'&&!button.disabled)`);
      assert.deepEqual(await browser.evaluate(`JSON.parse([...document.querySelectorAll('details.complete-response pre')][0].textContent)`), answer);
      await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='json').click()`);
      await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Copy JSON')`);
      await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Copy JSON').click();[...document.querySelectorAll('button')].find(button=>button.textContent==='Save export…').click();`);
      assert.deepEqual(await browser.evaluate(`window.requests.filter(message=>message.type==='copy'||message.type==='export')`),
        [{ type: 'copy', turn, format: 'json' }, { type: 'export', turn, format: 'json' }]);
      stage = 'answer-question-budget-controls';
      await browser.evaluate(`{const input=document.querySelector('textarea[aria-label="Request indexed evidence"]');input.value='Explicit context question';input.dispatchEvent(new Event('input',{bubbles:true}));}`);
      await browser.evaluate(`document.querySelector('button[aria-controls="evidence-options"]').click()`);
      await browser.observed(`document.querySelector('#evidence-options[role="menu"]')`);
      await browser.evaluate(`[...document.querySelectorAll('[role="menuitem"]')].find(button=>button.textContent==='Gather cited context…').click()`);
      await browser.observed(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Gather cited context'&&!button.disabled)`);
      assert.deepEqual(JSON.parse(await browser.evaluate(`document.querySelector('[aria-label="Native context budget"]').value`)), cases.default_budget, 'the form starts from the Rust-emitted default');
      await browser.evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Gather cited context').click()`);
      assert.deepEqual(await browser.evaluate(`window.requests.find(message=>message.type==='answer')`), { type: 'answer', question: 'Explicit context question', budget: cases.default_budget });
      stage = 'running-answer-status';
      await deliver('running', undefined, [], true, { schema: cases.request_schema, question: 'q', budget: cases.default_budget });
      await browser.observed(`document.body.textContent.includes('it waits up to 305 s for the reply')`);
    } catch (error) { t.diagnostic(JSON.stringify({ stage, browser_ownership: browser?.observation })); throw error; }
  });

test('partial, queued and combined Answer /3 replies disclose their state and keep every member whole',
  { skip: browserAvailable ? false : 'Chromium unavailable.' }, async t => {
    let stage = 'compile';
    try {
      await mount();
      stage = 'partial';
      const partial = emitted('two_parts_valid'); partial.partial = { stage: 'coverage', reason: 'deadline', part: 2, continuation: 'ab'.repeat(32) };
      await deliver('partial', partial);
      await browser.observed(`document.body.textContent.includes('Partial: stopped at the coverage stage (deadline) in part 2.')`);
      assert.ok((await browser.evaluate('document.body.textContent')).includes(`Continuation ${'ab'.repeat(32)}`));
      stage = 'queued';
      const queued = emitted('ticket_valid');
      await deliver('queued', queued);
      await browser.observed(`document.body.textContent.includes('Queued: position')`);
      assert.deepEqual(await browser.evaluate(`JSON.parse(document.querySelector('details.complete-response pre').textContent)`), queued);
      stage = 'combined';
      const first = emitted('two_parts_valid'), other = 'idx_' + '77'.repeat(16), second = JSON.parse(JSON.stringify(first).replaceAll(first.scope.index_id, other));
      const combined = { schema: 'airk.combined-read/1', context: { schema: 'airk.combined-context/2', future: { kept: true } },
        publication_vector: { members: [{ index_id: first.scope.index_id, publication: first.scope.publication }, { index_id: other, publication: first.scope.publication }] },
        members: [{ index_id: first.scope.index_id, outcome: 'complete', result: { content: [], structuredContent: first } }, { index_id: other, outcome: 'failed', result: { isError: true, content: [{ type: 'text', text: 'retry later' }] } }] };
      await deliver('combined', combined);
      await browser.observed(`document.body.textContent.includes('Combined Answer')`);
      assert.equal(await browser.evaluate(`document.querySelectorAll('section[aria-label^="Answer partition "]').length`), 2);
      assert.deepEqual(await browser.evaluate(`JSON.parse(document.querySelector('details.complete-response pre').textContent)`), combined);
      const body = await browser.evaluate('document.body.textContent');
      assert.ok(body.includes(`Index ${first.scope.index_id} · member 0 · complete`) && body.includes(`Index ${other} · member 1 · failed`));
      assert.ok(body.includes('No valid locally bound current Answer packet was returned for this member'));
    } catch (error) { t.diagnostic(JSON.stringify({ stage, browser_ownership: browser?.observation })); throw error; }
  });
