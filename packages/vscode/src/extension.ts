// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { Client, IndexReport, LaunchSpec, Usage, collectionId, configFor, eligibleEvent, launchArgs } from './model';
import { installJsonConfig, isJsonClient, jsonConfigPath, readJsonConfig } from './agentConfig';
import { agentChoices, codexConfigPath, connectionAction, createCodexConfig, renderConnectionGuide, verificationPrompt } from './agentConnections';
import { NativeWriteQueue, prepareHelper, probeMcp, runJson } from './native';
import { CapacityError, Options, PausedError, Scheduler } from './scheduler';
import { Coverage, CoverageQueue, coverageDetails, coverageLabel } from './coverage';
import { RetentionStatus, RetentionPreview, retentionSummary, retentionPolicyForMode } from './retention';
import { withPolicySnapshot } from './extractionPolicy';
import { aboveNormal, authorizeLimitIncrease, indexWithLimitApproval, largerLimitsExplanation, LimitsDescription, ScanLimits } from './ingestionLimits';
import { historyControl, HistoryAction } from './history';
import { renderReport, ReportKind } from './report';
import { ControlGroup, DetailStatus, IndexDetail, McpVerification, SidebarState, renderSidebar, sidebarContent, sidebarAction, sidebarControlGroups } from './sidebar';

interface KnownCollection { name: string; root: string; lastIndexed?: string }
class DashboardView implements vscode.WebviewViewProvider, vscode.Disposable {
    private view?: vscode.WebviewView;
    private timer?: NodeJS.Timeout;
    private last = '';
    private listeners: vscode.Disposable[] = [];
    constructor(private readonly controller: Controller) {}
    get visible(): boolean { return this.view?.visible ?? false; }
    resolveWebviewView(view: vscode.WebviewView): void {
        this.listeners.forEach(listener => listener.dispose());
        this.view = view; this.last = '';
        view.webview.options = { enableScripts: true, localResourceRoots: [] };
        view.webview.html = renderSidebar(this.controller.sidebarState());
        this.listeners = [view.webview.onDidReceiveMessage(message =>
            this.controller.sidebarMessage(message).catch(error => {
                this.controller.output.appendLine(String(error));
                void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error ? error.message : String(error)}`);
            })), view.onDidChangeVisibility(() => { if (view.visible) { this.refresh(); } }),
        view.onDidDispose(() => { if (this.view === view) { this.view = undefined; } })];
    }
    refresh(): void {
        if (!this.visible || this.timer) { return; }
        this.timer = setTimeout(() => {
            this.timer = undefined;
            if (!this.visible) { return; }
            const html = sidebarContent(this.controller.sidebarState());
            if (html !== this.last) { this.last = html; void this.view!.webview.postMessage({ type: 'render', html }); }
        }, 100);
    }
    dispose(): void { clearTimeout(this.timer); this.listeners.forEach(listener => listener.dispose()); this.view = undefined; }
}
interface PruneReport { dry_run: boolean; reclaimable_bytes: number; removed_bytes: number; candidates: string[]; pinned: string[]; unreferenced_objects?: number; receipt_cleanup_pending?: boolean }
class CollectionItem extends vscode.TreeItem {
    constructor(readonly collection: string, readonly dataDir: string, readonly active: boolean, removable: boolean, name: string,
        state: string, coverage: string, bytes: number | undefined, lastIndexed: string | undefined, root: string | undefined) {
        super(name, vscode.TreeItemCollapsibleState.None);
        this.contextValue = active ? 'airkActiveCollection' : removable ? 'airkInactiveCollection' : 'airkUnavailableCollection';
        const size = bytes == null ? 'size unknown' : `${(bytes / 1048576).toFixed(2)} MiB`;
        const last = lastIndexed ? new Date(lastIndexed).toLocaleString() : 'never';
        this.description = `${state} · ${coverage} · ${size} · indexed ${last}`;
        this.tooltip = `${name}\n${root ?? collection}\nState: ${state}\nCoverage: ${coverage}\nDisk use: ${size}\nLast indexed: ${last}`;
        this.iconPath = new vscode.ThemeIcon(state === 'paused' ? 'debug-pause' : state === 'indexed' ? 'database' : 'clock');
    }
}
class CollectionTree implements vscode.TreeDataProvider<CollectionItem> {
    readonly changed = new vscode.EventEmitter<void>();
    readonly onDidChangeTreeData = this.changed.event;
    constructor(private readonly controller: Controller) {}
    getTreeItem(item: CollectionItem): vscode.TreeItem { return item; }
    getChildren(): Promise<CollectionItem[]> { return this.controller.collectionTreeItems(); }
    refresh(): void { this.changed.fire(); }
    dispose(): void { this.changed.dispose(); }
}
const locations: Record<Client, string> = {
    vscode: 'VS Code: use MCP: List Servers to enable the registered AIRK collection, or merge into .vscode/mcp.json.',
    claude: 'Install project .mcp.json, then restart Claude Code and review the server in /mcp.',
    codex: 'Codex: merge the TOML table into .codex/config.toml for a trusted project or your user config.toml, then check its MCP server list.',
    cursor: 'Cursor: merge into .cursor/mcp.json and enable it in Cursor MCP settings.',
    gemini: 'Install .gemini/settings.json, then restart Gemini CLI and check /mcp. Existing allow/exclude and approval policies remain in force.',
    antigravity: 'Install .agents/mcp_config.json for this project. In Antigravity IDE, open MCP Servers → Manage MCP Servers to reload; in the CLI use /mcp reload and /mcp list.',
    cline: 'Cline: open Configure MCP Servers and merge the entry into the settings file that Cline opens.',
    roo: 'Roo Code: merge into .roo/mcp.json using Edit Project MCP.',
    junie: 'Junie: merge into .junie/mcp/mcp.json, then check /mcp.',
    'copilot-cli': 'Copilot CLI: merge into your MCP configuration, preserving existing server entries and tool policies.',
    opencode: 'OpenCode: merge into opencode.json under mcp. Verify your installed version uses this schema.',
    'opencode-v2': 'OpenCode V2: merge into the installed version\'s configuration under mcp.servers.',
    kilo: 'Kilo Code: merge into kilo.json under mcp, then check Settings → MCP.',
    'amazon-q': 'Amazon Q Developer: use Tools → Add stdio server and supply these command/args details. This is connection guidance, not a complete default.json agent configuration.',
    windsurf: 'Windsurf/Devin: identify the installed agent/version and open its MCP configuration UI. Supply these command/args details; legacy Cascade and Devin Local use different configuration schemas.'
};

class Controller implements vscode.Disposable {
    readonly output = vscode.window.createOutputChannel('AIRK');
    readonly status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 40);
    readonly definitions = new vscode.EventEmitter<void>();
    readonly collectionTree = new CollectionTree(this);
    readonly sidebar = new DashboardView(this);
    readonly scheduler: Scheduler;
    private readonly coverage: CoverageQueue;
    private readonly nativeWrites = new NativeWriteQueue();
    private readonly retentionViews = new Map<string, RetentionStatus>();
    private readonly rescan: NodeJS.Timeout;
    private readonly maintenanceTimer: NodeJS.Timeout;
    private maintenanceBusy = false;
    private maintenanceCursor = 0;
    private specs = new Map<string, LaunchSpec>();
    private rootBindings: { opened: string; canonical: string }[] = [];
    private helper?: Promise<string>;
    private refreshRevision = 0;
    private refreshing = false;
    private disposed = false;
    private storageChoice?: Promise<void>;
    private indexErrors = new Map<string, string>();
    private detailViews = new Map<string, DetailStatus>();
    private mcpChecks = new Map<string, McpVerification & { result?: { tools: string[]; status: unknown } }>();
    private mcpPending = new Map<string, Promise<void>>();
    private treeUsage?: { dataDir: string; updated: number; entries: Usage[] };

    constructor(private readonly context: vscode.ExtensionContext) {
        this.status.command = 'airk.menu'; this.status.show();
        this.scheduler = new Scheduler(this.options(), (spec, full, automatic) => this.index(spec, full, automatic),
            () => this.updateStatus(), (spec, error) => {
                this.indexErrors.set(JSON.stringify([spec.dataDir, spec.collection]), String(error));
                this.output.appendLine(`${spec.name}: ${String(error)}`);
            });
        this.coverage = new CoverageQueue(spec => this.writeNative<Coverage>(spec, 'coverage', ['--refresh']), () => this.updateStatus());
        this.configure();
        this.rescan = setInterval(() => {
            if (vscode.workspace.isTrusted) { for (const spec of this.specs.values()) { this.coverage.request(spec); } }
        }, 300000);
        this.maintenanceTimer = setInterval(() => { void this.continueMaintenance(); }, 30000);
    }
    private async continueMaintenance(): Promise<void> {
        if (this.disposed || !vscode.workspace.isTrusted || this.refreshing || this.storageChoice || this.maintenanceBusy || !this.scheduler.canRunMaintenance) { return; }
        const specs = [...this.specs.values()];
        if (!specs.length) { return; }
        const spec = specs[this.maintenanceCursor++ % specs.length];
        this.maintenanceBusy = true;
        try {
            await this.scheduler.holdWhile(async idle => {
                await idle;
                await this.nativeWrites.run(spec, async () => {
                    if (this.disposed || !vscode.workspace.isTrusted || this.storageChoice
                        || this.specs.get(spec.collection)?.dataDir !== spec.dataDir) { return; }
                    // Native code rechecks policy/pending state under the collection
                    // writer lock, including changes made by another VS Code window.
                    const result = await runJson<unknown>(spec, 'collections', ['retention', 'continue']);
                    if (result) { await this.refreshRetention(spec); this.invalidateCollectionTree(); }
                });
            });
        } catch (error) {
            if (!this.disposed) { this.output.appendLine(`${spec.name}: automatic cleanup continuation failed: ${String(error)}`); }
        } finally { this.maintenanceBusy = false; }
    }
    private trusted(): void {
        if (!vscode.workspace.isTrusted) { throw new Error('Trust this workspace before starting AIRK or indexing files.'); }
    }
    private writeNative<T>(spec: LaunchSpec, command: string, extra: string[] = []): Promise<T> {
        return this.nativeWrites.run(spec, async () => {
            this.trusted();
            if (this.disposed) { throw new Error('AIRK extension closed.'); }
            return runJson<T>(spec, command, extra);
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
            maxDelayMs: number('index.maxDelayMs', 30000, 1000, 300000), minIntervalMs: number('index.minIntervalMs', 5000, 0, 300000) };
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
        this.helper ??= prepareHelper(this.context.extensionPath, this.context.globalStorageUri.fsPath, this.context.extension.packageJSON.version)
            .catch(error => { this.helper = undefined; throw error; });
        return this.helper;
    }
    async refresh(): Promise<void> {
        const revision = ++this.refreshRevision;
        if (!vscode.workspace.isTrusted) {
            this.mcpChecks.clear();
            for (const spec of this.specs.values()) { this.scheduler.forget(spec.collection); this.coverage.forget(spec.collection); }
            this.specs.clear(); this.rootBindings = []; this.definitions.fire(); this.updateStatus(); return;
        }
        this.refreshing = true;
        try {
            const command = await this.command(), dataDir = await this.dataDir();
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
            const mcpBindings = new Set([...next.values()].map(spec => this.mcpKey(spec)));
            for (const key of this.mcpChecks.keys()) { if (!mcpBindings.has(key)) { this.mcpChecks.delete(key); } }
            this.rootBindings = bindings.sort((a, b) => b.opened.length - a.opened.length);
            const known = this.context.globalState.get<Record<string, KnownCollection>>('collections', {});
            for (const spec of next.values()) {
                known[spec.collection] = { ...known[spec.collection], name: spec.name, root: spec.root };
                if (!prior.has(spec.collection) || prior.get(spec.collection)?.dataDir !== spec.dataDir) {
                    this.coverage.request(spec);
                    if (this.options().mode !== 'manual') { void this.scheduler.request(spec); }
                }
            }
            await this.context.globalState.update('collections', known);
            await this.rememberStorage(dataDir);
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
        const summary = !vscode.workspace.isTrusted ? 'Trust required' : this.refreshing ? 'Starting'
            : state?.state === 'indexing' ? 'Indexing' : state?.state === 'capacity-limited' ? 'Storage budget'
            : state?.state === 'failed' ? 'Failed' : state?.state === 'paused' ? 'Paused' : !spec ? this.specs.size ? 'Choose folder' : 'Open a folder'
            : coverageLabel(view || undefined);
        this.status.text = `$(database) AIRK: ${summary}`;
        this.collectionTree.refresh();
        this.sidebar.refresh();
        const unsaved = spec ? (vscode.workspace.textDocuments ?? []).filter(document => document.isDirty && eligibleEvent(this.sourcePath(document.uri.fsPath), spec.root, spec.dataDir)).length : 0;
        this.status.tooltip = spec ? coverageDetails(spec.name, view || undefined, unsaved)
            + `\n${retentionSummary(this.retentionViews.get(JSON.stringify([spec.dataDir, spec.collection])))}`
            + `\nAuto-indexing: ${this.scheduler?.isPaused ? 'paused' : this.options().mode}${state?.pending ? '; indexing pending' : ''}\nClick to open AIRK controls.`
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
        if (!vscode.workspace.isTrusted) { return; }
        const mode = this.options().mode;
        const source = this.sourcePath(uri.fsPath);
        for (const spec of this.specs.values()) {
            if (eligibleEvent(source, spec.root, spec.dataDir)) {
                this.coverage.request(spec);
                if (mode !== 'onSave' || saved) { void this.scheduler.request(spec); }
            }
        }
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
        const maxBytes = this.machineValue('storage.maxBytes', 0);
        if (automatic && maxBytes > 0) {
            const usage = await runJson<Usage[]>(spec, 'collections', ['list']);
            if (usage.reduce((bytes, item) => bytes + item.disk_bytes, 0) >= maxBytes) {
                throw new CapacityError('Index storage budget (airk.storage.maxBytes) reached; automatic indexing stopped. Trim old generations, remove unused collections or raise the budget, then run Resume Auto-indexing. Index Now still works.');
            }
        }
        let report: IndexReport;
        try {
            report = await indexWithLimitApproval(
                () => this.writeNative<IndexReport>(spec, 'index', ['--scheduled', ...(automatic ? ['--automatic'] : []), ...(full ? ['--full'] : [])]),
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
            if (error instanceof Error && /storage quota exhausted|quota_blocked|database or disk is full|private catalog cannot grow|physical.*(capacity|space|headroom)|insufficient.*space/i.test(error.message)) {
                throw new CapacityError(`${error.message} Automatic indexing is stopped for this collection. Review Storage limits and retention, reclaim eligible old generations or increase the quota, then Resume or Index now.`);
            }
            if (error instanceof Error && error.message.includes('native automatic scheduling is paused')) { throw new PausedError(error.message); }
            throw error;
        }
        finally { if (this.specs.get(spec.collection)?.dataDir === spec.dataDir) { this.coverage.request(spec, true); } }
        this.output.appendLine(`${spec.name}: generation ${report.generation}; indexed ${report.indexed}, reused ${report.reused}, removed ${report.removed}, skipped ${report.skipped.length + (report.skipped_truncated ?? 0)}`);
        for (const skipped of report.skipped.slice(0, 20)) { this.output.appendLine(`Skipped ${skipped.path}: ${skipped.reason}`); }
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections', {});
        known[spec.collection] = { name: spec.name, root: spec.root, lastIndexed: new Date().toISOString() };
        await this.context.globalState.update('collections', known);
        await this.refreshRetention(spec);
        await this.refreshDetail(spec);
        this.invalidateCollectionTree();
        return report;
    }
    invalidateCollectionTree(): void {
        this.treeUsage = undefined; this.collectionTree.refresh();
        if (this.sidebar.visible) { void this.collectionTreeItems().then(() => this.sidebar.refresh()).catch(error => this.output.appendLine(String(error))); }
    }
    sidebarState(): SidebarState {
        const states = this.scheduler.states();
        const known = vscode.workspace.isTrusted ? this.context.globalState.get<Record<string, KnownCollection>>('collections', {}) : {};
        return { trusted: vscode.workspace.isTrusted, paused: this.scheduler.isPaused || states.some(state => state.state === 'paused'),
            registrationEnabled: this.registrationEnabled(),
            mode: this.options().mode, changingStorage: !!this.storageChoice, groups: this.controlGroups(),
            collections: vscode.workspace.isTrusted ? [...this.specs.values()].map(spec => ({ ...spec,
                state: states.find(state => state.collection === spec.collection)?.state ?? 'queued',
                coverage: this.coverage.get(spec.collection), lastIndexed: known[spec.collection]?.lastIndexed,
                diskBytes: this.treeUsage?.dataDir === spec.dataDir ? this.treeUsage.entries.find(item => item.collection === spec.collection)?.disk_bytes : undefined,
                error: this.indexErrors.get(JSON.stringify([spec.dataDir, spec.collection])), mcp: this.mcpChecks.get(this.mcpKey(spec)), detail: this.detailViews.get(this.mcpKey(spec)) })) : [] };
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
        if (action.type === 'detail') { await this.indexDetail(spec, action.level); return; }
        if (action.action === 'verify') { await this.verify(spec); return; }
        if (action.action === 'mcpDetails') {
            const check = this.mcpChecks.get(this.mcpKey(spec));
            if (check) { await this.showJson('AIRK helper verification; agent connection status is separate', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, ...check }); }
            return;
        }
        if (action.action === 'status') { await this.collectionStatus(spec); return; }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: `Indexing ${spec.name}` },
            () => this.scheduler.request(spec, true, action.action === 'reindex'));
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
            .map(spec => this.refreshDetail(spec)));
        const byId = new Map(usage.map(item => [item.collection, item]));
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections', {});
        const states = new Map(this.scheduler.states().map(item => [item.collection, item]));
        const ids = new Set([...this.specs.keys(), ...byId.keys()]);
        return [...ids].map(id => {
            const spec = this.specs.get(id), entry = byId.get(id), view = this.coverage.get(id), scheduler = states.get(id);
            const active = !!spec && spec.dataDir === dataDir;
            const state = scheduler?.state === 'failed' ? 'failed'
                : active && (this.scheduler.isPaused || scheduler?.state === 'paused') ? 'paused'
                : active && scheduler?.state === 'indexing' ? 'indexing'
                : active && (scheduler?.pending || view?.pending) ? 'pending'
                : entry?.status === 'indexed' ? 'indexed' : 'pending';
            const coverage = active ? coverageLabel(view) : 'coverage unavailable';
            return new CollectionItem(id, dataDir, active, !active && entry?.status === 'indexed', spec?.name || known[id]?.name || id,
                state, coverage, entry?.disk_bytes, known[id]?.lastIndexed, spec?.root || known[id]?.root);
        }).sort((a, b) => Number(b.active) - Number(a.active) || String(a.label).localeCompare(String(b.label)));
    }
    async treeIndex(item: CollectionItem, full: boolean): Promise<void> {
        this.trusted();
        const spec = this.specs.get(item.collection);
        if (!item.active || !spec || spec.dataDir !== item.dataDir) { throw new Error('Collection is no longer active in this window.'); }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification,
            title: `${full ? 'Reindexing' : 'Indexing'} ${spec.name}` }, () => this.scheduler.request(spec, true, full));
    }
    async treeRemove(item: CollectionItem): Promise<void> {
        this.trusted();
        if (item.active || item.dataDir !== await this.dataDir()) { throw new Error('Only an inactive collection in current storage can be removed.'); }
        await this.collections(true, item.collection);
    }
    async manualIndex(full = false): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: `${full ? 'Reindexing' : 'Indexing'} ${spec.name}` },
            () => this.scheduler.request(spec, true, full));
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
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections', {});
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
    async prune(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        if (this.scheduler.isBusy) { throw new Error('Wait for indexing to finish before trimming old generations.'); }
        const preview = await this.writeNative<PruneReport>(spec, 'collections', ['prune']);
        await this.showJson('AIRK generation trim preview', preview);
        if (!preview.candidates.length && !preview.unreferenced_objects && !preview.receipt_cleanup_pending) { void vscode.window.showInformationMessage('No unpinned obsolete generations or unreferenced objects can be reclaimed.'); return; }
        const objects = preview.unreferenced_objects ? ` and ${preview.unreferenced_objects} unreferenced objects` : '';
        const pending = preview.receipt_cleanup_pending ? ' Receipt cleanup or further bounded inspection remains pending.' : '';
        const confirm = await vscode.window.showWarningMessage(`Reclaim ${(preview.reclaimable_bytes / 1048576).toFixed(2)} MiB by trimming ${preview.candidates.length} superseded index generations${objects} from ${spec.name}?${pending} The current index and any generation an agent is still reading are kept.`, { modal: true }, 'Trim old generations');
        if (confirm !== 'Trim old generations') { return; }
        await this.showJson('AIRK generation trim result', await this.writeNative<PruneReport>(spec, 'collections', ['prune', '--apply']));
        if (!this.scheduler.isPaused) { this.scheduler.resume(); }
    }
    private async refreshRetention(spec: LaunchSpec): Promise<RetentionStatus | undefined> {
        const key = JSON.stringify([spec.dataDir, spec.collection]);
        try {
            this.trusted();
            const status = await runJson<RetentionStatus>(spec, 'collections', ['retention', 'show']);
            this.retentionViews.set(key, status); this.updateStatus(); return status;
        } catch (error) {
            this.retentionViews.delete(key); this.updateStatus();
            this.output.appendLine(`${spec.name}: retention status unavailable: ${String(error)}`);
            return undefined;
        }
    }
    async retention(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const status = await this.refreshRetention(spec);
        this.showReport('retention', 'AIRK retention status', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir,
            retention: status ?? null, statusUnavailable: !status,
            explanation: 'Retention concerns searchable index revisions. History and checkpoint controls manage retained committed evidence separately. Reader leases and the active revision remain protected. Indexing mode is separate from retention mode.' });
    }
    async history(action: HistoryAction): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        this.trusted();
        await historyControl({
            collection: `${spec.name} (${spec.collection})`,
            read: <T>(args: string[]) => { this.trusted(); return runJson<T>(spec, 'collections', args); },
            write: <T>(args: string[]) => this.writeNative<T>(spec, 'collections', args),
            show: (title, value) => this.showJson(title, { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, result: value }),
            confirm: async (message, action) => await vscode.window.showWarningMessage(message, { modal: true }, action) === action,
            pick: async (choices, title) => (await vscode.window.showQuickPick(choices, { title }))?.value,
            input: (prompt, value) => Promise.resolve(vscode.window.showInputBox({ prompt, value, ignoreFocusOut: true }))
        }, action);
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
    private async refreshDetail(spec: LaunchSpec): Promise<void> {
        try {
            const result = await runJson<DetailStatus>(spec, 'collections', ['extraction', 'show']);
            if (!['essential', 'balanced', 'detailed'].includes(result.configured_detail)
                || (result.published_detail !== null && !['essential', 'balanced', 'detailed'].includes(result.published_detail))) {
                throw new Error('Index detail requires the updated AIRK helper.');
            }
            if (vscode.workspace.isTrusted && this.specs.get(spec.collection)?.dataDir === spec.dataDir) {
                this.detailViews.set(this.mcpKey(spec), result);
            }
        } catch (error) { this.output.appendLine(`Index detail unavailable: ${String(error)}`); }
    }
    async indexDetail(spec: LaunchSpec, level: IndexDetail): Promise<void> {
        this.trusted();
        const confirm = await vscode.window.showWarningMessage(
            `Use ${level} detail for ${spec.name}? Essential keeps text and citations; Balanced adds declarations, imports and document facts; Detailed also keeps call occurrences and document change tracking. Rebuilding takes time and temporary disk space. The current index stays available until success.`,
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
        if (confirm === 'Save and index') { await this.scheduler.request(spec, true, false); }
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
        const collectionInput = await vscode.window.showInputBox({ title: `Storage quota for ${spec.name}`, prompt: 'Collection limit in GiB, including retained generations and temporary builds. Default: 100 GiB.', value: String((current.collections[spec.collection] ?? current.default_collection_bytes) / gib), validateInput, ignoreFocusOut: true });
        if (collectionInput === undefined) { return; }
        const rootInput = await vscode.window.showInputBox({ title: 'Shared AIRK storage quota', prompt: 'Total limit in GiB for all collections in this storage folder. Default: 500 GiB.', value: String(current.root_bytes / gib), validateInput, ignoreFocusOut: true });
        if (rootInput === undefined) { return; }
        if (validateInput(collectionInput) || validateInput(rootInput)) { throw new Error('Invalid storage quota.'); }
        const proposed = { ...current, root_bytes: Number(rootInput) * gib, collections: { ...current.collections, [spec.collection]: Number(collectionInput) * gib } };
        if (proposed.collections[spec.collection] > proposed.root_bytes) { throw new Error('The collection quota cannot exceed the shared storage quota.'); }
        const message = `Save ${collectionInput} GiB for ${spec.name} and ${rootInput} GiB total? Automatic retention may trim eligible old generations before a build or after storage pressure. Active indexes, readers and protected history remain safe. If lowering a limit, an automatic-retention cleanup pass may run before saving; protected allocations can prevent the reduction. No disk space is preallocated.`;
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
                    const retention = await this.refreshRetention(spec);
                    if (retention?.policy.policy.mode === 'automatic') { await this.writeNative(spec, 'collections', ['retention', 'run', '--apply']); }
                }
                await this.showJson('AIRK saved storage quotas', await this.writeNative(spec, 'collections', ['quota', 'set', '--json', JSON.stringify(proposed), '--expected-json', JSON.stringify(current)]));
            });
        });
        this.invalidateCollectionTree();
    }
    async retentionPolicy(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const status = await this.refreshRetention(spec); if (!status) { throw new Error('Cannot configure retention without current policy status.'); }
        const choice = await vscode.window.showQuickPick([
            { label: 'Manual retention', description: 'Keep generations until explicit maintenance; no quota-triggered cleanup.', mode: 'manual' as const },
            { label: 'Automatic retention', description: 'Bounded cleanup after publications; protected readers may delay convergence.', mode: 'automatic' as const }
        ], { title: `Retention policy for ${spec.name} (${spec.collection}) · current ${status.policy.policy.mode}` });
        if (!choice) { return; }
        const generations = await vscode.window.showQuickPick(Array.from({ length: 9 }, (_, predecessors) => ({
            label: predecessors === 0 ? '1 generation: active only (default)' : `${predecessors + 1} generations: active + ${predecessors} previous`,
            description: predecessors === status.policy.policy.searchable_predecessors ? 'Current setting' : undefined,
            predecessors
        })), { title: 'Completed generations to keep; live readers and protected history may retain more' });
        if (!generations) { return; }
        const json = JSON.stringify({ ...retentionPolicyForMode(status, choice.mode), searchable_predecessors: generations.predecessors });
        const preview = await runJson<RetentionPreview>(spec, 'collections', ['retention', 'preview', '--json', json]);
        await this.showJson('AIRK proposed retention policy', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, preview });
        const confirm = await vscode.window.showWarningMessage(`Set ${choice.mode} retention and keep ${generations.predecessors + 1} completed generation(s) for ${spec.name} (${spec.collection})? This persists the previewed policy without immediately deleting generations.`, { modal: true }, 'Save retention policy');
        if (confirm !== 'Save retention policy') { return; }
        await this.writeNative(spec, 'collections', ['retention', 'set', '--json', json]);
        await this.refreshRetention(spec);
    }
    async retentionRun(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const preview = await runJson<RetentionPreview>(spec, 'collections', ['retention', 'preview']);
        await this.showJson('AIRK retention maintenance preview', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, preview });
        const estimate = preview.estimated_reclaimable_bytes == null ? 'unknown reclaimable bytes' : `${preview.estimated_reclaimable_bytes} estimated reclaimable bytes`;
        const legacy = preview.legacy_candidates?.length ? ` Includes ${preview.legacy_candidates.length} legacy revision candidates identified by their original digests.` : '';
        const confirm = await vscode.window.showWarningMessage(`Run one bounded retention pass for ${spec.name} (${spec.collection}), ${estimate}?${legacy} Protected readers may leave pending work. Source files are retained.`, { modal: true }, 'Run retention maintenance');
        if (confirm !== 'Run retention maintenance') { return; }
        await this.showJson('AIRK retention maintenance result', await this.writeNative(spec, 'collections', ['retention', 'run', '--apply']));
        await this.refreshRetention(spec);
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
        const retention = await this.refreshRetention(spec);
        this.showReport('coverage', 'AIRK workspace coverage', { coverage, retention: retention ?? null, scheduling: this.scheduler.states(), dataDir: spec.dataDir,
            explanation: 'Coverage compares saved eligible files with indexed hashes at the inventory time. Ignored and unsupported files are excluded; extraction failures remain eligible. A partial scan cannot establish a percentage.' });
    }
    async usage(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        this.showReport('usage', 'AIRK local usage and estimated context reduction', {
            ...await runJson<object>(spec, 'usage', ['report']),
            explanation: 'Local aggregate activity and counterfactual context estimates; these are not measured provider tokens or billed savings.' });
    }
    async setupAgent(): Promise<void> {
        this.trusted();
        const selected = await vscode.window.showQuickPick(agentChoices(), {
            title: 'Connect a coding agent to AIRK', placeHolder: 'Which agent should use this workspace index?', matchOnDescription: true
        });
        if (!selected) { return; }
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const client = selected.client;
        const panel = vscode.window.createWebviewPanel('airk.connection', `AIRK → ${selected.label}`, vscode.ViewColumn.Active,
            { enableScripts: true, localResourceRoots: [] });
        let status = client === 'vscode' ? `VS Code registration is ${this.registrationEnabled() ? 'on' : 'off'}. Agent connection has not been verified.`
            : 'Configuration has not been installed by this guide.';
        let busy = false, closed = false;
        const render = () => { panel.webview.html = renderConnectionGuide(client, spec, locations[client], status, !!vscode.env.remoteName, busy); };
        const checkCurrent = () => {
            this.trusted();
            const current = this.specs.get(spec.collection);
            if (closed || this.storageChoice || this.refreshing || this.disposed || !current || this.mcpKey(current) !== this.mcpKey(spec)) {
                throw new Error('Workspace or storage changed. Close this guide and run Connect Coding Agent again.');
            }
        };
        const configDocument = (destination: string) => {
            const samePath = (left: string, right: string) => process.platform === 'win32'
                ? path.resolve(left).toLowerCase() === path.resolve(right).toLowerCase() : path.resolve(left) === path.resolve(right);
            const matches = vscode.workspace.textDocuments.filter(document => samePath(document.uri.fsPath, destination)
                || this.rootBindings.some(binding => samePath(binding.canonical, spec.root)
                    && samePath(document.uri.fsPath, path.join(binding.opened, path.relative(spec.root, destination)))));
            if (matches.some(document => document.isDirty)) { throw new Error('Agent configuration has unsaved edits. Save and review it before installing AIRK.'); }
            return matches[0];
        };
        render();
        const listener = panel.webview.onDidReceiveMessage(async value => {
            const action = connectionAction(value, client);
            if (!action || busy || closed) { return; }
            busy = true;
            try {
                checkCurrent();
                if (action === 'copy' || action === 'copyPrompt') {
                    await vscode.env.clipboard.writeText(action === 'copy' ? configFor(client, spec) : verificationPrompt(spec));
                    status = action === 'copy' ? 'Configuration copied. Merge it into the selected agent’s configuration; copying does not install it.' : 'Verification prompt copied. Send it in the selected agent to check its actual connection.';
                } else if (action === 'createCodex') {
                    if (vscode.env.remoteName) { throw new Error('Use manual configuration on the host running Codex.'); }
                    const destination = codexConfigPath(spec);
                    configDocument(destination);
                    await createCodexConfig(spec);
                    status = 'Codex project configuration created. Restart Codex and verify its tools below. Agent connection has not been verified.';
                } else if (action === 'installJson' && isJsonClient(client)) {
                    if (vscode.env.remoteName) { throw new Error('Use manual configuration on the host running this agent.'); }
                    const checkInstall = () => { checkCurrent(); configDocument(jsonConfigPath(client, spec)); };
                    await installJsonConfig(client, spec, checkInstall, async (destination, before, after) => {
                        const document = configDocument(destination) ?? await vscode.workspace.openTextDocument(vscode.Uri.file(destination));
                        const version = document.version;
                        if (document.isDirty || document.getText() !== before) { throw new Error('Configuration has unsaved or external changes. Review it before installing AIRK.'); }
                        const edit = new vscode.WorkspaceEdit();
                        edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(before.length)), after);
                        if (await readJsonConfig(destination) !== before) { throw new Error('Configuration changed on disk. Retry setup.'); }
                        checkInstall();
                        if (document.version !== version || !await vscode.workspace.applyEdit(edit)) { throw new Error('Configuration changed during setup. Review the file and retry.'); }
                        checkCurrent();
                        if (document.getText() !== after || !await document.save()) { throw new Error('Configuration was not saved. Review the editor’s changes and save manually.'); }
                    });
                    status = `${selected.label} project configuration installed. Reload the agent and complete its trust prompts. Agent connection has not been verified.`;
                } else if (action === 'openJson' && isJsonClient(client)) {
                    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(jsonConfigPath(client, spec)));
                    await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: false });
                    status = 'Project configuration opened. Review the AIRK entry and any existing client policies before reloading the agent.';
                } else if (action === 'enableCopilot') {
                    if (typeof vscode.lm?.registerMcpServerDefinitionProvider !== 'function'
                        || !(await vscode.commands.getCommands(true)).includes('workbench.mcp.listServer')) {
                        throw new Error('This editor does not support AIRK’s MCP registration. Update VS Code or use manual MCP configuration.');
                    }
                    checkCurrent();
                    await vscode.workspace.getConfiguration('airk').update('mcp.registrationEnabled', true, vscode.ConfigurationTarget.Global);
                    this.registrationChanged();
                    checkCurrent();
                    await this.manageVscodeMcp();
                    status = 'AIRK registration enabled for open workspace collections. Select this collection in VS Code’s MCP controls and complete any start or trust prompts. Copilot’s connection has not been verified.';
                } else if (action === 'openCodex') {
                    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(codexConfigPath(spec)));
                    await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: false });
                    status = 'Existing configuration opened. Merge the AIRK entry without duplicating its table or changing other settings, save, then restart Codex.';
                } else if (action === 'vscode') {
                    await this.manageVscodeMcp();
                    status = `VS Code MCP controls opened. Registration is ${this.registrationEnabled() ? 'on' : 'off'}; enable it in the AIRK sidebar if needed. Other agents use their own configuration.`;
                } else if (action === 'verify') {
                    status = 'Checking AIRK helper…'; render();
                    await this.verify(spec);
                    const check = this.mcpChecks.get(this.mcpKey(spec));
                    status = check?.phase === 'verified' ? `AIRK helper verified: ${check.tools?.length ?? 0} tools. Agent connection still needs the verification prompt below.`
                        : `AIRK helper check did not succeed: ${check?.error ?? 'workspace or storage changed'}`;
                }
            } catch (error) { status = error instanceof Error ? error.message : String(error); }
            finally { busy = false; if (!closed) { render(); } }
        });
        panel.onDidDispose(() => { closed = true; listener.dispose(); });
        this.context.subscriptions.push(panel);
    }
    private mcpKey(spec: LaunchSpec): string { return JSON.stringify([spec.collection, spec.root, spec.dataDir, spec.command]); }
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
                await this.scheduler.request(spec, true);
                if (!current()) { return; }
                check.phase = 'checking'; this.updateStatus();
                const result = await probeMcp(spec, 16, this.context.extension.packageJSON.version);
                if (!current()) { return; }
                check.phase = 'verified'; check.tools = result.tools; check.result = result; check.checkedAt = Date.now();
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
            const definition = new vscode.McpStdioServerDefinition(`AIRK: ${spec.name} (${spec.collection})`, spec.command, launchArgs(spec), { AIRK_CLIENT: 'vscode' },
                `${this.context.extension.packageJSON.version}:${spec.dataDir}`);
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
        // Existing snapshots remain usable while edits settle or automatic indexing is paused.
        try { await runJson(spec, 'status'); }
        catch (error) {
            if (!(error instanceof Error) || !error.message.includes('workspace is not indexed')) { throw error; }
            await this.scheduler.request(spec, true);
        }
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
    dispose(): void { this.disposed = true; clearInterval(this.rescan); clearInterval(this.maintenanceTimer); this.sidebar.dispose(); this.coverage.dispose(); this.scheduler.dispose(); this.definitions.dispose(); this.collectionTree.dispose(); this.status.dispose(); this.output.dispose(); }
}

export async function activate(context: vscode.ExtensionContext): Promise<{ getServerDefinitions(): Promise<vscode.McpStdioServerDefinition[]> }> {
    const controller = new Controller(context); context.subscriptions.push(controller);
    context.subscriptions.push(vscode.window.createTreeView('airk.collectionsView', { treeDataProvider: controller.collectionTree }));
    context.subscriptions.push(vscode.window.registerWebviewViewProvider('airk.dashboard', controller.sidebar));
    const safe = (action: () => void | Promise<unknown>, propagate = false) => async () => {
        try { await action(); } catch (error) {
            controller.output.appendLine(String(error)); controller.output.show(true);
            void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error ? error.message : String(error)}`);
            if (propagate) { throw error; }
        }
    };
    const commands: Record<string, () => void | Promise<unknown>> = {
        menu: () => controller.menu(), index: () => controller.manualIndex(), reindex: () => controller.manualIndex(true),
        indexMode: () => controller.chooseMode(), pause: () => controller.pauseAuto(true),
        resume: async () => { await controller.refresh(); await controller.pauseAuto(false); },
        chooseStorage: () => controller.chooseStorage(), storageLimits: () => controller.storageLimits(), collections: () => controller.collections(), removeCollection: () => controller.collections(true), prune: () => controller.prune(),
        retention: () => controller.retention(), retentionPolicy: () => controller.retentionPolicy(), retentionRun: () => controller.retentionRun(),
        history: () => controller.history('show'), historyPolicy: () => controller.history('policy'), historyRun: () => controller.history('run'),
        historyAvailable: () => controller.history('available'), checkpoints: () => controller.history('checkpoints'), createCheckpoint: () => controller.history('create'), removeCheckpoint: () => controller.history('remove'), purgeHistory: () => controller.history('purge'),
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
            if (event.affectsConfiguration('airk.mcp.registrationEnabled')) { controller.registrationChanged(); }
            if (event.affectsConfiguration('airk')) { controller.configure(); void safe(() => controller.refresh())(); }
        }));
    await safe(() => controller.refresh())();
    return { getServerDefinitions: () => controller.provideDefinitions() };
}
