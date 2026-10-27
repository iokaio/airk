// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as path from 'node:path';
import * as vm from 'node:vm';
import { buildSync } from 'esbuild';

// Execute the actual bundled UI state/operations with an inert VS Code messaging port.
function ui(document?: unknown) {
    const root = path.resolve(__dirname, '../..');
    const output = buildSync({ stdin: { contents: `export * from './src/explorerWebview/strategyStore'; export * from './src/explorerWebview/planner'; export {applyReviewedSuggestion} from './src/explorerWebview/inbox'; export {dialogFocus} from './src/explorerWebview/components';`, resolveDir: root },
        bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic', jsxImportSource: 'preact' }).outputFiles[0].text;
    const module = { exports: {} as any };
    vm.runInNewContext(output, { module, exports: module.exports, require, TextEncoder, setTimeout, clearTimeout, document,
        window: { addEventListener() {} }, acquireVsCodeApi: () => ({ getState: () => undefined, setState() {}, postMessage() {} }) });
    return module.exports;
}

test('Studio rejects delayed runs after edits/replacement/project switches and invalidates generated prompts', () => {
    const api = ui(); api.selectDraftProject('A', 1);
    api.updateStudioDraft((prior: any) => ({ ...prior, objective: 'old', prompt: 'old generated', promptEdited: false }));
    assert.equal(api.studioDraft().prompt, undefined);
    const revision = api.studioDraft().revision, owner = api.currentSelection();
    api.updateStudioDraft((prior: any) => ({ ...prior, params: { query: 'new' } }));
    assert.equal(api.acceptStudioRun({}, revision, owner, 'wrong prompt'), false);
    const latest = api.studioDraft().revision;
    assert.equal(api.acceptStudioRun({}, latest, owner, 'fresh'), true);
    assert.equal(api.studioDraft().prompt, 'fresh');
    api.selectDraftProject('B', 2);
    assert.equal(api.studioDraft().run, undefined); assert.equal(api.studioDraft().prompt, undefined);
    assert.equal(api.acceptStudioRun({}, api.studioDraft().revision, owner, 'A evidence'), false);
    api.updateStudioDraft((prior: any) => ({ ...prior, prompt: 'manual', promptEdited: true }));
    api.updateStudioDraft((prior: any) => ({ ...prior, pathPrefix: 'changed' }));
    assert.equal(api.studioDraft().prompt, 'manual'); assert.equal(api.studioDraft().promptStale, true);
});

test('dialog keyboard focus cycles and returns to its invoking control', () => {
    const document: any = { activeElement: undefined };
    const control = () => ({ isConnected: true, focus() { document.activeElement = this; } });
    const invoking = control(), first = control(), last = control(); invoking.focus();
    let key!: (event: any) => void, closed = false;
    const element = { querySelectorAll: () => [first, last], addEventListener: (_name: string, listener: any) => { key = listener; }, removeEventListener() {} };
    const dispose = ui(document).dialogFocus(element, () => { closed = true; });
    assert.equal(document.activeElement, first);
    key({ key: 'Tab', shiftKey: true, preventDefault() {} }); assert.equal(document.activeElement, last);
    key({ key: 'Tab', shiftKey: false, preventDefault() {} }); assert.equal(document.activeElement, first);
    key({ key: 'Escape', preventDefault() {} }); assert.equal(closed, true);
    dispose(); assert.equal(document.activeElement, invoking);
});

test('Planner refuses oversized/stale Studio instructions instead of truncating completion gates', () => {
    const api = ui();
    const plan = { tasks: [] };
    const studio = { strategy: { title: 'Test' }, objective: 'objective', prompt: 'x'.repeat(8001), profile: 'portable' };
    assert.throws(() => api.taskFromStudio(plan, studio), /exceeds/);
    studio.prompt = 'Complete all gates';
    assert.equal(api.taskFromStudio(plan, studio).instructions, studio.prompt);
    assert.equal(api.canAddStudio({ ...studio, promptStale: true }), false);
});

test('Inbox never changes a local draft on failed/expired admission or on a target revision race', async () => {
    const api = ui(); let writes = 0, generation = 'one', conflicts = 0;
    const update = () => { writes++; };
    await assert.rejects(api.applyReviewedSuggestion('one', () => generation, async () => { throw new Error('PROPOSAL_EXPIRED'); }, update, async () => {}), /EXPIRED/);
    await assert.rejects(api.applyReviewedSuggestion('one', () => generation, async () => { generation = 'two'; }, update, async () => { conflicts++; }), /changed/);
    assert.equal(writes, 0); assert.equal(conflicts, 1);
    await api.applyReviewedSuggestion('two', () => generation, async () => {}, update, async () => {});
    assert.equal(writes, 1);
});
