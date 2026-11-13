// SPDX-License-Identifier: Apache-2.0
//! Management › Agents in the Explorer (U104): the coding-agent entries of this
//! window's first folder, read with `airkctl agents list --no-user`, and
//! Disconnect (`airkctl agents remove --connection-id`). Connect goes through
//! the one hook `connectAgentToScope`; connecting an agent to the window's
//! search scope is being redesigned, so the hook answers that it is coming in
//! the next step and writes nothing. Every read returns the complete native report.
import type { AgentClient } from '../../explorerContracts';
import { listAgents, removeAgents, type AgentsRunner } from '../agents/index';

export const CONNECT_PENDING = 'Coming in the next step: connecting an agent to this window\'s search scope is being redesigned. Until then, connect agents in AIRK Admin › Agents.';
const CONNECTION = /^[0-9a-f]{32}$/;
/** The agent `agents list` names → the `--client` Management › Agents offers (U91). */
const CLIENT_OF: Readonly<Record<string, AgentClient>> = { claude: 'claude', codex: 'codex', copilot: 'vscode' };

/** One recognized entry as the Agents screen lists it. */
export interface AgentEntryView {
    readonly client: AgentClient; readonly agent: string; readonly file: string; readonly scope: string;
    readonly serverKey: string; readonly connectionId?: string; readonly status: string | null;
}
export interface AgentsView { readonly folder: string; readonly entries: AgentEntryView[]; readonly report: unknown }
export interface ConnectOutcome { readonly status: 'pending'; readonly client: AgentClient; readonly message: string }

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

export class WindowAgents {
    constructor(private readonly runner: AgentsRunner, private readonly folder: () => string | undefined,
        private readonly requireReady: () => void, private readonly log: (line: string) => void = () => {}) {}

    private requireFolder(): string {
        const folder = this.folder();
        if (!folder) { throw Object.assign(new Error('Open a folder in VS Code to connect coding agents to it.'), { code: 'NO_PROJECT' }); }
        return folder;
    }
    /** This folder's project-level agent entries with the complete native report. */
    async read(): Promise<AgentsView> {
        this.requireReady();
        const folder = this.requireFolder();
        const report = await listAgents(this.runner, [folder], false);
        return { folder, entries: agentEntries(report), report };
    }
    /** Remove this folder's entries naming `connectionId`; other servers and settings stay. */
    async disconnect(connectionId: string): Promise<unknown> {
        this.requireReady();
        if (!CONNECTION.test(connectionId)) { throw Object.assign(new Error('The connection identity is invalid.'), { code: 'INVALID_MESSAGE' }); }
        const result = await removeAgents(this.runner, [this.requireFolder()], connectionId);
        this.log(`AIRK: agents remove --connection-id ${connectionId} completed.`);
        return result;
    }
    /** The single Connect hook (U104). Pending the connection redesign it writes nothing. */
    async connectAgentToScope(client: AgentClient): Promise<ConnectOutcome> {
        return { status: 'pending', client, message: CONNECT_PENDING };
    }
}
