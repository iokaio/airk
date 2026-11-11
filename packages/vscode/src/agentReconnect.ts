// SPDX-License-Identifier: Apache-2.0
/**
 * Host-owned reconnect ledger and the pure decisions behind Explorer's
 * Reconnect (one-airk.md §9, §10.3). Nothing here reads or writes files,
 * starts processes or contacts the service: the extension host supplies the
 * already bounded file contents and native observations, and owns every save.
 *
 * The ledger is captured before `runtime replace-installation --apply` removes
 * the folders its `client_references` name. It records only the AIRK
 * declarations themselves with exact file and entry digests, never unrelated
 * configuration bytes or secrets.
 */
import { createHash } from 'node:crypto';
import * as path from 'node:path';
import { httpUrl } from './serviceHttp';
import { commandExecutable } from './model';
import { globalConfigDeclarations, type ConfigurationAgent } from './unifiedAgentConfig';
import type { LocalAgent } from './agentMigration';

/** Typed per-entry reconnect outcome (one-airk.md §10.3 step 9, plus `awaiting_index`). */
export type ReconnectStatus = 'complete' | 'awaiting_host_reconnect' | 'awaiting_index' | 'blocked_unknown_owner'
    | 'blocked_client_compatibility' | 'recovery_required';
export type ReconnectScope = 'project' | 'user';

/** Native `runtime replace-installation` plan field naming what its removal invalidates. */
export interface ClientReferences {
    /** Absolute paths of every executable in the folders the plan removes. */
    readonly executables: readonly string[];
    readonly httpEndpoint: string | null;
    readonly caSha256: string | null;
}

export interface ReconnectLedgerEntry {
    readonly agent: LocalAgent;
    readonly scope: ReconnectScope;
    readonly destination: string;
    readonly containerPath: readonly string[];
    readonly serverKey: string;
    /** The captured AIRK declaration only. */
    readonly fields: Record<string, unknown>;
    readonly entrySha256: string;
    /** Exact preimage of the whole file at capture. */
    readonly fileSha256: string;
    /** `--root` / `--collection` recorded in a stdio declaration. */
    readonly root?: string;
    readonly collection?: string;
    readonly reference: 'executable' | 'endpoint';
    status?: ReconnectStatus;
    detail?: string;
}
export interface ReconnectLedger {
    readonly schema: 1;
    readonly capturedAt: number;
    readonly installDir: string;
    readonly references: ClientReferences;
    readonly entries: ReconnectLedgerEntry[];
    readonly blockers: { destination: string; code: string }[];
}
export interface ReconnectSourceFile { readonly agent: LocalAgent; readonly scope: ReconnectScope; readonly destination: string; readonly source: string }

export const RECONNECT_LEDGER_KEY = 'airk.reconnect.ledger.1';
const MAX_FILES = 96, MAX_ENTRIES = 512, MAX_BYTES = 8 * 1_048_576, MAX_EXECUTABLES = 256;
const fail = (code: string): never => { throw new Error(code); };
export const sha256 = (value: string): string => createHash('sha256').update(value, 'utf8').digest('hex');
const clone = (value: Record<string, unknown>): Record<string, unknown> => JSON.parse(JSON.stringify(value));
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const absolute = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && value.length <= 4096 && path.isAbsolute(value) && !/[\x00-\x1f]/.test(value);

/** Remove the Win32 extended-length prefix (`\\?\C:\…`, `\\?\UNC\…`) a native argument may carry. */
export function plainPath(value: string): string {
    if (/^\\\\\?\\UNC\\/i.test(value)) { return '\\\\' + value.slice(8); }
    return /^\\\\\?\\[A-Za-z]:/.test(value) ? value.slice(4) : value;
}
export function samePath(left: string, right: string, platform: NodeJS.Platform = process.platform): boolean {
    const paths = platform === 'win32' ? path.win32 : path.posix;
    const normal = (value: string) => { const resolved = paths.resolve(plainPath(value)); return platform === 'win32' ? resolved.toLowerCase() : resolved; };
    return normal(left) === normal(right);
}
export function parserAgent(agent: LocalAgent): ConfigurationAgent { return agent === 'codex' ? 'codex' : agent === 'copilot' ? 'copilot' : 'claude'; }

/** The validated `client_references` of a native replacement plan, or undefined when the plan has none. */
export function clientReferences(plan: unknown): ClientReferences | undefined {
    if (!object(plan) || plan.client_references === undefined) { return undefined; }
    const value = plan.client_references;
    if (!object(value) || !Array.isArray(value.executables) || value.executables.length > MAX_EXECUTABLES || !value.executables.every(absolute)) { fail('invalid_replacement_client_references'); }
    const raw = value as Record<string, unknown>;
    let endpoint: string | null = null;
    if (raw.http_endpoint !== null && raw.http_endpoint !== undefined) { try { endpoint = httpUrl(raw.http_endpoint); } catch { fail('invalid_replacement_client_references'); } }
    const ca = raw.ca_sha256 ?? null;
    if (ca !== null && (typeof ca !== 'string' || !/^[a-f0-9]{64}$/.test(ca))) { fail('invalid_replacement_client_references'); }
    return { executables: [...raw.executables as string[]], httpEndpoint: endpoint, caSha256: ca as string | null };
}

/** One spelling per endpoint: scheme and host case, the default port and a trailing slash do not distinguish URLs. */
export function normalizedUrl(value: string): string {
    try { const url = new URL(value); return `${url.protocol}//${url.host}${url.pathname.replace(/\/+$/, '')}${url.search}`; }
    catch { return value; }
}
/** The value following `flag` in a stdio argument list, if it is a string. */
function argument(args: unknown, flag: string): string | undefined {
    if (!Array.isArray(args)) { return undefined; }
    const at = args.indexOf(flag), value = at >= 0 ? args[at + 1] : undefined;
    return typeof value === 'string' && args.lastIndexOf(flag) === at ? value : undefined;
}
/** The project folder a stdio AIRK declaration served (`--root`), without a namespace prefix. */
export function recordedRoot(fields: Record<string, unknown>): string | undefined {
    const root = argument(fields.args, '--root');
    return root !== undefined && absolute(plainPath(root)) ? plainPath(root) : undefined;
}
export function recordedCollection(fields: Record<string, unknown>): string | undefined {
    const collection = argument(fields.args, '--collection');
    return collection !== undefined && /^[A-Za-z0-9._-]{1,128}$/.test(collection) && collection !== '.' && collection !== '..' ? collection : undefined;
}
export function entryIdentity(entry: { destination: string; containerPath: readonly string[]; serverKey: string }): string {
    return JSON.stringify([process.platform === 'win32' ? plainPath(entry.destination).toLowerCase() : entry.destination, entry.containerPath, entry.serverKey]);
}

/**
 * Capture every declaration that references an executable the plan removes or
 * its endpoint. Call this before `--apply`, while the files still show the
 * original entries. An unparsable file becomes a blocker, never a guess.
 */
export function captureReconnectLedger(files: readonly ReconnectSourceFile[], references: ClientReferences, installDir: string, now: number): ReconnectLedger {
    if (!absolute(installDir) || files.length > MAX_FILES) { fail('reconnect_ledger_bound'); }
    const ledger: ReconnectLedger = { schema: 1, capturedAt: now, installDir, references: structuredClone(references), entries: [], blockers: [] };
    const seen = new Set<string>();
    let bytes = 0;
    for (const file of files) {
        if (!absolute(file.destination)) { fail('reconnect_ledger_destination'); }
        bytes += Buffer.byteLength(file.source); if (bytes > MAX_BYTES) { fail('reconnect_ledger_bound'); }
        let rows: ReturnType<typeof globalConfigDeclarations>;
        try { rows = globalConfigDeclarations(parserAgent(file.agent), file.source); }
        catch (error) { ledger.blockers.push({ destination: file.destination, code: error instanceof Error ? error.message.slice(0, 256) : 'configuration_unreadable' }); continue; }
        const fileSha256 = sha256(file.source);
        for (const row of rows) {
            const command = row.fields.command, url = row.fields.url;
            // A removed image can be named by the stdio command or by a credential helper's first token.
            const removed = (value: unknown) => typeof value === 'string' && path.isAbsolute(plainPath(value)) && references.executables.some(executable => samePath(executable, value));
            const reference = removed(command) || removed(commandExecutable(row.fields.headersHelper)) || removed(commandExecutable(row.fields.http_headers_helper)) ? 'executable'
                : typeof url === 'string' && references.httpEndpoint !== null && normalizedUrl(url) === normalizedUrl(references.httpEndpoint) ? 'endpoint' : undefined;
            if (!reference) { continue; }
            const entry: ReconnectLedgerEntry = { agent: file.agent, scope: file.scope, destination: file.destination, containerPath: [...row.containerPath], serverKey: row.serverKey,
                fields: clone(row.fields), entrySha256: sha256(JSON.stringify(row.fields)), fileSha256, reference,
                ...(recordedRoot(row.fields) ? { root: recordedRoot(row.fields) } : {}), ...(recordedCollection(row.fields) ? { collection: recordedCollection(row.fields) } : {}) };
            const identity = entryIdentity(entry);
            if (seen.has(identity)) { continue; }
            seen.add(identity); ledger.entries.push(entry);
            if (ledger.entries.length > MAX_ENTRIES) { fail('reconnect_ledger_bound'); }
        }
    }
    return ledger;
}

const STATUSES: readonly ReconnectStatus[] = ['complete', 'awaiting_host_reconnect', 'awaiting_index', 'blocked_unknown_owner', 'blocked_client_compatibility', 'recovery_required'];
const AGENTS: readonly LocalAgent[] = ['codex', 'claude', 'copilot', 'gemini', 'antigravity', 'cursor', 'roo', 'junie'];
/** Strict shape and bound check of a persisted ledger. */
export function validateReconnectLedger(value: unknown): ReconnectLedger {
    if (!object(value) || value.schema !== 1 || !Number.isSafeInteger(value.capturedAt) || !absolute(value.installDir) || !Array.isArray(value.entries)
        || value.entries.length > MAX_ENTRIES || !Array.isArray(value.blockers) || value.blockers.length > MAX_ENTRIES || Buffer.byteLength(JSON.stringify(value)) > MAX_BYTES) { fail('invalid_reconnect_ledger'); }
    const ledger = value as unknown as ReconnectLedger;
    clientReferences({ client_references: { executables: ledger.references?.executables, http_endpoint: ledger.references?.httpEndpoint, ca_sha256: ledger.references?.caSha256 } });
    for (const entry of ledger.entries) {
        if (!object(entry) || !AGENTS.includes(entry.agent) || !['project', 'user'].includes(entry.scope) || !absolute(entry.destination) || typeof entry.serverKey !== 'string'
            || !entry.serverKey || entry.serverKey.length > 256 || !Array.isArray(entry.containerPath) || !entry.containerPath.length || entry.containerPath.length > 3
            || entry.containerPath.some(part => typeof part !== 'string' || !part || part.length > 4096) || !object(entry.fields)
            || !/^[a-f0-9]{64}$/.test(entry.entrySha256) || !/^[a-f0-9]{64}$/.test(entry.fileSha256) || !['executable', 'endpoint'].includes(entry.reference)
            || entry.root !== undefined && !absolute(entry.root) || entry.status !== undefined && !STATUSES.includes(entry.status)
            || entry.detail !== undefined && (typeof entry.detail !== 'string' || entry.detail.length > 2048)) { fail('invalid_reconnect_ledger'); }
    }
    for (const blocker of ledger.blockers) { if (!object(blocker) || !absolute(blocker.destination) || typeof blocker.code !== 'string' || blocker.code.length > 256) { fail('invalid_reconnect_ledger'); } }
    return ledger;
}
/** A later capture keeps earlier unfinished entries it no longer sees (their folders may already be gone). */
export function mergeReconnectLedgers(previous: ReconnectLedger | undefined, next: ReconnectLedger): ReconnectLedger {
    if (!previous) { return next; }
    const known = new Set(next.entries.map(entryIdentity));
    const kept = previous.entries.filter(entry => entry.status !== 'complete' && !known.has(entryIdentity(entry)));
    const entries = [...next.entries, ...kept].slice(0, MAX_ENTRIES);
    const executables = [...new Set([...next.references.executables, ...previous.references.executables])].slice(0, MAX_EXECUTABLES);
    return { ...next, references: { ...next.references, executables }, entries, blockers: [...next.blockers, ...previous.blockers].slice(0, MAX_ENTRIES) };
}
/** Launcher images the ledger proved belonged to AIRK: recognition evidence for migration. */
export function ledgerLaunchers(ledger: ReconnectLedger | undefined): string[] {
    if (!ledger) { return []; }
    const commands = ledger.entries.flatMap(entry => entry.reference === 'executable' && typeof entry.fields.command === 'string' ? [entry.fields.command] : []);
    return [...new Set([...ledger.references.executables, ...commands])];
}
/** The ledger entry for one declaration, if the current bytes still match the captured declaration. */
export function ledgerEntryFor(ledger: ReconnectLedger | undefined, row: { destination: string; containerPath: readonly string[]; serverKey: string; fields: Record<string, unknown> }): ReconnectLedgerEntry | undefined {
    const identity = entryIdentity(row), digest = sha256(JSON.stringify(row.fields));
    return ledger?.entries.find(entry => entryIdentity(entry) === identity && entry.entrySha256 === digest);
}

/** The exact action that makes a running client pick up the rewritten entry. AIRK never claims it happened. */
export function hostReconnectAction(agent: LocalAgent): string {
    switch (agent) {
        case 'claude': return 'In Claude Code run /mcp and choose Reconnect for this server, or start a new session. A project .mcp.json helper runs only after you accept that folder\u2019s trust prompt.';
        case 'codex': return 'Restart Codex: start a new Codex CLI session, or in the IDE extension open the gear menu \u2192 MCP servers \u2192 Restart extension. Project .codex/config.toml loads only in a trusted project.';
        case 'copilot': return 'In VS Code run MCP: List Servers \u2192 AIRK \u2192 Restart Server.';
        default: return 'Restart the agent so it reloads its MCP configuration.';
    }
}

/** Digest of a finding set: the once-per-findings notification key. Order-independent. */
export function findingsDigest(findings: readonly { file: string; containerPath?: readonly string[]; serverKey?: string; problem: string; connectionId?: string; command?: string; url?: string }[]): string {
    const rows = findings.map(finding => JSON.stringify([process.platform === 'win32' ? finding.file.toLowerCase() : finding.file, finding.containerPath ?? [], finding.serverKey ?? '', finding.problem,
        finding.connectionId ?? '', finding.command ?? '', finding.url ?? ''])).sort();
    return sha256(JSON.stringify(rows));
}

/** A proposal roster after the caller has independently proved native predecessor
 * ownership and captured exact preimages. Key prefixes do not establish ownership. */
export interface HostArtifactObservation {
    readonly mementoKeys: readonly string[];
    readonly environment: readonly { name: string; replace: boolean; value: string }[];
    readonly bundles: readonly { file: string; certificates: readonly string[] }[];
}
export interface HostArtifactRetirement { readonly mementos: string[]; readonly environment: string[]; readonly bundles: string[]; readonly retained: string[] }
const HTTP_MEMENTO = /^airk\.http\.(?:legacy|standalone|connection|union)\./;
/**
 * Select eligible already-proven rows. The caller must recheck each preimage
 * before mutation and report actual outcomes, including retained failures.
 * Without the removed CA digest no bundle is proposed for retirement.
 */
export function replacedHostArtifacts(observed: HostArtifactObservation, references: ClientReferences, trustDirectory: string): HostArtifactRetirement {
    const result: HostArtifactRetirement = { mementos: [], environment: [], bundles: [], retained: [] };
    if (observed.mementoKeys.length > 4096 || observed.environment.length > 4096 || observed.bundles.length > 1024) { fail('replaced_host_artifact_bound'); }
    for (const key of observed.mementoKeys) { if (HTTP_MEMENTO.test(key)) { result.mementos.push(key); } }
    for (const bundle of observed.bundles) {
        if (!samePath(path.dirname(bundle.file), trustDirectory)) { continue; }
        if (references.caSha256 !== null && bundle.certificates.includes(references.caSha256)) { result.bundles.push(bundle.file); }
        else { result.retained.push(bundle.file); }
    }
    for (const variable of observed.environment) {
        if (!variable.replace) { continue; }
        if (/^AIRK_MCP_TOKEN_[A-F0-9]{32}$/.test(variable.name)) { result.environment.push(variable.name); continue; }
        if ((variable.name === 'NODE_EXTRA_CA_CERTS' || variable.name === 'CODEX_CA_CERTIFICATE') && samePath(path.dirname(variable.value), trustDirectory)
            && !result.retained.some(file => samePath(file, variable.value))) { result.environment.push(variable.name); }
    }
    return result;
}

/** One Reconnect target: a recorded project folder and the entries that served it. */
export interface ReconnectGroup<T> { readonly root: string | undefined; readonly collection?: string; readonly entries: T[] }
/** Group entries by recorded folder (case-insensitively on Windows). Entries without one form a single unowned group. */
export function reconnectGroups<T extends { root?: string; collection?: string }>(entries: readonly T[]): ReconnectGroup<T>[] {
    const groups: { root: string | undefined; collection?: string; entries: T[] }[] = [];
    for (const entry of entries) {
        const group = groups.find(candidate => candidate.root === undefined ? entry.root === undefined : entry.root !== undefined && samePath(candidate.root, entry.root));
        if (group) { group.entries.push(entry); group.collection ??= entry.collection; }
        else { groups.push({ root: entry.root, ...(entry.collection ? { collection: entry.collection } : {}), entries: [entry] }); }
    }
    return groups;
}
