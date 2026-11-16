// SPDX-License-Identifier: Apache-2.0
//! Management › Agents in the Explorer (U104, U110): connect a coding agent to
//! this window's first folder, the agent entries of that folder
//! (`airkctl agents list --no-user`), the MCP endpoint (`airkctl agents endpoint`)
//! and the workspace defaults of this window's folders (`airkctl scope defaults`).
//!
//! U110: the MCP endpoint needs no credential, and agents choose their scope per
//! call (`index_ids`, `index_set_ids`, `workspace`). Connect writes only the
//! endpoint URL and the response-mode header (`airkctl agents connect`). The
//! window's applied search scope becomes the **workspace default** of every folder
//! of this window (`airkctl scope default --workspace <folder>…`; a cleared scope
//! clears it). Runs are serialized and the newest scope wins. A failure is
//! reported on this screen and in the AIRK log; it never fails the scope change.
//! Every read returns the complete native reports.
import type { AgentClient } from '../../explorerContracts';
import type { Entry, AgentManualEntry, AgentConfigRefusal } from '../../generated/i2';
import { AirkctlCommandError } from '../session';
import {
    agentEndpoint, agentEntries as nativeEntries, connectAgent, defaultFor, listAgents, removeAgentEntries, setWorkspaceDefault,
    workspaceDefaultEntries, workspaceDefaults, type AgentDefaultScope, type AgentsRunner, type WorkspaceDefault,
} from '../agents/index';

/** The agent `agents list` names → the `--client` Management › Agents offers (U91). */
const CLIENT_OF: Readonly<Record<string, AgentClient>> = { claude: 'claude', codex: 'codex', copilot: 'vscode' };
const ENDPOINT_URL = /^https:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}\/mcp$/;

/** One recognized entry as the Agents screen lists it. */
export interface AgentEntryView {
    readonly client: AgentClient; readonly agent: string; readonly file: string; readonly scope: string;
    readonly serverKey: string; readonly url?: string; readonly status: string | null;
}
/** One index or index set of a scope, named from the loaded search choices when known. */
export interface ScopeItemView { readonly kind: 'index' | 'set'; readonly id: string; readonly name?: string; readonly description?: string }
export interface FailureView { readonly code: string; readonly message: string }
/** The stored workspace default that applies to one folder of this window. */
export interface WorkspaceDefaultView {
    readonly folder: string;
    /** The most specific stored folder containing `folder`, or null when agents must name a scope per call. */
    readonly stored: (WorkspaceDefault & { readonly items: ScopeItemView[] }) | null;
}
export interface AgentsView {
    /** The folder Connect writes entries for (this window's first folder). */
    readonly folder: string;
    /** Every folder of this window; each receives the window's applied scope as its workspace default. */
    readonly folders: string[];
    readonly entries: AgentEntryView[]; readonly report: unknown;
    readonly manual: AgentManualEntry[]; readonly manualErrors?: FailureView[];
    /** The MCP endpoint (`agents endpoint`), or why it could not be read. */
    readonly endpoint: { readonly url: string; readonly state?: string } | null;
    readonly endpointReport?: unknown; readonly endpointError?: FailureView;
    /** Per window folder: the workspace default agents use when a call names no scope. */
    readonly workspaceDefaults: WorkspaceDefaultView[];
    readonly defaultsReport?: unknown; readonly defaultsError?: FailureView;
    /** The scope this window passes on now; null without an applied scope. */
    readonly windowScope: { readonly index_ids: string[]; readonly project_ids: string[]; readonly items: ScopeItemView[] } | null;
    /** The last failure to store this window's scope as the workspace default (cleared by the next success). */
    readonly followError?: FailureView;
}
export interface ConnectOutcome {
    readonly status: 'connected' | 'refused';
    readonly refusal?: AgentConfigRefusal;
    readonly manual?: AgentManualEntry;
    readonly client: AgentClient;
    /** The complete `agents connect` answer. */
    readonly connected: unknown;
    readonly file?: string; readonly url?: string; readonly changed?: boolean;
    /** The window's scope, which is (or was just made) the workspace default; null without an applied scope. */
    readonly scope: ScopeItemView[] | null;
    /** Why the window's scope could not be stored as the workspace default. */
    readonly defaultError?: FailureView;
}

/** What the Agents host needs of the window search scope (windowScope.ts). */
export interface AgentScopeSource {
    agentDefaultScope(): AgentDefaultScope | undefined;
    choiceInfo?(kind: 'index' | 'set', id: string): { name?: string; description?: string } | undefined;
    loadChoices?(): Promise<void>;
}
/** Entry removal needs the verified CLI capability (`agents-entry-remove/1`). */
type Runner = AgentsRunner & { readonly state?: { readonly kind: string; readonly capabilities?: readonly string[] } };

/** The connectable entries of an `agents list` report (Claude Code, Codex, VS Code), field for field. */
export function agentEntries(report: unknown): AgentEntryView[] {
    const entries = report && typeof report === 'object' ? (report as Record<string, unknown>).entries : undefined;
    return (Array.isArray(entries) ? entries : []).flatMap(value => {
        const entry = value && typeof value === 'object' ? value as Record<string, unknown> : {};
        const client = typeof entry.agent === 'string' ? CLIENT_OF[entry.agent] : undefined;
        if (!client || typeof entry.file !== 'string' || typeof entry.serverKey !== 'string') { return []; }
        return [{ client, agent: String(entry.agent), file: entry.file, scope: typeof entry.scope === 'string' ? entry.scope : 'project', serverKey: entry.serverKey,
            ...(typeof entry.url === 'string' && ENDPOINT_URL.test(entry.url) ? { url: entry.url } : {}),
            status: typeof entry.status === 'string' ? entry.status : null }];
    });
}
/** The URL and state of an `agents endpoint` answer; undefined when it names no loopback MCP URL. */
export function endpointOf(answer: unknown): { url: string; state?: string } | undefined {
    const record = answer && typeof answer === 'object' ? answer as Record<string, unknown> : {};
    const inner = record.endpoint && typeof record.endpoint === 'object' ? record.endpoint as Record<string, unknown> : record;
    return typeof inner.url === 'string' && ENDPOINT_URL.test(inner.url)
        ? { url: inner.url, ...(typeof inner.state === 'string' ? { state: inner.state } : {}) } : undefined;
}
/** Narrow native DTOs without constructing or rewriting their text. */
export function manualEntryOf(value: unknown): AgentManualEntry | undefined {
    const row = value && typeof value === 'object' ? value as Record<string, unknown> : {};
    return ['claude', 'codex', 'vscode'].includes(String(row.client)) && ['project', 'user'].includes(String(row.scope))
        && ['json', 'toml'].includes(String(row.format)) && typeof row.file === 'string' && typeof row.text === 'string'
        ? value as AgentManualEntry : undefined;
}
function refusalOf(error: unknown): AgentConfigRefusal | undefined {
    const native = error instanceof AirkctlCommandError ? error.result.error?.native : undefined;
    const row = native && typeof native === 'object' ? native as Record<string, unknown> : {};
    if (!['automatic_policy', 'conflict_retained', 'outcome_unknown'].includes(String(row.reason))
        || typeof row.detail !== 'string' || typeof row.file !== 'string'
        || !(row.manual === null || manualEntryOf(row.manual))) return;
    return native as AgentConfigRefusal;
}
function failure(error: unknown): FailureView {
    const code = (error as { code?: unknown })?.code;
    return { code: typeof code === 'string' && /^[A-Za-z0-9_]{1,128}$/.test(code) ? code : 'agent_command_failed',
        message: (error instanceof Error ? error.message : String(error)).slice(0, 600) };
}
const scopeKey = (folders: readonly string[], scope: AgentDefaultScope | null) =>
    JSON.stringify([[...folders].sort(), scope ? [[...scope.index_ids].sort(), [...scope.project_ids].sort()] : 'clear']);

export class WindowAgents {
    /** The scope passed on next (newest wins) and the run in flight. */
    private pending?: { scope: AgentDefaultScope | null; user: boolean };
    private running?: Promise<void>;
    /** The folders and scope last stored successfully, and the last failure. */
    private sent?: string;
    private followError?: FailureView;

    constructor(private readonly runner: Runner, private readonly folders: () => readonly string[],
        private readonly requireReady: () => void, private readonly log: (line: string) => void = () => {},
        private readonly options: { scope?: AgentScopeSource } = {}) {}

    private requireFolder(): string {
        const folder = this.folders()[0];
        if (!folder) { throw Object.assign(new Error('Open a folder in VS Code to connect coding agents to it.'), { code: 'NO_PROJECT' }); }
        return folder;
    }
    private items(indexIds: readonly string[], setIds: readonly string[]): ScopeItemView[] {
        const item = (kind: 'index' | 'set', id: string): ScopeItemView => ({ kind, id, ...this.options.scope?.choiceInfo?.(kind, id) });
        return [...setIds.map(id => item('set', id)), ...indexIds.map(id => item('index', id))];
    }

    /** This folder's agent entries, the endpoint, the workspace defaults of this window's folders and the window's scope. */
    async read(): Promise<AgentsView> {
        this.requireReady();
        const folder = this.requireFolder(), folders = [...this.folders()];
        const report = await listAgents(this.runner, [folder], false);
        let endpointReport: unknown, endpointError: FailureView | undefined;
        try { endpointReport = await agentEndpoint(this.runner); } catch (error) { endpointError = failure(error); }
        const manual: AgentManualEntry[] = [], manualErrors: FailureView[] = [];
        for (const client of ['claude', 'codex', 'vscode'] as const) {
            try {
                const entry = manualEntryOf(await this.runner.session.json(['agents', 'manual', '--client', client, '--project', folder]));
                if (!entry || entry.client !== client) throw new Error('The native manual entry is unavailable or malformed.');
                manual.push(entry);
            } catch (error) { manualErrors.push(failure(error)); }
        }
        let defaultsReport: unknown, defaultsError: FailureView | undefined, stored: WorkspaceDefault[] = [];
        try { defaultsReport = await workspaceDefaults(this.runner); stored = workspaceDefaultEntries(defaultsReport); }
        catch (error) { defaultsError = failure(error); }
        if (this.folders()[0] !== folder) throw Object.assign(new Error('Workspace folder changed while reading agent setup. Refresh Agents.'), { code: 'STALE_BINDING' });
        await this.options.scope?.loadChoices?.();
        const scope = this.options.scope?.agentDefaultScope();
        return { folder, folders, entries: agentEntries(report), report, manual, ...(manualErrors.length ? { manualErrors } : {}),
            endpoint: endpointOf(endpointReport) ?? null,
            ...(endpointReport !== undefined ? { endpointReport } : {}), ...(endpointError ? { endpointError } : {}),
            workspaceDefaults: folders.map(item => {
                const entry = defaultsError ? undefined : defaultFor(stored, item);
                return { folder: item, stored: entry ? { ...entry, items: this.items(entry.index_ids, entry.index_set_ids) } : null };
            }),
            ...(defaultsReport !== undefined ? { defaultsReport } : {}), ...(defaultsError ? { defaultsError } : {}),
            windowScope: scope ? { index_ids: [...scope.index_ids], project_ids: [...scope.project_ids], items: this.items(scope.index_ids, scope.project_ids) } : null,
            ...(this.followError ? { followError: this.followError } : {}) };
    }

    /** (U110) Write `client`'s entry for this folder (URL and response mode only) and make sure this window's
     * applied scope is the workspace default of its folders. */
    async connect(client: AgentClient): Promise<ConnectOutcome> {
        this.requireReady();
        const folder = this.requireFolder();
        let connected: unknown;
        try { connected = await connectAgent(this.runner, client, folder); }
        catch (error) {
            const refusal = refusalOf(error);
            if (!refusal) throw error;
            return { status: 'refused', client, connected: (error as AirkctlCommandError).result, refusal,
                ...(refusal.manual ? { manual: refusal.manual } : {}), file: refusal.file, scope: null };
        }
        const answer = connected && typeof connected === 'object' ? connected as Record<string, unknown> : {};
        this.log(`AIRK: agents connect --client ${client} --project ${folder} completed.`);
        const scope = this.options.scope?.agentDefaultScope();
        if (scope) { await this.followScope(scope, true); }
        return { status: 'connected', client, connected,
            ...(manualEntryOf(answer.manual) ? { manual: manualEntryOf(answer.manual) } : {}),
            ...(typeof answer.file === 'string' ? { file: answer.file } : {}),
            ...(typeof answer.url === 'string' ? { url: answer.url } : {}),
            ...(typeof answer.changed === 'boolean' ? { changed: answer.changed } : {}),
            scope: scope ? this.items(scope.index_ids, scope.project_ids) : null,
            ...(scope && this.followError ? { defaultError: this.followError } : {}) };
    }

    /** Remove this folder's project-level entries for `client` (`agents remove --entry`, complete native rows). */
    async disconnect(client: AgentClient): Promise<{ removed: unknown }> {
        this.requireReady();
        const folder = this.requireFolder();
        const runner = this.runner;
        if (!runner.state) { throw Object.assign(new Error('Selected agent removal requires verified CLI support; update AIRK Service and try again.'), { code: 'CAPABILITY_DISABLED' }); }
        const rows = nativeEntries(await listAgents(runner, [folder], false))
            .filter((entry: Entry) => entry.scope === 'project' && CLIENT_OF[entry.agent] === client);
        if (!rows.length) { throw Object.assign(new Error('This folder has no AIRK entry for that agent.'), { code: 'agent_entry_not_listed' }); }
        const removed = await removeAgentEntries({ session: runner.session, state: runner.state }, [folder], rows);
        this.log(`AIRK: agents remove --client ${client} removed ${rows.length} entr${rows.length === 1 ? 'y' : 'ies'}.`);
        return { removed };
    }

    /**
     * (U110) The window's scope changed: store it as the workspace default of every folder of this window
     * (`null`: the user cleared it, so the default is cleared). Runs are serialized; a newer scope replaces
     * one not yet started. An unchanged automatic or restored scope is not sent again. A failure is
     * recorded and logged; the returned promise never rejects.
     */
    followScope(scope: AgentDefaultScope | null, user: boolean): Promise<void> {
        this.pending = { scope, user };
        if (this.running) { return this.running; }
        const run = (async () => {
            try {
                while (this.pending) {
                    const next = this.pending; this.pending = undefined;
                    await this.passOn(next.scope, next.user);
                }
            } finally { this.running = undefined; }
        })();
        this.running = run;
        return run;
    }
    /** Resolves when no run is in flight. */
    settled(): Promise<void> { return this.running ?? Promise.resolve(); }
    private async passOn(scope: AgentDefaultScope | null, user: boolean): Promise<void> {
        const folders = [...this.folders()];
        if (!folders.length) { return; }
        const key = scopeKey(folders, scope);
        if (!user && this.sent === key) { return; }
        try {
            await setWorkspaceDefault(this.runner, folders, scope);
            this.sent = key; this.followError = undefined;
        } catch (error) {
            this.followError = failure(error);
            this.log(`AIRK: this window's search scope was not stored as the workspace default (${this.followError.code}): ${this.followError.message}`);
        }
    }
}
