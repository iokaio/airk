// SPDX-License-Identifier: Apache-2.0
//! Explorer and Workbench host over the `airkctl` facade (plan §8.1, §10.3
//! "features"). Reads use the window session; management (index library,
//! storage, service, recovery, usage, logs) is in the admin pages, so the
//! control source here is read-only and every management action answers with
//! the Open AIRK Admin guidance instead of a second implementation. The window
//! search scope (W4), Assistance drafts and handoffs use the session's window
//! plane when their capabilities are advertised (windowScope.ts,
//! windowAssistance.ts).
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { AgentConfigHealth } from '../../agentConfigHealth';
import type { ExplorerHost, HandoffPublisher } from '../../explorerController';
import type { SearchDraft } from '../../explorerDrafts';
import type { ControlSource } from '../../explorerControl';
import { SuggestionStore } from '../../explorerSuggestions';
import type { LaunchSpec } from '../../model';
import type { ConsolePage, RecordValue } from '../../serviceClient';
import type { ConsoleObservation, ServiceConsoleState } from '../../serviceConsoleView';
import { AirkctlUnavailable, type Airkctl, type AirkctlState } from '../index';
import type { AdminOpener, AdminTarget } from './adminOpener';
import { WindowScope } from './windowScope';
import { HANDOFF_CAPABILITY, WindowAssistance, WindowHandoffs } from './windowAssistance';

/** The Workbench side of the window search scope: prefills an inert search form under it. */
export interface WindowScopeWorkbench { prefillWindowSearch(search: SearchDraft): Promise<void> }

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
        case 'updating': return 'AIRK Service is being updated. This window released AIRK for the installer and reconnects when setup finishes.';
        case 'not_installed': return state.predecessor
            ? 'AIRK Service is not installed. An older AIRK is still present; this extension does not use or change it, and the AIRK Service installer (https://airk.ioka.io/#downloads) will remove it.'
            : 'AIRK Service is not installed. Download its installer from https://airk.ioka.io/#downloads (Download AIRK Service), run it, then choose Check again.';
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
    /** This window's search scope (U77, W4). */
    readonly windowScope: WindowScope;
    readonly assistance: WindowAssistance;
    private readonly publisher: WindowHandoffs;
    private workbench?: WindowScopeWorkbench;

    constructor(context: vscode.ExtensionContext, private readonly airkctl: Airkctl, private readonly output: vscode.OutputChannel,
        private readonly status: vscode.StatusBarItem, private readonly admin?: AdminOpener) {
        this.extensionUri = context.extensionUri;
        this.version = String(context.extension?.packageJSON?.version ?? '0.0.0');
        this.workspaceState = context.workspaceState;
        this.current = consoleState(airkctl.state, [], this.trusted(), 0);
        this.suggestions = new SuggestionStore({
            binding: () => this.suggestionsBinding(),
            revisions: () => ({}), publication: () => undefined,
            ttlMs: () => Math.max(1, vscode.workspace.getConfiguration('airk').get<number>('explorer.suggestions.ttlHours', 24)) * 3_600_000,
        });
        const capabilities = () => airkctl.state.kind === 'ready' ? airkctl.state.capabilities : undefined;
        this.windowScope = new WindowScope(airkctl.session, capabilities, context.workspaceState, () => {
            const installation = airkctl.installation, first = this.specs()[0];
            return installation && first ? { command: installation.airkctl, installDir: installation.record.locations.program, root: first.root } : undefined;
        }, line => output.appendLine(line));
        this.assistance = new WindowAssistance(airkctl.session, capabilities, this.suggestions, line => output.appendLine(line));
        this.publisher = new WindowHandoffs(airkctl.session, () => this.suggestionsBinding());
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
        this.subscriptions.push(airkctl.onDidChangeState(() => { void this.refresh(false); }), airkctl.session.onEvent(event => {
            void this.assistance.event(event);
            // The window and its scope grant are gone; the remembered choice is applied again on the next refresh.
            if (event.kind === 'session_lost') { this.windowScope.lost(); }
            if (event.kind === 'resync' && event.reason === 'service_restarted') { this.windowScope.lost(); void this.windowScope.restore(); }
        }));
    }
    private suggestionsBinding(): string { return vscode.workspace.workspaceFolders?.map(folder => folder.uri.toString()).join('\n') ?? ''; }

    /** Handoffs through the service (W8), only with `handoff/1`. */
    get handoffs(): HandoffPublisher | undefined {
        return this.airkctl.state.kind === 'ready' && this.airkctl.state.capabilities.includes(HANDOFF_CAPABILITY) ? this.publisher : undefined;
    }
    /** The Workbench registers its prefill target once it exists. */
    bindWorkbench(workbench: WindowScopeWorkbench | undefined): void { this.workbench = workbench; }
    /** Workbench entry for the granted window scope (read with no selection). */
    windowScopeSpec(): LaunchSpec | undefined { return this.windowScope.spec(); }
    /** Recommendations "Use this query scope": select it as the window scope, then prefill Workbench. */
    prefillRecommendedSearch(search: SearchDraft): Promise<void> { return this.prefillUnderScope(search); }
    /** Suggestions "Apply" of an agent search draft naming index sets: the same path. */
    prefillCombinedSearch(search: SearchDraft): Promise<void> { return this.prefillUnderScope(search); }
    private async prefillUnderScope(search: SearchDraft): Promise<void> {
        if (!this.workbench) { throw Object.assign(new Error('The AIRK Workbench is not available in this window.'), { code: 'workbench_unavailable' }); }
        await this.windowScope.selectIds(search.project_ids, search.index_ids);
        await this.workbench.prefillWindowSearch(search);
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
    /** Read-only index library (U97): one native catalog read through the session's I1 metadata relay. */
    async catalogControl(method: Record<string, unknown>): Promise<Record<string, unknown>> {
        const state = this.airkctl.state;
        if (state.kind !== 'ready') { throw new AirkctlUnavailable(state.kind); }
        return this.airkctl.session.control(method);
    }
    /** Open AIRK Admin on a page (U97, `airk.admin.openIn`). */
    async openAdmin(target: AdminTarget, choose: boolean): Promise<void> {
        if (!this.admin) { throw new AdminOnly('Open AIRK Admin'); }
        await this.admin.open(target, choose);
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
        if (this.airkctl.state.kind === 'ready') {
            // Each starts nothing without its capability; the scope is re-applied only after a fresh catalog read.
            await Promise.all([this.assistance.start(), this.windowScope.restore()]);
        } else { this.windowScope.lost(); }
    }

    dispose(): void { for (const item of this.subscriptions.splice(0)) { item.dispose(); } this.workbench = undefined; this.specsChanged.dispose(); this.stateChanged.clear(); }
}
