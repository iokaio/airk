// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { Client, IndexReport, LaunchSpec, Usage, clients, collectionId, configFor, eligibleEvent, launchArgs } from './model';
import { NativeWriteQueue, prepareHelper, probeMcp, runJson } from './native';
import { CapacityError, Options, PausedError, Scheduler } from './scheduler';
import { Coverage, CoverageQueue, coverageDetails, coverageLabel } from './coverage';
import { RetentionStatus, RetentionPreview, retentionSummary, retentionPolicyForMode } from './retention';
import { withPolicySnapshot } from './extractionPolicy';
import { historyControl, HistoryAction } from './history';

interface KnownCollection { name: string; root: string; lastIndexed?: string }
interface PruneReport { dry_run: boolean; reclaimable_bytes: number; removed_bytes: number; candidates: string[]; pinned: string[]; unreferenced_objects?: number; receipt_cleanup_pending?: boolean }
const locations: Record<Client, string> = {
    vscode: 'VS Code: use MCP: List Servers to enable the registered AIRK collection, or merge into .vscode/mcp.json.',
    claude: 'Claude Code: merge into project .mcp.json, then review and enable the server in Claude Code.',
    codex: 'Codex: merge the TOML table into .codex/config.toml for a trusted project or your user config.toml, then check its MCP server list.',
    cursor: 'Cursor: merge into .cursor/mcp.json and enable it in Cursor MCP settings.',
    gemini: 'Gemini CLI: merge into .gemini/settings.json, then check /mcp.',
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
    readonly scheduler: Scheduler;
    private readonly coverage: CoverageQueue;
    private readonly nativeWrites = new NativeWriteQueue();
    private readonly retentionViews = new Map<string, RetentionStatus>();
    private readonly rescan: NodeJS.Timeout;
    private specs = new Map<string, LaunchSpec>();
    private rootBindings: { opened: string; canonical: string }[] = [];
    private helper?: Promise<string>;
    private refreshRevision = 0;
    private refreshing = false;
    private disposed = false;

    constructor(private readonly context: vscode.ExtensionContext) {
        this.status.command = 'airk.index'; this.status.show();
        this.scheduler = new Scheduler(this.options(), (spec, full, automatic) => this.index(spec, full, automatic),
            () => this.updateStatus(), (spec, error) => this.output.appendLine(`${spec.name}: ${String(error)}`));
        this.coverage = new CoverageQueue(spec => this.writeNative<Coverage>(spec, 'coverage', ['--refresh']), () => this.updateStatus());
        this.configure();
        this.rescan = setInterval(() => {
            if (vscode.workspace.isTrusted) { for (const spec of this.specs.values()) { this.coverage.request(spec); } }
        }, 300000);
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
            for (const old of this.specs.values()) {
                if (!next.has(old.collection) || next.get(old.collection)?.dataDir !== old.dataDir) { this.scheduler.forget(old.collection); this.coverage.forget(old.collection); }
            }
            const prior = this.specs;
            this.specs = next;
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
        const unsaved = spec ? (vscode.workspace.textDocuments ?? []).filter(document => document.isDirty && eligibleEvent(this.sourcePath(document.uri.fsPath), spec.root, spec.dataDir)).length : 0;
        this.status.tooltip = spec ? coverageDetails(spec.name, view || undefined, unsaved)
            + `\n${retentionSummary(this.retentionViews.get(JSON.stringify([spec.dataDir, spec.collection])))}`
            + `\nAuto-indexing: ${this.scheduler?.isPaused ? 'paused' : this.options().mode}${state?.pending ? '; indexing pending' : ''}\nAIRK: Open Controls is available in the command palette.`
            : 'Select a workspace folder to index. AIRK: Open Controls is available in the command palette.';
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
        const maxBytes = this.machineValue('storage.maxBytes', 0);
        if (automatic && maxBytes > 0) {
            const usage = await runJson<Usage[]>(spec, 'collections', ['list']);
            if (usage.reduce((bytes, item) => bytes + item.disk_bytes, 0) >= maxBytes) {
                throw new CapacityError('Collection storage budget reached. Remove unused collections or increase the budget, then resume indexing.');
            }
        }
        let report: IndexReport;
        try { report = await this.writeNative<IndexReport>(spec, 'index', ['--scheduled', ...(automatic ? ['--automatic'] : []), ...(full ? ['--full'] : [])]); }
        catch (error) {
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
        return report;
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
        const choices = [
            { label: 'After edits settle', mode: 'debounced', description: 'Coalesce filesystem changes, including agent edits' },
            { label: 'When idle', mode: 'idle', description: 'Wait until filesystem and editor activity settle' },
            { label: 'On save', mode: 'onSave', description: 'Batch saved documents; external edits wait for Index Now' },
            { label: 'Manual', mode: 'manual', description: 'Index only when explicitly requested' }
        ];
        const choice = await vscode.window.showQuickPick(choices, { title: 'AIRK indexing mode' });
        if (choice) { await vscode.workspace.getConfiguration('airk').update('index.mode', choice.mode, vscode.ConfigurationTarget.Global); }
    }
    async chooseStorage(): Promise<void> {
        this.trusted();
        if (this.scheduler.isBusy) { throw new Error('Wait for indexing to finish before changing storage.'); }
        const selected = await vscode.window.showOpenDialog({ canSelectFiles: false, canSelectFolders: true, canSelectMany: false, title: 'Choose AIRK index storage; existing collections remain in the previous location' });
        if (!selected?.[0]) { return; }
        const directory = await fs.realpath(selected[0].fsPath);
        for (const folder of vscode.workspace.workspaceFolders ?? []) {
            const root = await fs.realpath(folder.uri.fsPath), relative = path.relative(directory, root);
            if (!relative || (!relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative))) {
                throw new Error('Index storage must not be the workspace root or contain a workspace root.');
            }
        }
        await this.rememberStorage(await this.dataDir());
        await vscode.workspace.getConfiguration('airk').update('storage.path', directory, vscode.ConfigurationTarget.Global);
        await this.refresh();
        void vscode.window.showInformationMessage('AIRK storage selected. Old collections remain available through Manage Collections; restart already-connected agents to use the new location.');
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
    async collections(remove = false): Promise<void> {
        const spec = await this.maintenanceSpec(); if (!spec) { return; }
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
        const selected = await vscode.window.showQuickPick(candidates.map(item => ({ label: known[item.collection]?.name || item.collection,
            description: `${(item.disk_bytes / 1048576).toFixed(2)} MiB · ${item.collection}`, detail: known[item.collection]?.root || item.status, item })), { title: 'Remove an unneeded AIRK collection' });
        if (!selected) { return; }
        const confirm = await vscode.window.showWarningMessage(`Remove derived index ${selected.item.collection}? Disconnect its MCP workers and close that project in other windows first. Source files are retained.`, { modal: true }, 'Remove index');
        if (confirm !== 'Remove index') { return; }
        await runJson(spec, 'collections', ['remove', selected.item.collection]);
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
        const confirm = await vscode.window.showWarningMessage(`Trim ${preview.candidates.length} obsolete generations${objects} from ${spec.name} (${(preview.reclaimable_bytes / 1048576).toFixed(2)} MiB)?${pending} Active evidence and pinned readers are protected.`, { modal: true }, 'Trim old generations');
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
        await this.showJson('AIRK retention status', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir,
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
    async retentionPolicy(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const status = await this.refreshRetention(spec); if (!status) { throw new Error('Cannot configure retention without current policy status.'); }
        const choice = await vscode.window.showQuickPick([
            { label: 'Manual retention', description: 'Keep generations until explicit maintenance; no quota-triggered cleanup.', mode: 'manual' as const },
            { label: 'Automatic retention', description: 'Bounded cleanup after publications; protected readers may delay convergence.', mode: 'automatic' as const }
        ], { title: `Retention policy for ${spec.name} (${spec.collection}) · current ${status.policy.policy.mode}` });
        if (!choice) { return; }
        const json = JSON.stringify(retentionPolicyForMode(status, choice.mode));
        const preview = await runJson<RetentionPreview>(spec, 'collections', ['retention', 'preview', '--json', json]);
        await this.showJson('AIRK proposed retention policy', { collection: spec.collection, root: spec.root, dataDir: spec.dataDir, preview });
        const confirm = await vscode.window.showWarningMessage(`Set ${choice.mode} retention for ${spec.name} (${spec.collection})? This persists the previewed policy without immediately deleting generations.`, { modal: true }, 'Save retention policy');
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
    async collectionStatus(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const coverage = await this.writeNative<Coverage>(spec, 'coverage', ['--refresh']);
        const retention = await this.refreshRetention(spec);
        await this.showJson('AIRK workspace coverage', { coverage, retention: retention ?? null, scheduling: this.scheduler.states(), dataDir: spec.dataDir,
            explanation: 'Coverage compares saved eligible files with indexed hashes at the inventory time. Ignored and unsupported files are excluded; extraction failures remain eligible. A partial scan cannot establish a percentage.' });
    }
    async usage(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await this.showJson('AIRK local usage and estimated context reduction', {
            ...await runJson<object>(spec, 'usage', ['report']),
            explanation: 'Local aggregate activity and counterfactual context estimates; these are not measured provider tokens or billed savings.' });
    }
    async setupAgent(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        const selected = await vscode.window.showQuickPick([...clients], { title: 'Coding agent to connect to AIRK' });
        if (!selected) { return; }
        const client = selected as Client;
        await this.scheduler.request(spec, true);
        const content = configFor(client, spec);
        const document = await vscode.workspace.openTextDocument({ language: client === 'codex' ? 'toml' : 'json', content });
        await vscode.window.showTextDocument(document, { preview: true });
        if (await vscode.window.showInformationMessage(locations[client], 'Copy configuration') === 'Copy configuration') {
            await vscode.env.clipboard.writeText(content);
        }
    }
    async verify(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await this.scheduler.request(spec, true);
        await this.showJson('AIRK native stdio MCP verified; client approval is separate', await probeMcp(spec));
    }
    async provideDefinitions(): Promise<vscode.McpStdioServerDefinition[]> {
        if (!vscode.workspace.isTrusted) { return []; }
        await this.refresh();
        return [...this.specs.values()].map(spec => {
            const definition = new vscode.McpStdioServerDefinition(`AIRK: ${spec.name} (${spec.collection})`, spec.command, launchArgs(spec), { AIRK_CLIENT: 'vscode' },
                `${this.context.extension.packageJSON.version}:${spec.dataDir}`);
            definition.cwd = vscode.Uri.file(spec.root);
            return definition;
        });
    }
    async resolveDefinition(definition: vscode.McpServerDefinition): Promise<vscode.McpServerDefinition> {
        this.trusted();
        if (!(definition instanceof vscode.McpStdioServerDefinition)) { throw new Error('AIRK requires native stdio.'); }
        const spec = [...this.specs.values()].find(item => item.command === definition.command && JSON.stringify(launchArgs(item)) === JSON.stringify(definition.args));
        if (!spec) { throw new Error('AIRK workspace/storage configuration changed. Refresh its MCP definitions.'); }
        // Existing snapshots remain usable while edits settle or automatic indexing is paused.
        try { await runJson(spec, 'status'); }
        catch (error) {
            if (!(error instanceof Error) || !error.message.includes('workspace is not indexed')) { throw error; }
            await this.scheduler.request(spec, true);
        }
        this.trusted(); return definition;
    }
    async menu(): Promise<void> {
        const commands = [
            ['Index Now', 'airk.index'], ['Reindex Collection', 'airk.reindex'], ['Choose Indexing Mode', 'airk.indexMode'],
            ['Pause Auto-indexing', 'airk.pause'], ['Resume Auto-indexing', 'airk.resume'], ['Choose Index Storage', 'airk.chooseStorage'],
            ['Manage Collections and Disk Space', 'airk.collections'], ['Remove Unneeded Collection', 'airk.removeCollection'],
            ['Trim Obsolete Index Generations', 'airk.prune'],
            ['Show Retention Status', 'airk.retention'], ['Choose Retention Policy', 'airk.retentionPolicy'], ['Run Retention Maintenance', 'airk.retentionRun'],
            ['Show History Status', 'airk.history'], ['Choose History Policy', 'airk.historyPolicy'], ['Run History Maintenance', 'airk.historyRun'],
            ['List Retained Publication Identities', 'airk.historyAvailable'], ['Show Checkpoints', 'airk.checkpoints'], ['Create Checkpoint', 'airk.createCheckpoint'], ['Remove Checkpoint', 'airk.removeCheckpoint'], ['Preview Logical History Purge', 'airk.purgeHistory'],
            ['Show Extraction Policy', 'airk.extractionPolicy'], ['Configure Extraction Policy', 'airk.configureExtractionPolicy'],
            ['Set Up Coding Agent', 'airk.setupAgent'], ['Verify Native MCP Connection', 'airk.verifyMcp'], ['Workspace Coverage', 'airk.status'], ['Local Usage and Estimated Savings', 'airk.usage'], ['Show Output', 'airk.logs']
        ];
        const choice = await vscode.window.showQuickPick(commands.map(([label, command]) => ({ label, command })), { title: 'AIRK controls' });
        if (choice) { await vscode.commands.executeCommand(choice.command); }
    }
    configure(): void { const options = this.options(); this.scheduler.configure(options); this.coverage?.configure(options.quietMs, options.mode === 'idle' ? options.idleMs : 0); }
    dispose(): void { this.disposed = true; clearInterval(this.rescan); this.coverage.dispose(); this.scheduler.dispose(); this.definitions.dispose(); this.status.dispose(); this.output.dispose(); }
}

export async function activate(context: vscode.ExtensionContext): Promise<{ getServerDefinitions(): Promise<vscode.McpStdioServerDefinition[]> }> {
    const controller = new Controller(context); context.subscriptions.push(controller);
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
        chooseStorage: () => controller.chooseStorage(), collections: () => controller.collections(), removeCollection: () => controller.collections(true), prune: () => controller.prune(),
        retention: () => controller.retention(), retentionPolicy: () => controller.retentionPolicy(), retentionRun: () => controller.retentionRun(),
        history: () => controller.history('show'), historyPolicy: () => controller.history('policy'), historyRun: () => controller.history('run'),
        historyAvailable: () => controller.history('available'), checkpoints: () => controller.history('checkpoints'), createCheckpoint: () => controller.history('create'), removeCheckpoint: () => controller.history('remove'), purgeHistory: () => controller.history('purge'),
        extractionPolicy: () => controller.extractionPolicy(), configureExtractionPolicy: () => controller.extractionPolicy(true),
        status: () => controller.collectionStatus(), usage: () => controller.usage(), setupAgent: () => controller.setupAgent(), verifyMcp: () => controller.verify(), logs: () => controller.output.show()
    };
    for (const [name, action] of Object.entries(commands)) { context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`, safe(action, true))); }
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
            if (event.affectsConfiguration('airk')) { controller.configure(); void safe(() => controller.refresh())(); }
        }));
    await safe(() => controller.refresh())();
    return { getServerDefinitions: () => controller.provideDefinitions() };
}
