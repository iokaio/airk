// SPDX-License-Identifier: Apache-2.0
import { execFile, ExecFileException } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { McpSessionError, McpStdioSession } from './mcpStdioSession';
import { ASSISTANCE_SERVER_NAME, ASSISTANCE_TOOLS } from './explorerContracts';
import { LaunchSpec, launchArgs, ServerEntry, UNIFIED_TOOLS } from './model';

export interface IndexProgress {
    collection: string; phase: string; revision: string | null; processed_artifacts: number; committed_artifacts: number;
    selected_artifacts: number | null; completed_artifacts: number | null;
}

export function parseIndexProgress(line: string): IndexProgress | undefined {
    if (line.length > 4096) { return undefined; }
    try {
        const value: unknown = JSON.parse(line);
        if (!value || typeof value !== 'object' || Array.isArray(value)) { return undefined; }
        const event = value as Record<string, unknown>;
        const count = (field: string) => Number.isSafeInteger(event[field]) && Number(event[field]) >= 0;
        if (typeof event.collection !== 'string' || !event.collection || event.collection.length > 256
            || !['waiting_collection', 'waiting_capacity', 'scan_started', 'base_committed', 'enrichment_started', 'enrichment_prepared', 'enrichment_committed', 'base_coverage_complete'].includes(String(event.phase))
            || !count('processed_artifacts') || !count('committed_artifacts')
            || (event.revision !== null && (typeof event.revision !== 'string' || event.revision.length > 256))
            || (event.selected_artifacts !== null && !count('selected_artifacts'))
            || (event.completed_artifacts !== null && !count('completed_artifacts'))) { return undefined; }
        return event as unknown as IndexProgress;
    } catch { return undefined; }
}

/** Serialize this extension host's writers; native locks still arbitrate other processes. */
export class NativeWriteQueue {
    private tails = new Map<string, Promise<void>>();
    run<T>(spec: LaunchSpec, operation: () => Promise<T>): Promise<T> {
        const directory = path.resolve(spec.dataDir);
        const key = JSON.stringify([process.platform === 'win32' ? directory.toLowerCase() : directory, spec.collection]);
        const result = (this.tails.get(key) ?? Promise.resolve()).then(operation);
        const tail = result.then(() => undefined, () => undefined);
        this.tails.set(key, tail);
        void tail.then(() => { if (this.tails.get(key) === tail) { this.tails.delete(key); } });
        return result;
    }
}

export async function prepareHelper(extensionPath: string, storage: string, version: string, repaired?: (message: string) => void): Promise<string> {
    const executable = process.platform === 'win32' ? 'airk.exe' : 'airk';
    const native = path.join(extensionPath, 'native');
    const manifest = JSON.parse(await fs.readFile(path.join(native, 'manifest.json'), 'utf8'));
    const platform = `${process.platform}-${process.arch}`;
    if (manifest.version !== version || manifest.vsix_target !== platform) {
        throw new Error(`AIRK helper does not match extension ${version} / ${platform}. Install the matching platform VSIX.`);
    }
    const digest: unknown = manifest.payload?.[executable];
    if (typeof digest !== 'string' || !/^[0-9a-f]{64}$/.test(digest)) { throw new Error('AIRK native manifest has no valid executable digest.'); }
    const source = path.join(native, executable);
    const bytes = await fs.readFile(source);
    if (createHash('sha256').update(bytes).digest('hex') !== digest) { throw new Error('AIRK bundled helper digest mismatch.'); }
    // Version/digest-specific cached paths survive extension directory replacement.
    const directory = path.join(storage, 'helpers', `${version}-${platform}-${digest.slice(0, 16)}`);
    await fs.mkdir(directory, { recursive: true });
    const destination = path.join(directory, executable);
    let replace = false;
    try {
        const cached = await fs.readFile(destination);
        replace = createHash('sha256').update(cached).digest('hex') !== digest;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; }
        replace = true;
    }
    if (replace) {
        const temporary = path.join(directory, randomUUID() + '.tmp');
        try {
            await fs.writeFile(temporary, bytes, { flag: 'wx', mode: 0o755 });
            await fs.rename(temporary, destination);
        } finally { await fs.unlink(temporary).catch(() => undefined); }
        repaired?.(`Prepared verified AIRK ${version} helper from the bundled copy: ${destination}`);
    }
    const actual = await runText(destination, ['--version']);
    if (actual.trim() !== `airk ${version}`) { throw new Error('AIRK helper version mismatch.'); }
    return destination;
}

function boundedDiagnostic(message: string): string {
    const limit = 8192;
    return message.length <= limit ? message : `${message.slice(0, limit / 2)}\n[diagnostic truncated]\n${message.slice(-limit / 2)}`;
}

/** Preserve process failure details even when the helper supplied its own stderr. */
export class NativeProcessError extends Error {
    readonly code: string | number | null | undefined;
    readonly signal: NodeJS.Signals | null | undefined;
    readonly killed: boolean;
    readonly timedOut: boolean;
    readonly stderr: string;
    readonly retryableGateContention: boolean;
    readonly nativeMessage?: string;
    readonly pendingIndexRevision?: number;
    readonly scanPaused: boolean;
    constructor(command: string, error: ExecFileException, stderr: string, readonly elapsedMs: number, readonly timeoutMs: number) {
        let nativeMessage: string | undefined, retryableGateContention = false, scanPaused = false, pendingIndexRevision: number | undefined;
        if (error.code === 1 && !error.killed && !error.signal) {
            try {
                // execFile already bounds captured output. Parse before applying
                // the diagnostic display limit: JSON escaping can expand a valid
                // 4,096-character native message beyond that limit.
                for (const line of stderr.trim().split(/\r?\n/).reverse()) {
                    let outcome: unknown;
                    try { outcome = JSON.parse(line); } catch { continue; }
                    if (!outcome || typeof outcome !== 'object' || Array.isArray(outcome)) { continue; }
                    const fields = outcome as Record<string, unknown>;
                    if (!Object.hasOwn(fields, 'airk_error')) { continue; }
                    // The last error envelope is authoritative. Malformed fields
                    // never authorize retry using an earlier, unrelated error.
                    if (fields.airk_error === 1 && typeof fields.code === 'string'
                        && typeof fields.retryable === 'boolean' && typeof fields.message === 'string') {
                        nativeMessage = boundedDiagnostic(fields.message);
                        retryableGateContention = fields.code === 'current_gate_busy' && fields.retryable;
                        scanPaused = fields.code === 'scheduled_index_paused' && fields.retryable;
                        if (fields.code === 'scheduled_index_pending' && fields.retryable
                            && Number.isSafeInteger(fields.requested_revision) && Number(fields.requested_revision) > 0) {
                            pendingIndexRevision = Number(fields.requested_revision);
                        }
                    }
                    break;
                }
            } catch { /* Unstructured stderr never authorizes a retry. */ }
        }
        // With no AbortSignal or exposed child handle, execFile only kills a
        // child for its timeout or output cap. An external signal does not set killed.
        const timedOut = !!error.killed && timeoutMs > 0 && error.code !== 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER';
        const code = typeof error.code === 'number' && (error.code < 0 || error.code >= 0x80000000)
            ? `${error.code} (0x${(error.code >>> 0).toString(16).padStart(8, '0')})` : error.code;
        const details = [code != null ? `exit code ${code}` : undefined,
            error.signal ? `signal ${error.signal}` : undefined, error.killed ? 'terminated' : undefined,
            `elapsed ${elapsedMs} ms`].filter(Boolean).join('; ');
        const diagnostic = boundedDiagnostic(nativeMessage ?? (stderr.trim() || error.message));
        super(`${path.basename(command)} ${timedOut ? `timed out (limit ${timeoutMs} ms)` : 'failed'}: ${details}\n${diagnostic}`);
        this.name = 'NativeProcessError';
        this.code = error.code; this.signal = error.signal; this.killed = !!error.killed; this.timedOut = timedOut;
        this.stderr = boundedDiagnostic(stderr.trim());
        this.nativeMessage = nativeMessage;
        this.retryableGateContention = retryableGateContention;
        this.pendingIndexRevision = pendingIndexRevision; this.scanPaused = scanPaused;
    }
}

export function runText(command: string, args: string[], cwd?: string, timeout = 300000,
    onStderrLine?: (line: string) => boolean, idleTimeout = 0, input?: string): Promise<string> {
    if (input !== undefined && Buffer.byteLength(input, 'utf8') > 65536) { return Promise.reject(new Error('AIRK native control input exceeds 64 KiB.')); }
    return new Promise((resolve, reject) => {
        const started = performance.now();
        let idleExpired = false;
        let idleTimer: NodeJS.Timeout | undefined;
        const child = execFile(command, args, { cwd, env: { ...process.env, AIRK_CLIENT: 'vscode', AIRK_ERROR_FORMAT: 'json' }, windowsHide: true, maxBuffer: 16 * 1024 * 1024, timeout }, (error, stdout, stderr) => {
            if (idleTimer) { clearTimeout(idleTimer); }
            if (idleExpired && error) { reject(new Error(`AIRK index stopped after ${idleTimeout} ms without progress. Committed evidence remains available; use Diagnose & Repair, then retry indexing.`)); return; }
            if (error) { reject(new NativeProcessError(command, error, stderr, Math.round(performance.now() - started), timeout)); }
            else { resolve(stdout); }
        });
        if (input !== undefined) {
            child.stdin?.on('error', () => { /* The exit result/deadline owns error reporting. */ });
            child.stdin?.end(input);
        }
        const armIdle = () => {
            if (!idleTimeout) { return; }
            if (idleTimer) { clearTimeout(idleTimer); }
            idleTimer = setTimeout(() => { idleExpired = true; child.kill(); }, idleTimeout);
        };
        armIdle();
        if (onStderrLine) {
            let pending = '';
            child.stderr?.on('data', (bytes: Buffer) => {
                pending += bytes.toString();
                if (pending.length > 8192) { pending = ''; return; }
                let end: number;
                while ((end = pending.indexOf('\n')) >= 0) {
                    const line = pending.slice(0, end); pending = pending.slice(end + 1);
                    if (line.length <= 4096) {
                        try { if (onStderrLine(line)) { armIdle(); } }
                        catch { /* Progress display cannot interrupt native indexing. */ }
                    }
                }
            });
        }
    });
}

export async function runJson<T>(spec: LaunchSpec, command: string, extra: string[] = [], onStderrLine?: (line: string) => boolean): Promise<T> {
    const indexing = command === 'index' || command === 'schedule' && extra[0] === 'wait';
    return JSON.parse(await runText(spec.command, [...launchArgs(spec, command), ...extra], spec.root,
        indexing ? 3600000 : command === 'semantic' && extra[0] === 'run' ? 1200000 : 300000,
        onStderrLine, indexing && onStderrLine ? 600000 : 0)) as T;
}

export class ScheduledIndexStillQueued extends Error {
    constructor(readonly revision: number, readonly paused = false) {
        super(`AIRK indexing request ${revision} is ${paused ? 'paused' : 'still queued'}. Committed evidence remains available.`);
    }
}

/** Continue the same durable request after a bounded native join expires. */
export async function joinScheduledIndex<T>(initialArgs: string[],
    run: (command: string, args: string[]) => Promise<T>, onPending: () => boolean | void, initialRevision?: number): Promise<T> {
    let revision = initialRevision;
    for (let joins = 0; ; joins++) {
        try { return await run(revision === undefined ? 'index' : 'schedule',
            revision === undefined ? initialArgs : ['wait', String(revision), '--progress']); }
        catch (error) {
            if (!(error instanceof NativeProcessError) || error.pendingIndexRevision === undefined) { throw error; }
            if (revision !== undefined && error.pendingIndexRevision !== revision) { throw new Error('Native indexing wait returned a different request revision.'); }
            revision = error.pendingIndexRevision;
            // The caller rechecks trust, shutdown and collection binding between joins.
            const paused = onPending() === false;
            if (paused || joins >= 2) { throw new ScheduledIndexStillQueued(revision, paused); }
        }
    }
}

/** Real bounded stdio initialization/tool-list/status probe, independent of any coding agent. */
export async function probeMcp(spec: LaunchSpec, expectedTools: 14 | 15 | 16 | 26 | 27 = 27, clientVersion = 'development', entry?: ServerEntry,
    report: (message:string)=>void = ()=>{}): Promise<{ tools: string[]; status: unknown; handshakeMs?:number }> {
    const startedAt=Date.now(),deadline = startedAt + (spec.service ? 90000 : 15000);
    const remaining = () => Math.max(1, deadline - Date.now());
    let session: McpStdioSession | undefined;
    let stage='starting the launcher and waiting for the MCP handshake';
    try {
        report('Starting the launcher and waiting for the MCP handshake…');
        session = (await McpStdioSession.open({ command: entry?.command??spec.command, args: entry?[...entry.args]:launchArgs(spec), cwd: spec.service ? path.dirname(spec.command) : spec.root,
            env: { AIRK_CLIENT: 'vscode' }, clientName: 'airk-extension-check', clientVersion }, remaining())).session;
        const handshakeMs=Date.now()-startedAt;
        stage='listing MCP tools';report(`MCP handshake completed in ${(handshakeMs/1000).toFixed(1)} seconds. Reading the tool list…`);
        const listed = await session.request('tools/list', {}, remaining());
        const tools: string[] = listed.tools.map((tool: { name: string }) => tool.name);
        const required = expectedTools>=26?UNIFIED_TOOLS.filter(name=>expectedTools===27||name!=='diagnostics'):['airk_status', 'airk_symbols', 'airk_code_facts', 'airk_relationships', ...(expectedTools >= 15 ? ['airk_changes'] : []), ...(expectedTools >= 16 ? ['airk_help'] : [])];
        if (tools.length !== expectedTools || !required.every(name => tools.includes(name))) { throw new Error(`AIRK MCP tool inventory differs from the expected ${expectedTools} tools.`); }
        stage='reading index status';report(`${tools.length} MCP tools available. Reading index status…`);
        const status = await session.request('tools/call', { name: expectedTools>=26?'status':'airk_status', arguments: {} }, remaining());
        if (status.isError) {
            const message = status.content?.map((part: { text?: string }) => part.text ?? '').join('\n') ?? '';
            if (!message.includes('workspace is not indexed')) { throw new Error(`AIRK MCP status failed: ${message.slice(0, 300)}`); }
            return { tools, status: { state: 'not_indexed' }, handshakeMs };
        }
        if(expectedTools>=16){
            const text=status.content?.find((part:any)=>part.type==='text')?.text;
            const payload=JSON.parse(text??'null');
            if(payload?.index_state==='unknown')status.state='index_unknown';
            else if(payload?.index_state&&payload.index_state!=='available')status.state='index_unavailable';
        }
        if(handshakeMs>10000)report(`AIRK responded, but its handshake took ${(handshakeMs/1000).toFixed(1)} seconds. An agent with a shorter startup deadline may time out.`);
        return { tools, status, handshakeMs };
    } catch (error) {
        throw new Error(`AIRK connection check failed while ${stage}: ${error instanceof Error?error.message:String(error)}`);
    } finally { session?.close(); }
}

/** Start Explorer Assistance exactly as an agent entry would and check its tool inventory and bundled help. */
export async function probeAssistance(entry: ServerEntry, cwd: string, clientVersion = 'development'): Promise<{ tools: string[]; version: string }> {
    const deadline = Date.now() + 30000;
    const remaining = () => Math.max(1, deadline - Date.now());
    let session: McpStdioSession | undefined;
    try {
        const opened = await McpStdioSession.open({ command: entry.command, args: [...entry.args], cwd, clientName: 'airk-extension-check', clientVersion }, remaining());
        session = opened.session;
        if (opened.initialize?.serverInfo?.name !== ASSISTANCE_SERVER_NAME) { throw new Error('The server is not AIRK Explorer Assistance.'); }
        const tools: string[] = (await session.request('tools/list', {}, remaining())).tools.map((tool: { name: string }) => tool.name);
        if (JSON.stringify(tools) !== JSON.stringify(ASSISTANCE_TOOLS)) { throw new Error(`Explorer Assistance tool inventory differs: ${tools.join(', ').slice(0, 300)}`); }
        const help = await session.request('tools/call', { name: 'airk_explorer_help_search', arguments: { query: 'search', limit: 1 } }, remaining());
        if (help.isError || !help.structuredContent?.results?.length) { throw new Error('Explorer Assistance help did not answer.'); }
        return { tools, version: String(opened.initialize.serverInfo.version ?? '') };
    } catch (error) {
        if (error instanceof McpSessionError && error.code === 'timeout') { throw new Error('Explorer Assistance check timed out.'); }
        throw error;
    } finally { session?.close(); }
}
