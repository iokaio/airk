// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test, mock } from 'node:test';
import { createHash } from 'node:crypto';
import Module = require('node:module');
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import * as native from '../native';
import * as cliInstall from '../cliInstall';
import { collectionId, LaunchSpec } from '../model';
import type { DiagnosticReport } from '../diagnostics';

// Existing native CI uses AIRK_TEST_BINARY. The explicit diagnostic override is
// useful when qualifying a particular repaired helper independently.
const binary = process.env.AIRK_NATIVE_TEST_BINARY || process.env.AIRK_TEST_BINARY;
type Status = { files: number; generation: number; indexed_vectors: number; publication: { token: string };
    semantic_readiness: { state: string; completed_artifacts: number; selected_artifacts: number } };
type Panel = { webview: { html: string }; receive(value: unknown): Promise<void>; dispose(): void };

test('Diagnose & Repair uses the real helper to recover retained data, resume, rebuild and preserve collection scope',
    { skip: !binary, timeout: 240000 }, async () => {
    const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-diagnostics-native-'));
    const directory = await fs.realpath(temporary), source = path.join(directory, 'Project A'), other = path.join(directory, 'Project B');
    const storage = path.join(directory, 'Host'), dataDir = path.join(storage, 'indexes');
    const model = path.join(dataDir, 'models', 'local');
    const command = path.resolve(binary!);
    await fs.mkdir(source); await fs.mkdir(other); await fs.mkdir(model, { recursive: true });
    const sourceText = 'authenticate the login request\n', otherText = 'isolate the other collection\n';
    await fs.writeFile(path.join(source, 'login.txt'), sourceText);
    await fs.writeFile(path.join(source, 'guide.txt'), 'authenticate a fixture account\n');
    // Exceed the bounded diagnostic page so the real UI must verify another page.
    const files = 130;
    await Promise.all(Array.from({ length: files - 2 }, (_, index) => fs.writeFile(
        path.join(source, `z_extra_${String(index).padStart(3, '0')}.txt`), 'authenticate a fixture account\n')));
    await fs.writeFile(path.join(other, 'other.txt'), otherText);
    const vocabulary = JSON.stringify({ authenticate: [1, 0], login: [1, 0], isolate: [0, 1], other: [0, 1] });
    await fs.writeFile(path.join(model, 'vocabulary.json'), vocabulary);
    await fs.writeFile(path.join(model, 'model.json'), JSON.stringify({ schema: 1, name: 'original-diagnostics-fixture',
        license: 'Apache-2.0', dimensions: 2, sha256: createHash('sha256').update(vocabulary).digest('hex') }));
    const primary: LaunchSpec = { command, root: source, dataDir, collection: collectionId(source), name: 'Project A' };
    const secondary: LaunchSpec = { command, root: other, dataDir, collection: collectionId(other), name: 'Project B' };
    const version = (await native.runText(command, ['--version'])).trim().match(/^airk (\S+)$/)?.[1];
    assert.ok(version, 'Read the version from the actual executable');
    const commands = new Map<string, () => Promise<unknown>>(), remembered = new Map<string, unknown>();
    const panels: Panel[] = [], errors: string[] = [], copied: string[] = [], output: string[] = [], verificationProgress: string[] = [];
    let controller: any, outputShown = 0, rebuildConfirmations = 0;
    const contexts: { subscriptions: { dispose(): void }[] }[] = [];
    const disposable = { dispose() {} }, event = () => disposable;
    const folders = [{ name: primary.name, uri: { scheme: 'file', fsPath: source } }, { name: secondary.name, uri: { scheme: 'file', fsPath: other } }];
    const stub: any = {
        workspace: { isTrusted: true, workspaceFolders: [...folders], textDocuments: [],
            getConfiguration: () => ({ get: (key: string, fallback: unknown) => key === 'index.mode' ? 'manual' : fallback,
                inspect: (key: string) => key === 'cli.autoInstall' ? { globalValue: false } : undefined }),
            createFileSystemWatcher: () => ({ ...disposable, onDidChange: event, onDidCreate: event, onDidDelete: event }),
            onDidSaveTextDocument: event, onDidChangeTextDocument: event, onDidGrantWorkspaceTrust: event,
            onDidChangeWorkspaceFolders: event, onDidChangeConfiguration: event },
        window: { activeTextEditor: { document: { uri: { fsPath: path.join(source, 'login.txt') } } },
            createOutputChannel: () => ({ ...disposable, appendLine: (line: string) => output.push(line), show: () => { outputShown++; } }),
            createStatusBarItem: () => ({ ...disposable, show() {} }), createTreeView: () => disposable,
            registerWebviewViewProvider: (_name: string, view: any) => { controller = view.controller; return disposable; },
            onDidChangeTextEditorSelection: event, onDidChangeActiveTextEditor: event,
            showInformationMessage() {}, showErrorMessage: (error: string) => { errors.push(error); },
            showWarningMessage: async (_message: string, _options: unknown, choice: string) => { rebuildConfirmations++; return choice; },
            withProgress: async (_options: unknown, run: (progress: unknown, token: unknown) => Promise<unknown>) =>
                run({ report: (value: { message?: string }) => { if (value.message) { verificationProgress.push(value.message); } } }, { isCancellationRequested: false }),
            createWebviewPanel: () => {
                let receive: (value: unknown) => Promise<void> = async () => undefined, disposed = () => undefined;
                const webview = { html: '', onDidReceiveMessage: (handler: typeof receive) => { receive = handler; return disposable; } };
                const panel = { webview, receive: (value: unknown) => receive(value), dispose: () => disposed(),
                    onDidDispose: (handler: typeof disposed) => { disposed = handler; return disposable; } };
                panels.push(panel); return panel;
            } },
        commands: { registerCommand: (name: string, action: () => Promise<unknown>) => { commands.set(name, action); return disposable; } },
        env: { clipboard: { writeText: async (value: string) => { copied.push(value); } } },
        StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 1 }, ViewColumn: { Active: 1 },
        TreeItem: class { constructor(public label: string) {} }, TreeItemCollapsibleState: { None: 0 },
        ThemeIcon: class { constructor(public id: string) {} }, EventEmitter: class { event = event; fire() {} dispose() {} }, lm: {}
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    mock.method(native, 'prepareHelper', async () => command);
    mock.method(cliInstall, 'installCli', async () => []);
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    const activate = async () => {
        const context = { subscriptions: [] as { dispose(): void }[], extensionPath: path.join(directory, 'Extension'),
            extension: { packageJSON: { version } }, globalStorageUri: { fsPath: storage },
            globalState: { get: (key: string, fallback: unknown) => remembered.get(key) ?? fallback,
                update: async (key: string, value: unknown) => { remembered.set(key, value); } } };
        contexts.push(context);
        for (const name of ['../extension', '../agentSetup', '../dashboardView', '../collectionTree']) { delete require.cache[require.resolve(name)]; }
        await require('../extension').activate(context);
        return context;
    };
    const send = (panel: Panel, action: string) => panel.receive({ type: 'diagnostics', action });
    const report = async (panel: Panel): Promise<DiagnosticReport> => {
        await send(panel, 'copy'); assert.ok(copied.length, 'The UI exposes a copyable diagnostic report');
        return JSON.parse(copied.at(-1)!);
    };
    const status = (spec: LaunchSpec): Promise<Status> => controller
        ? controller.nativeWrites.run(spec, () => native.runJson<Status>(spec, 'status')) : native.runJson<Status>(spec, 'status');
    const retainedFiles = async (): Promise<[string, string][]> => {
        const files: [string, string][] = [];
        const visit = async (directory: string): Promise<void> => {
            for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
                const file = path.join(directory, entry.name);
                if (entry.isDirectory()) { await visit(file); }
                else {
                    assert.ok(entry.isFile(), 'Fixture derived state must contain only regular files');
                    files.push([path.relative(dataDir, file), createHash('sha256').update(await fs.readFile(file)).digest('hex')]);
                }
            }
        };
        await visit(dataDir); return files.sort(([left], [right]) => left.localeCompare(right));
    };
    const settle = async () => {
        await controller.scheduler.holdWhile(async (idle: Promise<void>) => { await idle; });
        const deadline = Date.now() + 60000;
        while (controller.scheduler.isBusy || controller.scheduler.states().some((job: { pending: boolean }) => job.pending) || [primary, secondary].some(spec => {
            const view = controller.coverage.get(spec.collection); return view?.scanning || view?.pending;
        })) {
            assert.ok(Date.now() < deadline, 'Fixture background indexing or coverage did not settle');
            await new Promise(resolve => setTimeout(resolve, 20));
        }
        await Promise.all([primary, secondary].map(spec => controller.nativeWrites.run(spec, async () => undefined)));
    };
    try {
        await native.runJson(primary, 'index'); await native.runJson(secondary, 'index');
        const untouched = await native.runJson<Status>(secondary, 'status');
        const beforeInspection = await retainedFiles();
        await native.runJson(primary, 'collections', ['diagnose']);
        assert.deepEqual(await retainedFiles(), beforeInspection,
            'The direct retained diagnostic CLI must not change files, journals or usage records');
        const collectionRoot = await fs.realpath(path.join(dataDir, 'collections-1.2', primary.collection));
        const digest = createHash('sha256').update(sourceText).digest('hex');
        const damaged = await fs.realpath(path.join(collectionRoot, 'objects', 'sources', digest));
        assert.ok(damaged.startsWith(collectionRoot + path.sep), 'Only the fixture collection may be damaged');
        assert.ok(collectionRoot.startsWith(directory + path.sep));
        await fs.unlink(damaged);
        await assert.rejects(native.runJson(primary, 'read', ['login.txt', '--start-line', '1', '--end-line', '1']), /retained object missing/);
        const firstContext = await activate();
        await commands.get('airk.diagnose')!();
        const panel = panels.at(-1)!;
        const before = await report(panel);
        assert.equal(before.binding.collection, primary.collection);
        assert.ok(['failed', 'blocked'].includes(before.assessment.state), 'Diagnosis must detect the missing retained source');
        assert.equal(before.probes.integrity.value?.failed_objects, 1);
        assert.ok((before.probes.integrity.value?.failures as { component: string; kind: string }[])
            .some(failure => failure.component === 'source' && failure.kind === 'missing'));
        await send(panel, 'showOutput'); assert.equal(outputShown, 1);
        await send(panel, 'retry'); await settle();
        assert.doesNotMatch(panel.webview.html, /role="alert"/);
        assert.equal(await fs.readFile(path.join(source, 'login.txt'), 'utf8'), sourceText);
        assert.equal(await fs.readFile(damaged, 'utf8'), sourceText, 'Retry reconstructs the missing derived source from exact saved bytes');
        const recovered = await status(primary);
        assert.equal(recovered.files, files); assert.equal(recovered.semantic_readiness.state, 'ready');
        assert.equal(recovered.semantic_readiness.completed_artifacts, files); assert.ok(recovered.indexed_vectors > 0);
        assert.equal((await status(secondary)).publication.token, untouched.publication.token);
        await commands.get('airk.pause')!(); await send(panel, 'refresh');
        assert.equal((await report(panel)).assessment.state, 'paused');
        await send(panel, 'resume'); await settle();
        for (const spec of [primary, secondary]) { assert.equal((await native.runJson<{ paused: boolean }>(spec, 'schedule', ['status'])).paused, false); }
        await send(panel, 'rebuild'); await settle();
        assert.equal(rebuildConfirmations, 1); assert.doesNotMatch(panel.webview.html, /role="alert"/);
        assert.ok((await status(primary)).generation > recovered.generation);
        assert.equal((await status(secondary)).publication.token, untouched.publication.token, 'Selected rebuild cannot publish to another collection');
        await send(panel, 'refresh');
        const partial = await report(panel);
        assert.equal(partial.assessment.state, 'unknown', 'A bounded first page cannot claim complete integrity');
        assert.equal(partial.probes.integrity.value?.complete, false);
        await send(panel, 'verify');
        const verified = await report(panel);
        assert.equal(verified.assessment.state, 'ready', JSON.stringify({ assessment: verified.assessment,
            probes: verified.probes, editor: verified.editor }));
        assert.equal(verified.probes.integrity.value?.checked_files, files);
        assert.equal(verified.probes.integrity.value?.verified_sources, files);
        assert.equal(verified.probes.integrity.value?.verified_extractions, files);
        assert.equal(verified.probes.integrity.value?.complete, true);
        assert.ok(verificationProgress.length > 0, 'The native page verifier reports progress through the UI');
        const beforeClose = (await status(primary)).generation;
        stub.workspace.workspaceFolders = [folders[1]];
        await send(panel, 'rebuild');
        assert.match(panel.webview.html, /Reopen Diagnose &amp; Repair/);
        assert.equal(rebuildConfirmations, 1, 'A stale panel cannot reach the repair confirmation');
        assert.equal((await status(primary)).generation, beforeClose);
        panel.dispose(); await Promise.all(firstContext.subscriptions.map(item => item.dispose()));
        stub.workspace.workspaceFolders = [...folders];
        await activate(); await settle();
        await commands.get('airk.diagnose')!();
        await send(panels.at(-1)!, 'verify');
        const reopened = await report(panels.at(-1)!);
        assert.equal(reopened.assessment.state, 'ready');
        assert.ok(reopened.recentEvents.some(item => item.kind === 'repair' && item.action === 'retry'), 'Bounded repair evidence survives extension reopen');
        const read = await native.runJson<{ hits: { text: string }[] }>(primary, 'read', ['login.txt', '--start-line', '1', '--end-line', '1']);
        assert.ok(read.hits.some(hit => hit.text.includes('authenticate the login request')));
        const found = await native.runJson<{ hits: unknown[] }>(primary, 'search', ['login', '--mode', 'vector']);
        assert.ok(found.hits.length > 0, 'Semantic retrieval remains usable after reopening');
        assert.equal(await fs.readFile(path.join(other, 'other.txt'), 'utf8'), otherText);
        assert.deepEqual(errors, []);
        const historyDirectory = path.join(storage, 'diagnostics');
        const historyFiles = (await fs.readdir(historyDirectory)).filter(name => name.endsWith('.json'));
        assert.ok(historyFiles.length > 0);
        await Promise.all(historyFiles.map(name => fs.writeFile(path.join(historyDirectory, name), 'not valid diagnostic history')));
        await fs.unlink(damaged);
        await send(panels.at(-1)!, 'refresh');
        const unavailableHistory = await report(panels.at(-1)!);
        assert.ok(unavailableHistory.historyError, 'Unreadable history is exposed separately from live probes');
        assert.equal(unavailableHistory.assessment.state, 'failed');
        assert.match(panels.at(-1)!.webview.html, /Previous attempt history is unavailable/);
        await send(panels.at(-1)!, 'retry'); await settle();
        assert.equal(await fs.readFile(damaged, 'utf8'), sourceText, 'Unavailable history cannot prevent safe derived-input repair');
        await send(panels.at(-1)!, 'refresh'); await send(panels.at(-1)!, 'verify');
        const repairedWithoutHistory = await report(panels.at(-1)!);
        assert.equal(repairedWithoutHistory.assessment.state, 'ready');
        assert.ok(repairedWithoutHistory.historyError);
        assert.deepEqual(errors, []);
        // Corruption is different from missing derived bytes: it must stay a
        // visible failure and must not receive a healthy or safe-retry claim.
        await fs.writeFile(damaged, 'x'.repeat(Buffer.byteLength(sourceText)));
        await send(panels.at(-1)!, 'refresh'); await send(panels.at(-1)!, 'verify');
        const corrupted = await report(panels.at(-1)!);
        assert.equal(corrupted.assessment.state, 'failed');
        assert.equal(corrupted.assessment.recommendedAction, 'review');
        assert.ok((corrupted.probes.integrity.value?.failures as { component: string; kind: string }[])
            .some(failure => failure.component === 'source' && failure.kind === 'invalid'));
        assert.equal(await fs.readFile(path.join(source, 'login.txt'), 'utf8'), sourceText);
    } finally {
        panels.forEach(panel => panel.dispose());
        await Promise.all(contexts.flatMap(context => context.subscriptions.map(item => item.dispose())));
        if (controller) { await Promise.all([primary, secondary].map(spec => controller.nativeWrites.run(spec, async () => undefined))); }
        loader._load = original; mock.restoreAll();
        assert.equal(path.dirname(directory), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(directory).startsWith('airk-diagnostics-native-'));
        await fs.rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    }
});
