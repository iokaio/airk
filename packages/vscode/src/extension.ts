// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { Client, IndexReport, LaunchSpec, Usage, clients, collectionId, configFor, eligibleEvent, launchArgs } from './model';
import { prepareHelper, probeMcp, runJson } from './native';
import { CapacityError, Options, Scheduler } from './scheduler';

interface KnownCollection { name: string; root: string; lastIndexed?: string }
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
    private specs = new Map<string, LaunchSpec>();
    private helper?: Promise<string>;
    private refreshRevision = 0;
    private refreshing = false;
    private disposed = false;

    constructor(private readonly context: vscode.ExtensionContext) {
        this.status.command = 'airk.menu'; this.status.show();
        this.scheduler = new Scheduler(this.options(), (spec, full, automatic) => this.index(spec, full, automatic),
            () => this.updateStatus(), (spec, error) => this.output.appendLine(`${spec.name}: ${String(error)}`));
    }
    private trusted(): void {
        if (!vscode.workspace.isTrusted) { throw new Error('Trust this workspace before starting AIRK or indexing files.'); }
    }
    private options(): Options {
        const config = vscode.workspace.getConfiguration('airk');
        const mode = config.get<Options['mode']>('index.mode', 'debounced');
        const number = (key: string, fallback: number, min: number, max: number) => Math.max(min, Math.min(max, config.get<number>(key, fallback)));
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
        if (!vscode.workspace.isTrusted) { this.specs.clear(); this.definitions.fire(); this.updateStatus(); return; }
        this.refreshing = true;
        try {
            const command = await this.command(), dataDir = await this.dataDir();
            const next = new Map<string, LaunchSpec>();
            for (const folder of vscode.workspace.workspaceFolders ?? []) {
                if (folder.uri.scheme !== 'file' && !(vscode.env.remoteName && folder.uri.scheme === 'vscode-remote')) { continue; }
                const root = await fs.realpath(folder.uri.fsPath);
                const collection = collectionId(root);
                next.set(collection, { command, root, dataDir, collection, name: folder.name });
            }
            if (revision !== this.refreshRevision || this.disposed) { return; }
            const storageChanged = [...this.specs.values()].some(spec => next.has(spec.collection) && next.get(spec.collection)?.dataDir !== spec.dataDir);
            if (storageChanged && this.scheduler.isBusy) { throw new Error('Wait for the running index job to finish, then refresh AIRK controls.'); }
            for (const old of this.specs.values()) {
                if (!next.has(old.collection) || next.get(old.collection)?.dataDir !== old.dataDir) { this.scheduler.forget(old.collection); }
            }
            const prior = this.specs;
            this.specs = next;
            const known = this.context.globalState.get<Record<string, KnownCollection>>('collections', {});
            for (const spec of next.values()) {
                known[spec.collection] = { ...known[spec.collection], name: spec.name, root: spec.root };
                if (!prior.has(spec.collection) || prior.get(spec.collection)?.dataDir !== spec.dataDir) {
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
        const summary = !vscode.workspace.isTrusted ? 'Trust required' : this.refreshing ? 'Starting' : this.scheduler?.isBusy ? 'Indexing'
            : this.scheduler?.isPaused ? 'Paused' : states.some(s => s.state === 'capacity-limited') ? 'Storage budget'
            : states.some(s => s.state === 'failed') ? 'Failed' : states.some(s => s.pending) ? 'Changes queued'
            : states.some(s => s.state === 'partial') ? 'Partially indexed' : this.options().mode === 'manual' ? 'Manual mode'
            : this.specs.size ? 'Ready' : 'Open a folder';
        this.status.text = `$(database) AIRK: ${summary}`;
        this.status.tooltip = states.map(s => `${s.name}: ${s.state}${s.pending ? ' (changes pending)' : ''}`).join('\n') || 'Open AIRK controls';
    }
    changed(uri: vscode.Uri, saved = false): void {
        if (!vscode.workspace.isTrusted) { return; }
        const mode = this.options().mode;
        if (mode === 'onSave' && !saved) { return; }
        for (const spec of this.specs.values()) {
            if (eligibleEvent(uri.fsPath, spec.root, spec.dataDir)) { void this.scheduler.request(spec); }
        }
    }
    activity(): void { this.scheduler.activity(); }
    async chooseSpec(): Promise<LaunchSpec | undefined> {
        this.trusted(); await this.refresh();
        const specs = [...this.specs.values()];
        if (!specs.length) { throw new Error('Open a filesystem workspace folder to use AIRK.'); }
        if (specs.length === 1) { return specs[0]; }
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
        const report = await runJson<IndexReport>(spec, 'index', full ? ['--full'] : []);
        this.output.appendLine(`${spec.name}: generation ${report.generation}; indexed ${report.indexed}, reused ${report.reused}, removed ${report.removed}, skipped ${report.skipped.length}`);
        for (const skipped of report.skipped.slice(0, 20)) { this.output.appendLine(`Skipped ${skipped.path}: ${skipped.reason}`); }
        const known = this.context.globalState.get<Record<string, KnownCollection>>('collections', {});
        known[spec.collection] = { name: spec.name, root: spec.root, lastIndexed: new Date().toISOString() };
        await this.context.globalState.update('collections', known);
        return report;
    }
    async manualIndex(full = false): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: `${full ? 'Reindexing' : 'Indexing'} ${spec.name}` },
            () => this.scheduler.request(spec, true, full));
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
        this.scheduler.resume();
    }
    async showJson(title: string, value: unknown): Promise<void> {
        const document = await vscode.workspace.openTextDocument({ language: 'json', content: JSON.stringify({ title, ...value as object }, null, 2) });
        await vscode.window.showTextDocument(document, { preview: true });
    }
    async collectionStatus(): Promise<void> {
        const spec = await this.chooseSpec(); if (!spec) { return; }
        await this.showJson('AIRK indexed collection status', { ...await runJson<object>(spec, 'status'), scheduling: this.scheduler.states(), dataDir: spec.dataDir });
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
            const definition = new vscode.McpStdioServerDefinition(`AIRK: ${spec.name} (${spec.collection})`, spec.command, launchArgs(spec), {},
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
            ['Set Up Coding Agent', 'airk.setupAgent'], ['Verify Native MCP Connection', 'airk.verifyMcp'], ['Collection Status', 'airk.status'], ['Show Output', 'airk.logs']
        ];
        const choice = await vscode.window.showQuickPick(commands.map(([label, command]) => ({ label, command })), { title: 'AIRK controls' });
        if (choice) { await vscode.commands.executeCommand(choice.command); }
    }
    configure(): void { this.scheduler.configure(this.options()); }
    dispose(): void { this.disposed = true; this.scheduler.dispose(); this.definitions.dispose(); this.status.dispose(); this.output.dispose(); }
}

export async function activate(context: vscode.ExtensionContext): Promise<{ getServerDefinitions(): Promise<vscode.McpStdioServerDefinition[]> }> {
    const controller = new Controller(context); context.subscriptions.push(controller);
    const safe = (action: () => void | Promise<unknown>) => async () => {
        try { await action(); } catch (error) {
            controller.output.appendLine(String(error)); controller.output.show(true);
            void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error ? error.message : String(error)}`);
        }
    };
    const commands: Record<string, () => void | Promise<unknown>> = {
        menu: () => controller.menu(), index: () => controller.manualIndex(), reindex: () => controller.manualIndex(true),
        indexMode: () => controller.chooseMode(), pause: () => controller.scheduler.pause(),
        resume: async () => { await controller.refresh(); controller.scheduler.resume(); },
        chooseStorage: () => controller.chooseStorage(), collections: () => controller.collections(), removeCollection: () => controller.collections(true),
        status: () => controller.collectionStatus(), setupAgent: () => controller.setupAgent(), verifyMcp: () => controller.verify(), logs: () => controller.output.show()
    };
    for (const [name, action] of Object.entries(commands)) { context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`, safe(action))); }
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
        vscode.workspace.onDidGrantWorkspaceTrust(safe(() => controller.refresh())),
        vscode.workspace.onDidChangeWorkspaceFolders(safe(() => controller.refresh())),
        vscode.workspace.onDidChangeConfiguration(event => {
            if (event.affectsConfiguration('airk')) { controller.configure(); void safe(() => controller.refresh())(); }
        }));
    await safe(() => controller.refresh())();
    return { getServerDefinitions: () => controller.provideDefinitions() };
}
