// SPDX-License-Identifier: Apache-2.0
//! Explorer and Workbench host over the `airkctl` facade (plan §8.1, §10.3
//! "features"). Reads use the window session; management (index library,
//! storage, service, recovery, usage, logs) is in the admin pages, so the
//! control source here is read-only and every management action answers with
//! the Open AIRK Admin guidance instead of a second implementation.
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { AgentConfigHealth } from '../../agentConfigHealth';
import type { ExplorerHost } from '../../explorerController';
import type { ControlSource } from '../../explorerControl';
import { SuggestionStore } from '../../explorerSuggestions';
import type { LaunchSpec } from '../../model';
import type { ConsolePage, RecordValue } from '../../serviceClient';
import type { ConsoleObservation, ServiceConsoleState } from '../../serviceConsoleView';
import type { Airkctl, AirkctlState } from '../index';

/** One registered project as `airkctl projects --output json` lists it. */
export interface ProjectRow { project: string; root: string; data_root: string; collection: string; name?: string }

/** Parse the `projects` document; malformed rows are dropped, never invented. */
export function projectRows(value: unknown): ProjectRow[] {
    const projects = value && typeof value === 'object' ? (value as { projects?: unknown }).projects : undefined;
    if (!Array.isArray(projects)) { return []; }
    return projects.flatMap(row => {
        const item = row as Record<string, unknown>;
        return typeof item?.project === 'string' && /^[0-9a-f]{64}$/.test(item.project) && typeof item.root === 'string'
            && typeof item.data_root === 'string' && typeof item.collection === 'string'
            ? [{ project: item.project, root: item.root, data_root: item.data_root, collection: item.collection, ...(typeof item.name === 'string' ? { name: item.name } : {}) }]
            : [];
    });
}
const within = (child: string, parent: string) => {
    const relative = path.relative(process.platform === 'win32' ? parent.toLowerCase() : parent, process.platform === 'win32' ? child.toLowerCase() : child);
    return relative === '' || !relative.startsWith('..') && !path.isAbsolute(relative);
};
/** Launch specs for the projects that belong to this window's folders; all
 * projects when none does (the user still chooses what to read). */
export function specsFor(rows: readonly ProjectRow[], folders: readonly string[], airkctl: string, program: string): LaunchSpec[] {
    const related = rows.filter(row => folders.some(folder => within(row.root, folder) || within(folder, row.root)));
    return (related.length ? related : rows).map(row => ({
        command: airkctl, root: row.root, dataDir: row.data_root, collection: row.collection,
        name: row.name ?? (path.basename(row.root) || row.collection), service: { installDir: program, project: row.project },
    }));
}

const observation = (state: ConsoleObservation['state'], value?: RecordValue, code?: string): ConsoleObservation =>
    ({ state, ...(value ? { value } : {}), ...(code ? { code } : {}), checkedAt: Date.now() });
/** The read-only console state the Explorer renders from the facade state. */
export function consoleState(state: AirkctlState, rows: readonly ProjectRow[], trusted: boolean, revision: number): ServiceConsoleState {
    const connected = state.kind === 'ready';
    const health = connected && state.health && typeof state.health === 'object'
        ? observation('live', { observation: state.health as RecordValue }) : observation('unavailable', undefined, state.kind);
    const page: ConsolePage | undefined = connected ? {
        revision, cached: true, observed_unix_ms: Date.now(), after: null,
        projects: rows.map(row => ({ project: { registration: { binding: row.project, root: row.root, data_root: row.data_root, collection: row.collection } } })),
    } : undefined;
    return {
        revision: String(revision), trusted, busy: state.kind === 'checking', authority: 'airkctl', version: 'version' in state ? state.version : '',
        connected, health, resources: observation('unavailable'), model: observation('unavailable'), sources: observation('unavailable'),
        bootstrap: observation('unavailable'), events: [], historyOmitted: false, selected: new Set(), pending: [], includePaths: false,
        ...(page ? { page } : {}),
        ...(state.kind !== 'ready' && state.kind !== 'checking' ? { message: describe(state) } : {}),
    };
}
/** One line for the status bar and the Explorer notice. */
export function describe(state: AirkctlState): string {
    switch (state.kind) {
        case 'gated': return state.reason === 'workspace_untrusted' ? 'AIRK runs only in a trusted workspace.' : 'AIRK native execution is turned off (airk.runtime.nativeExecution).';
        case 'checking': return 'Checking for AIRK Service…';
        case 'not_installed': return state.predecessor
            ? 'AIRK Service is not installed. An older AIRK is still present; this extension does not use or change it, and the AIRK Service installer will remove it.'
            : 'AIRK Service is not installed.';
        case 'unverified': return `AIRK Service could not be verified (${state.code}): ${state.detail} Reinstall it with the AIRK Service installer.`;
        case 'incompatible': return `AIRK Service ${state.airkctlVersion} (I2 ${state.airkctlContracts.join(', ') || '?'}) is not compatible with this extension ${state.extensionVersion} (I2 ${state.extensionContracts.join(', ')}): ${state.reason}${state.missing ? ` (${state.missing.join(', ')})` : ''}. Install a matching AIRK Service or extension.`;
        case 'stopped': return state.explicitlyStopped ? 'AIRK Service was stopped. Start it to continue.' : 'AIRK Service is installed but not running.';
        case 'refused': return `AIRK Service refused the request (${state.error.code}${state.error.retryable ? ', retryable' : ''}): ${state.error.message}`;
        case 'ready': return `AIRK Service ${state.version} is ready.`;
    }
}

export class AdminOnly extends Error {
    readonly code = 'admin_only';
    constructor(action: string) { super(`"${action}" moved to the AIRK admin pages. Run "AIRK: Open AIRK Admin" (airkctl admin open).`); }
}

export class AirkctlExplorerHost implements ExplorerHost, vscode.Disposable {
    private rows: ProjectRow[] = [];
    private revision = 0;
    private readonly specsChanged = new vscode.EventEmitter<void>();
    private readonly stateChanged = new Set<(state: ServiceConsoleState) => void>();
    private current: ServiceConsoleState;
    private readonly subscriptions: vscode.Disposable[] = [];
    readonly onDidChangeSpecs = this.specsChanged.event;
    readonly suggestions: SuggestionStore;
    readonly control: ControlSource;
    readonly extensionUri: vscode.Uri;
    readonly version: string;
    readonly workspaceState: Pick<vscode.Memento, 'get' | 'update'>;

    constructor(context: vscode.ExtensionContext, private readonly airkctl: Airkctl, private readonly output: vscode.OutputChannel,
        private readonly status: vscode.StatusBarItem) {
        this.extensionUri = context.extensionUri;
        this.version = String(context.extension?.packageJSON?.version ?? '0.0.0');
        this.workspaceState = context.workspaceState;
        this.current = consoleState(airkctl.state, [], this.trusted(), 0);
        this.suggestions = new SuggestionStore({
            binding: () => vscode.workspace.workspaceFolders?.map(folder => folder.uri.toString()).join('\n') ?? '',
            revisions: () => ({}), publication: () => undefined,
            ttlMs: () => Math.max(1, vscode.workspace.getConfiguration('airk').get<number>('explorer.suggestions.ttlHours', 24)) * 3_600_000,
        });
        const host = this;
        this.control = {
            get controller() { return { state: host.current, observationIdentity: String(host.revision) }; },
            run: async action => {
                if (action === 'refresh' || action === 'reconnect' || action === 'connect') { await host.refresh(true); return; }
                throw new AdminOnly(action);
            },
            retain: () => ({ dispose: () => {} }),
            onDidChangeState: listener => { host.stateChanged.add(listener); return { dispose: () => { host.stateChanged.delete(listener); } }; },
        };
        this.subscriptions.push(airkctl.onDidChangeState(() => { void this.refresh(false); }));
    }

    trusted(): boolean { return vscode.workspace.isTrusted; }
    workspaceTrusted(): boolean { return vscode.workspace.isTrusted; }
    nativeExecutionEnabled(): boolean { return this.airkctl.state.kind !== 'gated' || this.airkctl.state.reason !== 'native_execution_disabled'; }
    specs(): readonly LaunchSpec[] {
        const installation = this.airkctl.installation;
        if (!installation || this.airkctl.state.kind !== 'ready') { return []; }
        return specsFor(this.rows, (vscode.workspace.workspaceFolders ?? []).map(folder => folder.uri.fsPath), installation.airkctl, installation.record.locations.program);
    }
    activeProject(): string | undefined {
        const file = vscode.window.activeTextEditor?.document.uri;
        const specs = this.specs();
        const match = file?.scheme === 'file' ? specs.find(spec => within(file.fsPath, spec.root)) : undefined;
        return (match ?? specs[0])?.service?.project;
    }
    /** Explorer's agent-entry health is the native `airkctl agents list` report
     * (findings are `AgentConfigFinding` field for field, plan §6.4). */
    async agentConfigHealth(root: string, check: () => void): Promise<AgentConfigHealth> {
        const state = this.airkctl.state;
        if (state.kind !== 'ready') { throw new Error(describe(state)); }
        const report = await this.airkctl.session.json(['agents', 'list', '--project', root, '--no-user']) as Record<string, unknown>;
        check();
        const list = <T>(value: unknown): T[] => Array.isArray(value) ? value as T[] : [];
        return { checked: list<string>(report.checked), findings: list(report.findings), entries: list(report.entries) };
    }
    log(line: string): void { this.output.appendLine(line); }
    setStatusBar(text: string, tooltip: string): void { this.status.text = text; this.status.tooltip = tooltip; }

    /** Re-read projects through the session when ready; publish the console state. */
    async refresh(recheck: boolean): Promise<void> {
        if (recheck) { await this.airkctl.check(); }
        const before = JSON.stringify(this.rows);
        if (this.airkctl.state.kind === 'ready') {
            try { this.rows = projectRows(await this.airkctl.session.json(['projects'])); }
            catch (error) { this.output.appendLine(`AIRK: projects could not be read: ${error instanceof Error ? error.message : String(error)}`); this.rows = []; }
        } else { this.rows = []; }
        this.revision++;
        this.current = consoleState(this.airkctl.state, this.rows, this.trusted(), this.revision);
        for (const listener of [...this.stateChanged]) { try { listener(this.current); } catch { /* Listener failures stay local. */ } }
        if (JSON.stringify(this.rows) !== before) { this.specsChanged.fire(); }
    }

    dispose(): void { for (const item of this.subscriptions.splice(0)) { item.dispose(); } this.specsChanged.dispose(); this.stateChanged.clear(); }
}
