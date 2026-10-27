// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { test } from 'node:test';
import type { ExplorerViewState } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';

// Minimal vscode stub: panels record HTML/messages and expose their message handler.
const disposable = { dispose() {} };
const panels: any[] = [];
const vscodeStub = {
    Uri: { joinPath: (base: any, ...parts: string[]) => ({ path: [base.path, ...parts].join('/'), toString() { return `vscode-resource:${this.path}`; } }) },
    ViewColumn: { Active: 1 },
    EventEmitter: class { event = () => disposable; fire() {} dispose() {} },
    window: {
        createWebviewPanel: (_type: string, _title: string, _column: unknown, options: any) => {
            let receive: (value: unknown) => void = () => undefined;
            let closed: () => void = () => undefined;
            const panel = {
                options, posted: [] as any[], disposed: false,
                webview: { html: '', cspSource: 'vscode-resource:', asWebviewUri: (uri: any) => uri, postMessage: async (message: unknown) => { panel.posted.push(message); return true; },
                    onDidReceiveMessage: (handler: (value: unknown) => void) => { receive = handler; return disposable; } },
                onDidChangeViewState: () => disposable,
                onDidDispose: (handler: () => void) => { closed = handler; return disposable; },
                reveal() {}, dispose() { panel.disposed = true; closed(); },
                send: (value: unknown) => receive(value),
            };
            panels.push(panel); return panel;
        },
    },
};
const load = (Module as any)._load;
(Module as any)._load = function (request: string, ...rest: unknown[]) { return request === 'vscode' ? vscodeStub : load.call(this, request, ...rest); };
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { ExplorerController } = require('../explorerController') as typeof import('../explorerController');
const { explorerHtml } = require('../explorerPanel') as typeof import('../explorerPanel');
const { renderSidebar, sidebarContent, sidebarMessage } = require('../explorerSidebar') as typeof import('../explorerSidebar');

const project = 'ab'.repeat(32);
function controller(trusted = true, stored = new Map<string, unknown>()) {
    const statusBar: string[] = [];
    const runs: unknown[] = [];
    const consoleState: any = { trusted, connected: true, busy: false, health: { state: 'live', checkedAt: 1, value: { observation: { policy: { accepting: true } } } },
        bootstrap: { state: 'live', checkedAt: 1, value: {} }, currentProjects: new Set([project]),
        page: { revision: 1, cached: true, observed_unix_ms: 7, after: null, projects: [{ project: { registration: { binding: project, root: '/src/demo', collection: 'demo' } } }] } };
    const host = {
        extensionUri: { path: '/ext' } as any, version: '1.5.0-test', trusted: () => trusted,
        specs: () => [{ command: '/launcher', root: '/src/demo', dataDir: '/data', collection: 'demo', name: 'demo', service: { installDir: '/install', project } }],
        activeProject: () => project, onDidChangeSpecs: () => disposable, log: () => undefined,
        setStatusBar: (text: string) => { statusBar.push(text); },
        workspaceState: { get: <T>(key: string, fallback?: T) => (stored.has(key) ? stored.get(key) : fallback) as T, update: async (key: string, value: unknown) => { stored.set(key, value); } },
        control: { controller: { state: consoleState }, run: async (action: string, fields?: unknown) => { runs.push([action, fields]); },
            retain: () => disposable, onDidChangeState: () => disposable },
    };
    class TestController extends ExplorerController {
        protected override session(): any {
            return { call: async (tool: string, args: any) => {
                if (tool === 'airk_search') {
                    assert.deepEqual(args, { query: 'x', limit: 5, max_bytes: 8192, full_chunks: false, mode: 'lexical' });
                    return { publication: 'airk-r1:t', payload: { truncated: false, publication: { token: 'airk-r1:t' },
                        hits: [{ citation: { path: 'a.ts', start_line: 2, end_line: 4, content_hash: 'cd'.repeat(32) }, text: 'x', truncated: false }] } };
                }
                assert.equal(tool, 'airk_status');
                return { payload: { index_state: 'available', publication: { token: 'airk-r1:t' }, semantic_readiness: { state: 'ready' }, freshness: { state: 'idle' } } };
            }, dispose() {} };
        }
    }
    return { explorer: new TestController(host as any), statusBar, runs };
}
const flush = () => new Promise(resolve => setTimeout(resolve, 5));

test('the panel shell uses a nonce CSP, no command URIs and only the out/ bundles', () => {
    const html = explorerHtml({ cspSource: 'vscode-resource:', asWebviewUri: (uri: any) => uri }, { path: '/ext' } as any, 'n0nce');
    assert.match(html, /default-src 'none'/);
    assert.match(html, /script-src 'nonce-n0nce'/);
    assert.doesNotMatch(html, /unsafe-inline|unsafe-eval|https?:\/\//);
    assert.match(html, /\/ext\/out\/explorerWebview\.js/);
    assert.match(html, /\/ext\/out\/explorerWebview\.css/);
});

test('the controller publishes state, loads published status and rejects invalid or unknown messages', async () => {
    const { explorer, statusBar, runs } = controller();
    explorer.open('#search/query');
    const panel = panels.at(-1);
    assert.equal(panel.options.enableCommandUris, false);
    assert.deepEqual(panel.options.localResourceRoots.map((uri: any) => uri.path), ['/ext/out']);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const state = panel.posted.filter((message: any) => message.type === 'state').at(-1).state as ExplorerViewState;
    assert.equal(state.selectedProject, project);
    assert.equal(state.status?.publication, 'airk-r1:t');
    assert.equal(state.projects[0].collection, 'demo');
    assert.deepEqual(state.features, FEATURES);
    assert.ok(panel.posted.some((message: any) => message.type === 'navigate' && message.route === '#search/query'));
    assert.equal(statusBar.at(-1), 'AIRK: ready');

    panel.send({ v: 1, type: 'project.select', requestId: 'p1', project: 'cd'.repeat(32) });
    panel.send({ v: 1, type: 'nav.go', requestId: 'n1', route: '#nowhere' });
    panel.send({ v: 1, type: 'search.run', requestId: 's1', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false });
    panel.send({ v: 1, type: 'source.open', requestId: 'o1', ref: 'h404' });
    panel.send({ v: 1, type: 'search.run', requestId: 's2', query: 'x', mode: 'lexical', limit: 5, maxBytes: 8192, fullChunks: false, pathPrefix: '../secret' });
    panel.send({ v: 1, type: 'manage.action', requestId: 'm1', action: 'index', project });
    panel.send({ v: 1, type: 'manage.action', requestId: 'm2', action: 'shell', project });
    panel.send({ v: 1, type: 'shell.exec', requestId: 'x1', command: 'rm -rf /' });
    await flush();
    const results = Object.fromEntries(panel.posted.filter((message: any) => message.type === 'result').map((message: any) => [message.requestId, message]));
    assert.equal(results.p1.ok, false); assert.equal(results.p1.error.code, 'NO_PROJECT');
    assert.equal(results.n1.ok, false); assert.equal(results.n1.error.code, 'INVALID_MESSAGE');
    assert.equal(results.s1.ok, true, JSON.stringify(results.s1)); assert.equal(results.s1.data.hits[0].path, 'a.ts'); assert.match(results.s1.data.hits[0].ref, /^h/);
    assert.equal(results.s1.data.ordering, 'index_identity');
    assert.equal(results.o1.ok, false); assert.equal(results.o1.error.code, 'UNKNOWN_REFERENCE');
    assert.equal(results.s2.ok, false); assert.equal(results.s2.error.code, 'INVALID_MESSAGE');
    assert.equal(results.m1.ok, true, JSON.stringify(results.m1));
    assert.equal(results.m2, undefined, 'unknown console actions are dropped by the message validator');
    assert.deepEqual(runs, [['index', { project }]], 'index runs only through the shared console for a project on its page');
    assert.equal(results.x1, undefined, 'unrecognized messages are dropped before the controller');
    explorer.dispose();
    assert.equal(panel.disposed, true);
});

test('an untrusted window publishes no projects and never loads evidence', async () => {
    const { explorer } = controller(false);
    explorer.open();
    const panel = panels.at(-1);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const state = panel.posted.filter((message: any) => message.type === 'state').at(-1).state as ExplorerViewState;
    assert.equal(state.trusted, false);
    assert.deepEqual(state.projects, []);
    assert.equal(state.status, undefined);
    explorer.dispose();
});

test('the sidebar accepts only open and bounded search, escapes values and falls back to setup guidance', () => {
    assert.deepEqual(sidebarMessage({ type: 'sidebar', action: 'open' }), { type: 'sidebar', action: 'open' });
    assert.deepEqual(sidebarMessage({ type: 'sidebar', action: 'search', query: 'refresh token' }), { type: 'sidebar', action: 'search', query: 'refresh token' });
    for (const bad of [{ type: 'sidebar', action: 'search', query: '' }, { type: 'sidebar', action: 'search', query: 'a\nb' },
        { type: 'sidebar', action: 'open', command: 'x' }, { type: 'sidebar', action: 'run' }, { type: 'other', action: 'open' }, null]) {
        assert.equal(sidebarMessage(bad), undefined, JSON.stringify(bad));
    }
    assert.match(sidebarContent(undefined, 1), /Get started with AIRK/);
    const state: ExplorerViewState = { revision: 1, extensionVersion: 'x', trusted: true, connection: 'ready', features: FEATURES, selectedProject: project,
        projects: [{ project, collection: 'c', name: '<b>n</b>', root: '/r', current: true }] };
    const html = sidebarContent(state, 1);
    assert.match(html, /&lt;b&gt;n&lt;\/b&gt;/);
    assert.match(html, /Search indexed evidence/);
    assert.match(html, /not observed/);
    const page = renderSidebar(state, 1, 'abc');
    assert.match(page, /script-src 'nonce-abc'/);
    assert.doesNotMatch(page, /unsafe-inline/);
});

test('search recipes are validated, kept per workspace, replaced by name and capped', async () => {
    const stored = new Map<string, unknown>([['airk.explorer.searchRecipes', [{ schema: 'airk.explorer.search-recipe/1', name: 'damaged', query: 'x', mode: 'shell', limit: 1, maxBytes: 1024, fullChunks: false }]]]);
    const { explorer } = controller(true, stored);
    explorer.open('#search/recipes');
    const panel = panels.at(-1);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const reply = async (id: string, message: Record<string, unknown>) => {
        panel.send({ v: 1, requestId: id, ...message });
        await flush();
        return panel.posted.find((entry: any) => entry.type === 'result' && entry.requestId === id);
    };
    const recipe = (name: string, extra: Record<string, unknown> = {}) => ({ schema: 'airk.explorer.search-recipe/1', name, query: 'renew', mode: 'lexical', limit: 20, maxBytes: 16384, fullChunks: false, ...extra });
    assert.deepEqual((await reply('l1', { type: 'recipes.list' })).data, [], 'a damaged stored recipe is dropped, never run');
    assert.deepEqual((await reply('s1', { type: 'recipes.save', recipe: recipe('b') })).data.map((entry: any) => entry.name), ['b']);
    assert.deepEqual((await reply('s2', { type: 'recipes.save', recipe: recipe('a', { pathPrefix: 'src/' }) })).data.map((entry: any) => entry.name), ['a', 'b']);
    assert.equal((await reply('s3', { type: 'recipes.save', recipe: recipe('a', { query: 'changed' }) })).data.find((entry: any) => entry.name === 'a').query, 'changed');
    assert.equal((await reply('s4', { type: 'recipes.save', recipe: recipe('c', { pathPrefix: '../outside' }) })).error.code, 'INVALID_SCHEMA');
    assert.equal((await reply('s5', { type: 'recipes.save', recipe: recipe('c', { command: 'x' }) })).error.code, 'INVALID_SCHEMA');
    assert.deepEqual((await reply('d1', { type: 'recipes.delete', name: 'b' })).data.map((entry: any) => entry.name), ['a']);
    for (let index = 0; index < 49; index++) { await reply(`f${index}`, { type: 'recipes.save', recipe: recipe(`r${String(index).padStart(2, '0')}`) }); }
    assert.match((await reply('over', { type: 'recipes.save', recipe: recipe('one-too-many') })).error.message, /At most 50 recipes/);
    explorer.dispose();
});
