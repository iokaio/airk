// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import * as path from 'node:path';
import { test } from 'node:test';
import type { ExplorerViewState } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';
import { HandoffStore } from '../explorerHandoffs';

// Minimal vscode stub: panels record HTML/messages and expose their message handler.
const disposable = { dispose() {} };
const panels: any[] = [];
const vscodeStub = {
    workspace: { textDocuments: [] as any[] },
    Uri: { file: (fsPath: string) => ({ scheme: 'file', fsPath }), joinPath: (base: any, ...parts: string[]) => ({ path: [base.path, ...parts].join('/'), toString() { return `vscode-resource:${this.path}`; } }) },
    ViewColumn: { Active: 1 },
    EventEmitter: class { event = () => disposable; fire() {} dispose() {} },
    window: {
        tabGroups: { all: [] as any[] },
        showWarningMessage: async (): Promise<string | undefined> => undefined,
        showOpenDialog: async (): Promise<any[] | undefined> => undefined,
        showQuickPick: async (_items: any[]): Promise<any> => undefined,
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
const { ExplorerSidebar, renderSidebar, sidebarContent, sidebarMessage } = require('../explorerSidebar') as typeof import('../explorerSidebar');
const { SuggestionStore } = require('../explorerSuggestions') as typeof import('../explorerSuggestions');

const project = 'ab'.repeat(32);
test('sharing requires explicit confirmation and rechecks the draft revision after the modal', async t => {
    let revision = 2, approved = false, changeDuringDialog = false;
    const handoffs = new HandoffStore({ authorized: () => true, binding: () => 'ab'.repeat(8), revisions: () => ({ studio: revision }), publication: () => undefined });
    const { explorer } = controller(true, new Map(), handoffs);
    t.mock.method(vscodeStub.window, 'showWarningMessage', async () => { if (changeDuringDialog) { revision++; } return approved ? 'Share snapshot' : undefined; });
    const request = { v: 1, requestId: 'share', type: 'handoff.share', kind: 'studio', revision: 2, content: 'Complete reviewed instructions' };
    try {
        assert.deepEqual(await (explorer as any).dispatch(request), { shared: false });
        approved = true; const shared = await (explorer as any).dispatch(request);
        assert.equal(handoffs.get(shared.token)?.content, request.content);
        changeDuringDialog = true;
        await assert.rejects((explorer as any).dispatch(request), /draft, authorization or evidence changed/);
        assert.equal(handoffs.get(shared.token), undefined);
    } finally { explorer.dispose(); }
});
function controller(trusted = true, stored = new Map<string, unknown>(), handoffs?: HandoffStore) {
    const suggestions = new SuggestionStore({ binding: () => 'ab12ab12ab12ab12', revisions: () => ({}), publication: () => undefined, ttlMs: () => 3600_000 });
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
        suggestions, handoffs,
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
    return { explorer: new TestController(host as any), statusBar, runs, suggestions };
}
const flush = () => new Promise(resolve => setTimeout(resolve, 5));

test('Inspector without a citation uses only the host-selected file in the selected project', async () => {
    const { RefRegistry } = require('../explorerSearch') as typeof import('../explorerSearch');
    const instance = Object.create(ExplorerController.prototype) as any;
    const calls: unknown[] = [];
    Object.assign(instance, { selected: project, sourceFiles: new Map([[project, 'chosen.ts']]), refs: new RefRegistry(),
        requireProject: () => project, session: () => ({ call: async (tool: string, args: unknown) => {
            calls.push([tool, args]); return { publication: 'p', payload: { content_hash: 'ab'.repeat(32) } };
        } }) });
    const view = await instance.dispatch({ type: 'evidence.inspect' });
    assert.equal(view.path, 'chosen.ts');
    assert.deepEqual(calls, [['airk_file_info', { path: 'chosen.ts', response_mode: 'full' }]]);
    instance.selected = 'other-project';
    await assert.rejects(instance.dispatch({ type: 'evidence.inspect' }), /source file/);
    assert.equal(calls.length, 1);
});

test('source context survives Explorer focus and stays isolated by project', async t => {
    const rootA = path.resolve('source-a'), rootB = path.resolve('source-b');
    const instance = Object.create(ExplorerController.prototype) as any;
    Object.assign(instance, { selected: 'A', selectionEpoch: 1, sourceFiles: new Map(), publish() {}, host: { trusted: () => true,
        specs: () => [{ root: rootA, service: { project: 'A' } }, { root: rootB, service: { project: 'B' } }] } });
    const editor = (file: string) => ({ document: { uri: { scheme: 'file', fsPath: file } } });
    assert.equal(instance.rememberSource(editor(path.join(rootA, 'src', 'one.ts'))), true);
    instance.rememberSource(undefined);
    assert.equal(instance.activeRelativePath(), 'src/one.ts', 'webview focus does not erase the target');
    instance.selected = 'B'; assert.equal(instance.activeRelativePath(), undefined);
    instance.rememberSource(editor(path.join(rootB, 'two.ts'))); assert.equal(instance.activeRelativePath(), 'two.ts');
    instance.selected = 'A'; assert.equal(instance.activeRelativePath(), 'src/one.ts');
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => items.at(-1));
    t.mock.method(vscodeStub.window, 'showOpenDialog', async () => [{ scheme: 'file', fsPath: path.join(rootB, 'foreign.ts') }]);
    await assert.rejects(instance.dispatch({ type: 'source.choose' }), /Choose a file inside/);
    t.mock.method(vscodeStub.window, 'showOpenDialog', async () => [{ scheme: 'file', fsPath: path.join(rootA, 'chosen.ts') }]);
    await instance.dispatch({ type: 'source.choose' }); assert.equal(instance.activeRelativePath(), 'chosen.ts');
    t.mock.method(vscodeStub.window, 'showOpenDialog', async () => { instance.selectionEpoch++; return [{ scheme: 'file', fsPath: path.join(rootA, 'stale.ts') }]; });
    await assert.rejects(instance.dispatch({ type: 'source.choose' }), /binding changed/);
    assert.equal(instance.activeRelativePath(), 'chosen.ts');
    vscodeStub.window.tabGroups.all = [{ tabs: [{ input: { uri: { scheme: 'file', fsPath: path.join(rootB, 'open-tab.ts') } } }] }];
    t.after(() => { vscodeStub.window.tabGroups.all = []; });
    instance.select = (selected: string) => { instance.selected = selected; };
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => { assert.match(items[0].detail, /switches the selected project/); return items[0]; });
    await instance.dispatch({ type: 'source.choose' });
    assert.equal(instance.selected, 'B'); assert.equal(instance.activeRelativePath(), 'open-tab.ts');
});

test('Help offers native context and general choices without retrieving evidence', async t => {
    const instance = Object.create(ExplorerController.prototype) as any;
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => { assert.equal(items[0].label, 'Help for Code › Outline'); return items[0]; });
    assert.equal(await instance.dispatch({ type: 'help.choose', route: 'code/outline' }), 'screen');
    t.mock.method(vscodeStub.window, 'showQuickPick', async (items: any[]) => items[1]);
    assert.equal(await instance.dispatch({ type: 'help.choose', route: 'code/outline' }), 'general');
    t.mock.method(vscodeStub.window, 'showQuickPick', async () => undefined);
    assert.equal(await instance.dispatch({ type: 'help.choose', route: 'code/outline' }), null);
});

function sidebarView() {
    let visibility = () => {}, disposed = () => {}, receive = (_value: unknown) => {};
    const view = { visible: true, webview: { options: {}, html: '', posted: [] as any[],
        postMessage: async (value: any) => { view.webview.posted.push(value); return true; },
        onDidReceiveMessage: (listener: typeof receive) => { receive = listener; return disposable; } },
        onDidChangeVisibility: (listener: () => void) => { visibility = listener; return disposable; },
        onDidDispose: (listener: () => void) => { disposed = listener; return disposable; } };
    return { view, visible: (value: boolean) => { view.visible = value; visibility(); },
        send: (value: unknown) => receive(value), dispose: () => disposed() };
}

test('activity entry opens Explorer once per reveal, even without a service or trusted snapshot', async () => {
    const fixture = sidebarView(); let opens = 0, hides = 0, released = 0;
    let finish!: () => void;
    const sidebar = new ExplorerSidebar({ commandUris: [], folders: () => 1,
        attach: async () => ({ dispose: () => { released++; } }),
        open: () => { opens++; return new Promise<void>(resolve => { finish = resolve; }); },
        hide: async () => { hides++; fixture.visible(false); }, search: () => {} });
    sidebar.resolveWebviewView(fixture.view as any);
    fixture.visible(true); await flush();
    assert.equal(opens, 1, 'visibility during resolution cannot open a second panel');
    assert.match(fixture.view.webview.html, /Opening AIRK Explorer/);
    assert.doesNotMatch(fixture.view.webview.html, /Get started with AIRK/);
    finish(); await flush(); assert.equal(hides, 1);
    fixture.visible(true); assert.equal(opens, 2, 'a subsequent activity click reveals Explorer again');
    fixture.visible(false); finish(); await flush();
    assert.equal(hides, 1, 'a delayed launch cannot close the sidebar the user switched to');
    fixture.dispose(); sidebar.dispose(); assert.equal(released, 1);
});

test('activity entry shows the old panel only on an opening failure and can retry', async () => {
    const fixture = sidebarView(); let fail = true, hides = 0;
    const sidebar = new ExplorerSidebar({ commandUris: [], folders: () => 1,
        attach: async () => { throw new Error('snapshot unavailable'); },
        open: async () => { if (fail) { throw new Error('<missing bundle>'); } },
        hide: async () => { hides++; }, search: () => {} });
    sidebar.resolveWebviewView(fixture.view as any); await flush();
    const html = fixture.view.webview.posted.at(-1).html;
    assert.match(html, /Get started with AIRK/); assert.match(html, /&lt;missing bundle&gt;/);
    assert.match(html, /Retry AIRK Explorer/); assert.equal(hides, 0);
    fail = false; fixture.send({ type: 'sidebar', action: 'open' }); await flush();
    assert.equal(hides, 1); assert.doesNotMatch(fixture.view.webview.posted.at(-1).html, /Get started with AIRK/);
    fixture.dispose(); sidebar.dispose();
});

test('a recreated activity view waits for the old launch and then opens its own Explorer', async () => {
    const first = sidebarView(), second = sidebarView(); let finish!: () => void, opens = 0, hides = 0;
    const sidebar = new ExplorerSidebar({ commandUris: [], folders: () => 1, attach: async () => disposable,
        open: () => { opens++; return opens === 1 ? new Promise<void>(resolve => { finish = resolve; }) : Promise.resolve(); },
        hide: async () => { hides++; }, search() {} });
    sidebar.resolveWebviewView(first.view as any); first.dispose(); sidebar.resolveWebviewView(second.view as any);
    finish(); await flush();
    assert.equal(opens, 2); assert.equal(hides, 1);
    second.dispose(); sidebar.dispose();
});

test('status selection generations load the new project after a pending old request settles', async () => {
    const instance = Object.create(ExplorerController.prototype) as any;
    let release!: (value: any) => void;
    const calls: string[] = [];
    Object.assign(instance, { selected: 'A', selectionEpoch: 1, host: { trusted: () => true }, publish() {},
        session() { const selected = this.selected; return { call: () => {
            calls.push(selected);
            return selected === 'A' ? new Promise(resolve => { release = resolve; }) : Promise.resolve({ payload: { index_state: 'available', publication: { token: 'B' } } });
        } }; } });
    const old = instance.loadStatus();
    instance.selected = 'B'; instance.selectionEpoch++;
    void instance.loadStatus();
    release({ payload: { index_state: 'available', publication: { token: 'A' } } });
    await old; await flush();
    assert.deepEqual(calls, ['A', 'B']); assert.equal(instance.statusFor, 'B'); assert.equal(instance.status.publication, 'B');
});

test('Expand uses exact hash/revision and bounded context rather than a range beyond EOF', async () => {
    const instance = Object.create(ExplorerController.prototype) as any;
    const calls: any[] = [];
    Object.assign(instance, { requireProject: () => project, refs: { get: () => ({ path: 'a.ts', startLine: 1, endLine: 1, contentHash: 'ab'.repeat(32), publication: 'p' }), add: () => 'h2' },
        session: () => ({ call: async (tool: string, args: any) => {
            calls.push({ tool, args });
            return { publication: 'p', payload: tool === 'airk_file_info' ? { line_basis: 'source' } : { hits: [{ citation: { path: 'a.ts', start_line: 1, end_line: 1, content_hash: 'ab'.repeat(32) }, text: 'one line' }] } };
        } }) });
    const result = await instance.dispatch({ type: 'evidence.expand', ref: 'h1', before: 5, after: 5 });
    assert.equal(result.text, 'one line');
    assert.equal(calls[1].tool, 'airk_context');
    assert.equal(calls[1].args.revision, 'p'); assert.equal(calls[1].args.known_hash, 'ab'.repeat(32));
    assert.equal(calls[1].args.end_line, undefined);
});

test('live help selectors and continuation call only airk_help and preserve its page metadata', async () => {
    const instance = Object.create(ExplorerController.prototype) as any, calls: any[] = [];
    Object.assign(instance, { requireProject: () => project, session: () => ({ call: async (tool: string, args: any) => {
        calls.push({ tool, args }); return { payload: { topic: 'tools', title: 'Tool reference', content: 'schema page', tool: 'airk_code_facts', version: '1.5.0', start_line: 8, next_line: 20, truncated: true } };
    } }) });
    const reply = await instance.dispatch({ type: 'help.engine', tool: 'airk_code_facts', startLine: 8 });
    assert.deepEqual(calls, [{ tool: 'airk_help', args: { tool: 'airk_code_facts', start_line: 8, max_bytes: 16384 } }]);
    assert.equal(reply.startLine, 8); assert.equal(reply.nextLine, 20); assert.equal(reply.tool, 'airk_code_facts');
    await assert.rejects(instance.dispatch({ type: 'help.engine', tool: 'airk_search', command: 'runtime' }), /Choose one help selector/);
    assert.equal(calls.length, 1, 'conflicting selectors reach no native operation');
});

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
    assert.equal(results.m2.error.code, 'INVALID_MESSAGE', 'unknown console actions are refused without dispatch');
    assert.deepEqual(runs, [['index', { project }]], 'index runs only through the shared console for a project on its page');
    assert.equal(results.x1.error.code, 'INVALID_MESSAGE', 'unrecognized messages receive a bounded refusal before dispatch');
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
    assert.equal((await reply('import1', { type: 'recipes.save', recipe: recipe('imported'), expected: 'null' })).ok, true);
    const conflict = await reply('import2', { type: 'recipes.save', recipe: recipe('imported', { query: 'replacement' }), expected: 'null' });
    assert.equal(conflict.error.code, 'PROPOSAL_CONFLICT', 'a file import cannot overwrite a recipe added after review');
    await reply('import-delete', { type: 'recipes.delete', name: 'imported' });
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

test('the Inbox reaches the webview as summaries; review, resolve and discard go through typed messages', async () => {
    const { explorer, suggestions } = controller();
    explorer.open('#overview');
    const panel = panels.at(-1);
    panel.send({ v: 1, type: 'ready', requestId: 'ready' });
    await flush();
    const admitted = suggestions.submit('search', { schema_version: '1.0', idempotency_key: 'k', title: 'Find renewals', draft: { query: 'renew <script>' } }, { name: 'agent', version: '1' });
    assert.ok(admitted.ok);
    await flush();
    const state = panel.posted.filter((message: any) => message.type === 'state').at(-1).state as ExplorerViewState;
    assert.equal(state.suggestions?.open, 1);
    assert.equal((state.suggestions?.cards[0] as any).body, undefined, 'state carries no draft bodies');
    const reply = async (id: string, message: Record<string, unknown>) => {
        panel.send({ v: 1, requestId: id, ...message });
        await flush();
        return panel.posted.find((entry: any) => entry.type === 'result' && entry.requestId === id);
    };
    const detail = await reply('g1', { type: 'suggestions.get', id: admitted.receipt.proposal_id });
    assert.equal(detail.data.body.search.query, 'renew <script>');
    assert.equal((await reply('r1', { type: 'suggestions.resolve', id: admitted.receipt.proposal_id, status: 'executed' })).error.code, 'INVALID_MESSAGE', 'unknown statuses are refused without changing the proposal');
    assert.equal(suggestions.status(admitted.receipt.proposal_id)?.status, 'pending_review');
    assert.equal((await reply('r2', { type: 'suggestions.resolve', id: admitted.receipt.proposal_id, status: 'deferred' })).ok, true);
    assert.equal(suggestions.status(admitted.receipt.proposal_id)?.status, 'deferred');
    assert.deepEqual((await reply('d1', { type: 'suggestions.discardAll' })).data, { discarded: 1 });
    assert.equal((await reply('r3', { type: 'suggestions.resolve', id: admitted.receipt.proposal_id, status: 'applied' })).error.code, 'PROPOSAL_EXPIRED', 'a rejected draft cannot be applied');
    await reply('v1', { type: 'suggestions.revisions', studio: 3, planner: 7 });
    assert.deepEqual(explorer.draftRevisions, { studio: 3, planner: 7 });
    explorer.showInbox();
    assert.ok(panel.posted.some((message: any) => message.type === 'inbox'));
    explorer.dispose();
});
