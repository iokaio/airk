// SPDX-License-Identifier: Apache-2.0
/**
 * Read-only classification of recognized AIRK entries in agent configuration
 * files: a project's own files (`locations`) and, when the host supplies them,
 * the agent's user-level files (already read by the bounded global inventory)
 * and the reconnect ledger captured before an installation was replaced.
 *
 * Every stdio AIRK entry is retired (the native runtime refuses stdio MCP);
 * an absent absolute command is reported as `stdio_missing_command`. HTTPS
 * entries are classified only from host-supplied native observations. Nothing
 * is written, no process is started, redirected project folders are not
 * followed, and a user-level entry needs ownership evidence before it is
 * reported, so unrelated servers are never flagged.
 */
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { readJsonConfig } from './agentConfig';
import { httpConfigurationConnectionId, locations, type LocalAgent } from './agentMigration';
import { findingsDigest, ledgerEntryFor, parserAgent, plainPath, recordedCollection, recordedRoot, samePath, type ReconnectLedger } from './agentReconnect';
import type { AgentConfigFinding, AgentEntryStatus } from './explorerContracts';
import { globalConfigDeclarations, type ConfigurationAgent, type ConfigurationFile } from './unifiedAgentConfig';
import { commandExecutable } from './model';

export const AGENT_CONFIG_LABELS: Record<LocalAgent, string> = {
    claude: 'Claude Code', codex: 'Codex', copilot: 'VS Code / GitHub Copilot Chat', gemini: 'Gemini CLI',
    antigravity: 'Google Antigravity', cursor: 'Cursor', roo: 'Roo Code', junie: 'JetBrains Junie',
};

/** One classified entry, including healthy ones. */
export interface AgentConfigEntry {
    readonly agent: LocalAgent;
    readonly scope: 'project' | 'user';
    readonly file: string;
    readonly containerPath: readonly string[];
    readonly serverKey: string;
    readonly status: AgentEntryStatus;
    readonly fields: Record<string, unknown>;
    readonly root?: string;
    readonly collection?: string;
    readonly connectionId?: string;
    readonly ledger?: boolean;
}
export interface AgentConfigHealth {
    /** Project-relative files that were present and parsed, in `locations` order. */
    readonly checked: string[];
    readonly findings: AgentConfigFinding[];
    /** Every classified entry, `ok` included. */
    readonly entries?: AgentConfigEntry[];
    /** Order-independent digest of `findings`. */
    readonly digest?: string;
}

/** Native observation of one connection ID. `unavailable` leaves the entry unclassified. */
export type HttpOwnership = { readonly status: 'owned'; readonly url: string } | { readonly status: 'not_owned' } | { readonly status: 'unavailable' };
export interface AgentHealthHttp {
    lookup(connectionId: string): Promise<HttpOwnership>;
    /** The URL the current service publishes, when known. */
    readonly endpointUrl?: string;
    /** Whether this user's OS store trusts the local issuer; undefined when unknown. Asked at most once per check. */
    issuerTrusted?(): Promise<boolean | undefined>;
}
export interface AgentConfigHealthOptions {
    /** Whether an absolute command path exists. Only `false` produces `stdio_missing_command`. */
    readonly exists?: (command: string) => Promise<boolean>;
    /** Throws when the caller's trust or project selection changed; called around every file read. */
    readonly check?: () => void;
    /** Native observations for HTTPS entries. Without them HTTPS entries are not classified. */
    readonly http?: AgentHealthHttp;
    /** Executables proven to belong to an AIRK installation of this user. */
    readonly verifiedLaunchers?: readonly string[];
    readonly ledger?: ReconnectLedger;
    /** User-level files from the bounded global inventory. */
    readonly userFiles?: readonly Pick<ConfigurationFile, 'agent' | 'destination' | 'source'>[];
    /** Also check the project folders of captured ledger entries (default). A per-project view passes false. */
    readonly ledgerRoots?: boolean;
}

/** Keys AIRK writes or recognizes: `airk`, `airk-<collection>` and the earlier `airk_explorer`. */
const AIRK_KEY = /^airk(?:$|[-_])/;

/** The single declaration parser each recognized location uses, and its project-level container. */
function container(agent: LocalAgent): { parser: ConfigurationAgent; path: string } {
    return agent === 'codex' ? { parser: 'codex', path: 'mcp_servers' }
        : agent === 'copilot' ? { parser: 'copilot', path: 'servers' } : { parser: 'claude', path: 'mcpServers' };
}

/** A stdio launch command; HTTP(S) declarations have no command to check. */
function stdioCommand(fields: Record<string, unknown>): string | undefined {
    if (typeof fields.command !== 'string' || 'url' in fields || 'serverUrl' in fields) { return undefined; }
    if (fields.type !== undefined && fields.type !== 'stdio' && fields.type !== 'local') { return undefined; }
    return fields.command;
}

/** Only an observed absence counts. Permission or other errors leave the entry unflagged. */
async function commandExists(command: string): Promise<boolean> {
    try { await fs.stat(command); return true; }
    catch (error) { const code = (error as NodeJS.ErrnoException).code; return code !== 'ENOENT' && code !== 'ENOTDIR'; }
}

const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const verifiedCommand = (command: string, verified: readonly string[]) => path.isAbsolute(plainPath(command)) && verified.some(launcher => samePath(launcher, command));

interface Classifier { exists: (command: string) => Promise<boolean>; check: () => void; http?: AgentHealthHttp; verified: readonly string[]; ledger?: ReconnectLedger; lookups: Map<string, Promise<HttpOwnership>>; trust?: Promise<boolean | undefined> }

/** Classify one declaration, or return undefined when it is not a recognized AIRK entry. */
async function classify(agent: LocalAgent, scope: 'project' | 'user', file: string, row: { serverKey: string; containerPath: string[]; fields: Record<string, unknown> }, context: Classifier): Promise<AgentConfigEntry | undefined> {
    const ledger = ledgerEntryFor(context.ledger, { destination: file, ...row });
    const command = stdioCommand(row.fields);
    const base = { agent, scope, file, containerPath: [...row.containerPath], serverKey: row.serverKey, fields: row.fields, ...(ledger ? { ledger: true } : {}) };
    if (command !== undefined) {
        // A user-level entry needs ownership evidence: an AIRK installation's
        // executable, or a ledger capture. A project entry is recognized by key.
        const owned = verifiedCommand(command, context.verified) || !!ledger;
        if (scope === 'user' ? !owned : !AIRK_KEY.test(row.serverKey) && !owned) { return undefined; }
        const root = ledger?.root ?? recordedRoot(row.fields), collection = ledger?.collection ?? recordedCollection(row.fields);
        const located = { ...base, ...(root ? { root } : {}), ...(collection ? { collection } : {}) };
        if (path.isAbsolute(command)) {
            const present = await context.exists(command); context.check();
            if (!present) { return { ...located, status: 'stdio_missing_command' }; }
        }
        return { ...located, status: 'stdio_retired' };
    }
    const id = httpConfigurationConnectionId(agent, row.fields);
    if (!id || typeof row.fields.url !== 'string') { return undefined; }
    if (scope === 'project' && !AIRK_KEY.test(row.serverKey) && !ledger) { return undefined; }
    const http = context.http;
    if (!http) { return undefined; }
    let pending = context.lookups.get(id);
    if (!pending) { pending = http.lookup(id).catch((): HttpOwnership => ({ status: 'unavailable' })); context.lookups.set(id, pending); }
    const owner = await pending; context.check();
    const located = { ...base, connectionId: id, ...(ledger?.root ? { root: ledger.root } : {}) };
    if (owner.status === 'unavailable') { return undefined; }
    if (owner.status === 'not_owned') {
        // An unknown user-level connection is reported only when it named the
        // current endpoint or was captured, never for an unrelated server.
        if (scope === 'user' && !ledger && row.fields.url !== http.endpointUrl) { return undefined; }
        return { ...located, status: 'https_unknown_connection' };
    }
    if (row.fields.url !== owner.url) { return { ...located, status: 'https_endpoint_mismatch' }; }
    // A credential helper naming a retired runtime image is refreshed to the current one, not reconnected.
    const helper = commandExecutable(agent === 'codex' ? row.fields.http_headers_helper : agent === 'claude' ? row.fields.headersHelper : undefined);
    if (helper && path.isAbsolute(helper)) {
        const present = await context.exists(helper); context.check();
        if (!present) { return { ...located, status: 'https_helper_missing' }; }
    }
    if (agent === 'claude' || agent === 'codex') {
        context.trust ??= http.issuerTrusted ? http.issuerTrusted().catch(() => undefined) : Promise.resolve(undefined);
        const trusted = await context.trust; context.check();
        if (trusted === false) { return { ...located, status: 'https_trust_missing' }; }
    }
    return { ...located, status: 'ok' };
}

function finding(entry: AgentConfigEntry): AgentConfigFinding {
    const { fields } = entry, command = typeof fields.command === 'string' ? fields.command : undefined, url = typeof fields.url === 'string' ? fields.url : undefined;
    return { agent: entry.agent, label: AGENT_CONFIG_LABELS[entry.agent], file: entry.file, scope: entry.scope, serverKey: entry.serverKey,
        ...(entry.containerPath.length !== 1 ? { containerPath: entry.containerPath } : {}), ...(command ? { command } : {}), ...(url ? { url } : {}),
        ...(entry.connectionId ? { connectionId: entry.connectionId } : {}), ...(entry.root ? { root: entry.root } : {}), ...(entry.collection ? { collection: entry.collection } : {}),
        problem: entry.status as Exclude<AgentEntryStatus, 'ok'>, ...(entry.ledger ? { ledger: true } : {}) };
}

/**
 * Findings for one already-read file. A file that cannot be parsed is reported
 * as `unreadable` (it could hide a broken entry) instead of throwing.
 */
export async function agentConfigFindings(agent: LocalAgent, file: string, source: string,
    exists: (command: string) => Promise<boolean> = commandExists, check: () => void = () => undefined, options: Omit<AgentConfigHealthOptions, 'exists' | 'check' | 'userFiles'> = {}, scope: 'project' | 'user' = 'project'): Promise<AgentConfigFinding[]> {
    return (await classifyFile(agent, scope, file, source, { exists, check, http: options.http, verified: options.verifiedLaunchers ?? [], ledger: options.ledger, lookups: new Map() })).findings;
}

async function classifyFile(agent: LocalAgent, scope: 'project' | 'user', file: string, source: string, context: Classifier): Promise<{ findings: AgentConfigFinding[]; entries: AgentConfigEntry[]; unreadable: boolean }> {
    const label = AGENT_CONFIG_LABELS[agent], selected = container(agent);
    let declarations: ReturnType<typeof globalConfigDeclarations>;
    try { declarations = globalConfigDeclarations(scope === 'user' ? parserAgent(agent) : selected.parser, source); }
    catch (error) { return { findings: [{ agent, label, file, scope, problem: 'unreadable', detail: message(error) }], entries: [], unreadable: true }; }
    const entries: AgentConfigEntry[] = [];
    for (const row of declarations) {
        // A project file is read only at its own container; a user file at every container.
        if (scope === 'project' && (row.containerPath.length !== 1 || row.containerPath[0] !== selected.path)) { continue; }
        const entry = await classify(agent, scope, file, row, context);
        if (entry) { entries.push(entry); }
    }
    return { findings: entries.filter(entry => entry.status !== 'ok').map(finding), entries, unreadable: false };
}

/** Folders between the project root and a configuration file must be real folders inside the root. */
async function directoryProblem(realRoot: string, root: string, parts: readonly string[]): Promise<'absent' | 'redirected' | undefined> {
    let current = root;
    for (const part of parts) {
        current = path.join(current, part);
        try {
            const info = await fs.lstat(current);
            if (info.isSymbolicLink()) { return 'redirected'; }
            if (!info.isDirectory()) { return 'absent'; }
            const relative = path.relative(realRoot, await fs.realpath(current));
            if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) { return 'redirected'; }
        } catch (error) {
            const code = (error as NodeJS.ErrnoException).code;
            if (code === 'ENOENT' || code === 'ENOTDIR') { return 'absent'; }
            throw error;
        }
    }
    return undefined;
}

function context(options: AgentConfigHealthOptions): Classifier {
    return { exists: options.exists ?? commandExists, check: options.check ?? (() => undefined), http: options.http, verified: options.verifiedLaunchers ?? [], ledger: options.ledger, lookups: new Map() };
}

async function projectFiles(root: string, classifier: Classifier, checked: string[], findings: AgentConfigFinding[], entries: AgentConfigEntry[]): Promise<void> {
    const { check } = classifier;
    check();
    const realRoot = await fs.realpath(root); check();
    for (const agent of Object.keys(locations) as LocalAgent[]) {
        const parts = locations[agent], file = path.join(root, ...parts), relative = parts.join('/'), label = AGENT_CONFIG_LABELS[agent];
        let source: string | undefined;
        try {
            const folder = await directoryProblem(realRoot, root, parts.slice(0, -1)); check();
            if (folder === 'absent') { continue; }
            if (folder === 'redirected') {
                findings.push({ agent, label, file, scope: 'project', problem: 'unreadable', detail: 'The configuration folder is a link or resolves outside this project, so it was not checked.' });
                continue;
            }
            source = await readJsonConfig(file); check();
        } catch (error) {
            check();
            findings.push({ agent, label, file, scope: 'project', problem: 'unreadable', detail: message(error) });
            continue;
        }
        if (source === undefined) { continue; }
        const found = await classifyFile(agent, 'project', file, source, classifier);
        if (!found.unreadable) { checked.push(relative); }
        findings.push(...found.findings); entries.push(...found.entries);
    }
}

/** Check every recognized per-project agent configuration file under `root`. Read-only. */
export async function checkAgentConfigHealth(root: string, options: AgentConfigHealthOptions = {}): Promise<AgentConfigHealth> {
    const checked: string[] = [], findings: AgentConfigFinding[] = [], entries: AgentConfigEntry[] = [], classifier = context(options);
    await projectFiles(root, classifier, checked, findings, entries);
    return { checked, findings };
}

/**
 * Proactive check across project folders, user-level files and the reconnect
 * ledger. Missing project folders are skipped; a ledger entry whose
 * declaration no longer exists is dropped (the user removed or replaced it).
 */
export async function checkAgentConnections(roots: readonly string[], options: AgentConfigHealthOptions = {}): Promise<Required<AgentConfigHealth>> {
    const checked: string[] = [], findings: AgentConfigFinding[] = [], entries: AgentConfigEntry[] = [], classifier = context(options);
    const projectRoots: string[] = [];
    const add = (root: string | undefined) => { if (root && path.isAbsolute(root) && !projectRoots.some(known => samePath(known, root)) && projectRoots.length < 64) { projectRoots.push(root); } };
    roots.forEach(add);
    // A captured project entry's own folder is checked even when it is not open.
    for (const entry of options.ledgerRoots === false ? [] : options.ledger?.entries ?? []) {
        if (entry.scope !== 'project') { continue; }
        const parts = locations[entry.agent];
        const folder = parts && samePath(path.join(path.sep, ...parts), path.join(path.sep, ...plainPath(entry.destination).split(/[\\/]/).slice(-parts.length)))
            ? path.resolve(plainPath(entry.destination), ...parts.map(() => '..')) : undefined;
        add(folder);
    }
    for (const root of projectRoots) {
        try { await projectFiles(root, classifier, checked, findings, entries); }
        catch (error) { classifier.check(); const code = (error as NodeJS.ErrnoException).code; if (code !== 'ENOENT' && code !== 'ENOTDIR') { throw error; } }
    }
    for (const file of options.userFiles ?? []) {
        classifier.check();
        const agent: LocalAgent = file.agent;
        const found = await classifyFile(agent, 'user', file.destination, file.source, classifier);
        // User-level unreadable files are inventory blockers, reported by the host.
        findings.push(...found.findings.filter(item => item.problem !== 'unreadable')); entries.push(...found.entries);
        if (!found.unreadable) { checked.push(file.destination); }
    }
    return { checked, findings, entries, digest: findingsDigest(findings) };
}
