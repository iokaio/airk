// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import { test } from 'node:test';
import { buildSync } from 'esbuild';

function store() {
    const file = path.resolve(__dirname, '../../src/explorerWebview/searchStore.ts'), sent: any[] = [];
    const module = { exports: {} as any };
    const built = buildSync({ entryPoints: [file], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['preact/hooks'] }).outputFiles[0].text;
    vm.runInNewContext(built, { module, exports: module.exports, require: createRequire(file),
        acquireVsCodeApi: () => ({ postMessage: (message: any) => sent.push(message) }),
        window: { addEventListener() {} }, setTimeout, clearTimeout });
    return { api: module.exports, sent };
}
const project = 'ab'.repeat(32);
const session = (api: any, count = 100) => ({ project, selectionEpoch: 1, form: { ...api.DEFAULT_SEARCH, query: 'renew', limit: 100 }, busy: false, page: 0,
    result: { query: 'renew', hits: Array.from({ length: count }, (_, at) => ({ ref: `h${at}`, text: `result ${at}` })), publication: 'old-publication' } });

test('a search survives tab consumers, keeps edited form/page and consumes incoming runs once', () => {
    const { api, sent } = store();
    api.selectSearchProject(project, 1); api.acceptSearchSession(session(api));
    api.editSearchPage(3); api.editSearch({ ...api.searchSnapshot().form, query: 'draft edit' });
    // Recreating a tab's consumer does not recreate the session or submit a read.
    assert.equal(api.searchSnapshot().result.hits.length, 100);
    assert.equal(api.searchSnapshot().page, 3); assert.equal(api.searchSnapshot().form.query, 'draft edit');
    api.acceptSearchSession(session(api));
    assert.equal(api.searchSnapshot().form.query, 'draft edit', 'an in-flight host snapshot cannot overwrite newer typing');
    assert.equal(api.searchSnapshot().page, 3);
    assert.equal(api.consumeSearchInput(7), true); assert.equal(api.consumeSearchInput(7), false);
    assert.ok(sent.every(message => ['search.form', 'search.page'].includes(message.type)), 'navigation does not rerun the search');
    // A newly opened webview receives the host-held session including its page.
    const reopened = store().api;
    reopened.selectSearchProject(project, 1); reopened.acceptSearchSession(api.searchSnapshot());
    assert.equal(reopened.searchSnapshot().form.query, 'draft edit'); assert.equal(reopened.searchSnapshot().page, 3);
    assert.equal(reopened.searchSnapshot().result.publication, 'old-publication');
});

test('search pages contain twenty results with no omissions, and new runs reset the page', () => {
    const { api } = store(); api.selectSearchProject(project, 1); api.acceptSearchSession(session(api));
    const hits = api.searchSnapshot().result.hits;
    const pages = Array.from({ length: 5 }, (_, page) => api.searchPage(hits, page));
    assert.ok(pages.every(page => page.length === 20));
    assert.equal(pages.flat().map(hit => hit.ref).join(','), hits.map((hit: any) => hit.ref).join(','));
    assert.equal(api.searchPage(hits.slice(0, 45), 2).length, 5);
    api.editSearchPage(99); assert.equal(api.searchSnapshot().page, 4);
    const old = api.beginSearch(api.searchSnapshot().form), latest = api.beginSearch(api.searchSnapshot().form);
    api.failSearch(old, 'late failure'); assert.equal(api.searchSnapshot().error, undefined);
    api.failSearch(latest, 'current failure'); assert.equal(api.searchSnapshot().error, 'current failure');
    assert.equal(api.searchSnapshot().page, 0);
    api.selectSearchProject(project, 2); api.acceptSearchSession(session(api));
    assert.equal(api.searchSnapshot().result, undefined, 'old binding snapshots are refused');
    api.selectSearchProject('cd'.repeat(32), 3); api.failSearch(latest, 'foreign failure');
    assert.equal(api.searchSnapshot().error, undefined); assert.equal(api.searchSnapshot().form.query, '');
});
