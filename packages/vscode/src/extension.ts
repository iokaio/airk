// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { IndexReport, LaunchSpec, Usage, collectionId, eligibleEvent, launchArgs } from './model';
import { setupAgent } from './agentSetup';
import { DashboardView } from './dashboardView';
import { ConsoleRuntime, ServiceConsole, serviceRuntimeOptions } from './serviceConsole';
import { RuntimeHost } from './serviceBootstrap';
import { collectDiagnostics, DiagnosticHistory, verifyDiagnosticIntegrity } from './diagnostics';
import { openDiagnosticsPanel } from './diagnosticsView';
import { CollectionItem, CollectionTree } from './collectionTree';
import { IndexProgress, NativeProcessError, NativeWriteQueue, joinScheduledIndex, ScheduledIndexStillQueued, parseIndexProgress, prepareHelper, probeMcp, runJson } from './native';
import { IndexProgressTracker } from './indexProgress';
import { installCli } from './cliInstall';
import { CapacityError, Options, PausedError, Scheduler, SemanticResult } from './scheduler';
import { Coverage, CoverageQueue, coverageDetails, coverageLabel } from './coverage';
import { withPolicySnapshot } from './extractionPolicy';
import { VectorStatus, prepareVectorModel } from './vectorModel';
import { aboveNormal, authorizeLimitIncrease, indexWithLimitApproval, largerLimitsExplanation, LimitsDescription, ScanLimits } from './ingestionLimits';
import { renderReport, ReportKind } from './report';
import { openUsagePanel } from './usagePanel';
import { ControlGroup, DetailStatus, IndexDetail, McpVerification, SidebarCollection, SidebarState, sidebarAction, sidebarControlGroups, detailLabel, semanticCompletion } from './sidebar';

interface KnownCollection { name: string; root: string; lastIndexed?: string }

function currentReadBusy(error: unknown): boolean {
    return error instanceof NativeProcessError && error.retryableGateContention;
}

class Controller implements vscode.Disposable {
    readonly output = vscode.window.createOutputChannel('AIRK');
    readonly status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 40);
    readonly definitions = new vscode.EventEmitter<void>();
    readonly collectionTree = new CollectionTree(this);
    readonly sidebar = new DashboardView(this);
    readonly scheduler: Scheduler;
    private historyStore?: DiagnosticHistory;
    private disposal?: Promise<void>;
    private get diagnosticHistory(): DiagnosticHistory {
        this.historyStore ??= new DiagnosticHistory(path.join(this.context.globalStorageUri.fsPath, 'diagnostics'));
        if (this.disposed) { void this.historyStore.close(); }
        return this.historyStore;
    }
    private readonly coverage: CoverageQueue;
    private readonly nativeWrites = new NativeWriteQueue();
    private readonly rescan: NodeJS.Timeout;
    private specs = new Map<string, LaunchSpec>();
    private rootBindings: { opened: string; canonical: string }[] = [];
    private helper?: Promise<string>;
    private cliAttempted = false;
    private cliStatus?: SidebarState['cliStatus'];
    private refreshRevision = 0;
    private refreshing = false;
    private disposed = false;
    private storageChoice?: Promise<void>;
    private indexErrors = new Map<string, string>();
    private detailViews = new Map<string, DetailStatus>();
    private vectorViews = new Map<string, VectorStatus>();
    private vectorProgress = new Map<string, string>();
    private queuedIndex = new Map<string, number>();
    private indexProgress = new Map<string, { event: IndexProgress; started: number; updated: number }>();
    private semanticStarted = new Map<string, number>();
    private semanticProgress = new Map<string, IndexProgress>();
    private changedPaths = new Map<string, Set<string> | null>();
    private semanticSignals = new Map<string, Promise<void>>();
    private semanticSignaled = new Set<string>();
    private reconciledCoverage = new Map<string, number>();
    private readonly progressRefresh: NodeJS.Timeout;
    private mcpChecks = new Map<string, McpVerification & { result?: { tools: string[]; status: unknown } }>();
    private mcpPending = new Map<string, Promise<void>>();
    private toolsTerminals = new Map<string, { spec: LaunchSpec; terminal: vscode.Terminal }>();
    private toolsClosed?: vscode.Disposable;
    private treeUsage?: { dataDir: string; updated: number; entries: Usage[] };
    private usageSummaries = new Map<string, SidebarCollection['usage']>();

    constructor(private readonly context: vscode.ExtensionContext) {
        this.status.command = 'airk.menu'; this.status.show();
        this.scheduler = new Scheduler(this.options(), (spec, full, automatic) => this.index(spec, full, automatic),
            () => this.updateStatus(), (spec, error) => {
                this.indexErrors.set(JSON.stringify([spec.dataDir, spec.collection]), String(error));
                this.output.appendLine(`[${new Date().toISOString()}] ${spec.name}: ${String(error)}`);
                void this.diagnosticHistory.append(spec, { kind: 'index_failed', helperVersion: context.extension.packageJSON.version, error })
                    .catch(historyError => this.output.appendLine(`AIRK diagnostic history unavailable: ${String(historyError)}`));
            }, undefined, spec => this.enrich(spec));
        this.coverage = new CoverageQueue(spec => this.writeNative<Coverage>(spec, 'coverage', ['--refresh']), () => this.coverageChanged());
        this.configure();
        this.rescan = setInterval(() => {
            if (vscode.workspace.isTrusted) { for (const spec of this.specs.values()) { this.coverage.request(spec); } }
        }, 300000);
        this.progressRefresh = setInterval(() => {
            if (this.indexProgress.size || this.semanticStarted.size) { this.updateStatus(); }
        }, 15000);
    }
    private coverageChanged(): void {
        if (this.disposed || !vscode.workspace.isTrusted) { return; }
        for (const spec of this.specs.values()) {
            const view = this.coverage.get(spec.collection), report = view?.report;
            if (!report || view?.pending || view.scanning || !report.scan_complete) { continue; }
            const key = this.mcpKey(spec);
            if (report.stale > 0 || report.missing > 0 || report.pending_removals > 0 || report.source_configuration_pending) {
                // Inventory epochs change on every rescan; the published revision
                // stays stable after failure. Try reconciliation once per head.
                if (this.reconciledCoverage.get(key) === report.generation) { continue; }
                this.reconciledCoverage.set(key, report.generation);
                this.changedPaths.set(key, null);
                void this.scheduler.request(spec);
            } else { this.reconciledCoverage.delete(key); }
        }
        this.updateStatus();
    }
    private trusted(): void {
        if (!vscode.workspace.isTrusted) { throw new Error('Trust this workspace before starting AIRK or indexing files.'); }
    }
    private writeNative<T>(spec: LaunchSpec, command: string, extra: string[] = [], onStderrLine?: (line: string) => boolean,
        onStart?: () => void): Promise<T> {
        return this.nativeWrites.run(spec, async () => {
            this.trusted();
            if (this.disposed) { throw new Error('AIRK extension closed.'); }
            onStart?.();
            return runJson<T>(spec, command, extra, onStderrLine);
        });
    }
    private options(): Options {
        const config = vscode.workspace.getConfiguration('airk');
        const mode = config.get<Options['mode']>('index.mode', 'debounced');
        const number = (key: string, fallback: number, min: number, max: number) => {
            const value = config.get<unknown>(key, fallback);
            return typeof value === 'number' && Number.isFinite(value) ? Math.max(min, Math.min(max, value)) : fallback;
        };
        return { mode: ['manual', 'onSave', 'debounced', 'idle'].includes(mode) ? mode : 'manual',
            quietMs: number('index.quietMs', 2000, 250, 60000), idleMs: number('index.idleMs', 10000, 1000, 300000),
            maxDelayMs: number('index.maxDelayMs', 30000, 1000, 300000), minIntervalMs: number('index.minIntervalMs', 1000, 0, 300000) };
    }
    private machineValue<T>(key: string, fallback: T): T {
        // Workspace-controlled configuration never selects a storage destination.
        return vscode.workspace.getConfiguration('airk').inspect<T>(key)?.globalValue ?? fallback;
    }
    async dataDir(): Promise<string> {
        const configured = this.machineValue('storage.path', '');
        if (configured && !path.isAbsolute(configured)) { throw new Error('AIRK storage.path must be an absolute path on the extension host.'); }
        const directory = configured || path.join(this.context.globalStorageUri.fsPath, 'indexes');
        await fs.mkdir(directory, { recursive: true });
        return fs.realpath(directory);
    }
    private command(): Promise<string> {
        this.trusted();
        this.helper ??= prepareHelper(this.context.extensionPath, this.context.globalStorageUri.fsPath, this.context.extension.packageJSON.version,
            message => this.output.appendLine(`[${new Date().toISOString()}] ${message}`))
            .then(command => { this.output.appendLine(`AIRK helper verified: ${this.context.extension.packageJSON.version}; ${command}`); return command; })
            .catch(error => { this.helper = undefined; throw error; });
        return this.helper;
    }
    private async setupCli(command: string, explicit: boolean): Promise<void> {
        try {
            const errors = await installCli(command, this.context);
            if (errors.length) { throw new Error(errors.join('\n')); }
            this.cliStatus = 'ready';
            const message = 'AIRK CLI installed. Open a new VS Code terminal. Restart external terminal applications to pick up PATH changes.';
            this.output.appendLine(message);
            if (explicit) { void vscode.window.showInformationMessage(message); }
        } catch (error) {
            this.output.appendLine(`AIRK CLI setup: ${String(error)}`);
            this.cliStatus = 'failed';
            if (explicit) { throw error; }
            void vscode.window.showWarningMessage('AIRK CLI setup could not finish. Indexing remains available. See AIRK output, then run AIRK: Install or Repair CLI.');
        } finally { this.sidebar.refresh(); }
    }
    async installCommandLine(): Promise<void> {
        this.trusted();
        await this.setupCli(await this.command(), true);
    }
    async refresh(): Promise<void> {
        const revision = ++this.refreshRevision;
        if (!vscode.workspace.isTrusted) {
            this.closeToolsTerminals();
            this.mcpChecks.clear();
            for (const spec of this.specs.values()) { this.scheduler.forget(spec.collection); this.coverage.forget(spec.collection); }
            this.specs.clear(); this.rootBindings = []; this.definitions.fire(); this.updateStatus(); return;
        }
        this.refreshing = true;
        try {
            const command = await this.command(), dataDir = await this.dataDir();
            if (!this.cliAttempted && this.machineValue('cli.autoInstall', true)) {
                this.cliAttempted = true;
                await this.setupCli(command, false);
            }
            if (!this.cliAttempted && !this.cliStatus && !this.machineValue('cli.autoInstall', true)) { this.cliStatus = 'disabled'; }
            const next = new Map<string, LaunchSpec>();
            const bindings: typeof this.rootBindings = [];
            for (const folder of vscode.workspace.workspaceFolders ?? []) {
                if (folder.uri.scheme !== 'file' && !(vscode.env.remoteName && folder.uri.scheme === 'vscode-remote')) { continue; }
                const root = await fs.realpath(folder.uri.fsPath);
                const collection = collectionId(root);
                next.set(collection, { command, root, dataDir, collection, name: folder.name });
                bindings.push({ opened: folder.uri.fsPath, canonical: root });
            }
            if (revision !== this.refreshRevision || this.disposed) { return; }
            const storageChanged = [...this.specs.values()].some(spec => next.has(spec.collection) && next.get(spec.collection)?.dataDir !== spec.dataDir);
            if (storageChanged && this.scheduler.isBusy) { throw new Error('Wait for the running index job to finish, then refresh AIRK controls.'); }
            if (storageChanged) { this.treeUsage = undefined; }
            for (const old of this.specs.values()) {
                if (!next.has(old.collection) || next.get(old.collection)?.dataDir !== old.dataDir) { this.scheduler.forget(old.collection); this.coverage.forget(old.collection); }
            }
            const prior = this.specs;
            this.specs = next;
            this.closeToolsTerminals(next);
            const mcpBindings = new Set([...next.values()].map(spec => this.mcpKey(spec)));
            for (const map of [this.queuedIndex, this.reconciledCoverage]) {
                for (const key of map.keys()) { if (!mcpBindings.has(key)) { map.delete(key); } }
            }
            for (const key of this.changedPaths.keys()) { if (!mcpBindings.has(key)) { this.changedPaths.delete(key); } }
            for (const key of this.usageSummaries.keys()) { if (!mcpBindings.has(key)) { this.usageSummaries.delete(key); } }
            for (const key of this.mcpChecks.keys()) { if (!mcpBindings.has(key)) { this.mcpChecks.delete(key); } }
            this.rootBindings = bindings.sort((a, b) => b.opened.length - a.opened.length);
            const known = this.context.globalState.get<Record<string, KnownCollection>>('collections-1.2', {});
            for (const spec of next.values()) {
                known[spec.collection] = { ...known[spec.collection], name: spec.name, root: spec.root };
                if (!prior.has(spec.collection) || prior.get(spec.collection)?.command !== spec.command) {
                    void this.diagnosticHistory.append(spec, { kind: 'helper', helperVersion: this.context.extension.packageJSON.version })
                        .catch(error => this.output.appendLine(`AIRK diagnostic history unavailable: ${String(error)}`));
                }
                if (!prior.has(spec.collection) || prior.get(spec.collection)?.dataDir !== spec.dataDir) {
                    this.coverage.request(spec);
                    if (this.options().mode !== 'manual') { void this.scheduler.request(spec); }
                }
            }
            await this.context.globalState.update('collections-1.2', known);
            await this.rememberStorage(dataDir);
            // Resume already committed semantic work even when the dashboard is
            // hidden or source indexing is in Manual mode.
            await Promise.all([...next.values()].map(spec => this.refreshVectors(spec)));
            if (JSON.stringify([...prior.values()]) !== JSON.stringify([...next.values()])) { this.definitions.fire(); }
        } finally { this.refreshing = false; this.updateStatus(); }
    }
    private async rememberStorage(directory: string): Promise<void> {
        const history = this.context.globalState.get<string[]>('storageHistory', []);
        await this.context.globalState.update('storageHistory', [...new Set([directory, ...history])].slice(0, 32));
    }
    private updateStatus(): void {
        if (this.disposed) { return; }
        const states = this.scheduler?.states() ?? [];
        const spec = this.activeSpec();
        const state = states.find(state => state.collection === spec?.collection);
        const view = spec && this.coverage?.get(spec.collection);
        const lastIndexed = spec && this.context.globalState.get<Record<string, KnownCollection>>('collections-1.2', {})[spec.collection]?.lastIndexed;
        const progress = spec && this.indexProgress.get(this.mcpKey(spec))?.event;
        const semantic = spec && semanticCompletion(this.vectorViews.get(this.mcpKey(spec)), this.semanticProgress.get(this.mcpKey(spec)));
        const summary = !vscode.workspace.isTrusted ? 'Trust required' : this.refreshing ? 'Starting'
            : state?.state === 'indexing' ? `Indexing${progress?.committed_artifacts ? `: ${progress.committed_artifacts} files committed` : ''}`
            : state?.state === 'enriching' ? `Building semantic index${semantic ? ` ${semantic.percent}%` : ''}`
            : state?.state === 'semantic-pending' ? `Semantic indexing pending${semantic ? ` ${semantic.percent}%` : ''}`
            : state?.state === 'capacity-limited' ? 'Storage budget'
            : state?.state === 'failed' ? 'Failed' : state?.state === 'paused' ? 'Paused' : !spec ? this.specs.size ? 'Choose folder' : 'Open a folder'
            : coverageLabel(view || undefined, lastIndexed || undefined);
        this.status.text = `$(database) AIRK: ${summary}`;
        this.collectionTree.refresh();
        this.sidebar.refresh();
        const unsaved = spec ? (vscode.workspace.textDocuments ?? []).filter(document => document.isDirty && eligibleEvent(this.sourcePath(document.uri.fsPath), spec.root, spec.dataDir)).length : 0;
        this.status.tooltip = spec ? coverageDetails(spec.name, view || undefined, unsaved, lastIndexed || undefined)
            + (state?.state === 'enriching' && semantic ? `\nSemantic progress: ${semantic.completed} of ${semantic.total} selected files published.` : '')
            + `\nAuto-indexing: ${this.scheduler?.isPaused ? 'paused' : this.options().mode}${state?.pending ? '; indexing pending' : ''}`
            + (this.indexProgress.get(this.mcpKey(spec))?.event.revision ? '\nPublished indexed evidence is available to MCP while indexing continues.' : '')
            + '\nClick to open AIRK controls.'
            : 'Select a workspace folder to index. Click to open AIRK controls.';
    }
    private sourcePath(file: string): string {
        // Editors/events retain opened aliases (including Windows short paths).
        // Translate only workspace roots already verified by async refresh; no
        // filesystem work belongs in status updates or native event callbacks.
        for (const binding of this.rootBindings) {
            const relative = path.relative(binding.opened, file);
            if (!path.isAbsolute(relative) && relative !== '..' && !relative.startsWith('..' + path.sep)) {
                return path.resolve(binding.canonical, relative);
            }
        }
        return file;
    }
    private activeSpec(): LaunchSpec | undefined {
        if (this.specs.size === 1) { return this.specs.values().next().value; }
        const opened = vscode.window.activeTextEditor?.document.uri.fsPath;
        const active = opened && this.sourcePath(opened);
        return active ? [...this.specs.values()].filter(spec => {
            const relative = path.relative(spec.root, active);
            return !path.isAbsolute(relative) && relative !== '..' && !relative.startsWith('..' + path.sep);
        }).sort((a, b) => b.root.length - a.root.length)[0] : undefined;
    }
    editorChanged(): void { this.updateStatus(); }
    changed(uri: vscode.Uri, saved = false): void {
        if (this.disposed || !vscode.workspace.isTrusted) { return; }
        const mode = this.options().mode;
        const source = this.sourcePath(uri.fsPath);
        for (const spec of this.specs.values()) {
            if (eligibleEvent(source, spec.root, spec.dataDir)) {
                this.coverage.invalidate(spec);
                if (mode !== 'onSave' || saved) {
                    const key = this.mcpKey(spec), paths = this.changedPaths.get(key);
                    if (paths !== null) {
                        const selected = paths ?? new Set<string>();
                        selected.add(path.relative(spec.root, source).split(path.sep).join('/'));
                        this.changedPaths.set(key, selected.size > 1024 ? null : selected);
                    }
                    if (mode !== 'manual') { this.signalSourceChange(spec, path.relative(spec.root, source).split(path.sep).join('/')); }
                    void this.scheduler.request(spec);
                }
            }
        }
    }
    private requestIndex(spec: LaunchSpec, full: boolean): Promise<void> {
        this.signalSourceChange(spec);
        return this.scheduler.request(spec, true, full);
    }
    private signalSourceChange(spec: LaunchSpec, logicalPath?: string): void {
        const key = this.mcpKey(spec);
        if (!this.semanticStarted.has(key) || this.semanticSignaled.has(key)) { return; }
        this.semanticSignaled.add(key);
        // Queue admission uses a separate SQLite database and needs no collection
        // writer. One durable hint lets a running drain yield at its next commit;
        // the settled source pass later submits the complete accumulated path set.
        const signal = Promise.resolve().then(async () => {
            if (this.currentSpec(spec)) { await runJson(spec, 'schedule', ['enqueue', ...(logicalPath ? ['--paths', logicalPath] : [])]); }
        }).catch(error => { if (!this.disposed) { this.output.appendLine(`${spec.name}: source update notification failed: ${String(error)}`); } });
        this.semanticSignals.set(key, signal);
        void signal.finally(() => { if (this.semanticSignals.get(key) === signal) { this.semanticSignals.delete(key); } });
    }
    activity(): void { this.scheduler.activity(); this.coverage.activity(); this.updateStatus(); }
    async chooseSpec(): Promise<LaunchSpec | undefined> {
        this.trusted(); await this.refresh();
        const specs = [...this.specs.values()];
        if (!specs.length) { throw new Error('Open a filesystem workspace folder to use AIRK.'); }
        const active = this.activeSpec(); if (active) { return active; }
        return (await vscode.window.showQuickPick(specs.map(spec => ({ label: spec.name, description: spec.root, spec })), { title: 'AIRK collection' }))?.spec;
    }
    private async index(spec: LaunchSpec, full: boolean, automatic: boolean): Promise<IndexReport> {
        this.trusted();
        this.indexErrors.delete(JSON.stringify([spec.dataDir, spec.collection]));
        const progressKey = this.mcpKey(spec);
        if (!automatic && this.scheduler.isPaused) { this.queuedIndex.delete(progressKey); }
        const maxBytes = this.machineValue('storage.maxBytes', 0);
        if (automatic && maxBytes > 0) {
            const usage = await runJson<Usage[]>(spec, 'collections', ['list']);
            if (usage.reduce((bytes, item) => bytes + item.disk_bytes, 0) >= maxBytes) {
                throw new CapacityError('Index storage budget (airk.storage.maxBytes) reached; automatic indexing stopped. Reclaim unreferenced current objects, remove unused collections or raise the budget, then run Resume Auto-indexing. Index Now still works.');
            }
        }
        let report!: IndexReport;
        try {
            // Detect an obsolete owned format before model preflight. The native
            // preparation verifies ownership and is a no-op for current indexes.
            // A recovered upgrade must scan every saved source, even when the
            // request began as an incremental or automatic pass.
            const upgraded = await this.writeNative<boolean>(spec, 'collections', ['prepare-rebuild']);
            if (typeof upgraded !== 'boolean') { throw new Error('AIRK helper returned an invalid index upgrade result.'); }
            if (upgraded) {
                full = true;
                this.queuedIndex.delete(progressKey);
                const message = `${spec.name}: older collection format detected; rebuilding the index from saved source files. Collection settings and source files are preserved.`;
                this.output.appendLine(message);
                this.vectorProgress.set(progressKey, 'Upgrading collection index');
                this.sidebar.refresh();
                void vscode.window.showInformationMessage(message);
            }
            const vectors = await runJson<VectorStatus>(spec, 'collections', ['vectors', 'show']);
            if (typeof vectors.enabled !== 'boolean') { throw new Error('Vector search requires the updated AIRK helper.'); }
            this.vectorViews.set(this.mcpKey(spec), vectors);
            if (vectors.enabled) {
                await prepareVectorModel(spec.dataDir, message => {
                    this.vectorProgress.set(this.mcpKey(spec), message);
                    this.output.appendLine(message); this.sidebar.refresh();
                }, async () => {
                    this.trusted();
                    if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed.'); }
                    await this.writeNative(spec, 'collections', ['vectors', 'install-bundled']);
                }, () => {
                    this.trusted();
                    return runJson(spec, 'model-catalog');
                });
                this.vectorProgress.set(this.mcpKey(spec), 'Indexing saved files before vector indexing');
                this.sidebar.refresh();
            }
            this.trusted();
            if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed.'); }
            const joining = this.queuedIndex.has(progressKey);
            await this.semanticSignals.get(progressKey);
            const hintedPaths = automatic && !full && !joining ? this.changedPaths.get(progressKey) : undefined;
            if (!joining) { this.changedPaths.delete(progressKey); }
            let selectedPaths = hintedPaths?.size ? [...hintedPaths].sort() : undefined;
            // Leave room for Windows argv quoting and launch flags. A large
            // edit burst safely becomes a full pass rather than failing to spawn.
            if (selectedPaths && Buffer.byteLength(JSON.stringify([spec.command, spec.root, spec.dataDir, spec.collection, ...selectedPaths]), 'utf8') > 16000) {
                selectedPaths = undefined;
            }
            report = await indexWithLimitApproval(
                () => {
                    const tracker = new IndexProgressTracker(spec.collection, observation => {
                        if (this.currentSpec(spec)) { this.indexProgress.set(progressKey, observation); this.updateStatus(); }
                    });
                    return joinScheduledIndex<IndexReport>(['--progress', '--scheduled', '--coalesced', ...(automatic ? ['--automatic'] : []), ...(full ? ['--full'] : []), ...(selectedPaths ? ['--paths', ...selectedPaths] : [])],
                        (command, args) => {
                            this.trusted();
                            if (!this.currentSpec(spec)) { throw new Error('Collection closed or storage changed.'); }
                            return this.writeNative<IndexReport>(spec, command, args, line => tracker.line(line), () => tracker.start());
                        }, () => {
                            this.trusted();
                            if (this.disposed || !this.currentSpec(spec)) { throw new Error('Collection closed or storage changed.'); }
                            tracker.waiting();
                            return !this.scheduler.isPaused;
                        }, this.queuedIndex.get(progressKey)).then(result => {
                            this.queuedIndex.delete(progressKey); return result;
                        }).catch(error => {
                            if (error instanceof ScheduledIndexStillQueued) {
                                this.queuedIndex.set(progressKey, error.revision);
                                this.output.appendLine(`${spec.name}: ${error.message}`);
                                if (!automatic) { void vscode.window.showInformationMessage(`${spec.name}: ${error.message}`); }
                            } else if (!(error instanceof NativeProcessError && error.scanPaused)) {
                                this.queuedIndex.delete(progressKey);
                            }
                            throw error;
                        });
                },
                error => authorizeLimitIncrease({
                    describe: () => { this.trusted(); return runJson<LimitsDescription>(spec, 'collections', ['limits', 'describe']); },
                    review: async message => await vscode.window.showWarningMessage(message, 'Review larger limit') === 'Review larger limit',
                    input: async (prompt, value, validateInput) => vscode.window.showInputBox({ prompt, value, validateInput, ignoreFocusOut: true }),
                    confirm: async message => await vscode.window.showWarningMessage(message, { modal: true }, 'Save limit and retry') === 'Save limit and retry',
                    save: async (proposed, expected) => {
                        this.trusted();
                        if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed; review its current limits.'); }
                        await this.writeNative(spec, 'collections', ['limits', 'set', '--json', JSON.stringify(proposed), '--expected-json', JSON.stringify(expected)]);
                    }
                }, spec.name, error));
        }
        catch (error) {
            if (error instanceof NativeProcessError && error.scanPaused) { throw new PausedError(error.message); }
            if (error instanceof Error && /storage quota exhausted|quota_blocked|database or disk is full|private catalog cannot grow|physical.*(capacity|space|headroom)|insufficient.*space/i.test(error.message)) {
                throw new CapacityError(`${error.message} Automatic indexing is stopped for this collection. Review Storage limits, run current index cleanup or increase the quota, then Resume or Index now.`);
            }
            if (error instanceof Error && (error.message.includes('native automatic scheduling is paused') || error.message.includes('scan paused; committed batches remain available'))) { throw new PausedError(error.message); }
            throw error;
        }
        finally {
            this.indexProgress.delete(progressKey); this.vectorProgress.delete(progressKey);
            if (!report || !this.applyIndexReport(spec, report)) { await this.refreshVectors(spec); }
            this.updateStatus();
        }
        this.output.appendLine(`${spec.name}: generation ${report.generation}; indexed ${report.indexed}, reused ${report.reused}, removed ${report.removed}, skipped ${report.skipped.length + (report.skipped_truncated ?? 0)}`);
        this.logIndexTimings(spec, report);
        if (report.repaired_retained_artifacts || report.retained_repair_retries) {
            const action = `Retained data repair: ${report.repaired_retained_artifacts ?? 0} files rebuilt during reuse; ${report.retained_repair_retries ?? 0} fresh scan retries.`;
            this.output.appendLine(`[${new Date().toISOString()}] ${spec.name}: ${action}`);
            void this.diagnosticHistory.append(spec, { kind: 'repair', action, helperVersion: this.context.extension.packageJSON.version })
                .catch(error => this.output.appendLine(`AIRK diagnostic history unavailable: ${String(error)}`));
        }
        for (const skipped of report.skipped.slice(0, 20)) { this.output.appendLine(`Skipped ${skipped.path}: ${skipped.reason}`); }
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections-1.2', {});
        known[spec.collection] = { name: spec.name, root: spec.root, lastIndexed: new Date().toISOString() };
        await this.context.globalState.update('collections-1.2', known);
        await this.refreshDetail(spec);
        void this.diagnosticHistory.append(spec, { kind: 'index_completed', helperVersion: this.context.extension.packageJSON.version })
            .catch(error => this.output.appendLine(`AIRK diagnostic history unavailable: ${String(error)}`));
        this.invalidateCollectionTree();
        return report;
    }
    invalidateCollectionTree(): void {
        this.treeUsage = undefined; this.collectionTree.refresh();
        if (this.sidebar.visible) { void this.collectionTreeItems().then(() => this.sidebar.refresh()).catch(error => this.output.appendLine(String(error))); }
    }
    sidebarState(): SidebarState {
        const states = this.scheduler.states();
        const known = vscode.workspace.isTrusted ? this.context.globalState.get<Record<string, KnownCollection>>('collections-1.2', {}) : {};
        return { trusted: vscode.workspace.isTrusted, paused: this.scheduler.isPaused || states.some(state => state.state === 'paused'),
            registrationEnabled: this.registrationEnabled(),
            responseMode: this.responseMode(),
            cliStatus: this.cliStatus,
            mode: this.options().mode, changingStorage: !!this.storageChoice, groups: this.controlGroups(),
            collections: vscode.workspace.isTrusted ? [...this.specs.values()].map(spec => ({ ...spec,
                state: states.find(state => state.collection === spec.collection)?.state ?? 'queued',
                coverage: this.coverage.get(spec.collection), lastIndexed: known[spec.collection]?.lastIndexed,
                diskBytes: this.treeUsage?.dataDir === spec.dataDir ? this.treeUsage.entries.find(item => item.collection === spec.collection)?.disk_bytes : undefined,
                error: this.indexErrors.get(JSON.stringify([spec.dataDir, spec.collection])), mcp: this.mcpChecks.get(this.mcpKey(spec)), detail: this.detailViews.get(this.mcpKey(spec)), vectors: this.vectorViews.get(this.mcpKey(spec)), vectorProgress: this.vectorProgress.get(this.mcpKey(spec)),
                indexActivity: this.indexProgress.get(this.mcpKey(spec)) ? {
                    ...this.indexProgress.get(this.mcpKey(spec))!.event,
                    elapsedSeconds: Math.floor((Date.now() - this.indexProgress.get(this.mcpKey(spec))!.started) / 1000),
                    quietSeconds: Math.floor((Date.now() - this.indexProgress.get(this.mcpKey(spec))!.updated) / 1000)
                } : undefined,
                semanticElapsedSeconds: this.semanticStarted.has(this.mcpKey(spec))
                    ? Math.floor((Date.now() - this.semanticStarted.get(this.mcpKey(spec))!) / 1000) : undefined,
                semanticStartedAt: this.semanticStarted.get(this.mcpKey(spec)),
                semanticProgress: this.semanticProgress.get(this.mcpKey(spec)),
                usage: this.usageSummaries.get(this.mcpKey(spec)) })) : [] };
    }
    async sidebarMessage(message: unknown): Promise<void> {
        const action = sidebarAction(message, new Set(this.controlGroups().flatMap(group => group.commands.map(([, command]) => command))));
        if (!action || this.disposed) { return; }
        if (action.type === 'ready') { await this.collectionTreeItems(); this.sidebar.refresh(); return; }
        if (action.type === 'command') {
            // Registered commands already report their own failures to the user.
            await vscode.commands.executeCommand(action.command).then(undefined, () => undefined); return;
        }
        this.trusted();
        if (action.type === 'responseMode') {
            await vscode.workspace.getConfiguration('airk').update('mcp.responseMode', action.value, vscode.ConfigurationTarget.Global);
            this.registrationChanged(); return;
        }
        if (action.type === 'registration') {
            await vscode.workspace.getConfiguration('airk').update('mcp.registrationEnabled', action.enabled, vscode.ConfigurationTarget.Global);
            this.registrationChanged(); return;
        }
        if (action.type === 'mode') {
            await vscode.workspace.getConfiguration('airk').update('index.mode', action.value, vscode.ConfigurationTarget.Global);
            this.configure(); this.updateStatus(); return;
        }
        const spec = this.specs.get(action.collection);
        if (!spec || spec.dataDir !== action.dataDir) { throw new Error('Collection storage changed. Use the refreshed AIRK dashboard.'); }
        if (action.type === 'vectors') { await this.setVectors(spec, action.enabled); return; }
        if (action.type === 'detail') { await this.indexDetail(spec, action.level); return; }
        if (action.action === 'compression') { await this.compression(spec); return; }
        if (action.action === 'verify') { await this.verify(spec); return; }
        if (action.action === 'tools') { await this.openTools(spec); return; }
        if (action.action === 'diagnose') { await this.diagnose(spec); return; }
        if (action.action === 'mcpDetails') {
            const check = this.mcpChecks.get(this.mcpKey(spec));
            if (check) { await this.showJson('AIRK helper verification; agent connection status is separate', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, ...check }); }
            return;
        }
        if (action.action === 'status') { await this.collectionStatus(spec); return; }
        if (action.action === 'usage') { await this.usage(spec); return; }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: `Indexing ${spec.name}` },
            () => this.requestIndex(spec, action.action === 'reindex'));
    }
    async collectionTreeItems(): Promise<CollectionItem[]> {
        if (!vscode.workspace.isTrusted || this.disposed) { return []; }
        const dataDir = await this.dataDir();
        const refreshDetail = this.treeUsage?.dataDir !== dataDir || Date.now() - this.treeUsage.updated >= 30000;
        let usage: Usage[] = [];
        if (this.treeUsage?.dataDir === dataDir && Date.now() - this.treeUsage.updated < 30000) {
            usage = this.treeUsage.entries;
        } else {
            try {
                const command = await this.command();
                const spec: LaunchSpec = { command, dataDir, root: this.specs.values().next().value?.root || this.context.extensionPath,
                    collection: 'maintenance', name: 'Storage' };
                const listed = await runJson<Usage[]>(spec, 'collections', ['list']);
                if (!Array.isArray(listed)) { throw new Error('Invalid native collection list.'); }
                usage = listed;
                this.treeUsage = { dataDir, updated: Date.now(), entries: usage };
            } catch (error) {
                this.output.appendLine(`Collection view unavailable: ${String(error)}`);
                this.treeUsage = { dataDir, updated: Date.now(), entries: [] };
            }
        }
        await Promise.all([...this.specs.values()].filter(spec => refreshDetail || !this.detailViews.has(this.mcpKey(spec)))
            .map(async spec => { await this.refreshDetail(spec); await this.refreshVectors(spec); await this.refreshUsageSummary(spec); }));
        const byId = new Map(usage.map(item => [item.collection, item]));
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections-1.2', {});
        const states = new Map(this.scheduler.states().map(item => [item.collection, item]));
        const ids = new Set([...this.specs.keys(), ...byId.keys()]);
        return [...ids].map(id => {
            const spec = this.specs.get(id), entry = byId.get(id), view = this.coverage.get(id), scheduler = states.get(id);
            const active = !!spec && spec.dataDir === dataDir;
            const state = scheduler?.state === 'failed' ? 'failed'
                : active && (this.scheduler.isPaused || scheduler?.state === 'paused') ? 'paused'
                : active && scheduler && ['indexing', 'enriching', 'semantic-pending'].includes(scheduler.state) ? scheduler.state
                : active && (scheduler?.pending || view?.pending) ? 'pending'
                : entry?.status === 'indexed' ? 'indexed' : 'pending';
            const coverage = active ? coverageLabel(view, known[id]?.lastIndexed) : 'coverage unavailable';
            const key = spec && this.mcpKey(spec);
            const semantic = key && semanticCompletion(this.vectorViews.get(key), this.semanticProgress.get(key));
            const committed = key && this.indexProgress.get(key)?.event.committed_artifacts;
            const progress = state === 'enriching' || state === 'semantic-pending'
                ? semantic ? `semantic ${semantic.percent}% (${semantic.completed}/${semantic.total})` : 'semantic starting'
                : state === 'indexing' ? `${committed ?? 0} files committed` : undefined;
            return new CollectionItem(id, dataDir, active, !active && entry?.status === 'indexed', spec?.name || known[id]?.name || id,
                state, coverage, entry?.disk_bytes, known[id]?.lastIndexed, spec?.root || known[id]?.root, progress);
        }).sort((a, b) => Number(b.active) - Number(a.active) || String(a.label).localeCompare(String(b.label)));
    }
    async treeIndex(item: CollectionItem, full: boolean): Promise<void> {
        this.trusted();
        const spec = this.specs.get(item.collection);
        if (!item.active || !spec || spec.dataDir !== item.dataDir) { throw new Error('Collection is no longer active in this window.'); }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification,
            title: `${full ? 'Reindexing' : 'Indexing'} ${spec.name}` }, () => this.requestIndex(spec, full));
    }
    async treeRemove(item: CollectionItem): Promise<void> {
        this.trusted();
        if (item.active || item.dataDir !== await this.dataDir()) { throw new Error('Only an inactive collection in current storage can be removed.'); }
        await this.collections(true, item.collection);
    }
    private closeToolsTerminals(next?: Map<string, LaunchSpec>): void {
        for (const [key, { spec, terminal }] of this.toolsTerminals) {
            const current = next?.get(spec.collection);
            if (!current || current.command !== spec.command || current.root !== spec.root || current.dataDir !== spec.dataDir) {
                terminal.dispose(); this.toolsTerminals.delete(key);
            }
        }
    }
    async openTools(selected?: LaunchSpec): Promise<void> {
        this.trusted();
        const spec = selected ?? await this.chooseSpec(); if (!spec) { return; }
        this.trusted();
        const current = this.specs.get(spec.collection);
        if (this.disposed || this.storageChoice || !current || current.command !== spec.command || current.root !== spec.root || current.dataDir !== spec.dataDir) {
            throw new Error('Collection storage changed. Use the refreshed AIRK dashboard.');
        }
        const key = JSON.stringify([spec.command, spec.root, spec.dataDir, spec.collection]);
        let entry = this.toolsTerminals.get(key);
        if (!entry || entry.terminal.exitStatus !== undefined) {
            entry?.terminal.dispose();
            if (!this.toolsClosed) {
                this.toolsClosed = vscode.window.onDidCloseTerminal(closed => {
                    for (const [id, item] of this.toolsTerminals) { if (item.terminal === closed) { this.toolsTerminals.delete(id); } }
                });
            }
            // Launch the verified helper directly; no shell interpolation or PATH dependency.
            const terminal = vscode.window.createTerminal({
                name: `AIRK Tools — ${spec.name}`, cwd: spec.root, shellPath: spec.command,
                shellArgs: [...launchArgs(spec, 'tools'), 'shell'], env: { AIRK_CLIENT: 'vscode' },
                location: vscode.TerminalLocation.Panel, isTransient: true
            });
            entry = { spec, terminal }; this.toolsTerminals.set(key, entry);
        }
        entry.terminal.show();
    }
    async diagnose(selected?: LaunchSpec): Promise<void> {
        this.trusted();
        const configuredStorage = this.machineValue('storage.path', '');
        const folders = await Promise.all((vscode.workspace.workspaceFolders ?? [])
            .filter(folder => folder.uri.scheme === 'file' || (!!vscode.env.remoteName && folder.uri.scheme === 'vscode-remote'))
            .map(async folder => ({ folder, root: await fs.realpath(folder.uri.fsPath) })));
        const active = selected ?? this.activeSpec();
        const chosen = active ? folders.find(item => item.root === active.root)
            : folders.length === 1 ? folders[0]
            : await vscode.window.showQuickPick(folders.map(item => ({ ...item, label: item.folder.name, description: item.root })), { title: 'Diagnose AIRK collection' });
        if (!chosen) { return; }
        let spec: LaunchSpec = active ?? { command: path.join(this.context.extensionPath, 'native', process.platform === 'win32' ? 'airk.exe' : 'airk'),
            root: chosen.root, dataDir: await this.dataDir(), collection: collectionId(chosen.root), name: chosen.folder.name };
        const registered = this.specs.has(spec.collection);
        const valid = () => !this.disposed && vscode.workspace.isTrusted && !this.storageChoice
            && this.machineValue('storage.path', '') === configuredStorage
            && !!vscode.workspace.workspaceFolders?.some(folder => folder.uri.fsPath === chosen.folder.uri.fsPath)
            && (!registered || this.specs.has(spec.collection))
            && this.rootBindings.every(binding => binding.opened !== chosen.folder.uri.fsPath || binding.canonical === spec.root)
            && (!this.specs.has(spec.collection) || (this.specs.get(spec.collection)?.root === spec.root && this.specs.get(spec.collection)?.dataDir === spec.dataDir));
        const canVerify = () => !this.scheduler.states().some(item => item.collection === spec.collection && ['indexing', 'enriching'].includes(item.state));
        await openDiagnosticsPanel({ name: spec.name, valid, canVerify,
            diagnose: async () => {
                if (!valid()) { throw new Error('Collection closed or storage changed. Reopen Diagnose & Repair.'); }
                let helperError: unknown;
                try { spec = { ...spec, command: await this.command() }; }
                catch (error) { helperError = error; }
                const editor = this.scheduler.states().find(item => item.collection === spec.collection);
                let events: Awaited<ReturnType<DiagnosticHistory['read']>> = [], historyError: unknown;
                try { events = await this.diagnosticHistory.read(spec); }
                catch (error) { historyError = error; }
                return collectDiagnostics(spec, { expectedVersion: this.context.extension.packageJSON.version, helperError, events, historyError, shouldContinue: valid,
                    agentConfiguration: { installedCliPath: path.join(this.context.globalStorageUri.fsPath, 'cli', 'bin', process.platform === 'win32' ? 'airk.exe' : 'airk'), verifiedHelper: helperError === undefined },
                    editor: { paused: this.scheduler.isPaused, busy: ['indexing', 'enriching'].includes(editor?.state ?? ''), state: editor?.state, pending: editor?.pending } });
            },
            verify: (report, onProgress, shouldContinue) => verifyDiagnosticIntegrity(spec, report, { onProgress,
                shouldContinue: () => valid() && canVerify() && shouldContinue() }),
            act: async (action, report) => {
                if (!valid()) { throw new Error('Collection closed or storage changed. Reopen Diagnose & Repair.'); }
                if (action === 'showOutput') { this.output.show(); return; }
                if (action === 'rebuild' && await vscode.window.showWarningMessage(
                    `Rebuild the derived AIRK index for ${spec.name}? An index-format upgrade removes retained source copies, extractions and captured model objects, then recaptures eligible saved files and recomputes semantic evidence. Source files and saved settings are preserved.`,
                    { modal: true }, 'Rebuild index') !== 'Rebuild index') { return; }
                if (!valid()) { throw new Error('Collection closed or storage changed.'); }
                if (report?.probes.helper.state === 'failed') { this.helper = undefined; }
                await this.refresh();
                if (!valid()) { throw new Error('Collection closed or storage changed.'); }
                const current = this.specs.get(spec.collection);
                if (!current || current.root !== spec.root || current.dataDir !== spec.dataDir) { throw new Error('Collection binding changed.'); }
                spec = current;
                await this.diagnosticHistory.append(spec, { kind: 'repair', action, helperVersion: this.context.extension.packageJSON.version })
                    .catch(error => this.output.appendLine(`AIRK diagnostic history unavailable: ${String(error)}`));
                if (action === 'resume') { await this.pauseAuto(false); return; }
                await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification,
                    title: `${action === 'rebuild' ? 'Rebuilding' : 'Retrying'} AIRK: ${spec.name}` },
                    () => this.requestIndex(spec, action === 'rebuild'));
            }
        });
    }
    async manualIndex(full = false): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: `${full ? 'Reindexing' : 'Indexing'} ${spec.name}` },
            () => this.requestIndex(spec, full));
    }
    async pauseAuto(paused: boolean): Promise<void> {
        this.trusted();
        if (paused) { this.scheduler.pause(); }
        for (const spec of this.specs.values()) { await runJson(spec, 'schedule', [paused ? 'pause' : 'resume']); }
        if (!paused) { this.scheduler.resume(); }
    }
    async chooseMode(): Promise<void> {
        const current = this.options().mode;
        const choices = [
            { label: 'After edits settle (default)', mode: 'debounced', description: 'Batch file changes, including agent edits',
                detail: 'Recommended when a coding agent edits files in this workspace.' },
            { label: 'When idle', mode: 'idle', description: 'Wait until file changes and editor activity settle' },
            { label: 'On save', mode: 'onSave', description: 'Index saved documents; external edits wait for Index Now' },
            { label: 'Manual', mode: 'manual', description: 'Index only when you run Index Now' }
        ].map(choice => ({ ...choice, picked: choice.mode === current }));
        const choice = await vscode.window.showQuickPick(choices, { title: 'AIRK indexing mode' });
        if (choice) { await vscode.workspace.getConfiguration('airk').update('index.mode', choice.mode, vscode.ConfigurationTarget.Global); }
    }
    chooseStorage(): Promise<void> {
        this.trusted();
        this.storageChoice ??= this.scheduler.holdWhile(idle => this.chooseStorageWhenIdle(idle))
            .finally(() => { this.storageChoice = undefined; this.updateStatus(); });
        this.updateStatus();
        return this.storageChoice;
    }
    private async chooseStorageWhenIdle(idle: Promise<void>): Promise<void> {
        const selected = await vscode.window.showOpenDialog({ canSelectFiles: false, canSelectFolders: true, canSelectMany: false, title: 'Choose AIRK index storage; existing collections remain in the previous location' });
        if (!selected?.[0]) { return; }
        const directory = await fs.realpath(selected[0].fsPath);
        const validate = async () => {
            for (const folder of vscode.workspace.workspaceFolders ?? []) {
                const root = await fs.realpath(folder.uri.fsPath), relative = path.relative(directory, root);
                if (!relative || (!relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative))) {
                    throw new Error('Index storage must not be the workspace root or contain a workspace root.');
                }
            }
        };
        await validate();
        if (this.scheduler.isBusy) {
            await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification,
                title: 'AIRK: Waiting for the current indexing pass to finish before changing storage. New indexing jobs are held.' }, () => idle);
        } else { await idle; }
        this.trusted();
        if (this.disposed) { throw new Error('AIRK extension closed.'); }
        // Workspace folders may have changed while the current pass finished.
        await validate();
        await this.rememberStorage(await this.dataDir());
        await vscode.workspace.getConfiguration('airk').update('storage.path', directory, vscode.ConfigurationTarget.Global);
        await this.refresh();
        void vscode.window.showInformationMessage(`AIRK now stores new indexes in ${directory}. Existing indexes stay where they were and remain available in Manage Collections. Restart any agents already connected to AIRK.`);
    }
    private async maintenanceSpec(): Promise<LaunchSpec | undefined> {
        this.trusted();
        const command = await this.command(), current = await this.dataDir();
        const history = this.context.globalState.get<string[]>('storageHistory', []);
        const directories = [...new Set([current, ...history])];
        const directory = directories.length === 1 ? current : await vscode.window.showQuickPick(directories, { title: 'AIRK collection storage location' });
        if (!directory) { return; }
        return { command, root: this.specs.values().next().value?.root || this.context.extensionPath, dataDir: directory, collection: 'maintenance', name: 'Storage' };
    }
    async collections(remove = false, requestedId?: string): Promise<void> {
        const spec = requestedId ? { command: await this.command(), root: this.specs.values().next().value?.root || this.context.extensionPath,
            dataDir: await this.dataDir(), collection: 'maintenance', name: 'Storage' } : await this.maintenanceSpec();
        if (!spec) { return; }
        const usage = await runJson<Usage[]>(spec, 'collections', ['list']);
        const free = await fs.statfs(spec.dataDir).then(stat => stat.bavail * stat.bsize).catch(() => null);
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections-1.2', {});
        const total = usage.reduce((bytes, item) => bytes + item.disk_bytes, 0);
        if (!remove) {
            await this.showJson('AIRK collection storage', { dataDir: spec.dataDir, freeBytes: free, collectionBytes: total,
                softBudgetBytes: this.machineValue('storage.maxBytes', 0), collections: usage.map(item => ({ ...item, ...known[item.collection], activeInThisWindow: this.specs.get(item.collection)?.dataDir === spec.dataDir })) });
            return;
        }
        if (this.scheduler.isBusy) { throw new Error('Wait for indexing to finish before removing collections.'); }
        const candidates = usage.filter(item => this.specs.get(item.collection)?.dataDir !== spec.dataDir && item.status === 'indexed');
        if (!candidates.length) { void vscode.window.showInformationMessage('No removable unused collections. Open workspace collections are protected in this window.'); return; }
        const selected = requestedId
            ? candidates.filter(item => item.collection === requestedId).map(item => ({ item }))[0]
            : await vscode.window.showQuickPick(candidates.map(item => ({ label: known[item.collection]?.name || item.collection,
                description: `${(item.disk_bytes / 1048576).toFixed(2)} MiB · ${item.collection}`, detail: known[item.collection]?.root || item.status, item })), { title: 'Remove an unneeded AIRK collection' });
        if (!selected) {
            if (requestedId) { throw new Error('Collection is no longer removable. Refresh the AIRK view.'); }
            return;
        }
        const label = known[selected.item.collection]?.name;
        const confirm = await vscode.window.showWarningMessage(`Remove the index for ${label ? `${label} (${selected.item.collection})` : selected.item.collection}? Source files are not affected. Stop agents using this collection and close that folder in other windows first; removal is refused while it is still open.`, { modal: true }, 'Remove index');
        if (confirm !== 'Remove index') { return; }
        await runJson(spec, 'collections', ['remove', selected.item.collection]);
        this.invalidateCollectionTree();
        this.output.appendLine(`Removed derived collection ${selected.item.collection} from ${spec.dataDir}`);
        if (!this.scheduler.isPaused) { this.scheduler.resume(); }
    }
    async currentMaintenance(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await this.showJson('AIRK current index cleanup', await this.writeNative(spec, 'collections', ['maintain']));
        this.invalidateCollectionTree();
    }
    async ingestionLimits(configure = false): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const current = await runJson<LimitsDescription>(spec, 'collections', ['limits', 'describe']);
        await this.showJson('AIRK ingestion limits', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir,
            ...current, coverage: await runJson<object>(spec, 'coverage'),
            quota: await runJson<object>(spec, 'collections', ['quota', 'status']) });
        if (!configure) { return; }
        const choice = await vscode.window.showQuickPick(['Use automatic machine sizing', 'Import limits JSON'],
            { title: `Ingestion limits for ${spec.name}` });
        if (!choice) { return; }
        if (choice === 'Use automatic machine sizing') {
            if (await vscode.window.showWarningMessage(`Restore automatic ingestion limits for ${spec.name}? Existing indexed evidence is retained.`,
                { modal: true }, 'Restore automatic limits') !== 'Restore automatic limits') { return; }
            this.trusted();
            await this.showJson('AIRK saved ingestion limits', await this.writeNative<object>(spec, 'collections', ['limits', 'reset']));
        } else {
            const files = await vscode.window.showOpenDialog({ canSelectMany: false, canSelectFolders: false,
                filters: { 'Ingestion limits JSON': ['json'] }, title: `Choose ingestion limits for ${spec.name}` });
            if (!files?.[0]) { return; }
            await withPolicySnapshot(files[0].fsPath, async snapshot => {
                this.trusted();
                const proposed = await runJson<ScanLimits>(spec, 'collections', ['limits', 'validate', snapshot]);
                await this.showJson('AIRK proposed ingestion limits', { current, proposed });
                if (await vscode.window.showWarningMessage(`Save these ingestion limits for ${spec.name}? ${aboveNormal(proposed, current.automatic) ? 'These exceed the normal limits for this machine. ' : ''}${largerLimitsExplanation}`,
                    { modal: true }, 'Save ingestion limits') !== 'Save ingestion limits') { return; }
                this.trusted();
                await this.showJson('AIRK saved ingestion limits', await this.writeNative<object>(spec, 'collections', ['limits', 'set', snapshot, '--expected-json', JSON.stringify(current.effective)]));
            });
        }
        this.coverage.request(spec, true);
    }
    private currentSpec(spec: LaunchSpec): boolean {
        const current = this.specs.get(spec.collection);
        return !this.disposed && vscode.workspace.isTrusted && !!current && this.mcpKey(current) === this.mcpKey(spec);
    }
    private async enrich(spec: LaunchSpec): Promise<SemanticResult> {
        if (!this.currentSpec(spec)) { return 'ready'; }
        const progressKey = this.mcpKey(spec);
        try {
            this.indexErrors.delete(JSON.stringify([spec.dataDir, spec.collection]));
            let result: {state: string; error?: string | null; report?: IndexReport | null} | undefined;
            try {
                result = await this.nativeWrites.run(spec, async () => {
                    // A collection may have closed or changed storage while this write
                    // waited behind coverage or another native operation.
                    if (!this.currentSpec(spec)) { return undefined; }
                    this.semanticStarted.set(progressKey, Date.now());
                    this.semanticProgress.delete(progressKey);
                    this.updateStatus();
                    return runJson<{state: string; error?: string | null; report?: IndexReport | null}>(spec, 'semantic', ['run', '--scheduled', '--progress'], line => {
                        const event = parseIndexProgress(line);
                        if (!event || event.collection !== spec.collection || !event.phase.startsWith('enrichment_') || !this.currentSpec(spec)) { return false; }
                        this.semanticProgress.set(progressKey, event);
                        this.updateStatus();
                        return true;
                    });
                });
            } catch (error) {
                if (currentReadBusy(error)) { return this.currentSpec(spec) ? 'busy' : 'ready'; }
                throw error;
            }
            if (!result || !this.currentSpec(spec)) { return 'ready'; }
            if (result.state === 'paused') { return 'paused'; }
            if (result.state === 'busy') { return 'busy'; }
            if (result.state === 'blocked' || result.state === 'semantic_failed') {
                throw new Error(result.error || 'Semantic indexing failed. Use Resume or Index now after resolving the error.');
            }
            if (result.state === 'unregistered') { throw new Error('Semantic work has no scheduler registration. Use Index now to resume it.'); }
            if (!['enriched', 'idle'].includes(result.state)) { throw new Error(`Unexpected semantic indexing state: ${result.state}`); }
            let status: VectorStatus | undefined;
            try {
                status = result.report ? this.applyIndexReport(spec, result.report) : undefined;
                if (result.report) { this.logIndexTimings(spec, result.report); }
                status ??= await this.refreshVectors(spec, true);
            }
            catch (error) {
                if (currentReadBusy(error)) { return this.currentSpec(spec) ? result.state === 'enriched' ? 'pending' : 'busy' : 'ready'; }
                throw error;
            }
            if (!this.currentSpec(spec)) { return 'ready'; }
            this.treeUsage = undefined;
            this.sidebar.refresh();
            if (status?.semantic_readiness?.state === 'failed') {
                throw new Error(status.semantic_readiness.failure || 'Semantic indexing failed.');
            }
            return status?.enabled && ['pending', 'partial'].includes(status.semantic_readiness?.state ?? '')
                ? result.report?.yield_to_source ? 'yield' : 'pending' : 'ready';
        } finally {
            this.semanticStarted.delete(progressKey);
            this.semanticProgress.delete(progressKey);
            this.semanticSignaled.delete(progressKey);
            this.updateStatus();
        }
    }
    private applyIndexReport(spec: LaunchSpec, report: IndexReport): VectorStatus | undefined {
        if (!this.currentSpec(spec) || !report.semantic_readiness || typeof report.indexed_vectors !== 'number') { return undefined; }
        const prior = this.vectorViews.get(this.mcpKey(spec));
        if (!prior) { return undefined; }
        const status: VectorStatus = { ...prior, published: true, pending_reindex: false,
            indexed_vectors: report.indexed_vectors, semantic_readiness: report.semantic_readiness,
            discovery_readiness: report.discovery_readiness ?? prior.discovery_readiness };
        this.vectorViews.set(this.mcpKey(spec), status);
        this.scheduler.observeSemantic(spec, status.enabled && ['pending', 'partial', 'failed'].includes(status.semantic_readiness?.state ?? ''));
        return status;
    }
    private logIndexTimings(spec: LaunchSpec, report: IndexReport): void {
        const timings = [ ['scan/extraction', report.scan_extract_us], ['publication/drain', report.publish_us],
            ['first evidence', report.first_ready_us], ['model load', report.embedding_model_load_us],
            ['inference', report.embedding_inference_us], ['graph insertion', report.vector_graph_insert_us] ] as const;
        const measured = timings.filter((entry): entry is readonly [typeof entry[0], number] => typeof entry[1] === 'number')
            .map(([name, us]) => `${name} ${(us / 1000).toFixed(1)} ms`);
        if (measured.length) { this.output.appendLine(`${spec.name}: ${report.path_scoped ? 'path update' : 'index work'} timings: ${measured.join('; ')}`); }
        if (report.maintenance_error) { this.output.appendLine(`${spec.name}: ${report.maintenance_error}`); }
    }
    private async refreshVectors(spec: LaunchSpec, strict = false): Promise<VectorStatus | undefined> {
        try {
            const status = await runJson<VectorStatus>(spec, 'collections', ['vectors', 'show']);
            if (typeof status.enabled !== 'boolean' || typeof status.published !== 'boolean') { throw new Error('Vector search requires the updated AIRK helper.'); }
            if (this.currentSpec(spec)) {
                this.vectorViews.set(this.mcpKey(spec), status);
                this.scheduler.observeSemantic(spec, status.enabled && ['pending', 'partial', 'failed'].includes(status.semantic_readiness?.state ?? ''));
            }
            return status;
        } catch (error) {
            this.output.appendLine(`[${new Date().toISOString()}] ${spec.name}: Vector status unavailable: ${String(error)}`);
            if (strict) { throw error; }
            return undefined;
        }
    }
    async setVectors(spec: LaunchSpec, enabled: boolean): Promise<void> {
        this.trusted();
        await this.scheduler.holdWhile(async idle => {
            await idle; this.trusted();
            if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed.'); }
            const status = await this.writeNative<VectorStatus>(spec, 'collections', ['vectors', 'set', '--enabled', String(enabled)]);
            this.vectorViews.set(this.mcpKey(spec), status);
        });
        this.sidebar.refresh();
        await this.requestIndex(spec, false);
    }
    private async refreshDetail(spec: LaunchSpec): Promise<void> {
        try {
            const result = await runJson<DetailStatus>(spec, 'collections', ['extraction', 'show']);
            if (!['agent_lean', 'essential', 'balanced', 'detailed'].includes(result.configured_detail)
                || (result.published_detail !== null && !['agent_lean', 'essential', 'balanced', 'detailed'].includes(result.published_detail))) {
                throw new Error('Index detail requires the updated AIRK helper.');
            }
            if (vscode.workspace.isTrusted && this.specs.get(spec.collection)?.dataDir === spec.dataDir) {
                this.detailViews.set(this.mcpKey(spec), result);
            }
        } catch (error) { this.output.appendLine(`[${new Date().toISOString()}] ${spec.name}: Index detail unavailable: ${String(error)}`); }
    }
    async compression(spec: LaunchSpec): Promise<void> {
        this.trusted();
        const current = await runJson<{mode: string; unique_encoded_bytes: number; logical_reference_bytes: number}>(spec, 'compression', ['show']);
        const modes = [
            { label: 'Off', mode: 'off', description: 'Store new retained objects without compression' },
            { label: 'Automatic', mode: 'automatic', description: 'Use lossless blocks when encoded storage saves at least 10%' },
            { label: 'Space', mode: 'space', description: 'Keep smaller compressed objects, including modest savings' }
        ];
        const selected = await vscode.window.showQuickPick(modes, { title: `Compression for ${spec.name} (current: ${current.mode})` });
        if (!selected) { return; }
        await this.scheduler.holdWhile(async idle => {
            await idle;
            if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed.'); }
            await this.writeNative(spec, 'compression', ['set', selected.mode]);
        });
        const repack = await vscode.window.showInformationMessage('Compression policy saved for new retained objects. Repack current objects in bounded batches?', 'Repack', 'Later');
        if (repack !== 'Repack') { return; }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: `Repacking ${spec.name}`, cancellable: true }, async (progress, cancellation) => {
            await this.scheduler.holdWhile(async idle => {
                await idle;
                let after: string | undefined, examined = 0;
                do {
                    const report = await this.writeNative<{examined: number; next_after: string | null}>(spec, 'compression', ['repack', '--max-files', '32', ...(after ? ['--after', after] : [])]);
                    if (!Number.isSafeInteger(report.examined) || report.examined < 0 || report.examined > 32
                        || (report.next_after !== null && (typeof report.next_after !== 'string' || (after && report.next_after <= after)))) { throw new Error('Invalid repack progress.'); }
                    examined += report.examined;
                    if (examined > 100000) { throw new Error('Repack inventory bound exceeded.'); }
                    progress.report({message: `${examined} files examined`});
                    after = report.next_after ?? undefined;
                } while (after && !cancellation.isCancellationRequested);
            });
        });
        await this.showJson('Retained object storage', await runJson(spec, 'compression', ['show']));
    }
    async indexDetail(spec: LaunchSpec, level: IndexDetail): Promise<void> {
        this.trusted();
        const confirm = await vscode.window.showWarningMessage(
            `Use ${detailLabel(level)} for ${spec.name}? Agent Lean keeps selected declarations, call occurrences and document facts. Rich Structure adds broader facts and declaration tracking while retaining calls; Search Only keeps text and citations. Detailed adds document tracking. Applying a changed mode requires indexing and temporary disk space.`,
            { modal: true }, 'Save and index', 'Save for next index');
        if (!confirm) { this.sidebar.refresh(); return; }
        await this.scheduler.holdWhile(async idle => {
            await idle;
            this.trusted();
            if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed.'); }
            await this.writeNative(spec, 'collections', ['extraction', 'detail', level]);
            await this.refreshDetail(spec);
        });
        this.sidebar.refresh();
        if (confirm === 'Save and index') { await this.requestIndex(spec, false); }
    }
    async extractionPolicy(configure = false): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const current = await runJson<object>(spec, 'collections', ['extraction', 'show']);
        if (!configure) {
            await this.showJson('AIRK extraction policy', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, ...current });
            return;
        }
        const files = await vscode.window.showOpenDialog({ canSelectMany: false, canSelectFolders: false, filters: { 'Extraction policy JSON': ['json'] }, title: `Choose extraction policy for ${spec.name}` });
        if (!files?.[0]) { return; }
        await withPolicySnapshot(files[0].fsPath, async snapshot => {
            this.trusted();
            const proposed = await runJson<object>(spec, 'collections', ['extraction', 'validate', '--file', snapshot]);
            await this.showJson('AIRK proposed extraction policy', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, current, proposed,
                explanation: 'The configured policy applies on the next successful index. Existing published evidence keeps its original extraction profiles. Validation does not execute build files or rewrite source configuration.' });
            const confirm = await vscode.window.showWarningMessage(`Save this extraction policy for ${spec.name} (${spec.collection})? It applies on the next successful index.`, { modal: true }, 'Save extraction policy');
            if (confirm !== 'Save extraction policy') { return; }
            await this.showJson('AIRK saved extraction policy', await this.writeNative<object>(spec, 'collections', ['extraction', 'set', '--file', snapshot]));
            if (await vscode.window.showInformationMessage('Extraction policy saved. Index this collection now to apply it?', 'Index now') === 'Index now') {
                this.trusted(); await this.scheduler.request(spec, false);
            }
        });
    }
    async storageLimits(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const current = await runJson<{ root_bytes: number; default_collection_bytes: number; collections: Record<string, number> }>(spec, 'collections', ['quota', 'policy']);
        const gib = 1073741824;
        const validateInput = (text: string) => {
            const bytes = Number(text) * gib;
            return text.trim() && Number.isSafeInteger(bytes) && bytes >= 1048576 ? undefined : 'Enter a positive GiB value (at least 1 MiB) that converts to a whole, safe byte count.';
        };
        const collectionInput = await vscode.window.showInputBox({ title: `Storage quota for ${spec.name}`, prompt: 'Collection limit in GiB, including the current index and temporary updates. Default: 256 GiB.', value: String((current.collections[spec.collection] ?? current.default_collection_bytes) / gib), validateInput, ignoreFocusOut: true });
        if (collectionInput === undefined) { return; }
        const rootInput = await vscode.window.showInputBox({ title: 'Shared AIRK storage quota', prompt: 'Total limit in GiB for all collections in this storage folder. Default: 500 GiB.', value: String(current.root_bytes / gib), validateInput, ignoreFocusOut: true });
        if (rootInput === undefined) { return; }
        if (validateInput(collectionInput) || validateInput(rootInput)) { throw new Error('Invalid storage quota.'); }
        const proposed = { ...current, root_bytes: Number(rootInput) * gib, collections: { ...current.collections, [spec.collection]: Number(collectionInput) * gib } };
        if (proposed.collections[spec.collection] > proposed.root_bytes) { throw new Error('The collection quota cannot exceed the shared storage quota.'); }
        const message = `Save ${collectionInput} GiB for ${spec.name} and ${rootInput} GiB total? Unreferenced objects may be reclaimed before lowering a limit. Current objects and pending updates remain protected; their allocation can prevent the reduction. No disk space is preallocated.`;
        if (await vscode.window.showWarningMessage(message, { modal: true }, 'Save storage limits') !== 'Save storage limits') { return; }
        this.trusted();
        if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed; review its current limits.'); }
        await this.scheduler.holdWhile(async idle => {
            await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'AIRK: Applying storage limits after current indexing finishes' }, async () => {
                await idle;
                this.trusted();
                if (this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { throw new Error('Collection storage changed; review its current limits.'); }
                const status = await runJson<{ used_bytes: number; reserved_bytes: number; collections: Record<string, { used_bytes: number; reserved_bytes: number }> }>(spec, 'collections', ['quota', 'status']);
                const used = status.collections[spec.collection];
                if ((used && used.used_bytes + used.reserved_bytes > proposed.collections[spec.collection]) || status.used_bytes + status.reserved_bytes > proposed.root_bytes) {
                                await this.writeNative(spec, 'collections', ['maintain']);
                }
                await this.showJson('AIRK saved storage quotas', await this.writeNative(spec, 'collections', ['quota', 'set', '--json', JSON.stringify(proposed), '--expected-json', JSON.stringify(current)]));
            });
        });
        this.invalidateCollectionTree();
    }
    async showJson(title: string, value: unknown): Promise<void> {
        const document = await vscode.workspace.openTextDocument({ language: 'json', content: JSON.stringify({ title, ...value as object }, null, 2) });
        await vscode.window.showTextDocument(document, { preview: true });
    }
    showReport(kind: ReportKind, title: string, value: unknown): void {
        const panel = vscode.window.createWebviewPanel('airk.report', title, vscode.ViewColumn.Active,
            { enableScripts: true, localResourceRoots: [] });
        panel.webview.html = renderReport(title, kind, value);
        const listener = panel.webview.onDidReceiveMessage(message => {
            if (message && typeof message === 'object' && !Array.isArray(message)
                && Object.keys(message).length === 1 && message.type === 'openJson') {
                void this.showJson(title, value).catch(error => {
                    this.output.appendLine(`Report JSON unavailable: ${String(error)}`);
                    void vscode.window.showErrorMessage(`AIRK: ${String(error)}`);
                });
            }
        });
        panel.onDidDispose(() => listener.dispose());
    }
    async collectionStatus(selected?: LaunchSpec): Promise<void> {
        const spec = selected ?? await this.chooseSpec(); if (!spec) { return; }
        const coverage = await this.writeNative<Coverage>(spec, 'coverage', ['--refresh']);
        this.showReport('coverage', 'AIRK workspace coverage', { coverage, scheduling: this.scheduler.states(), dataDir: spec.dataDir,
            explanation: 'Coverage compares saved eligible files with indexed hashes at the inventory time. Ignored and unsupported files are excluded; extraction failures remain eligible. A partial scan cannot establish a percentage.' });
    }
    private async refreshUsageSummary(spec: LaunchSpec): Promise<void> {
        const key = this.mcpKey(spec);
        try {
            const view = await runJson<{ status: string; report: { calls: number; comparisons?: { full_returned_sources?: { compared_calls: number; token_estimate: { nominal_reduction: string } } } } }>(spec, 'usage', ['report']);
            const c = view.report?.comparisons?.full_returned_sources;
            if (!Number.isSafeInteger(view.report?.calls)) { throw new Error('Usage summary unavailable'); }
            if (vscode.workspace.isTrusted && !this.disposed && this.specs.get(spec.collection) && this.mcpKey(this.specs.get(spec.collection)!) === key) {
                const reduction = c && /^-?\d{1,40}$/.test(c.token_estimate?.nominal_reduction) ? BigInt(c.token_estimate.nominal_reduction).toLocaleString() : undefined;
                this.usageSummaries.set(key, { calls: view.report.calls, compared: c?.compared_calls ?? 0, reduction, checkedAt: Date.now(), disabled: view.status === 'disabled' });
            }
        } catch { this.usageSummaries.delete(key); }
    }
    async usage(selected?: LaunchSpec): Promise<void> {
        this.trusted();
        const spec = selected ?? await this.chooseSpec(); if (!spec) { return; }
        await openUsagePanel(spec, () => {
            const current = this.specs.get(spec.collection);
            return !this.disposed && !this.storageChoice && !!current && this.mcpKey(current) === this.mcpKey(spec);
        },
            args => this.writeNative(spec, 'usage', args));
    }
    async setupAgent(): Promise<void> {
        await setupAgent({
            trusted: () => this.trusted(), chooseSpec: () => this.chooseSpec(),
            registrationEnabled: () => this.registrationEnabled(), registrationChanged: () => this.registrationChanged(),
            manageVscodeMcp: () => this.manageVscodeMcp(), verify: spec => this.verify(spec),
            verification: spec => this.mcpChecks.get(this.mcpKey(spec)), rootBindings: () => this.rootBindings,
            subscribe: panel => { this.context.subscriptions.push(panel); },
            isCurrent: spec => {
                const current = this.specs.get(spec.collection);
                return !this.storageChoice && !this.refreshing && !this.disposed && !!current && this.mcpKey(current) === this.mcpKey(spec);
            }
        });
    }
    private mcpKey(spec: LaunchSpec): string { return JSON.stringify([spec.collection, spec.root, spec.dataDir, spec.command]); }
    responseMode(): 'terse' | 'full' { return this.machineValue<string>('mcp.responseMode', 'terse') === 'full' ? 'full' : 'terse'; }
    registrationEnabled(): boolean { return this.machineValue('mcp.registrationEnabled', true) === true; }
    registrationChanged(): void { this.definitions.fire(); this.updateStatus(); }
    async manageMcp(): Promise<void> { await this.setupAgent(); }
    async manageVscodeMcp(): Promise<void> {
        this.trusted();
        const command = 'workbench.mcp.listServer';
        if (!(await vscode.commands.getCommands(true)).includes(command)) {
            throw new Error('This editor does not expose MCP: List Servers. Manage the AIRK server in your coding agent’s MCP settings.');
        }
        await vscode.commands.executeCommand(command);
    }
    async verify(selected?: LaunchSpec): Promise<void> {
        this.trusted();
        const spec = selected ?? await this.chooseSpec(); if (!spec) { return; }
        if (this.storageChoice) { throw new Error('Finish choosing storage before verifying MCP.'); }
        const key = this.mcpKey(spec), pending = this.mcpPending.get(key);
        if (pending) { return pending; }
        const check: McpVerification & { result?: { tools: string[]; status: unknown } } = { phase: 'preparing' };
        this.mcpChecks.set(key, check); this.updateStatus();
        const current = () => !this.disposed && vscode.workspace.isTrusted && this.mcpChecks.get(key) === check
            && [...this.specs.values()].some(item => this.mcpKey(item) === key);
        const operation = (async () => {
            try {
                check.phase = 'checking'; this.updateStatus();
                const result = await probeMcp(spec, 15, this.context.extension.packageJSON.version);
                if (!current()) { return; }
                check.phase = 'verified'; check.tools = result.tools; check.result = result; check.checkedAt = Date.now();
                check.indexState = (result.status as { state?: string })?.state === 'not_indexed' ? 'not_indexed' : 'published';
                this.output.appendLine(`${spec.name}: MCP helper verified; ${result.tools.length} tools available. Agent connection status is separate.`);
                if (!selected) { void vscode.window.showInformationMessage(`AIRK helper verified: ${result.tools.length} tools for ${spec.name}. See the dashboard for results.`); }
            } catch (error) {
                if (current()) {
                    check.phase = 'failed'; check.checkedAt = Date.now(); check.error = String(error).slice(0, 600);
                    this.output.appendLine(`${spec.name}: MCP verification failed: ${check.error}`);
                }
                // Sidebar failures remain inline; palette callers retain their normal error reporting.
                if (!selected) { throw error; }
            } finally { this.mcpPending.delete(key); this.updateStatus(); }
        })();
        this.mcpPending.set(key, operation);
        return operation;
    }
    async provideDefinitions(): Promise<vscode.McpStdioServerDefinition[]> {
        if (!vscode.workspace.isTrusted || !this.registrationEnabled()) { return []; }
        await this.refresh();
        if (!vscode.workspace.isTrusted || !this.registrationEnabled()) { return []; }
        return [...this.specs.values()].map(spec => {
            const definition = new vscode.McpStdioServerDefinition(`AIRK: ${spec.name} (${spec.collection})`, spec.command, launchArgs(spec), { AIRK_CLIENT: 'vscode', AIRK_MCP_RESPONSE_MODE: this.responseMode() },
                `${this.context.extension.packageJSON.version}:${spec.dataDir}:${this.responseMode()}`);
            definition.cwd = vscode.Uri.file(spec.root);
            return definition;
        });
    }
    async resolveDefinition(definition: vscode.McpServerDefinition): Promise<vscode.McpServerDefinition> {
        this.trusted();
        if (!this.registrationEnabled()) { throw new Error('AIRK automatic VS Code registration is off. Enable it in the AIRK dashboard.'); }
        if (!(definition instanceof vscode.McpStdioServerDefinition)) { throw new Error('AIRK requires native stdio.'); }
        const spec = [...this.specs.values()].find(item => item.command === definition.command && JSON.stringify(launchArgs(item)) === JSON.stringify(definition.args));
        if (!spec) { throw new Error('AIRK workspace/storage configuration changed. Refresh its MCP definitions.'); }
        // MCP starts independently of indexing. Tools disclose unavailable or partial evidence
        // until the first publication, then serve each committed snapshot during the scan.
        this.trusted();
        if (!this.registrationEnabled()) { throw new Error('AIRK automatic VS Code registration is off.'); }
        return definition;
    }
    controlGroups(): ControlGroup[] { return sidebarControlGroups(); }
    async menu(): Promise<void> {
        const items: (vscode.QuickPickItem & { command?: string })[] = [];
        for (const group of this.controlGroups()) {
            items.push({ label: group.label, kind: vscode.QuickPickItemKind.Separator });
            items.push(...group.commands.map(([label, command, description]) => ({ label, command, description })));
        }
        const choice = await vscode.window.showQuickPick(items, { title: 'AIRK controls', matchOnDescription: true });
        if (choice?.command) { await vscode.commands.executeCommand(choice.command); }
    }
    configure(): void { const options = this.options(); this.scheduler.configure(options); this.coverage?.configure(options.quietMs, options.mode === 'idle' ? options.idleMs : 0); }
    dispose(): Promise<void> {
        if (this.disposal) { return this.disposal; }
        this.disposed = true; this.closeToolsTerminals(); this.toolsClosed?.dispose(); clearInterval(this.rescan); clearInterval(this.progressRefresh);
        this.sidebar.dispose(); this.coverage.dispose(); this.scheduler.dispose(); this.definitions.dispose(); this.collectionTree.dispose(); this.status.dispose();
        return this.disposal = (this.historyStore?.close() ?? Promise.resolve()).finally(() => this.output.dispose());
    }
}

let activeController: Controller | undefined;
export async function deactivate(): Promise<void> { await activeController?.dispose(); activeController = undefined; }

export async function activate(context: vscode.ExtensionContext): Promise<{ getServerDefinitions(): Promise<vscode.McpStdioServerDefinition[]> }> {
    const controller = new Controller(context); context.subscriptions.push(controller);
    activeController = controller;
    context.subscriptions.push(vscode.window.createTreeView('airk.collectionsView', { treeDataProvider: controller.collectionTree }));
    context.subscriptions.push(vscode.window.registerWebviewViewProvider('airk.dashboard', controller.sidebar));
    let serviceConsole:ServiceConsole|undefined;
    let nativeRuntime:RuntimeHost|undefined,runtimeGeneration=0;
    const runtime=()=>{
        if(!nativeRuntime){
            const generation=runtimeGeneration;
            nativeRuntime=new RuntimeHost(context.extensionPath,context.extension.packageJSON.version,serviceRuntimeOptions(),undefined,
                ()=>vscode.workspace.isTrusted&&!!serviceConsole&&generation===runtimeGeneration);
        }
        return nativeRuntime;
    };
    const serviceRuntime:ConsoleRuntime={discover:()=>runtime().discover(),ensure:explicit=>runtime().ensure(explicit),
        binding:(inspection,installDir)=>runtime().binding(inspection,installDir),client:binding=>runtime().client(binding),
        lifecycle:(binding,intent,force)=>runtime().lifecycle(binding,intent,force),
        abandonLifecycle:(binding,intent,id)=>runtime().abandonLifecycle(binding,intent,id)};
    serviceConsole=new ServiceConsole(context,serviceRuntime);
    const activeServiceConsole=serviceConsole;
    context.subscriptions.push(activeServiceConsole,{dispose:()=>{serviceConsole=undefined;}},
        vscode.window.registerWebviewViewProvider('airk.service',activeServiceConsole));
    for(const [name,action] of Object.entries({serviceConnect:'connect',serviceStart:'start',serviceRestart:'restart',serviceStop:'stop',
        serviceLogs:'logs',serviceResources:'resources',serviceDiagnostics:'previewReport'})){
        context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`,()=>activeServiceConsole.command(action)));
    }
    const safe = (action: () => void | Promise<unknown>, propagate = false) => async () => {
        try { await action(); } catch (error) {
            controller.output.appendLine(String(error)); controller.output.show(true);
            void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error ? error.message : String(error)}`);
            if (propagate) { throw error; }
        }
    };
    const commands: Record<string, () => void | Promise<unknown>> = {
        installCli: () => controller.installCommandLine(),
        openTools: () => controller.openTools(),
        menu: () => controller.menu(), index: () => controller.manualIndex(), reindex: () => controller.manualIndex(true), diagnose: () => controller.diagnose(),
        indexMode: () => controller.chooseMode(), pause: () => controller.pauseAuto(true),
        resume: async () => { await controller.refresh(); await controller.pauseAuto(false); },
        chooseStorage: () => controller.chooseStorage(), storageLimits: () => controller.storageLimits(), collections: () => controller.collections(), removeCollection: () => controller.collections(true), currentMaintenance: () => controller.currentMaintenance(),
        extractionPolicy: () => controller.extractionPolicy(), configureExtractionPolicy: () => controller.extractionPolicy(true),
        ingestionLimits: () => controller.ingestionLimits(), configureIngestionLimits: () => controller.ingestionLimits(true),
        status: () => controller.collectionStatus(), usage: () => controller.usage(), setupAgent: () => controller.setupAgent(), verifyMcp: () => controller.verify(), manageMcp: () => controller.manageMcp(), manageVscodeMcp: () => controller.manageVscodeMcp(), logs: () => controller.output.show()
    };
    for (const [name, action] of Object.entries(commands)) { context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`, safe(action, true))); }
    context.subscriptions.push(
        vscode.commands.registerCommand('airk.treeRefresh', () => controller.invalidateCollectionTree()),
        vscode.commands.registerCommand('airk.treeIndex', (item: CollectionItem) => safe(() => controller.treeIndex(item, false), true)()),
        vscode.commands.registerCommand('airk.treeReindex', (item: CollectionItem) => safe(() => controller.treeIndex(item, true), true)()),
        vscode.commands.registerCommand('airk.treePause', () => safe(() => controller.pauseAuto(true), true)()),
        vscode.commands.registerCommand('airk.treeRemove', (item: CollectionItem) => safe(() => controller.treeRemove(item), true)()));
    if (vscode.lm?.registerMcpServerDefinitionProvider) {
        context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.collections', {
            onDidChangeMcpServerDefinitions: controller.definitions.event,
            provideMcpServerDefinitions: () => controller.provideDefinitions(),
            resolveMcpServerDefinition: definition => controller.resolveDefinition(definition)
        }));
    }
    const watcher = vscode.workspace.createFileSystemWatcher('**/*');
    context.subscriptions.push(watcher, watcher.onDidChange(uri => controller.changed(uri)), watcher.onDidCreate(uri => controller.changed(uri)),
        watcher.onDidDelete(uri => controller.changed(uri)), vscode.workspace.onDidSaveTextDocument(document => controller.changed(document.uri, true)),
        vscode.workspace.onDidChangeTextDocument(() => controller.activity()), vscode.window.onDidChangeTextEditorSelection(() => controller.activity()),
        vscode.window.onDidChangeActiveTextEditor(() => controller.editorChanged()),
        vscode.workspace.onDidGrantWorkspaceTrust(safe(() => controller.refresh())),
        vscode.workspace.onDidChangeWorkspaceFolders(safe(() => controller.refresh())),
        vscode.workspace.onDidChangeConfiguration(event => {
            if(event.affectsConfiguration('airk.service')){runtimeGeneration++;nativeRuntime=undefined;activeServiceConsole.reset();}
            if (event.affectsConfiguration('airk.mcp.registrationEnabled') || event.affectsConfiguration('airk.mcp.responseMode')) { controller.registrationChanged(); }
            if (event.affectsConfiguration('airk')) { controller.configure(); void safe(() => controller.refresh())(); }
        }));
    await safe(() => controller.refresh())();
    return { getServerDefinitions: () => controller.provideDefinitions() };
}
