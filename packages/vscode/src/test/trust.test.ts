// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { afterEach, beforeEach, mock, test } from 'node:test';
import Module from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import * as cliInstall from '../cliInstall';

// Controller fixtures must never change the developer's actual PATH or shell profiles.
beforeEach(() => { mock.method(cliInstall, 'installCli', async () => []); });
afterEach(() => { mock.restoreAll(); });

function addTreeApi(stub: any, capture?: (provider: any) => void, dashboard?: (provider: any) => void): void {
    stub.TreeItem = class { constructor(public label: string) {} };
    stub.TreeItemCollapsibleState = { None: 0 };
    stub.ThemeIcon = class { constructor(public id: string) {} };
    stub.window.createTreeView = (_id: string, options: { treeDataProvider: any }) => {
        capture?.(options.treeDataProvider);
        return { dispose() {} };
    };
    stub.window.registerWebviewViewProvider = (_id: string, provider: unknown) => { dashboard?.(provider); return { dispose() {} }; };
}
function addReportApi(stub: any): void {
    stub.ViewColumn = { Active: 1 };
    stub.window.createWebviewPanel = () => ({
        webview: { html: '', onDidReceiveMessage: (handler: (message: unknown) => void) => {
            handler({ type: 'ignored' }); handler({ type: 'openJson' }); return { dispose() {} };
        } },
        onDidDispose: () => ({ dispose() {} })
    });
}

test('storage picker holds startup indexing, waits for active writes, and restores scheduling on cancellation or invalid paths', { timeout: 20000 }, async () => {
    for (const scenario of ['startup', 'busy', 'cancel', 'invalid', 'root-added']) {
        const busy = scenario === 'busy' || scenario === 'root-added';
        const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-storage-'));
        const root = path.join(directory, 'Project'), storage = path.join(directory, 'Host'), destination = path.join(directory, 'NewStorage');
        await fs.mkdir(root); await fs.mkdir(destination);
        const commands = new Map<string, () => Promise<unknown>>(), state = new Map<string, unknown>();
        const disposable = { dispose() {} }, event = () => disposable;
        let configured: string | undefined, dialogs = 0, releaseIndex!: () => void, indexStarted!: () => void, progressStarted!: () => void;
        let select!: (value: { fsPath: string }[] | undefined) => void;
        const selected = new Promise<{ fsPath: string }[] | undefined>(resolve => { select = resolve; });
        const started = new Promise<void>(resolve => { indexStarted = resolve; });
        const waiting = new Promise<void>(resolve => { progressStarted = resolve; });
        const indexing = new Promise<void>(resolve => { releaseIndex = resolve; });
        const indexed: string[] = [], errors: string[] = [];
        const stub = {
            workspace: { isTrusted: true, workspaceFolders: [{ name: 'Project', uri: { scheme: 'file', fsPath: root } }],
                getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.quietMs' ? 250 : fallback,
                    inspect: (key: string) => key === 'storage.path' ? { globalValue: configured } : undefined,
                    update: async (key: string, value: string) => { assert.equal(key, 'storage.path'); configured = value; } }),
                createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
                onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
                onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event },
            window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
                createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
                showOpenDialog: () => { dialogs++; return selected; }, showInformationMessage() {},
                withProgress: async (_options: unknown, run: () => Promise<unknown>) => { progressStarted(); return run(); },
                showErrorMessage: (error: string) => { errors.push(error); } },
            commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
            env: {}, ConfigurationTarget: { Global: 1 }, StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 1 },
            EventEmitter: class { event = event; fire() {} dispose() {} }, lm: {}
        };
        addTreeApi(stub);
        const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
        const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
        native.prepareHelper = async () => '/fixture/airk';
        native.runJson = async (spec: { dataDir: string }, command: string, extra: string[] = []) => {
            if (command === 'collections' && extra[0] === 'vectors') { return { enabled: false, published: false, indexed_vectors: 0, pending_reindex: false, default_search_mode: 'lexical' }; }
            if (command === 'index') {
                indexed.push(spec.dataDir); indexStarted();
                if (busy && indexed.length === 1) { await indexing; }
                return { generation: indexed.length, indexed: 1, reused: 0, removed: 0, skipped: [] };
            }
            return { scan_complete: true, eligible: 1, current: 1, indexed: true };
        };
        loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
        const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/fixture/extension',
            extension: { packageJSON: { version: '1.1.0' } }, globalStorageUri: { fsPath: storage },
            globalState: { get: (key: string, fallback: unknown) => state.get(key) ?? fallback,
                update: async (key: string, value: unknown) => { state.set(key, value); } } };
        try {
            for (const module of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(module)]; } await require('../extension').activate(context);
            if (busy) { await started; }
            const choosing = commands.get('airk.chooseStorage')!();
            const duplicate = commands.get('airk.chooseStorage')!();
            assert.equal(dialogs, 1, 'Repeated clicks share one picker even during an active index');
            await new Promise<void>(resolve => setTimeout(resolve, 350));
            assert.equal(indexed.length, busy ? 1 : 0, 'Opening the picker holds startup indexing');
            select(scenario === 'cancel' ? undefined : [{ fsPath: scenario === 'invalid' ? root : destination }]);
            if (busy) {
                await waiting; assert.equal(configured, undefined, 'Storage cannot switch before the writer finishes');
                if (scenario === 'root-added') { stub.workspace.workspaceFolders.push({ name: 'Added', uri: { scheme: 'file', fsPath: destination } }); }
                releaseIndex();
            }
            if (scenario === 'invalid' || scenario === 'root-added') {
                const results = await Promise.allSettled([choosing, duplicate]);
                assert.ok(results.every(result => result.status === 'rejected' && /must not be the workspace root/.test(String(result.reason))));
            } else { await Promise.all([choosing, duplicate]); assert.deepEqual(errors, []); }
            const changed = scenario === 'startup' || scenario === 'busy';
            assert.equal(configured, changed ? await fs.realpath(destination) : undefined);
            if (scenario === 'root-added') { stub.workspace.workspaceFolders.pop(); }
            // Explicit requests also resume after the hold ends; no lingering suspension.
            await commands.get('airk.index')!();
            assert.equal(indexed.at(-1), await fs.realpath(changed ? destination : path.join(storage, 'indexes')));
            if (changed) { assert.ok((state.get('storageHistory') as string[]).includes(await fs.realpath(path.join(storage, 'indexes')))); }
        } finally {
            select(undefined); releaseIndex(); context.subscriptions.forEach(item => item.dispose());
            for (const module of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(module)]; }
            loader._load = original; native.prepareHelper = priorPrepare; native.runJson = priorRun;
            const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
            assert.ok(path.basename(resolved).startsWith('airk-vscode-storage-')); await fs.rm(resolved, { recursive: true, force: true });
        }
    }
});

test('untrusted workspaces expose no MCP servers and commands cannot launch a helper', async () => {
    const commands = new Map<string, () => Promise<unknown>>(); const errors: string[] = []; let provider: any;
    const disposable = { dispose() {} };
    const event = () => disposable;
    const stub = {
        workspace: { isTrusted: false, getConfiguration: () => ({ get: (_key: string, fallback: unknown) => fallback, inspect: () => undefined }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event },
        window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
            showErrorMessage: (error: string) => { errors.push(error); } },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        StatusBarAlignment: { Left: 1 },
        EventEmitter: class { event = event; fire() {} dispose() {} },
        lm: { registerMcpServerDefinitionProvider: (_id: string, value: unknown) => { provider = value; return disposable; } }
    };
    addTreeApi(stub);
    const loader = Module as unknown as { _load: (...args: any[]) => any };
    const original = loader._load;
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    try {
        const extension = require('../extension');
        const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/nonexistent-helper-fixture' };
        const exported = await extension.activate(context);
        const contributed = (require('../../package.json').contributes.commands as { command: string }[]).map(item => item.command);
        assert.deepEqual([...commands.keys()].sort(), contributed.sort()); assert.deepEqual(await exported.getServerDefinitions(), []);
        assert.deepEqual(await provider.provideMcpServerDefinitions(), []);
        await assert.rejects(commands.get('airk.index')!(), /Trust this workspace/);
        assert.equal(errors.length, 1); assert.match(errors[0], /Trust this workspace/);
        await assert.rejects(commands.get('airk.installCli')!(), /Trust this workspace/);
        for (const name of ['airk.ingestionLimits', 'airk.configureIngestionLimits', 'airk.currentMaintenance', 'airk.extractionPolicy', 'airk.configureExtractionPolicy']) {
            await assert.rejects(commands.get(name)!(), /Trust this workspace/);
        }
        await assert.rejects(provider.resolveMcpServerDefinition({}), /Trust this workspace/);
        context.subscriptions.forEach(item => item.dispose());
    } finally { loader._load = original; }
});

test('trusted MCP definitions ignore workspace storage overrides and reconnect without immediate reindexing', async context => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-provider-'));
    const root = path.join(directory, 'Project'), storage = path.join(directory, 'Host'); await fs.mkdir(root);
    const commands = new Map<string, () => Promise<unknown>>(); const state = new Map<string, unknown>();
    const disposable = { dispose() {} }, event = () => disposable; let enabled = true; let provider: any, tree: any, events = 0, emitters = 0, removals = 0;
    let cliEnabled = false, cliAttempts = 0;
    context.mock.method(cliInstall, 'installCli', async () => {
        if (++cliAttempts === 1) { throw new Error('fixture PATH permission failure'); }
        return [];
    });
    class Definition {
        cwd?: { fsPath: string };
        constructor(public label: string, public command: string, public args: string[], public env: object, public version: string) {}
    }
    const stub = {
        workspace: { isTrusted: true, workspaceFolders: [{ name: 'Project', uri: { scheme: 'file', fsPath: root } }],
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback,
                inspect: (key: string) => key === 'mcp.registrationEnabled' ? { globalValue: enabled, workspaceValue: false } : key === 'cli.autoInstall' ? { globalValue: cliEnabled, workspaceValue: true } : ({ workspaceValue: path.join(root, 'untrusted-destination') }) }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event },
        window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
            showWarningMessage: async () => 'Remove index',
            showInformationMessage() {},
            showErrorMessage: (error: string) => { throw new Error(error); } },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        env: {}, Uri: { file: (file: string) => ({ fsPath: file }) }, McpStdioServerDefinition: Definition,
        StatusBarAlignment: { Left: 1 }, EventEmitter: class { private readonly ordinal = ++emitters; event = event; fire() { if (this.ordinal === 1) { events++; } } dispose() {} },
        lm: { registerMcpServerDefinitionProvider: (_id: string, value: unknown) => { provider = value; return disposable; } }
    };
    addTreeApi(stub, value => { tree = value; });
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
    const calls: string[] = [];
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (_spec: unknown, command: string, extra: string[] = []) => {
        calls.push(command);
        if (command === 'collections') {
            if (extra[0] === 'remove') { removals++; return {}; }
            return [{ collection: 'retired', status: 'indexed', disk_bytes: 1024, indexed_files: 1, generation: 1 }];
        }
        return { generation: 1 };
    };
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    try {
        for (const module of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(module)]; }
        const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/fixture/extension',
            extension: { packageJSON: { version: '0.1.0' } }, globalStorageUri: { fsPath: storage },
            globalState: { get: (key: string, fallback: unknown) => state.get(key) ?? fallback,
                update: async (key: string, value: unknown) => { state.set(key, value); } } };
        const api = await require('../extension').activate(context);
        assert.equal(cliAttempts, 0, 'User opt-out wins over workspace CLI setting');
        cliEnabled = true;
        await commands.get('airk.resume')!();
        assert.equal(cliAttempts, 1, 'Trusted startup attempts CLI setup once');
        await commands.get('airk.resume')!();
        assert.equal(cliAttempts, 1, 'PATH failure does not retry on every refresh or block indexing');
        await commands.get('airk.installCli')!();
        assert.equal(cliAttempts, 2, 'Repair retries a failed installation');
        assert.deepEqual(calls, ['schedule', 'schedule']);
        calls.length = 0;
        const first: Definition[] = await api.getServerDefinitions(); assert.equal(first.length, 1);
        assert.equal(first[0].args[first[0].args.indexOf('--data-dir') + 1], await fs.realpath(path.join(storage, 'indexes')));
        const priorEvents = events; await api.getServerDefinitions(); assert.equal(events, priorEvents, 'No recursive definition-change loop');
        await provider.resolveMcpServerDefinition(first[0]); assert.deepEqual(calls, ['status']);
        const beforeDisabled = calls.length;
        enabled = false;
        assert.deepEqual(await api.getServerDefinitions(), []);
        await assert.rejects(provider.resolveMcpServerDefinition(first[0]), /registration is off/);
        assert.equal(calls.length, beforeDisabled, 'Disabled registration cannot start a helper');
        enabled = true;
        assert.equal((await api.getServerDefinitions()).length, 1, 'Turning registration back on restores definitions');
        const rows = await tree.getChildren();
        assert.equal(rows.length, 2);
        const active = rows.find((row: any) => row.active), inactive = rows.find((row: any) => !row.active);
        assert.equal(active.contextValue, 'airkActiveCollection');
        assert.match(active.description, /pending.*size unknown.*indexed never/);
        assert.equal(inactive.contextValue, 'airkInactiveCollection');
        await assert.rejects((commands.get('airk.treeRemove') as any)(active), /Only an inactive collection/);
        await (commands.get('airk.treeRemove') as any)(inactive);
        assert.equal(removals, 1, 'Only the inactive indexed collection is removable after confirmation');
        const unrelated = new Definition('Other', '/fixture/airk', ['--collection', 'other'], {}, '0.1.0');
        await assert.rejects(provider.resolveMcpServerDefinition(unrelated), /configuration changed/);
        context.subscriptions.forEach(item => item.dispose());
    } finally {
        loader._load = original; native.prepareHelper = priorPrepare; native.runJson = priorRun;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-provider-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});

test('multi-root Index Now selects the active folder and coverage refresh never restarts MCP definitions', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-multiroot-'));
    const roots = [path.join(directory, 'A'), path.join(directory, 'B')];
    const canonicalRoots = [path.join(directory, 'CanonicalA'), path.join(directory, 'CanonicalB')];
    await Promise.all(canonicalRoots.map(root => fs.mkdir(root)));
    // Editors keep the opened workspace spelling; native collections use realpath.
    // A junction also models the short/long temporary-directory aliases on Windows CI.
    await Promise.all(roots.map((root, index) => fs.symlink(canonicalRoots[index], root, process.platform === 'win32' ? 'junction' : 'dir')));
    roots.push(path.join(roots[1], 'Nested')); await fs.mkdir(roots[2]);
    const commands = new Map<string, () => Promise<unknown>>(); const state = new Map<string, unknown>();
    const disposable = { dispose() {} }, event = () => disposable;
    let notifications = 0, choices = 0, emitters = 0, mode = 'manual', registration = true;
    let dashboard: any;
    let saved!: (document: { uri: { fsPath: string } }) => void, changed!: (uri: { fsPath: string }) => void;
    let availableMcpCommands = ['workbench.mcp.listServer'];
    const invokedMcpCommands: string[] = [];
    const terminals: { options: any; shown: number; disposed: boolean; show(): void; dispose(): void }[] = [];
    let terminalClosed: (terminal: unknown) => void = () => {};
    const documents: { scheduling: { name: string; pending: boolean }[] }[] = [];
    const status = { ...disposable, show() {}, tooltip: '' };
    const storage = path.join(roots[1], 'Host'), managedFile = path.join(storage, 'indexes', 'state.json');
    const calls: { spec: { root: string }; command: string; extra: string[] }[] = [];
    const window = { activeTextEditor: { document: { uri: { fsPath: path.join(roots[1], 'fixture.ts') } } },
        createTerminal: (options: any) => {
            const terminal = { options, shown: 0, disposed: false, show() { this.shown++; }, dispose() { this.disposed = true; } };
            terminals.push(terminal); return terminal;
        },
        onDidCloseTerminal: (handler: typeof terminalClosed) => { terminalClosed = handler; return disposable; },
        createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
        createStatusBarItem: () => status, onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
        showQuickPick: async (items: unknown[]) => { choices++; return items[0]; },
        withProgress: async (_options: unknown, run: () => Promise<unknown>) => run(),
        showTextDocument: async () => undefined, showErrorMessage: (error: string) => { throw new Error(error); } };
    const stub = {
        workspace: { isTrusted: true, workspaceFolders: roots.map((root, index) => ({ name: String(index), uri: { scheme: 'file', fsPath: root } })),
            textDocuments: [path.join(roots[1], 'dirty.ts'), managedFile, path.join(roots[1], '.env'), path.join(roots[1] + '-outside', 'other.ts')]
                .map(fsPath => ({ isDirty: true, uri: { fsPath } })),
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? mode : fallback, inspect: (key: string) => key === 'mcp.registrationEnabled' ? { globalValue: registration } : undefined,
                update: async (key: string, value: unknown) => { if (key === 'mcp.registrationEnabled') { registration = value as boolean; } else { assert.equal(key, 'index.mode'); mode = value as string; } } }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: (handler: typeof changed) => { changed = handler; return disposable; }, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: (handler: typeof saved) => { saved = handler; return disposable; }, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event,
            openTextDocument: async ({ content }: { content: string }) => { documents.push(JSON.parse(content)); return {}; } },
        window, commands: { getCommands: async () => availableMcpCommands, executeCommand: async (command: string) => { invokedMcpCommands.push(command); }, registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        ConfigurationTarget: { Global: 1 }, TerminalLocation: { Panel: 1 },
        env: {}, StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 1 },
        EventEmitter: class { private readonly ordinal = ++emitters; event = event; fire() { if (this.ordinal === 1) { notifications++; } } dispose() {} }, lm: {}
    };
    addTreeApi(stub, undefined, provider => { dashboard = provider; });
    addReportApi(stub);
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson, priorProbe = native.probeMcp;
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (spec: { root: string }, command: string, extra: string[] = []) => {
        if (command === 'collections' && extra[0] === 'vectors') { return { enabled: false, published: false, indexed_vectors: 0, pending_reindex: false, default_search_mode: 'lexical' }; }
        calls.push({ spec, command, extra });
        return command === 'index' ? { generation: 1, indexed: 1, reused: 0, removed: 0, skipped: [] }
            : { scan_complete: true, eligible: 1, current: 1, indexed: true };
    };
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/fixture/extension',
        extension: { packageJSON: { version: '0.1.0' } }, globalStorageUri: { fsPath: storage },
        globalState: { get: (key: string, fallback: unknown) => state.get(key) ?? fallback,
            update: async (key: string, value: unknown) => { state.set(key, value); } } };
    try {
        for (const module of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(module)]; } await require('../extension').activate(context);
        await commands.get('airk.manageVscodeMcp')!();
        assert.deepEqual(invokedMcpCommands, ['workbench.mcp.listServer']);
        availableMcpCommands = [];
        await assert.rejects(commands.get('airk.manageVscodeMcp')!(), /does not expose MCP: List Servers/);
        assert.equal(invokedMcpCommands.length, 1, 'Unavailable editor controls do not invoke a guessed fallback');
        const initialNotifications = notifications;
        await commands.get('airk.index')!();
        let indexes = calls.filter(call => call.command === 'index');
        assert.equal(indexes.length, 1); assert.equal(indexes[0].spec.root, await fs.realpath(roots[1])); assert.equal(choices, 0);
        assert.ok(indexes[0].extra.includes('--scheduled'));
        const beforeJson = documents.length;
        await commands.get('airk.status')!(); assert.equal(notifications, initialNotifications);
        assert.equal(documents.length, beforeJson + 1, 'Only Open as JSON creates a diagnostic document');
        window.activeTextEditor.document.uri.fsPath = path.join(roots[2], 'nested.ts');
        await commands.get('airk.index')!(); indexes = calls.filter(call => call.command === 'index');
        assert.equal(indexes[1].spec.root, await fs.realpath(roots[2]), 'The most specific opened workspace alias must win');
        window.activeTextEditor.document.uri.fsPath = path.join(await fs.realpath(roots[2]), 'canonical.ts');
        await commands.get('airk.index')!(); indexes = calls.filter(call => call.command === 'index');
        assert.equal(indexes[2].spec.root, await fs.realpath(roots[2]), 'Canonical editor paths must select the same nested collection');
        assert.equal(choices, 0);
        window.activeTextEditor.document.uri.fsPath = path.join(directory, 'outside.txt');
        await commands.get('airk.index')!(); indexes = calls.filter(call => call.command === 'index');
        assert.equal(choices, 1); assert.equal(indexes[3].spec.root, await fs.realpath(roots[0]));
        window.activeTextEditor.document.uri.fsPath = path.join(roots[1], 'dirty.ts');
        changed({ fsPath: managedFile }); saved({ uri: { fsPath: path.join(roots[1], '.env') } });
        changed({ fsPath: path.join(roots[1] + '-outside', 'other.ts') });
        await commands.get('airk.status')!();
        assert.ok(documents.at(-1)!.scheduling.every(job => !job.pending), 'Aliased private state, secrets and sibling paths cannot schedule work');
        assert.match(status.tooltip, /1 unsaved document; AIRK indexes saved files\./, 'Dirty counts use the verified alias and existing eligibility exclusions');
        saved({ uri: { fsPath: path.join(roots[1], 'saved.ts') } });
        await commands.get('airk.status')!();
        assert.deepEqual(documents.at(-1)!.scheduling.filter(job => job.pending).map(job => job.name), ['1'], 'An aliased save queues only its eligible collection');
        await commands.get('airk.index')!();
        changed({ fsPath: path.join(roots[2], 'changed.ts') });
        await commands.get('airk.status')!();
        assert.deepEqual(documents.at(-1)!.scheduling.filter(job => job.pending).map(job => job.name).sort(), ['1', '2'], 'A nested event reaches both containing collections');
        let receive!: (message: unknown) => Promise<void>;
        const webview = { html: '', options: {}, postMessage: async () => true,
            onDidReceiveMessage: (handler: typeof receive) => { receive = handler; return disposable; } };
        const beforePaint = calls.length;
        dashboard.resolveWebviewView({ visible: true, webview, onDidChangeVisibility: event, onDidDispose: event });
        assert.match(webview.html, /saved-file coverage/);
        assert.equal(calls.length, beforePaint, 'Rendering the dashboard starts no native reads or writes');
        const model = require('../model');
        const collection = model.collectionId(await fs.realpath(roots[0])), dataDir = await fs.realpath(path.join(storage, 'indexes'));
        const toolsAction = { type: 'collection', action: 'tools', collection, dataDir };
        const beforeTools = calls.length;
        await receive(toolsAction);
        assert.equal(calls.length, beforeTools, 'Opening tools does not index or probe');
        assert.equal(terminals.length, 1);
        assert.equal(terminals[0].options.shellPath, '/fixture/airk');
        assert.equal(terminals[0].options.cwd, await fs.realpath(roots[0]));
        assert.deepEqual(terminals[0].options.shellArgs, ['--root', await fs.realpath(roots[0]), '--data-dir', dataDir, '--collection', collection, 'tools', 'shell']);
        assert.equal(terminals[0].options.location, stub.TerminalLocation.Panel);
        assert.equal(terminals[0].options.isTransient, true);
        await receive(toolsAction); assert.equal(terminals.length, 1); assert.equal(terminals[0].shown, 2);
        terminalClosed(terminals[0]); await receive(toolsAction); assert.equal(terminals.length, 2);
        await assert.rejects(receive({ ...toolsAction, dataDir: '/old/storage' }), /storage changed/);
        await commands.get('airk.openTools')!();
        assert.equal(terminals.at(-1)!.options.cwd, await fs.realpath(roots[1]), 'Palette command selects the active editor collection');
        const beforeChoice = choices;
        await receive({ type: 'collection', action: 'index', collection, dataDir });
        assert.equal(calls.filter(call => call.command === 'index').at(-1)!.spec.root, await fs.realpath(roots[0]),
            'The dashboard button targets its card, not the different active editor folder');
        assert.equal(choices, beforeChoice, 'Indexing from a card does not open a collection picker');
        await assert.rejects(receive({ type: 'collection', action: 'index', collection, dataDir: '/old/storage' }), /storage changed/);
        const beforeInvalid = calls.length;
        await receive({ type: 'command', command: 'workbench.action.terminal.sendSequence' });
        assert.equal(calls.length, beforeInvalid);
        await receive({ type: 'mode', value: 'idle' }); assert.equal(mode, 'idle');
        await receive({ type: 'mode', value: 'manual' });
        const beforeToggle = calls.length;
        await receive({ type: 'registration', enabled: false });
        assert.equal(registration, false); assert.equal(dashboard.controller.sidebarState().registrationEnabled, false);
        await receive({ type: 'registration', enabled: true });
        assert.equal(registration, true); assert.equal(calls.length, beforeToggle, 'Registration does not index or probe');
        const check = () => dashboard.controller.sidebarState().collections.find((item: any) => item.collection === collection)?.mcp;
        let releaseProbe!: () => void, probeStarted!: () => void, probes = 0;
        const started = new Promise<void>(resolve => { probeStarted = resolve; });
        const held = new Promise<void>(resolve => { releaseProbe = resolve; });
        native.probeMcp = async (spec: { root: string }) => { probes++; assert.equal(spec.root, await fs.realpath(roots[0])); probeStarted(); await held; return { tools: ['status', 'search'], status: {} }; };
        const verifying = receive({ type: 'collection', action: 'verify', collection, dataDir });
        assert.equal(check().phase, 'preparing');
        await started; assert.equal(check().phase, 'checking');
        const duplicate = receive({ type: 'collection', action: 'verify', collection, dataDir });
        releaseProbe(); await Promise.all([verifying, duplicate]);
        assert.equal(probes, 1); assert.equal(check().phase, 'verified'); assert.equal(check().tools.length, 2);
        assert.ok(check().checkedAt); assert.ok(dashboard.controller.sidebarState().collections.filter((item: any) => item.collection !== collection).every((item: any) => !item.mcp));
        const beforeDetails = calls.length;
        await receive({ type: 'collection', action: 'mcpDetails', collection, dataDir });
        assert.equal(calls.length, beforeDetails, 'Viewing cached details does not launch another helper');
        native.probeMcp = async () => { throw new Error('Fixture handshake failed'); };
        await receive({ type: 'collection', action: 'verify', collection, dataDir });
        assert.equal(check().phase, 'failed'); assert.match(check().error, /handshake failed/);
        let signalLate!: () => void, finishLate!: () => void;
        const lateStarted = new Promise<void>(resolve => { signalLate = resolve; });
        const late = new Promise<void>(resolve => { finishLate = resolve; });
        native.probeMcp = async () => { signalLate(); await late; return { tools: ['status'], status: {} }; };
        const closing = receive({ type: 'collection', action: 'verify', collection, dataDir });
        await lateStarted;
        stub.workspace.workspaceFolders = stub.workspace.workspaceFolders.slice(1);
        await dashboard.controller.refresh(); finishLate(); await closing;
        assert.equal(terminals[1].disposed, true, 'Removing a collection closes its bound terminal');
        assert.equal(check(), undefined, 'A removed collection cannot receive a late verification result');
        stub.workspace.isTrusted = false;
        await assert.rejects(commands.get('airk.openTools')!(), /Trust this workspace/);
        await dashboard.controller.refresh();
        assert.equal(terminals.at(-1)!.disposed, true, 'Trust loss closes existing tools terminals');
        await assert.rejects(receive({ type: 'registration', enabled: false }), /Trust this workspace/);
        assert.equal(registration, true);

    } finally {
        context.subscriptions.forEach(item => item.dispose()); loader._load = original;
        native.prepareHelper = priorPrepare; native.runJson = priorRun; native.probeMcp = priorProbe;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-multiroot-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});

test('Manage Agent Connections selects Codex, creates only on request, and refuses stale or untrusted guides', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-connections-'));
    const root = path.join(directory, 'Project'); await fs.mkdir(root);
    const openedRoot = path.join(directory, 'OpenedProject');
    await fs.symlink(root, openedRoot, process.platform === 'win32' ? 'junction' : 'dir');
    const commands = new Map<string, () => Promise<unknown>>(), state = new Map<string, unknown>();
    const disposable = { dispose() {} }, event = () => disposable;
    let registration = false, saveWorks = true;
    let controller: any, receive!: (value: unknown) => Promise<void>, picked: string | undefined = 'codex', panels = 0, indexes = 0;
    const clipboard: string[] = [], invoked: string[] = [];
    const panel = { ...disposable, onDidDispose: event, webview: { html: '', onDidReceiveMessage: (handler: typeof receive) => { receive = handler; return disposable; } } };
    const stub = {
        workspace: { isTrusted: true, textDocuments: [] as any[],
            workspaceFolders: [{ name: 'Project', uri: { scheme: 'file', fsPath: openedRoot } }],
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback, inspect: (key: string) => key === 'mcp.registrationEnabled' ? { globalValue: registration } : undefined,
                update: async (key: string, value: boolean) => { assert.equal(key, 'mcp.registrationEnabled'); registration = value; } }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event,
            openTextDocument: async (uri: { fsPath: string }) => {
                const document = { uri, isDirty: false, version: 1, text: await fs.readFile(uri.fsPath, 'utf8'),
                    getText() { return this.text; }, positionAt(offset: number) { return offset; },
                    async save() { if (!saveWorks) { return false; } await fs.writeFile(uri.fsPath, this.text); this.isDirty = false; return true; } };
                stub.workspace.textDocuments.push(document); return document;
            },
            applyEdit: async (edit: { uri: { fsPath: string }; text: string }) => {
                const document = stub.workspace.textDocuments.find(item => item.uri.fsPath === edit.uri.fsPath);
                document.text = edit.text; document.version++; document.isDirty = true; return true;
            } },
        window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
            showQuickPick: async (items: { client: string }[]) => items.find(item => item.client === picked),
            createWebviewPanel: () => { panels++; return panel; },
            withProgress: async (_options: unknown, run: () => Promise<unknown>) => run(), showErrorMessage() {} },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; },
            getCommands: async () => ['workbench.mcp.listServer'], executeCommand: async (command: string) => { invoked.push(command); } },
        env: { remoteName: undefined as string | undefined, clipboard: { writeText: async (text: string) => { clipboard.push(text); } } },
        Uri: { file: (fsPath: string) => ({ fsPath }) }, Range: class { constructor(_start: unknown, _end: unknown) {} },
        WorkspaceEdit: class { uri?: { fsPath: string }; text = ''; replace(uri: { fsPath: string }, _range: unknown, text: string) { this.uri = uri; this.text = text; } },
        ConfigurationTarget: { Global: 1 }, ViewColumn: { Active: 1 }, StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 1 },
        EventEmitter: class { event = event; fire() {} dispose() {} }, lm: {} as { registerMcpServerDefinitionProvider?: () => typeof disposable }
    };
    addTreeApi(stub, undefined, provider => { controller = provider.controller; });
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson, priorProbe = native.probeMcp;
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (_spec: unknown, command: string, extra: string[] = []) => {
        if (command === 'collections' && extra[0] === 'vectors') { return { enabled: false, published: false, indexed_vectors: 0, pending_reindex: false, default_search_mode: 'lexical' }; }
        if (command === 'index') { indexes++; return { generation: 1, indexed: 1, reused: 0, removed: 0, skipped: [] }; }
        return { scan_complete: true, eligible: 1, current: 1, indexed: true };
    };
    native.probeMcp = async () => ({ tools: ['airk_status'], status: {} });
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/fixture/extension',
        extension: { packageJSON: { version: '1.1.2' } }, globalStorageUri: { fsPath: path.join(directory, 'Host') },
        globalState: { get: (key: string, fallback: unknown) => state.get(key) ?? fallback,
            update: async (key: string, value: unknown) => { state.set(key, value); } } };
    try {
        for (const module of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(module)]; } await require('../extension').activate(context);
        picked = undefined; await commands.get('airk.manageMcp')!(); assert.equal(panels, 0);
        picked = 'codex'; await commands.get('airk.manageMcp')!();
        assert.equal(panels, 1); assert.deepEqual(invoked, []);
        assert.equal(indexes, 0, 'Opening setup does not force indexing');
        const destination = path.join(root, '.codex', 'config.toml');
        await assert.rejects(fs.stat(destination), { code: 'ENOENT' });
        assert.match(panel.webview.html, /Connect Codex to AIRK/);
        await receive({ action: 'copy' }); assert.match(clipboard[0], /mcp_servers.airk-project-/);
        assert.match(clipboard[0], /--collection/);
        stub.workspace.textDocuments.push({ isDirty: true, uri: { fsPath: path.join(openedRoot, '.codex', 'config.toml') } });
        await receive({ action: 'createCodex' }); assert.match(panel.webview.html, /unsaved edits/);
        stub.workspace.textDocuments = [];
        await receive({ action: 'createCodex' });
        assert.equal(await fs.readFile(destination, 'utf8'), clipboard[0]);
        assert.match(panel.webview.html, /configuration created/);
        await receive({ action: 'createCodex' }); assert.match(panel.webview.html, /already exists and was preserved/);
        await receive({ action: 'verify' }); assert.match(panel.webview.html, /helper verified: 1 tools/);
        assert.match(panel.webview.html, /Agent connection still needs/);
        await receive({ action: 'copyPrompt' }); assert.match(clipboard[1], /airk_status/);
        native.probeMcp = async () => { throw new Error('Fixture unavailable'); };
        await receive({ action: 'verify' }); assert.match(panel.webview.html, /Fixture unavailable/);
        stub.env.remoteName = 'ssh-remote';
        await receive({ action: 'createCodex' }); assert.match(panel.webview.html, /Use manual configuration/);
        stub.env.remoteName = undefined;
        for (const [client, relative] of [['claude', '.mcp.json'], ['gemini', '.gemini/settings.json'], ['antigravity', '.agents/mcp_config.json']]) {
            picked = client; await commands.get('airk.manageMcp')!();
            const config = path.join(root, relative);
            stub.workspace.textDocuments.push({ isDirty: true, uri: { fsPath: path.join(openedRoot, relative) } });
            await receive({ action: 'installJson' }); assert.match(panel.webview.html, /unsaved edits/);
            await assert.rejects(fs.stat(config), { code: 'ENOENT' });
            stub.workspace.textDocuments = [];
            stub.env.remoteName = 'ssh-remote';
            await receive({ action: 'installJson' }); assert.match(panel.webview.html, /Use manual configuration/);
            stub.env.remoteName = undefined;
            await receive({ action: 'installJson' }); assert.match(panel.webview.html, /configuration installed/);
            const installed = await fs.readFile(config, 'utf8');
            assert.ok(Object.keys(JSON.parse(installed).mcpServers).some(key => key.startsWith('airk-project-')));
            await receive({ action: 'installJson' }); assert.equal(await fs.readFile(config, 'utf8'), installed);
            assert.match(panel.webview.html, /connection has not been verified/);
            const prior = '{"mcpServers":{"other":{"command":"keep"}},"approval":false}';
            await fs.writeFile(config, prior);
            saveWorks = false;
            await receive({ action: 'installJson' }); assert.match(panel.webview.html, /not saved/);
            assert.equal(await fs.readFile(config, 'utf8'), prior);
            assert.ok(stub.workspace.textDocuments.some(document => document.isDirty));
            stub.workspace.textDocuments = []; saveWorks = true;
            await receive({ action: 'installJson' }); assert.match(panel.webview.html, /configuration installed/);
            const merged = JSON.parse(await fs.readFile(config, 'utf8'));
            assert.equal(merged.mcpServers.other.command, 'keep'); assert.equal(merged.approval, false);
            stub.workspace.textDocuments = [];
        }
        picked = 'vscode'; await commands.get('airk.manageMcp')!();
        await receive({ action: 'enableCopilot' }); assert.match(panel.webview.html, /does not support/);
        assert.equal(registration, false); assert.deepEqual(invoked, []);
        stub.lm.registerMcpServerDefinitionProvider = () => disposable;
        await receive({ action: 'enableCopilot' });
        assert.equal(registration, true); assert.deepEqual(invoked, ['workbench.mcp.listServer']);
        assert.match(panel.webview.html, /registration enabled/);
        assert.match(panel.webview.html, /connection has not been verified/);
        stub.workspace.workspaceFolders = []; await controller.refresh();
        await receive({ action: 'copy' }); assert.match(panel.webview.html, /Workspace or storage changed/);
        assert.equal(clipboard.length, 2);
        stub.workspace.isTrusted = false;
        await receive({ action: 'copy' }); assert.match(panel.webview.html, /Trust this workspace/);
        await assert.rejects(commands.get('airk.manageMcp')!(), /Trust this workspace/);
    } finally {
        context.subscriptions.forEach(item => item.dispose()); loader._load = original;
        native.prepareHelper = priorPrepare; native.runJson = priorRun; native.probeMcp = priorProbe;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-connections-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});

test('coverage and indexing share native writer exclusion and command failures remain awaitable', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-writers-'));
    const root = path.join(directory, 'Project'); await fs.mkdir(root);
    const commands = new Map<string, () => Promise<unknown>>(), state = new Map<string, unknown>();
    const errors: string[] = [], documents: string[] = [];
    const disposable = { dispose() {} }, event = () => disposable;
    let signalCoverage!: () => void, releaseCoverage!: () => void, signalProgress!: () => void;
    const coverageStarted = new Promise<void>(resolve => { signalCoverage = resolve; });
    const coverageRelease = new Promise<void>(resolve => { releaseCoverage = resolve; });
    const progressStarted = new Promise<void>(resolve => { signalProgress = resolve; });
    let writingCoverage = false, holdCoverage = true, rejectIndex = false, indexes = 0;
    const stub = {
        workspace: { isTrusted: true, workspaceFolders: [{ name: 'Project', uri: { scheme: 'file', fsPath: root } }],
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback, inspect: () => undefined }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event,
            openTextDocument: async ({ content }: { content: string }) => { documents.push(content); return {}; } },
        window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
            withProgress: async (_options: unknown, run: () => Promise<unknown>) => { const pending = run(); signalProgress(); return pending; },
            showTextDocument: async () => undefined, showErrorMessage: (error: string) => { errors.push(error); } },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        env: {}, StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 1 },
        EventEmitter: class { event = event; fire() {} dispose() {} }, lm: {}
    };
    let controller: any;
    addTreeApi(stub, undefined, provider => { controller = provider.controller; });
    addReportApi(stub);
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
    const vectorModel = require('../vectorModel'), priorVectorPrepare = vectorModel.prepareVectorModel;
    let vectorsEnabled = false, preparations = 0;
    vectorModel.prepareVectorModel = async () => { assert.equal(vectorsEnabled, true); preparations++; };
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (_spec: unknown, command: string, extra: string[] = []) => {
        if (command === 'collections' && extra[0] === 'vectors') {
            if (extra[1] === 'set') { vectorsEnabled = extra[3] === 'true'; }
            return { enabled: vectorsEnabled, published: vectorsEnabled, indexed_vectors: vectorsEnabled ? 1 : 0, pending_reindex: false, default_search_mode: vectorsEnabled ? 'hybrid' : 'lexical' };
        }
        if (command === 'coverage') {
            if (holdCoverage) { writingCoverage = true; signalCoverage(); await coverageRelease; writingCoverage = false; }
            return { scan_complete: true, eligible: 1, current: 1, indexed: true };
        }
        if (command === 'index') {
            indexes++;
            assert.equal(writingCoverage, false, 'Indexing must wait for the coverage writer');
            if (rejectIndex) { throw new Error('index writer locked by another AIRK process'); }
            return { generation: indexes, indexed: 1, reused: 0, removed: 0, skipped: [] };
        }
        throw new Error(`Unexpected fixture command ${command}`);
    };
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/fixture/extension',
        extension: { packageJSON: { version: '0.1.0' } }, globalStorageUri: { fsPath: path.join(directory, 'Host') },
        globalState: { get: (key: string, fallback: unknown) => state.get(key) ?? fallback,
            update: async (key: string, value: unknown) => { state.set(key, value); } } };
    try {
        for (const module of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(module)]; } await require('../extension').activate(context);
        const coverage = commands.get('airk.status')!(); await coverageStarted;
        const indexing = commands.get('airk.index')!(); await progressStarted;
        await new Promise<void>(resolve => setTimeout(resolve, 0));
        assert.equal(indexes, 0, 'A queued index must not race the active native coverage writer');
        holdCoverage = false; releaseCoverage(); await Promise.all([coverage, indexing]);
        assert.equal(indexes, 1); assert.deepEqual(errors, []);
        rejectIndex = true;
        await assert.rejects(commands.get('airk.index')!(), /writer locked by another AIRK process/);
        assert.equal(errors.length, 1, 'Explicit command failures must still notify the user');
        await commands.get('airk.status')!();
        assert.equal(JSON.parse(documents.at(-1)!).scheduling[0].pending, true, 'External contention retains pending work');
        rejectIndex = false; await commands.get('airk.index')!();
        assert.equal(indexes, 3, 'A failed native write does not poison subsequent explicit retries');
        assert.equal(preparations, 0, 'Opted-out collections must not download a model');
        const collection = controller.sidebarState().collections[0];
        const toggle = { type: 'vectors', collection: collection.collection, dataDir: collection.dataDir, enabled: true };
        await controller.sidebarMessage(toggle);
        assert.equal(vectorsEnabled, true); assert.equal(preparations, 1); assert.equal(indexes, 4);
        await controller.sidebarMessage({ ...toggle, enabled: false });
        assert.equal(vectorsEnabled, false); assert.equal(preparations, 1); assert.equal(indexes, 5);
        await assert.rejects(controller.sidebarMessage({ ...toggle, dataDir: '/stale' }), /storage changed/);
        stub.workspace.isTrusted = false;
        await assert.rejects(controller.sidebarMessage(toggle), /trust/i);
        assert.equal(vectorsEnabled, false);

    } finally {
        releaseCoverage(); context.subscriptions.forEach(item => item.dispose()); loader._load = original;
        native.prepareHelper = priorPrepare; native.runJson = priorRun; vectorModel.prepareVectorModel = priorVectorPrepare;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-writers-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});
