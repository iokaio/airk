// SPDX-License-Identifier: Apache-2.0
//! Composition root of the split extension (plan §8, §10.3). The extension
//! reaches the AIRK Service only through the verified `airkctl` (I2) found by
//! the install pointer (I4). It ships no native runtime and never installs,
//! repairs, updates or removes the service; management lives in the admin
//! pages (`airkctl admin open`).
import * as vscode from 'vscode';
import * as path from 'node:path';
// Name index modules explicitly: the isolated update loader resolves only `x`, `x.js` and
// `x.json` (extensionUpdateLoader.ts), and the packager applies the same rule.
import { Airkctl, AirkctlCommandError, DOWNLOAD_URL, errorOf, type AirkctlState } from './airkctl/index';
import { systemVerifyHost } from './airkctl/verify';
import { AirkctlToolSession } from './airkctl/tools';
import { AirkctlExplorerHost, describe } from './airkctl/features/explorerHost';
import { ACTION_LABELS, guidance, type GuidanceAction } from './airkctl/status/guidance';
import {
    connectAgent, connectionInfo, connectionUrl, credentialHeader, inputCredential, listAgents, removeAgents, repairAgents,
    reportedConnections, sweepWithoutService, type Client, type RemovedEntry,
} from './airkctl/agents/index';
import { setEvidenceTransport } from './explorerEvidence';
import { ExplorerSidebar } from './explorerSidebar';
import { WorkbenchView } from './workbenchView';
import type { ExplorerController } from './explorerController';
import { nativeExecutionAllowed, setNativeExecutionGate } from './nativeGate';

/** User scope only: a workspace cannot turn native execution back on. */
function nativeExecutionSetting(): boolean {
    return vscode.workspace.getConfiguration('airk').inspect<boolean>('runtime.nativeExecution')?.globalValue !== false;
}
const REMOVED_ENTRIES_KEY = 'airk.agents.removedWithoutService';
const VSCODE_CONNECTION_KEY = 'airk.vscodeMcp.connection';
const INCARNATION = /^[0-9a-f]{32}$/;

class SplitExtension implements vscode.Disposable {
    readonly output = vscode.window.createOutputChannel('AIRK');
    readonly status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
    readonly airkctl: Airkctl;
    readonly host: AirkctlExplorerHost;
    readonly definitions = new vscode.EventEmitter<void>();
    private explorer?: Promise<ExplorerController>;
    private explorerLoaded?: ExplorerController;
    private lastNotified?: string;
    private disposed = false;
    private readonly subscriptions: vscode.Disposable[] = [];

    constructor(private readonly context: vscode.ExtensionContext) {
        const version = String(context.extension?.packageJSON?.version ?? '0.0.0');
        const development = context.extensionMode !== vscode.ExtensionMode.Production;
        const verifyHost = systemVerifyHost(development);
        // Test seam (plan §8.2): a test host injects a fixture installation by
        // naming its home folder. Honored only in Test mode, never in production.
        const fixtureHome = context.extensionMode === vscode.ExtensionMode.Test ? process.env.AIRK_TEST_INSTALL_HOME : undefined;
        this.airkctl = new Airkctl({
            clientVersion: version,
            verifyHost: fixtureHome && path.isAbsolute(fixtureHome) ? { ...verifyHost, home: fixtureHome } : verifyHost,
            gate: () => !vscode.workspace.isTrusted ? 'workspace_untrusted' : !nativeExecutionAllowed() ? 'native_execution_disabled' : undefined,
            session: { label: (vscode.workspace.name ?? 'VS Code window').slice(0, 128) },
            log: line => this.output.appendLine(line),
        });
        this.host = new AirkctlExplorerHost(context, this.airkctl, this.output, this.status);
        this.status.command = 'airk.explorer.open';
        this.status.show();
        this.subscriptions.push(this.output, this.status, this.definitions, this.host,
            this.airkctl.onDidChangeState(state => this.stateChanged(state)));
        setEvidenceTransport(spec => new AirkctlToolSession(this.airkctl.session, {
            ...(spec.service?.project ? { project: spec.service.project } : {}),
            ...(spec.catalogSession ? { catalogSession: spec.catalogSession } : {}),
            ...(spec.combinedSession ? { combinedSession: spec.combinedSession } : {}),
        }));
    }

    private stateChanged(state: AirkctlState): void {
        const advice = guidance(state);
        this.status.text = advice.status; this.status.tooltip = advice.message;
        void vscode.commands.executeCommand('setContext', 'airk.nativeCommandsAvailable', state.kind === 'ready');
        void vscode.commands.executeCommand('setContext', 'airk.serviceState', state.kind);
        this.definitions.fire();
        if (state.kind === 'not_installed') { void this.sweep(); }
        // Notify once per distinct state; the site opens only on a user action.
        const key = JSON.stringify([state.kind, 'code' in state ? state.code : undefined, 'reason' in state ? state.reason : undefined]);
        if (advice.severity === 'none' || key === this.lastNotified) { return; }
        this.lastNotified = key;
        this.notify(advice.severity, advice.message, advice.actions);
    }
    notify(severity: 'info' | 'warning' | 'error', message: string, actions: readonly GuidanceAction[]): void {
        const show = severity === 'error' ? vscode.window.showErrorMessage : severity === 'warning' ? vscode.window.showWarningMessage : vscode.window.showInformationMessage;
        void Promise.resolve(show(`AIRK: ${message}`, ...actions.map(action => ACTION_LABELS[action]))).then(choice => {
            const action = actions.find(item => ACTION_LABELS[item] === choice);
            if (action) { return vscode.commands.executeCommand(action); }
        }).then(undefined, error => this.output.appendLine(String(error)));
    }

    async start(): Promise<void> {
        await this.airkctl.check();
        await this.host.refresh(false);
    }
    folders(): string[] {
        return (vscode.workspace.workspaceFolders ?? []).filter(folder => folder.uri.scheme === 'file').map(folder => folder.uri.fsPath);
    }

    /** U12 / §8.4 "Service not found": remove recognized AIRK entries and record them. */
    private sweeping?: Promise<void>;
    sweep(): Promise<void> {
        if (!vscode.workspace.isTrusted || !vscode.workspace.getConfiguration('airk').get<boolean>('mcp.registrationEnabled', true)) { return Promise.resolve(); }
        this.sweeping ??= (async () => {
            const { removed, skipped } = await sweepWithoutService(this.folders());
            for (const item of skipped) { this.output.appendLine(`AIRK: ${item.file} was not checked: ${item.reason}`); }
            if (!removed.length) { return; }
            const previous = this.context.workspaceState.get<RemovedEntry[]>(REMOVED_ENTRIES_KEY, []);
            await this.context.workspaceState.update(REMOVED_ENTRIES_KEY, [...previous, ...removed].slice(-256));
            for (const entry of removed) { this.output.appendLine(`AIRK: removed ${entry.agent} entry "${entry.serverKey}" from ${entry.file}: ${entry.reason}`); }
            this.notify('info', `Removed ${removed.length} AIRK agent ${removed.length === 1 ? 'entry' : 'entries'} that no installed AIRK Service can serve (see Output). Connect again once AIRK Service is installed and an index exists.`, ['airk.showExtensionOutput', 'airk.downloadService']);
        })().finally(() => { this.sweeping = undefined; });
        return this.sweeping;
    }

    /** "Reconnect Coding Agents…" (U28): one confirmation rewrites stale entries
     * to their live connection; entries with no live connection are removed and recorded. */
    async reconnectAgents(): Promise<void> {
        await this.requireReady();
        const projects = this.folders();
        const report = await listAgents(this.airkctl, projects, false) as Record<string, unknown>;
        const findings = Array.isArray(report.findings) ? report.findings as Record<string, unknown>[] : [];
        this.output.appendLine(`AIRK agents list:\n${JSON.stringify(report, null, 2)}`);
        if (!findings.length) { void vscode.window.showInformationMessage('AIRK: every recognized agent entry is current.'); return; }
        const choice = await vscode.window.showWarningMessage(`AIRK found ${findings.length} stale agent ${findings.length === 1 ? 'entry' : 'entries'}. Repair rewrites entries whose connection still exists; entries with no connection are removed and recorded.`, { modal: true }, 'Repair');
        if (choice !== 'Repair') { return; }
        const repaired = await repairAgents(this.airkctl, projects);
        this.output.appendLine(`AIRK agents repair:\n${JSON.stringify(repaired, null, 2)}`);
        const after = await listAgents(this.airkctl, projects, false) as Record<string, unknown>;
        const unknown = (Array.isArray(after.findings) ? after.findings as Record<string, unknown>[] : []).filter(item => item.problem === 'https_unknown_connection' || item.problem === 'stdio_retired' || item.problem === 'stdio_missing_command');
        if (unknown.length) {
            const removed = await removeAgents(this.airkctl, projects);
            this.output.appendLine(`AIRK agents remove:\n${JSON.stringify(removed, null, 2)}`);
        }
        void vscode.window.showInformationMessage('AIRK: agent entries were repaired. Running agents may need a restart to reconnect.', 'Show Output')
            .then(action => action ? this.output.show(true) : undefined);
    }

    /** "Connect Coding Agent…": write an entry for an existing connection. New
     * connections (grants) are created in the admin pages (U6). */
    async connectAgent(): Promise<void> {
        await this.requireReady();
        const folder = this.folders()[0];
        if (!folder) { throw new Error('Open a folder to connect an agent to it.'); }
        const client = await vscode.window.showQuickPick([
            { label: 'Claude Code', client: 'claude' as Client }, { label: 'Codex', client: 'codex' as Client }, { label: 'VS Code / GitHub Copilot Chat', client: 'vscode' as Client },
        ], { title: 'AIRK: Connect Coding Agent' });
        if (!client) { return; }
        const known = reportedConnections(await listAgents(this.airkctl, [folder]));
        const picked = await vscode.window.showQuickPick([
            ...known.map(id => ({ label: id, id })),
            { label: 'Create a connection in AIRK Admin…', id: '' },
        ], { title: 'AIRK: Choose an existing connection' });
        if (!picked) { return; }
        if (!picked.id) { await this.openAdmin(); return; }
        const result = await connectAgent(this.airkctl, client.client, picked.id, folder);
        this.output.appendLine(`AIRK agents connect:\n${JSON.stringify(result, null, 2)}`);
        void vscode.window.showInformationMessage(`AIRK: ${client.label} is configured for this folder.`);
    }

    /** VS Code MCP provider (§8.1, §8.4): the definition names an existing
     * connection; its token is resolved by a one-shot at start, never stored. */
    async provideDefinitions(): Promise<vscode.McpHttpServerDefinition[]> {
        if (this.disposed || !vscode.workspace.isTrusted || !vscode.workspace.getConfiguration('airk').get<boolean>('mcp.registrationEnabled', true)) { return []; }
        const id = this.context.workspaceState.get<string>(VSCODE_CONNECTION_KEY);
        if (!id || !INCARNATION.test(id) || this.airkctl.state.kind !== 'ready') { return []; }
        let url: string | undefined;
        try { url = connectionUrl(await connectionInfo(this.airkctl, id)); }
        catch (error) { this.output.appendLine(`AIRK: connection ${id} is unavailable: ${JSON.stringify(errorOf(error))}`); return []; }
        if (!url) { return []; }
        const mode = vscode.workspace.getConfiguration('airk').get<string>('mcp.responseMode', 'terse') === 'full' ? 'full' : 'terse';
        return [new vscode.McpHttpServerDefinition('AIRK', vscode.Uri.parse(url), { 'X-AIRK-Response-Mode': mode }, `${this.airkctl.state.version}:${id}:${mode}`)];
    }
    async resolveDefinition(definition: vscode.McpServerDefinition): Promise<vscode.McpServerDefinition> {
        const id = this.context.workspaceState.get<string>(VSCODE_CONNECTION_KEY);
        if (!(definition instanceof vscode.McpHttpServerDefinition) || !id || !INCARNATION.test(id)) { return definition; }
        definition.headers = { ...definition.headers, Authorization: await credentialHeader(this.airkctl, id) };
        return definition;
    }
    async manageVscodeMcp(): Promise<void> {
        await this.requireReady();
        const current = this.context.workspaceState.get<string>(VSCODE_CONNECTION_KEY);
        const known = reportedConnections(await listAgents(this.airkctl, this.folders()));
        const picked = await vscode.window.showQuickPick([
            ...known.map(id => ({ label: id, description: id === current ? 'current' : undefined, id })),
            { label: 'Stop providing AIRK to VS Code', id: '' },
        ], { title: 'AIRK: Connection the VS Code MCP provider uses' });
        if (!picked) { return; }
        await this.context.workspaceState.update(VSCODE_CONNECTION_KEY, picked.id || undefined);
        this.definitions.fire();
    }

    async requireReady(): Promise<void> {
        if (this.airkctl.state.kind !== 'ready') { await this.airkctl.check(); }
        const state = this.airkctl.state;
        if (state.kind !== 'ready') {
            const advice = guidance(state);
            this.notify(advice.severity === 'none' ? 'info' : advice.severity, advice.message, advice.actions);
            throw new Error(describe(state));
        }
    }
    async openAdmin(): Promise<void> {
        await this.airkctl.check();
        const result = await this.airkctl.openAdmin();
        if (result.class !== 'success') { throw new AirkctlCommandError(['admin', 'open'], result); }
    }
    async startService(): Promise<void> {
        await this.airkctl.check();
        const result = await this.airkctl.start();
        if (result.class !== 'success') { throw new AirkctlCommandError(['service', 'start'], result); }
        await this.host.refresh(false);
    }

    explorerController(): Promise<ExplorerController> {
        this.explorer ??= import('./explorerController').then(({ ExplorerController }) => {
            if (this.disposed) { throw new Error('extension_disposed'); }
            return this.explorerLoaded = new ExplorerController(this.host);
        }).catch(error => { this.explorer = undefined; throw error; });
        return this.explorer;
    }
    explorerHandoff(): { open: boolean; route?: string } {
        return { open: this.explorerLoaded?.panelOpen ?? false, route: this.explorerLoaded?.assistanceStatus.active_route };
    }

    dispose(): void {
        if (this.disposed) { return; }
        this.disposed = true;
        setEvidenceTransport(undefined);
        this.explorerLoaded?.dispose();
        this.airkctl.dispose();
        for (const item of this.subscriptions.splice(0)) { item.dispose(); }
    }
}

let active: SplitExtension | undefined;
export function deactivate(): void { active?.dispose(); active = undefined; }

export async function activate(context: vscode.ExtensionContext): Promise<{ getServerDefinitions(): Promise<vscode.McpServerDefinition[]>; whenReady(): Promise<void>; explorerHandoff(): { open: boolean; route?: string }; prepareForUpdate(): Promise<void> }> {
    setNativeExecutionGate(nativeExecutionSetting);
    const extension = new SplitExtension(context); active = extension;
    context.subscriptions.push(extension);
    const explorerOpen = (route?: unknown) => extension.explorerController().then(explorer => explorer.open(typeof route === 'string' ? route : undefined));
    const workbench = new WorkbenchView(context, extension.host);
    context.subscriptions.push(workbench, vscode.window.registerWebviewViewProvider('airk.workbench', workbench, { webviewOptions: { retainContextWhenHidden: true } }));
    const sidebar = new ExplorerSidebar({
        commandUris: ['airk.explorer.open', 'airk.openAdmin', 'airk.serviceStart', 'airk.downloadService', 'airk.checkAgain', 'airk.setupAgent', 'airk.explorer.showSuggestions'],
        folders: () => (vscode.workspace.workspaceFolders ?? []).filter(folder => ['file', 'vscode-remote'].includes(folder.uri.scheme)).length,
        attach: async listener => (await extension.explorerController()).onSnapshot(listener),
        open: () => explorerOpen(),
        hide: async () => { await vscode.commands.executeCommand('workbench.action.closeSidebar'); },
        fallback: unavailable => { void vscode.commands.executeCommand('setContext', 'airk.explorerUnavailable', unavailable); },
        search: query => { void extension.explorerController().then(explorer => explorer.search(query), error => extension.output.appendLine(String(error))); },
    });
    context.subscriptions.push(sidebar, vscode.window.registerWebviewViewProvider('airk.dashboard', sidebar));
    const safe = (action: (...args: unknown[]) => unknown) => async (...args: unknown[]) => {
        try { return await action(...args); }
        catch (error) {
            const detail = errorOf(error);
            extension.output.appendLine(`AIRK: ${JSON.stringify(detail)}`);
            void vscode.window.showErrorMessage(`AIRK: ${detail.message}`, 'Show Output').then(choice => choice ? extension.output.show(true) : undefined);
            throw error;
        }
    };
    const commands: Record<string, (...args: unknown[]) => unknown> = {
        // Status and guidance (§8.1): these only guide; none installs.
        openAdmin: () => extension.openAdmin(),
        serviceStart: () => extension.startService(),
        downloadService: () => vscode.env.openExternal(vscode.Uri.parse(DOWNLOAD_URL)),
        installationInstructions: () => vscode.env.openExternal(vscode.Uri.parse(DOWNLOAD_URL)),
        checkAgain: () => extension.start(),
        serviceConnect: () => extension.start(),
        enableNativeExecution: async () => { await vscode.workspace.getConfiguration('airk').update('runtime.nativeExecution', true, vscode.ConfigurationTarget.Global); await extension.start(); },
        manageWorkspaceTrust: () => vscode.commands.executeCommand('workbench.trust.manage'),
        showExtensionOutput: () => extension.output.show(false),
        // Agent connections through `airkctl agents …`.
        httpCredential: input => inputCredential(extension.airkctl, input),
        setupAgent: () => extension.connectAgent(),
        reconnectAgents: () => extension.reconnectAgents(),
        manageVscodeMcp: () => extension.manageVscodeMcp(),
        // Explorer and Workbench.
        'explorer.open': route => explorerOpen(route),
        'explorer.showSuggestions': async () => (await extension.explorerController()).showInbox(),
        'explorer.search': async query => {
            const text = typeof query === 'string' ? query : await vscode.window.showInputBox({ title: 'AIRK: Search indexed evidence' });
            if (text?.trim()) { (await extension.explorerController()).search(text); }
        },
        'workbench.focus': () => workbench.focus(),
        'workbench.newSession': () => workbench.newSession(),
        'workbench.clearSessions': () => workbench.clear(),
        'workbench.searchSelection': async () => {
            const editor = vscode.window.activeTextEditor;
            const text = editor?.document.getText(editor.selection).slice(0, 4096);
            await workbench.focus(text ? `/search ${JSON.stringify(text.replace(/\s+/g, ' '))}` : undefined);
        },
    };
    for (const [name, action] of Object.entries(commands)) { context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`, safe(action))); }
    if (vscode.lm?.registerMcpServerDefinitionProvider) {
        context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.collections', {
            onDidChangeMcpServerDefinitions: extension.definitions.event,
            provideMcpServerDefinitions: () => extension.provideDefinitions(),
            resolveMcpServerDefinition: value => extension.resolveDefinition(value),
        }));
    }
    context.subscriptions.push(
        vscode.workspace.onDidChangeWorkspaceFolders(() => { void extension.start(); }),
        vscode.workspace.onDidGrantWorkspaceTrust(() => { void extension.start(); }),
        vscode.workspace.onDidChangeConfiguration(event => {
            if (event.affectsConfiguration('airk.runtime.nativeExecution')) { void extension.start(); }
            else if (event.affectsConfiguration('airk.mcp')) { extension.definitions.fire(); }
        }));
    const ready = extension.start().catch(error => { extension.output.appendLine(`AIRK: discovery failed: ${error instanceof Error ? error.message : String(error)}`); });
    return {
        getServerDefinitions: async () => { await ready; return extension.provideDefinitions(); },
        whenReady: () => ready,
        explorerHandoff: () => extension.explorerHandoff(),
        prepareForUpdate: async () => { extension.dispose(); await workbench.finishPendingSaves(); },
    };
}
