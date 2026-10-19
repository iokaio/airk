// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { afterEach, beforeEach, mock, test } from 'node:test';
import Module from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { RetentionStatus, retentionSummary } from '../retention';
import * as cliInstall from '../cliInstall';

beforeEach(() => { mock.method(cliInstall, 'installCli', async () => []); });
afterEach(() => { mock.restoreAll(); });

test('retention status distinguishes unknown bytes, pending work and failed cleanup', () => {
    const status: RetentionStatus = { collection: 'original', policy: { policy: { version: 1, mode: 'manual', searchable_predecessors: 1 }, origin: 'existing_manual', digest: 'fixture' }, protected_bytes: null, pending_intents: 2, pending_bytes: null, last_maintenance: { error: 'original cleanup failure' } };
    assert.match(retentionSummary(status), /manual \(existing_manual\)/);
    assert.match(retentionSummary(status), /unknown protected bytes/);
    assert.match(retentionSummary(status), /2 pending maintenance intents/);
    assert.match(retentionSummary(status), /maintenance error/);
    status.legacy_protected = [{ revision: 'original-legacy-digest', bytes: null, reasons: ['predecessor'] }];
    status.pressure = { required_bytes: 200, available_bytes: 100 };
    assert.match(retentionSummary(status), /1 protected legacy revisions/);
    assert.match(retentionSummary(status), /200 needed, 100 available/);
    assert.equal(retentionSummary(), 'Retention: not inspected');
});

test('retention controls preview before explicit mutation and never rewrite client settings', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-retention-controls-'));
    const root = path.join(directory, 'Original Workspace'); await fs.mkdir(root);
    const policyFile = path.join(directory, 'original-policy.json');
    const inputs: (string | undefined)[] = [];
    const quotaPolicy = { root_bytes: 500 * 1073741824, default_collection_bytes: 100 * 1073741824, collections: { unrelated: 20 * 1073741824 } };
    const policyBytes = '{ "version": 1, "rules": [] }\n';
    await fs.writeFile(policyFile, policyBytes);
    const extractionInputs: string[] = [], snapshots: string[] = [];
    const commands = new Map<string, () => Promise<unknown>>(), calls: string[][] = [], documents: any[] = [];
    const disposable = { dispose() {} }, event = () => disposable;
    let confirm = false; const warnings: string[] = [], information: string[] = [];
    let pruneExtra: { unreferenced_objects?: number; receipt_cleanup_pending?: boolean } = {};
    const status: RetentionStatus = { collection: 'original', policy: { policy: { version: 1, mode: 'manual', searchable_predecessors: 1 }, origin: 'existing_manual', digest: 'fixture' }, protected_bytes: 123, pending_intents: 1, pending_bytes: 456, last_maintenance: null };
    const stub = {
        workspace: { isTrusted: true, workspaceFolders: [{ name: 'Original', uri: { scheme: 'file', fsPath: root } }],
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback, inspect: () => undefined, update: () => { throw new Error('must not rewrite settings'); } }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event, onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event,
            openTextDocument: async ({ content }: { content: string }) => { documents.push(JSON.parse(content)); return {}; } },
        window: { createOutputChannel: () => ({ ...disposable, appendLine() {}, show() {} }), createStatusBarItem: () => ({ ...disposable, show() {} }),
            onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event, showErrorMessage: (error: string) => { throw new Error(error); },
            showTextDocument: async () => {}, showInformationMessage: (message: string) => { information.push(message); }, showQuickPick: async (choices: any[]) => choices.find(choice => choice.mode === 'automatic') ?? choices[0],
            showOpenDialog: async () => [{ fsPath: policyFile }],
            showInputBox: async () => inputs.shift(),
            withProgress: async (_options: unknown, run: () => Promise<unknown>) => run(),
            showWarningMessage: async (message: string, _options: unknown, action: string) => { warnings.push(message); if (message.includes('extraction policy')) { await fs.writeFile(policyFile, '{"version":2}'); } return confirm ? action : undefined; } },
        commands: { registerCommand: (name: string, handler: () => Promise<unknown>) => { commands.set(name, handler); return disposable; } },
        ProgressLocation: { Notification: 1 }, StatusBarAlignment: { Left: 1 }, EventEmitter: class { event = event; fire() {} dispose() {} }, lm: {}
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    const native = require('../native'), priorPrepare = native.prepareHelper, priorRun = native.runJson;
    native.prepareHelper = async () => '/original/helper';
    native.runJson = async (_spec: unknown, command: string, extra: string[] = []) => {
        calls.push([command, ...extra]);
        if (extra[0] === 'quota') {
            if (extra[1] === 'policy') { return structuredClone(quotaPolicy); }
            if (extra[1] === 'status') { return { used_bytes: 0, reserved_bytes: 0, collections: {} }; }
            if (extra[1] === 'set') { return {}; }
        }
        if (extra[0] === 'extraction') {
            if (extra[1] !== 'show') { snapshots.push(extra[3]); extractionInputs.push(await fs.readFile(extra[3], 'utf8')); }
            return { policy: { version: 1, rules: [] }, digest: 'configured', applied_digest: 'published', pending_reindex: true, origin: extra[1] === 'validate' ? 'preview' : 'explicit' };
        }
        if (extra[0] === 'prune') { return { dry_run: !extra.includes('--apply'), candidates: [], pinned: [], reclaimable_bytes: 123, removed_bytes: 0, ...pruneExtra }; }
        if (extra[1] === 'show') { return structuredClone(status); }
        if (extra[1] === 'preview') { return { status, candidates: [], legacy_candidates: [{ revision: 'original-legacy-digest', bytes: 456, reasons: [] }], candidates_truncated: false, estimated_reclaimable_bytes: null }; }
        if (extra[1] === 'set') { status.policy.policy = JSON.parse(extra[3]); status.policy.origin = 'explicit'; return status.policy; }
        if (extra[1] === 'run') { return { receipt: { state: 'pending', pending: true }, receipt_persisted: true, status }; }
        throw new Error(`Unexpected native call ${command} ${extra}`);
    };
    (stub as any).TreeItem = class { constructor(public label: string) {} };
    (stub as any).TreeItemCollapsibleState = { None: 0 };
    (stub as any).ThemeIcon = class { constructor(public id: string) {} };
    (stub.window as any).createTreeView = () => ({ dispose() {} });
    (stub.window as any).registerWebviewViewProvider = () => ({ dispose() {} });
    (stub as any).ViewColumn = { Active: 1 };
    (stub.window as any).createWebviewPanel = () => ({
        webview: { html: '', onDidReceiveMessage: (handler: (message: unknown) => void) => {
            handler({ type: 'openJson' }); return disposable;
        } }, onDidDispose: () => disposable
    });
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    const context = { subscriptions: [] as { dispose(): void }[], extensionPath: '/original/extension', extension: { packageJSON: { version: '0.1.0' } },
        globalStorageUri: { fsPath: path.join(directory, 'Host') }, globalState: { get: (_key: string, fallback: unknown) => fallback, update: async () => {} } };
    try {
        delete require.cache[require.resolve('../extension')]; await require('../extension').activate(context);
        await commands.get('airk.retention')!();
        assert.deepEqual(calls, [['collections', 'retention', 'show']]);
        await commands.get('airk.retentionPolicy')!();
        assert.equal(calls.filter(call => call[2] === 'set').length, 0, 'Cancellation must not persist the proposed policy');
        confirm = true; await commands.get('airk.retentionPolicy')!();
        const set = calls.findIndex(call => call[2] === 'set');
        assert.equal(calls[set - 1][2], 'preview');
        assert.equal(calls[set - 1][4], calls[set][4], 'Persist exactly the previewed policy');
        assert.equal(status.policy.policy.mode, 'automatic');
        assert.equal(calls.filter(call => call[2] === 'run').length, 0, 'Setting policy does not run cleanup');
        await commands.get('airk.retentionRun')!();
        const applied = calls.findIndex(call => call[2] === 'run');
        assert.equal(calls[applied - 1][2], 'preview'); assert.equal(calls[applied][3], '--apply');
        assert.ok(documents.some(document => document.retention?.protected_bytes === 123));
        assert.ok(documents.some(document => document.receipt?.state === 'pending'));
        assert.ok(documents.some(document => document.preview?.legacy_candidates?.[0].revision === 'original-legacy-digest'));
        assert.ok(warnings.some(message => message.includes('1 legacy revision candidates')));
        const pruneApplies = () => calls.filter(call => call[1] === 'prune' && call[2] === '--apply').length;
        await commands.get('airk.prune')!();
        assert.equal(pruneApplies(), 0, 'Older reports without work retain the no-op behavior');
        assert.ok(information.some(message => message.includes('No unpinned')));
        pruneExtra = { unreferenced_objects: 2 }; confirm = false;
        await commands.get('airk.prune')!();
        assert.equal(pruneApplies(), 0, 'Orphan cleanup still requires confirmation');
        confirm = true; await commands.get('airk.prune')!();
        assert.equal(pruneApplies(), 1, 'Orphan-only work reaches apply');
        assert.ok(warnings.some(message => message.includes('2 unreferenced objects')));
        pruneExtra = { unreferenced_objects: 0, receipt_cleanup_pending: true };
        await commands.get('airk.prune')!();
        assert.equal(pruneApplies(), 2, 'Pending receipt cleanup reaches apply without generation candidates');
        assert.ok(warnings.some(message => message.includes('Receipt cleanup')));
        await commands.get('airk.extractionPolicy')!();
        confirm = false; await commands.get('airk.configureExtractionPolicy')!();
        assert.equal(calls.filter(call => call[1] === 'extraction' && call[2] === 'set').length, 0);
        await fs.writeFile(policyFile, policyBytes);
        confirm = true; await commands.get('airk.configureExtractionPolicy')!();
        assert.deepEqual(extractionInputs, [policyBytes, policyBytes, policyBytes], 'Validate and set use identical captured bytes despite source file edits');
        const extractionSet = calls.findIndex(call => call[1] === 'extraction' && call[2] === 'set');
        assert.equal(calls[extractionSet - 1][2], 'validate');
        assert.equal(calls[extractionSet - 1][4], calls[extractionSet][4]);
        for (const snapshot of snapshots) { await assert.rejects(fs.stat(snapshot), { code: 'ENOENT' }); }
        assert.ok(documents.some(document => document.proposed?.pending_reindex === true));
        assert.equal(calls.filter(call => call[0] === 'index').length, 0, 'Saving policy does not implicitly index');
        assert.equal(status.policy.policy.searchable_predecessors, 0, 'User can select active-only retention');
        inputs.push(undefined); await commands.get('airk.storageLimits')!();
        assert.equal(calls.filter(call => call[1] === 'quota' && call[2] === 'set').length, 0);
        confirm = false; inputs.push('150', '600'); await commands.get('airk.storageLimits')!();
        assert.equal(calls.filter(call => call[1] === 'quota' && call[2] === 'set').length, 0, 'Cancelling review does not save');
        confirm = true; inputs.push('150', '600'); await commands.get('airk.storageLimits')!();
        const quotaSet = calls.find(call => call[1] === 'quota' && call[2] === 'set')!;
        const proposed = JSON.parse(quotaSet[4]);
        assert.equal(proposed.root_bytes, 600 * 1073741824);
        assert.equal(proposed.collections.unrelated, quotaPolicy.collections.unrelated);
        assert.ok(Object.values(proposed.collections).includes(150 * 1073741824));
        assert.deepEqual(JSON.parse(quotaSet[6]), quotaPolicy, 'Native save must compare the reviewed shared policy');

    } finally {
        context.subscriptions.forEach(item => item.dispose()); loader._load = original;
        native.prepareHelper = priorPrepare; native.runJson = priorRun;
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-retention-controls-')); await fs.rm(resolved, { recursive: true, force: true });
    }
});
