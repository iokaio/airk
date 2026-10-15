// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';

function addTreeApi(stub: any, capture?: (provider: any) => void): void {
    stub.TreeItem = class { constructor(public label: string) {} };
    stub.TreeItemCollapsibleState = { None: 0 };
    stub.ThemeIcon = class { constructor(public id: string) {} };
    stub.window.createTreeView = (_id: string, options: { treeDataProvider: any }) => {
        capture?.(options.treeDataProvider);
        return { dispose() {} };
    };
}

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
        for (const name of ['airk.retention', 'airk.retentionPolicy', 'airk.retentionRun', 'airk.extractionPolicy', 'airk.configureExtractionPolicy', 'airk.history', 'airk.historyPolicy', 'airk.historyRun', 'airk.historyAvailable', 'airk.checkpoints', 'airk.createCheckpoint', 'airk.removeCheckpoint', 'airk.purgeHistory']) {
            await assert.rejects(commands.get(name)!(), /Trust this workspace/);
        }
        await assert.rejects(provider.resolveMcpServerDefinition({}), /Trust this workspace/);
        context.subscriptions.forEach(item => item.dispose());
    } finally { loader._load = original; }
});

test('trusted MCP definitions ignore workspace storage overrides and reconnect without immediate reindexing', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-provider-'));
    const root = path.join(directory, 'Project'), storage = path.join(directory, 'Host'); await fs.mkdir(root);
    const commands = new Map<string, () => Promise<unknown>>(); const state = new Map<string, unknown>();
    const disposable = { dispose() {} }, event = () => disposable; let provider: any, tree: any, events = 0, emitters = 0, removals = 0;
    class Definition {
        cwd?: { fsPath: string };
        constructor(public label: string, public command: string, public args: string[], public env: object, public version: string) {}
    }
    const stub = {
        workspace: { isTrusted: true, workspaceFolders: [{ name: 'Project', uri: { scheme: 'file', fsPath: root } }],
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback,
                inspect: () => ({ workspaceValue: path.join(root, 'untrusted-destination') }) }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event },
        window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
            showWarningMessage: async () => 'Remove index',
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
        delete require.cache[require.resolve('../extension')];
        const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/fixture/extension',
            extension: { packageJSON: { version: '0.1.0' } }, globalStorageUri: { fsPath: storage },
            globalState: { get: (key: string, fallback: unknown) => state.get(key) ?? fallback,
                update: async (key: string, value: unknown) => { state.set(key, value); } } };
        const api = await require('../extension').activate(context);
        const first: Definition[] = await api.getServerDefinitions(); assert.equal(first.length, 1);
        assert.equal(first[0].args[first[0].args.indexOf('--data-dir') + 1], await fs.realpath(path.join(storage, 'indexes')));
        const priorEvents = events; await api.getServerDefinitions(); assert.equal(events, priorEvents, 'No recursive definition-change loop');
        await provider.resolveMcpServerDefinition(first[0]); assert.deepEqual(calls, ['status']);
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
    let notifications = 0, choices = 0, emitters = 0;
    let saved!: (document: { uri: { fsPath: string } }) => void, changed!: (uri: { fsPath: string }) => void;
    const documents: { scheduling: { name: string; pending: boolean }[] }[] = [];
    const status = { ...disposable, show() {}, tooltip: '' };
    const storage = path.join(roots[1], 'Host'), managedFile = path.join(storage, 'indexes', 'state.json');
    const calls: { spec: { root: string }; command: string; extra: string[] }[] = [];
    const window = { activeTextEditor: { document: { uri: { fsPath: path.join(roots[1], 'fixture.ts') } } },
        createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }),
        createStatusBarItem: () => status, onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
        showQuickPick: async (items: unknown[]) => { choices++; return items[0]; },
        withProgress: async (_options: unknown, run: () => Promise<unknown>) => run(),
        showTextDocument: async () => undefined, showErrorMessage: (error: string) => { throw new Error(error); } };
    const stub = {
        workspace: { isTrusted: true, workspaceFolders: roots.map((root, index) => ({ name: String(index), uri: { scheme: 'file', fsPath: root } })),
            textDocuments: [path.join(roots[1], 'dirty.ts'), managedFile, path.join(roots[1], '.env'), path.join(roots[1] + '-outside', 'other.ts')]
                .map(fsPath => ({ isDirty: true, uri: { fsPath } })),
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback, inspect: () => undefined }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: (handler: typeof changed) => { changed = handler; return disposable; }, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: (handler: typeof saved) => { saved = handler; return disposable; }, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event,
            openTextDocument: async ({ content }: { content: string }) => { documents.push(JSON.parse(content)); return {}; } },
        window, commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        env: {}, StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 1 },
        EventEmitter: class { private readonly ordinal = ++emitters; event = event; fire() { if (this.ordinal === 1) { notifications++; } } dispose() {} }, lm: {}
    };
    addTreeApi(stub);
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (spec: { root: string }, command: string, extra: string[] = []) => {
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
        delete require.cache[require.resolve('../extension')]; await require('../extension').activate(context);
        const initialNotifications = notifications;
        await commands.get('airk.index')!();
        let indexes = calls.filter(call => call.command === 'index');
        assert.equal(indexes.length, 1); assert.equal(indexes[0].spec.root, await fs.realpath(roots[1])); assert.equal(choices, 0);
        assert.ok(indexes[0].extra.includes('--scheduled'));
        await commands.get('airk.status')!(); assert.equal(notifications, initialNotifications);
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
    } finally {
        context.subscriptions.forEach(item => item.dispose()); loader._load = original;
        native.prepareHelper = priorPrepare; native.runJson = priorRun;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-multiroot-')); await fs.rm(resolved, { recursive: true, force: true });
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
    addTreeApi(stub);
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (_spec: unknown, command: string) => {
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
        delete require.cache[require.resolve('../extension')]; await require('../extension').activate(context);
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
    } finally {
        releaseCoverage(); context.subscriptions.forEach(item => item.dispose()); loader._load = original;
        native.prepareHelper = priorPrepare; native.runJson = priorRun;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-writers-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});
