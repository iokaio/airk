// SPDX-License-Identifier: Apache-2.0
//! Agent entries through `airkctl agents …` (plan §6.4, §8.1, §8.4; U12,
//! U28, U92), workspace defaults through `airkctl scope …` (U110) and the
//! no-service removal sweep. The extension writes no agent
//! entry itself: with a service every edit is an `airkctl agents` command;
//! without one it only removes recognized entries (./recognizer.ts) and
//! records what it removed so Explorer can list it.
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { Airkctl } from '../index';
import type { Entry } from '../../generated/i2';
import { PROJECT_FILES, removeAll, MAX_BYTES, type Agent, type Removal } from './recognizer';
const INCARNATION = /^[0-9a-f]{32}$/;
export type Client = 'claude' | 'codex' | 'vscode';

/** The value `airk.httpCredential` substitutes into a pre-U110 entry's
 * `Bearer ${input:airkToken_<id>}`. Credentials no longer exist (local trust
 * plan B) and the endpoint strips the header unread, so it is a fixed,
 * non-secret placeholder that lets such an entry connect until it is repaired. */
export const RETIRED_CREDENTIAL_PLACEHOLDER = 'airk-local-session';
/** `airk.httpCredential` (compatibility only): validate the old input's
 * arguments and answer the placeholder; nothing is read or issued. */
export async function inputCredential(_airkctl: Airkctl, input: unknown): Promise<string> {
    const id = input && typeof input === 'object' ? (input as Record<string, unknown>).connection_id : undefined;
    if (typeof id !== 'string' || !INCARNATION.test(id) || Object.keys(input as object).some(key => key !== 'connection_id')) {
        throw new Error('invalid_http_credential_arguments');
    }
    return RETIRED_CREDENTIAL_PLACEHOLDER;
}

const projectArgs = (projects: readonly string[]) => projects.flatMap(project => ['--project', project]);
/** What the `agents` wrappers need of the facade: the window session's JSON command runner. */
export interface AgentsRunner { readonly session: { json(argv: readonly string[]): Promise<unknown> } }
/** `agents list` for the window's folders: the complete native report. */
export function listAgents(airkctl: AgentsRunner, projects: readonly string[], includeUser = true): Promise<unknown> {
    return airkctl.session.json(['agents', 'list', ...projectArgs(projects), ...(includeUser ? [] : ['--no-user'])]);
}
/** `agents repair`: rewrites stale entries; `replacement` replaces unknown connections. */
export function repairAgents(airkctl: AgentsRunner, projects: readonly string[], replacement?: string): Promise<unknown> {
    return airkctl.session.json(['agents', 'repair', ...projectArgs(projects), '--no-user', ...(replacement ? ['--connection-id', replacement] : [])]);
}

export const ENTRY_REMOVE_CAPABILITY = 'agents-entry-remove/1';
type EntryRemovalRunner = AgentsRunner & { readonly state: { readonly kind: string; readonly capabilities?: readonly string[] } };

export function requireEntryRemoval(airkctl: EntryRemovalRunner): void {
    if (airkctl.state.kind !== 'ready' || !airkctl.state.capabilities?.includes(ENTRY_REMOVE_CAPABILITY)) {
        throw new Error('Selected agent removal requires verified CLI support; update AIRK Service and try again.');
    }
}

/** Forward complete reviewed native rows; an empty selection never means remove all. */
export function removeAgentEntries(airkctl: EntryRemovalRunner, projects: readonly string[], entries: readonly Entry[]): Promise<unknown> {
    requireEntryRemoval(airkctl);
    if (!entries.length) { throw new Error('No reviewed agent entries were selected for removal.'); }
    return airkctl.session.json(['agents', 'remove', ...projectArgs(projects), '--no-user',
        ...entries.flatMap(entry => ['--entry', JSON.stringify(entry)])]);
}

/** Retain native rows unchanged; Rust owns exact input validation. */
export function agentEntries(report: unknown): Entry[] {
    const object = report && typeof report === 'object' && !Array.isArray(report) ? report as Record<string, unknown> : undefined;
    if (!object || !Array.isArray(object.entries) || !Array.isArray(object.findings) || !Array.isArray(object.blockers)) {
        throw new Error('Agent configuration evidence is unavailable or malformed.');
    }
    for (const row of object.entries) {
        if (!row || typeof row !== 'object' || Array.isArray(row)
            || typeof row.file !== 'string' || typeof row.agent !== 'string' || typeof row.scope !== 'string'
            || typeof row.serverKey !== 'string' || !Array.isArray(row.containerPath)
            || !row.containerPath.every((key: unknown) => typeof key === 'string')
            || typeof row.transport !== 'string' || !(row.status === null || typeof row.status === 'string')) {
            throw new Error('Agent configuration entry evidence is malformed.');
        }
    }
    return object.entries as Entry[];
}

export function removableAgentEntry(entry: Entry): boolean {
    return entry.status === 'https_unknown_connection' || entry.status === 'stdio_retired' || entry.status === 'stdio_missing_command';
}

/** Native Entry equality is order-independent for object keys; optional presence is retained. */
export function sameAgentEntry(left: Entry, right: Entry): boolean {
    const canonical = (value: unknown): string => {
        if (Array.isArray(value)) { return `[${value.map(canonical).join(',')}]`; }
        if (value && typeof value === 'object') {
            return `{${Object.entries(value as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b))
                .map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`).join(',')}}`;
        }
        return JSON.stringify(value) ?? 'undefined';
    };
    return canonical(left) === canonical(right);
}

/** Keep complete action receipts in caller logs; errors/blockers prevent a success announcement. */
export function agentActionErrors(result: unknown): string[] {
    const object = result && typeof result === 'object' ? result as Record<string, unknown> : undefined;
    if (!object || object.schema !== 1 || !Array.isArray(object.actions) || !Array.isArray(object.blockers)) {
        throw new Error('Agent action evidence is unavailable or malformed.');
    }
    const errors = object.actions.flatMap((action: unknown) => {
        if (!action || typeof action !== 'object') { throw new Error('Agent action receipt is malformed.'); }
        const row = action as Record<string, unknown>;
        if (typeof row.file !== 'string' || typeof row.serverKey !== 'string'
            || !(row.changed === null || typeof row.changed === 'boolean')
            || !(row.error === null || typeof row.error === 'string')
            || (row.changed === null && row.error === null)) { throw new Error('Agent action receipt is malformed.'); }
        return typeof row.error === 'string' ? [`${row.file} / ${row.serverKey}: ${row.error}`] : [];
    });
    return [...errors, ...object.blockers.map(blocker => `Agent configuration blocker: ${JSON.stringify(blocker)}`)];
}
/** (U110) `agents connect`: write the client's entry for `project` with only the endpoint URL and the
 * `X-AIRK-Response-Mode` header (no credential, no connection). */
export function connectArgv(client: Client, project: string): string[] {
    return ['agents', 'connect', '--client', client, '--project', project];
}
export function connectAgent(airkctl: AgentsRunner, client: Client, project: string): Promise<unknown> {
    return airkctl.session.json(connectArgv(client, project));
}
/** `agents endpoint`: the service's checked loopback MCP endpoint descriptor. */
export function agentEndpoint(airkctl: AgentsRunner): Promise<unknown> {
    return airkctl.session.json(['agents', 'endpoint']);
}
/** The window search scope as index ids and index set (`prj_`) ids, read as a union. */
export interface AgentDefaultScope { readonly index_ids: readonly string[]; readonly project_ids: readonly string[] }
/** (U110) `scope default`: set the workspace default of every folder (no selectors clears it). */
export function scopeDefaultArgv(folders: readonly string[], scope: AgentDefaultScope | null): string[] {
    if (!folders.length) { throw new Error('A workspace default needs at least one folder.'); }
    return ['scope', 'default', ...folders.flatMap(folder => ['--workspace', folder]),
        ...(scope ? [...scope.index_ids.flatMap(id => ['--index', id]), ...scope.project_ids.flatMap(id => ['--set', id])] : [])];
}
export function setWorkspaceDefault(airkctl: AgentsRunner, folders: readonly string[], scope: AgentDefaultScope | null): Promise<unknown> {
    return airkctl.session.json(scopeDefaultArgv(folders, scope));
}
/** (U110) `scope defaults`: every stored workspace default, read-only. */
export function workspaceDefaults(airkctl: AgentsRunner): Promise<unknown> {
    return airkctl.session.json(['scope', 'defaults']);
}
/** One stored workspace default as `scope default`/`scope defaults` print it. */
export interface WorkspaceDefault { folder: string; index_ids: string[]; index_set_ids: string[]; applied_unix_ms: number }
/** The entries of a `scope defaults` document; a malformed document throws. */
export function workspaceDefaultEntries(document: unknown): WorkspaceDefault[] {
    const object = document && typeof document === 'object' && !Array.isArray(document) ? document as Record<string, unknown> : undefined;
    if (!object || object.schema !== 1 || !Array.isArray(object.entries)) { throw new Error('The workspace defaults document is unavailable or malformed.'); }
    const strings = (value: unknown) => Array.isArray(value) && value.every(item => typeof item === 'string') ? [...value as string[]] : undefined;
    return object.entries.map(value => {
        const row = value && typeof value === 'object' ? value as Record<string, unknown> : {};
        const index_ids = strings(row.index_ids), index_set_ids = strings(row.index_set_ids);
        if (typeof row.folder !== 'string' || !index_ids || !index_set_ids || typeof row.applied_unix_ms !== 'number' || !Number.isSafeInteger(row.applied_unix_ms)) {
            throw new Error('A workspace default entry is malformed.');
        }
        return { folder: row.folder, index_ids, index_set_ids, applied_unix_ms: row.applied_unix_ms };
    });
}
/** (U110) The stored default that applies to `folder`: the most specific stored folder containing it
 * (Windows: case-insensitive, `/` and `\` equal). */
export function defaultFor(entries: readonly WorkspaceDefault[], folder: string, windows = process.platform === 'win32'): WorkspaceDefault | undefined {
    const separator = windows ? '\\' : '/';
    const normal = (value: string) => {
        let text = windows ? value.replace(/\//g, '\\').toLowerCase() : value;
        while (text.length > 1 && text.endsWith(separator) && !(windows && /^[a-z]:\\$/.test(text))) { text = text.slice(0, -1); }
        return text;
    };
    const target = normal(folder);
    let best: WorkspaceDefault | undefined, length = -1;
    for (const entry of entries) {
        const stored = normal(entry.folder);
        const inside = target === stored || target.startsWith(stored.endsWith(separator) ? stored : stored + separator);
        if (inside && stored.length > length) { best = entry; length = stored.length; }
    }
    return best;
}
/** The endpoint URL of an `agents endpoint` answer (or a connection's `agents info` answer). */
export function connectionUrl(info: unknown): string | undefined {
    const record = info && typeof info === 'object' ? info as Record<string, unknown> : {};
    const endpoint = record.endpoint && typeof record.endpoint === 'object' ? record.endpoint as Record<string, unknown> : {};
    const url = typeof endpoint.url === 'string' ? endpoint.url : typeof record.url === 'string' ? record.url : undefined;
    return url && /^http:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}\/mcp$/.test(url) ? url : undefined;
}

/** One removal the no-service sweep made, kept so Explorer can list it (U12). */
export interface RemovedEntry { file: string; agent: Agent; serverKey: string; kind: 'stdio' | 'https'; reason: string; removedAt: number }
export interface SweepFileSystem {
    lstat(file: string): Promise<{ directory: boolean; link: boolean } | undefined>;
    realpath(file: string): Promise<string>;
    read(file: string): Promise<string>;
    write(file: string, text: string): Promise<void>;
}
export const systemSweepFileSystem: SweepFileSystem = {
    async lstat(file) {
        try { const info = await fs.lstat(file); return { directory: info.isDirectory(), link: info.isSymbolicLink() }; }
        catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return undefined; } throw error; }
    },
    realpath: file => fs.realpath(file),
    async read(file) {
        const info = await fs.lstat(file);
        if (!info.isFile() || info.size > MAX_BYTES) { throw new Error('configuration file is not a regular file within 1 MiB'); }
        return fs.readFile(file, 'utf8');
    },
    async write(file, text) {
        // Same folder, then rename: a partial write never replaces the file.
        const temporary = `${file}.airk-${process.pid}-${Date.now()}.tmp`;
        await fs.writeFile(temporary, text, { encoding: 'utf8', flag: 'wx' });
        try { await fs.rename(temporary, file); } catch (error) { await fs.rm(temporary, { force: true }); throw error; }
    },
};

/** Which recognized project entries a sweep removes (`folder` is the window
 * folder holding the file) and the reason recorded for each. */
export interface SweepOptions {
    select?: (entry: Removal, folder: string) => boolean;
    reason?: (entry: Removal) => string;
}

/** Service found (§8.4, U28): a pre-split stdio entry can never be served (the
 * service is HTTP only). It is removed and recorded when no index of the
 * service covers its root; an entry whose root is indexed is left for
 * "Connect Coding Agent", which writes an HTTP entry for a connection. */
export function retiredStdioWithoutIndex(indexedRoots: readonly string[], windows = process.platform === 'win32'): SweepOptions {
    const same = (left: string, right: string) => {
        const a = path.resolve(left), b = path.resolve(right);
        return windows ? a.toLowerCase() === b.toLowerCase() : a === b;
    };
    return {
        select: (entry, folder) => entry.recognized.kind === 'stdio'
            && !indexedRoots.some(root => same(root, entry.recognized.kind === 'stdio' && entry.recognized.root ? entry.recognized.root : folder)),
        reason: () => 'Retired stdio AIRK entry with no AIRK Service index for its folder; the AIRK Service serves HTTP only.',
    };
}

/**
 * No service installed (U12, §8.4): remove recognized AIRK entries from this
 * window's project files with the removal-only recognizer and return what was
 * removed. Folders that are links or resolve outside the project are skipped
 * (as `agents list` skips them); user-level files are left to the service.
 */
export async function sweepWithoutService(folders: readonly string[], files: SweepFileSystem = systemSweepFileSystem, now = Date.now,
    options: SweepOptions = {}): Promise<{ removed: RemovedEntry[]; skipped: { file: string; reason: string }[] }> {
    const select = options.select ?? (() => true);
    const reasonFor = options.reason ?? ((entry: Removal) => entry.recognized.kind === 'stdio' ? 'Retired stdio AIRK entry; the AIRK Service serves HTTP only.'
        : 'AIRK HTTP entry with no installed AIRK Service to serve it.');
    const removed: RemovedEntry[] = [], skipped: { file: string; reason: string }[] = [];
    for (const root of folders) {
        let realRoot: string;
        try { realRoot = await files.realpath(root); } catch { continue; }
        for (const [agent, parts] of PROJECT_FILES) {
            const file = path.join(root, ...parts);
            let present = true, current = root;
            for (const folder of parts.slice(0, -1)) {
                current = path.join(current, folder);
                const info = await files.lstat(current).catch(() => undefined);
                if (!info || !info.directory) { present = false; break; }
                const real = await files.realpath(current).catch(() => '');
                if (info.link || !real || real === realRoot || path.relative(realRoot, real).startsWith('..')) {
                    skipped.push({ file, reason: 'The configuration folder is a link or resolves outside this project.' }); present = false; break;
                }
            }
            if (!present || !await files.lstat(file).catch(() => undefined)) { continue; }
            let source: string;
            try { source = await files.read(file); } catch (error) { skipped.push({ file, reason: error instanceof Error ? error.message : String(error) }); continue; }
            let result: { text: string; removed: Removal[] };
            try { result = removeAll(agent, source, entry => select(entry, root)); } catch (error) { skipped.push({ file, reason: error instanceof Error ? error.message : String(error) }); continue; }
            if (!result.removed.length || result.text === source) { continue; }
            try { await files.write(file, result.text); } catch (error) { skipped.push({ file, reason: error instanceof Error ? error.message : String(error) }); continue; }
            for (const entry of result.removed) {
                removed.push({ file, agent, serverKey: entry.target.serverKey, kind: entry.recognized.kind, removedAt: now(), reason: reasonFor(entry) });
            }
        }
    }
    return { removed, skipped };
}
