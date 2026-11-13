// SPDX-License-Identifier: Apache-2.0
//! Management › Agents in the Explorer (U104, U105): the coding-agent entries of
//! this window's first folder, read with `airkctl agents list --no-user`, each
//! open connection's service-stored default scope and agent choice
//! (`airkctl agents info`), Connect, Disconnect, the choice switch and following.
//!
//! Connect enrolls an **open** connection (`airkctl agents connect --open`, choice
//! off, empty default) and immediately sets its default scope to this window's
//! search scope (`airkctl agents default`). Connections connected here are
//! remembered per workspace; a followed one receives every later scope the
//! window applies or grants automatically (serialized, newest wins). A follow
//! failure is reported on this screen and in the AIRK log; it never fails the
//! scope change. Disconnect removes this folder's entries naming the connection
//! and revokes an open connection no other entry names (`airkctl agents revoke`).
//! Every read returns the complete native reports.
import type { AgentClient } from '../../explorerContracts';
import {
    connectionInfo, connectOpenAgent, listAgents, openConnectionState, removeAgents, reportedConnections, revokeConnection, setAgentDefaults,
    type AgentChoice, type AgentDefaultScope, type AgentsRunner, type OpenConnectionState,
} from '../agents/index';

const CONNECTION = /^[0-9a-f]{32}$/;
/** workspaceState key: the open connections connected from this workspace and whether each follows the window's scope. */
export const OPEN_AGENTS_KEY = 'airk.agents.openConnections';
/** At most this many connections are remembered and read per screen read. */
const MAX_CONNECTIONS = 16;
/** The agent `agents list` names → the `--client` Management › Agents offers (U91). */
const CLIENT_OF: Readonly<Record<string, AgentClient>> = { claude: 'claude', codex: 'codex', copilot: 'vscode' };

/** One recognized entry as the Agents screen lists it. */
export interface AgentEntryView {
    readonly client: AgentClient; readonly agent: string; readonly file: string; readonly scope: string;
    readonly serverKey: string; readonly connectionId?: string; readonly status: string | null;
}
/** One index or index set of a default scope, named from the loaded search choices when known. */
export interface ScopeItemView { readonly kind: 'index' | 'set'; readonly id: string; readonly name?: string; readonly description?: string }
export interface FailureView { readonly code: string; readonly message: string }
/** One connection named by this folder's entries. `open` is absent for a grant-bound connection. */
export interface AgentConnectionView {
    readonly connectionId: string;
    readonly open?: OpenConnectionState & { readonly scope: ScopeItemView[] };
    /** Remembered from a Connect in this workspace. */
    readonly remembered: boolean;
    /** Receives this window's search scope after every apply (open connections only). */
    readonly follows: boolean;
    /** The complete `agents info` answer, or why it could not be read. */
    readonly info?: unknown; readonly infoError?: FailureView;
    /** The last failure to pass this window's scope on (cleared by the next success). */
    readonly followError?: FailureView;
}
export interface AgentsView {
    readonly folder: string; readonly entries: AgentEntryView[]; readonly report: unknown;
    readonly connections: AgentConnectionView[];
    /** The default scope Connect and following send now; null without an applied scope. */
    readonly windowScope: { readonly index_ids: string[]; readonly project_ids: string[]; readonly items: ScopeItemView[] } | null;
}
export interface ConnectOutcome {
    /** `connected`: a new open connection was enrolled and its entry written. `already_connected`: this folder's
     * entry for the client already names a remembered open connection; only its default scope was set. */
    readonly status: 'connected' | 'already_connected';
    readonly client: AgentClient; readonly connectionId: string;
    /** The complete `agents connect --open` answer (absent when already connected). */
    readonly connected?: unknown;
    readonly file?: string; readonly changed?: boolean;
    /** The complete `agents default` answer, or why the default scope was not set. */
    readonly defaults?: unknown; readonly defaultsError?: FailureView;
    readonly open?: OpenConnectionState;
    readonly scope: ScopeItemView[] | null; readonly follows: boolean;
}

/** What the Agents host needs of the window search scope (windowScope.ts). */
export interface AgentScopeSource {
    agentDefaultScope(): AgentDefaultScope | undefined;
    choiceInfo?(kind: 'index' | 'set', id: string): { name?: string; description?: string } | undefined;
    loadChoices?(): Promise<void>;
}
export interface AgentsStore { get<T>(key: string): T | undefined; update(key: string, value: unknown): PromiseLike<void> }
interface RememberedConnection { connectionId: string; client: AgentClient; follow: boolean }

/** The connectable entries of an `agents list` report (Claude Code, Codex, VS Code), field for field. */
export function agentEntries(report: unknown): AgentEntryView[] {
    const entries = report && typeof report === 'object' ? (report as Record<string, unknown>).entries : undefined;
    return (Array.isArray(entries) ? entries : []).flatMap(value => {
        const entry = value && typeof value === 'object' ? value as Record<string, unknown> : {};
        const client = typeof entry.agent === 'string' ? CLIENT_OF[entry.agent] : undefined;
        if (!client || typeof entry.file !== 'string' || typeof entry.serverKey !== 'string') { return []; }
        return [{ client, agent: String(entry.agent), file: entry.file, scope: typeof entry.scope === 'string' ? entry.scope : 'project', serverKey: entry.serverKey,
            ...(typeof entry.connectionId === 'string' && CONNECTION.test(entry.connectionId) ? { connectionId: entry.connectionId } : {}),
            status: typeof entry.status === 'string' ? entry.status : null }];
    });
}
function failure(error: unknown): FailureView {
    const code = (error as { code?: unknown })?.code;
    return { code: typeof code === 'string' && /^[A-Za-z0-9_]{1,128}$/.test(code) ? code : 'agent_command_failed',
        message: (error instanceof Error ? error.message : String(error)).slice(0, 600) };
}
const scopeKey = (scope: AgentDefaultScope | null) => scope ? JSON.stringify([[...scope.index_ids].sort(), [...scope.project_ids].sort()]) : 'clear';
function remembered(value: unknown): RememberedConnection[] {
    return (Array.isArray(value) ? value : []).flatMap(item => {
        const record = item && typeof item === 'object' ? item as Record<string, unknown> : {};
        return typeof record.connectionId === 'string' && CONNECTION.test(record.connectionId) && typeof record.follow === 'boolean'
            && (record.client === 'claude' || record.client === 'codex' || record.client === 'vscode')
            ? [{ connectionId: record.connectionId, client: record.client as AgentClient, follow: record.follow }] : [];
    }).slice(0, MAX_CONNECTIONS);
}

export class WindowAgents {
    /** The scope the follower passes on next (newest wins) and the run in flight. */
    private pending?: { scope: AgentDefaultScope | null; user: boolean };
    private running?: Promise<void>;
    /** Per connection: the scope last passed on successfully, and the last follow failure. */
    private readonly sent = new Map<string, string>();
    private readonly followErrors = new Map<string, FailureView>();

    constructor(private readonly runner: AgentsRunner, private readonly folder: () => string | undefined,
        private readonly requireReady: () => void, private readonly log: (line: string) => void = () => {},
        private readonly options: { store?: AgentsStore; scope?: AgentScopeSource } = {}) {}

    private requireFolder(): string {
        const folder = this.folder();
        if (!folder) { throw Object.assign(new Error('Open a folder in VS Code to connect coding agents to it.'), { code: 'NO_PROJECT' }); }
        return folder;
    }
    private remembered(): RememberedConnection[] { return remembered(this.options.store?.get(OPEN_AGENTS_KEY)); }
    private async remember(records: RememberedConnection[]): Promise<void> { await this.options.store?.update(OPEN_AGENTS_KEY, records.slice(-MAX_CONNECTIONS)); }
    private requireConnection(connectionId: string): void {
        if (!CONNECTION.test(connectionId)) { throw Object.assign(new Error('The connection identity is invalid.'), { code: 'INVALID_MESSAGE' }); }
    }
    private items(scope: AgentDefaultScope): ScopeItemView[] {
        const item = (kind: 'index' | 'set', id: string): ScopeItemView => ({ kind, id, ...this.options.scope?.choiceInfo?.(kind, id) });
        return [...scope.project_ids.map(id => item('set', id)), ...scope.index_ids.map(id => item('index', id))];
    }

    /** This folder's project-level agent entries, each named connection's defaults and the window's scope, with the complete native reports. */
    async read(): Promise<AgentsView> {
        this.requireReady();
        const folder = this.requireFolder();
        const report = await listAgents(this.runner, [folder], false);
        const entries = agentEntries(report), saved = this.remembered();
        await this.options.scope?.loadChoices?.();
        const ids = [...new Set(entries.flatMap(entry => entry.connectionId ? [entry.connectionId] : []))].slice(0, MAX_CONNECTIONS);
        const connections: AgentConnectionView[] = [];
        for (const connectionId of ids) {
            const record = saved.find(item => item.connectionId === connectionId), followError = this.followErrors.get(connectionId);
            let info: unknown, infoError: FailureView | undefined;
            try { info = await connectionInfo(this.runner, connectionId); } catch (error) { infoError = failure(error); }
            const open = info && typeof info === 'object' ? openConnectionState((info as Record<string, unknown>).open) : undefined;
            connections.push({ connectionId, remembered: !!record, follows: !!open && record?.follow === true,
                ...(open ? { open: { ...open, scope: this.items({ index_ids: open.default_index_ids, project_ids: open.default_index_set_ids }) } } : {}),
                ...(info !== undefined ? { info } : {}), ...(infoError ? { infoError } : {}), ...(followError ? { followError } : {}) });
        }
        const scope = this.options.scope?.agentDefaultScope();
        return { folder, entries, report, connections,
            windowScope: scope ? { index_ids: [...scope.index_ids], project_ids: [...scope.project_ids], items: this.items(scope) } : null };
    }

    /** (U105) Connect `client` for this folder with an open connection whose default scope is this window's search scope. */
    async connectAgentToScope(client: AgentClient): Promise<ConnectOutcome> {
        this.requireReady();
        const folder = this.requireFolder();
        const scope = this.options.scope?.agentDefaultScope();
        // An entry for this client that already names a remembered open connection is kept: only its default changes.
        const saved = this.remembered();
        const existing = agentEntries(await listAgents(this.runner, [folder], false))
            .find(entry => entry.client === client && entry.connectionId && saved.some(item => item.connectionId === entry.connectionId));
        let connectionId: string, connected: unknown, file: string | undefined, changed: boolean | undefined, open: OpenConnectionState | undefined;
        if (existing?.connectionId) {
            connectionId = existing.connectionId; file = existing.file;
        } else {
            connected = await connectOpenAgent(this.runner, client, folder);
            const answer = connected && typeof connected === 'object' ? connected as Record<string, unknown> : {};
            if (typeof answer.connectionId !== 'string' || !CONNECTION.test(answer.connectionId)) {
                throw Object.assign(new Error('agents connect --open answered without a connection identity.'), { code: 'agents_connect_malformed', nativeResponse: answer });
            }
            connectionId = answer.connectionId;
            file = typeof answer.file === 'string' ? answer.file : undefined;
            changed = typeof answer.changed === 'boolean' ? answer.changed : undefined;
            open = openConnectionState(answer.open);
            this.log(`AIRK: agents connect --open --client ${client} enrolled connection ${connectionId}.`);
        }
        await this.remember([...saved.filter(item => item.connectionId !== connectionId), { connectionId, client, follow: true }]);
        let defaults: unknown, defaultsError: FailureView | undefined;
        if (scope) {
            try {
                defaults = await setAgentDefaults(this.runner, connectionId, { scope });
                this.sent.set(connectionId, scopeKey(scope)); this.followErrors.delete(connectionId);
                const stored = defaults && typeof defaults === 'object' ? openConnectionState((defaults as Record<string, unknown>).open) : undefined;
                if (stored) { open = stored; }
            } catch (error) {
                defaultsError = failure(error);
                this.followErrors.set(connectionId, defaultsError);
                this.log(`AIRK: the default scope of connection ${connectionId} was not set (${defaultsError.code}): ${defaultsError.message}`);
            }
        }
        return { status: existing ? 'already_connected' : 'connected', client, connectionId,
            ...(connected !== undefined ? { connected } : {}), ...(file ? { file } : {}), ...(changed !== undefined ? { changed } : {}),
            ...(defaults !== undefined ? { defaults } : {}), ...(defaultsError ? { defaultsError } : {}), ...(open ? { open } : {}),
            scope: scope ? this.items(scope) : null, follows: true };
    }

    /** (U105) Turn the agent's choice on (it may list and pick any index or index set) or off (default scope only). */
    async setChoice(connectionId: string, choice: AgentChoice): Promise<{ connectionId: string; choice: AgentChoice; result: unknown }> {
        this.requireReady();
        this.requireConnection(connectionId);
        if (choice !== 'off' && choice !== 'on') { throw Object.assign(new Error('The agent choice is off or on.'), { code: 'INVALID_MESSAGE' }); }
        const result = await setAgentDefaults(this.runner, connectionId, { choice });
        this.log(`AIRK: agents default --connection-id ${connectionId} --agent-choice ${choice} completed.`);
        return { connectionId, choice, result };
    }

    /** (U105) Follow (or stop following) this window's search scope; following passes the current scope on now. */
    async setFollow(connectionId: string, follow: boolean): Promise<{ connectionId: string; follow: boolean; result?: unknown; error?: FailureView }> {
        this.requireReady();
        this.requireConnection(connectionId);
        const saved = this.remembered(), record = saved.find(item => item.connectionId === connectionId);
        let client = record?.client;
        if (!client) {
            const entry = agentEntries(await listAgents(this.runner, [this.requireFolder()], false)).find(item => item.connectionId === connectionId);
            if (!entry) { throw Object.assign(new Error('No entry in this folder names that connection.'), { code: 'agent_connection_not_listed' }); }
            client = entry.client;
        }
        await this.remember([...saved.filter(item => item.connectionId !== connectionId), { connectionId, client, follow }]);
        if (!follow) { this.followErrors.delete(connectionId); return { connectionId, follow }; }
        const scope = this.options.scope?.agentDefaultScope();
        if (!scope) { return { connectionId, follow }; }
        try {
            const result = await setAgentDefaults(this.runner, connectionId, { scope });
            this.sent.set(connectionId, scopeKey(scope)); this.followErrors.delete(connectionId);
            return { connectionId, follow, result };
        } catch (error) {
            const reason = failure(error);
            this.followErrors.set(connectionId, reason);
            return { connectionId, follow, error: reason };
        }
    }

    /**
     * (U105) The window's scope changed: pass it on to every followed open connection of this workspace
     * (`null`: the user cleared it, so the default scope is cleared). Runs are serialized; a newer scope
     * replaces one not yet started. An unchanged automatic or restored scope is not sent again.
     * Failures are recorded per connection and logged; the returned promise never rejects.
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
    /** Resolves when no follow run is in flight. */
    settled(): Promise<void> { return this.running ?? Promise.resolve(); }
    private async passOn(scope: AgentDefaultScope | null, user: boolean): Promise<void> {
        const key = scopeKey(scope);
        for (const record of this.remembered().filter(item => item.follow)) {
            if (!user && this.sent.get(record.connectionId) === key) { continue; }
            try {
                await setAgentDefaults(this.runner, record.connectionId, { scope: scope ?? { index_ids: [], project_ids: [] } });
                this.sent.set(record.connectionId, key); this.followErrors.delete(record.connectionId);
            } catch (error) {
                const reason = failure(error);
                this.followErrors.set(record.connectionId, reason);
                this.log(`AIRK: connection ${record.connectionId} did not receive this window's search scope (${reason.code}): ${reason.message}`);
            }
        }
    }

    /** Remove this folder's entries naming `connectionId`; other servers and settings stay. An open connection
     * that no remaining entry (this folder or the user's files) names is revoked. */
    async disconnect(connectionId: string): Promise<{ removed: unknown; revoked?: unknown; revokeError?: FailureView; kept?: string }> {
        this.requireReady();
        this.requireConnection(connectionId);
        const folder = this.requireFolder(), saved = this.remembered();
        let open = saved.some(item => item.connectionId === connectionId);
        if (!open) {
            try { const info = await connectionInfo(this.runner, connectionId); open = !!openConnectionState((info as Record<string, unknown> | null)?.open); }
            catch { open = false; }
        }
        const removed = await removeAgents(this.runner, [folder], connectionId);
        this.log(`AIRK: agents remove --connection-id ${connectionId} completed.`);
        await this.remember(saved.filter(item => item.connectionId !== connectionId));
        this.sent.delete(connectionId); this.followErrors.delete(connectionId);
        if (!open) { return { removed }; }
        try {
            if (reportedConnections(await listAgents(this.runner, [folder], true)).includes(connectionId)) {
                return { removed, kept: 'Another agent entry still names this connection, so it was not revoked.' };
            }
            const revoked = await revokeConnection(this.runner, connectionId);
            this.log(`AIRK: agents revoke --connection-id ${connectionId} completed.`);
            return { removed, revoked };
        } catch (error) {
            const reason = failure(error);
            this.log(`AIRK: connection ${connectionId} was not revoked (${reason.code}): ${reason.message}`);
            return { removed, revokeError: reason };
        }
    }
}
