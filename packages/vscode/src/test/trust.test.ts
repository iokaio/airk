// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';

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
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event,
            showErrorMessage: (error: string) => { errors.push(error); } },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        StatusBarAlignment: { Left: 1 },
        EventEmitter: class { event = event; fire() {} dispose() {} },
        lm: { registerMcpServerDefinitionProvider: (_id: string, value: unknown) => { provider = value; return disposable; } }
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any };
    const original = loader._load;
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    try {
        const extension = require('../extension');
        const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/nonexistent-helper-fixture' };
        const exported = await extension.activate(context);
        assert.equal(commands.size, 13); assert.deepEqual(await exported.getServerDefinitions(), []);
        assert.deepEqual(await provider.provideMcpServerDefinitions(), []);
        await commands.get('airk.index')!();
        assert.equal(errors.length, 1); assert.match(errors[0], /Trust this workspace/);
        await assert.rejects(provider.resolveMcpServerDefinition({}), /Trust this workspace/);
        context.subscriptions.forEach(item => item.dispose());
    } finally { loader._load = original; }
});

test('trusted MCP definitions ignore workspace storage overrides and reconnect without immediate reindexing', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-vscode-provider-'));
    const root = path.join(directory, 'Project'), storage = path.join(directory, 'Host'); await fs.mkdir(root);
    const commands = new Map<string, () => Promise<unknown>>(); const state = new Map<string, unknown>();
    const disposable = { dispose() {} }, event = () => disposable; let provider: any, events = 0;
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
            createStatusBarItem: () => ({ ...disposable, show() {} }), onDidChangeTextEditorSelection: event,
            showErrorMessage: (error: string) => { throw new Error(error); } },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        env: {}, Uri: { file: (file: string) => ({ fsPath: file }) }, McpStdioServerDefinition: Definition,
        StatusBarAlignment: { Left: 1 }, EventEmitter: class { event = event; fire() { events++; } dispose() {} },
        lm: { registerMcpServerDefinitionProvider: (_id: string, value: unknown) => { provider = value; return disposable; } }
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
    const calls: string[] = [];
    native.prepareHelper = async () => '/fixture/airk';
    native.runJson = async (_spec: unknown, command: string) => { calls.push(command); return { generation: 1 }; };
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
        const unrelated = new Definition('Other', '/fixture/airk', ['--collection', 'other'], {}, '0.1.0');
        await assert.rejects(provider.resolveMcpServerDefinition(unrelated), /configuration changed/);
        context.subscriptions.forEach(item => item.dispose());
    } finally {
        loader._load = original; native.prepareHelper = priorPrepare; native.runJson = priorRun;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-vscode-provider-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});
