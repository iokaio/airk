// SPDX-License-Identifier: Apache-2.0
import { execFile, ExecFileException, spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { LaunchSpec, launchArgs } from './model';

export interface IndexProgress {
    phase: string; revision: string | null; processed_artifacts: number; committed_artifacts: number;
    selected_artifacts: number | null; completed_artifacts: number | null;
}

export function parseIndexProgress(line: string): IndexProgress | undefined {
    if (line.length > 4096) { return undefined; }
    try {
        const value: unknown = JSON.parse(line);
        if (!value || typeof value !== 'object' || Array.isArray(value)) { return undefined; }
        const event = value as Record<string, unknown>;
        const count = (field: string) => Number.isSafeInteger(event[field]) && Number(event[field]) >= 0;
        if (!['scan_started', 'base_committed', 'enrichment_committed', 'base_coverage_complete'].includes(String(event.phase))
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
    constructor(command: string, error: ExecFileException, stderr: string, readonly elapsedMs: number, readonly timeoutMs: number) {
        let nativeMessage: string | undefined, retryableGateContention = false, pendingIndexRevision: number | undefined;
        if (error.code === 1 && !error.killed && !error.signal) {
            try {
                // execFile already bounds captured output. Parse before applying
                // the diagnostic display limit: JSON escaping can expand a valid
                // 4,096-character native message beyond that limit.
                const lines = stderr.trim().split(/\r?\n/);
                const structured = lines.length === 1 || lines.slice(0, -1).every(line => parseIndexProgress(line));
                const outcome: unknown = structured ? JSON.parse(lines.at(-1)!) : undefined;
                if (outcome && typeof outcome === 'object' && !Array.isArray(outcome)) {
                    const fields = outcome as Record<string, unknown>;
                    if (fields.airk_error === 1 && typeof fields.code === 'string'
                        && typeof fields.retryable === 'boolean' && typeof fields.message === 'string') {
                        nativeMessage = boundedDiagnostic(fields.message);
                        retryableGateContention = fields.code === 'current_gate_busy' && fields.retryable;
                        if (fields.code === 'scheduled_index_pending' && fields.retryable
                            && Number.isSafeInteger(fields.requested_revision) && Number(fields.requested_revision) > 0) {
                            pendingIndexRevision = Number(fields.requested_revision);
                        }
                    }
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
        this.pendingIndexRevision = pendingIndexRevision;
    }
}

export function runText(command: string, args: string[], cwd?: string, timeout = 300000,
    onStderrLine?: (line: string) => boolean, idleTimeout = 0): Promise<string> {
    return new Promise((resolve, reject) => {
        const started = performance.now();
        let idleExpired = false;
        let idleTimer: NodeJS.Timeout | undefined;
        const child = execFile(command, args, { cwd, env: { ...process.env, AIRK_CLIENT: 'vscode', AIRK_ERROR_FORMAT: 'json' }, windowsHide: true, maxBuffer: 16 * 1024 * 1024, timeout }, (error, stdout, stderr) => {
            if (idleTimer) { clearTimeout(idleTimer); }
            if (idleExpired) { reject(new Error(`AIRK index stopped after ${idleTimeout} ms without progress. Committed evidence remains available; use Diagnose & Repair, then retry indexing.`)); return; }
            if (error) { reject(new NativeProcessError(command, error, stderr, Math.round(performance.now() - started), timeout)); }
            else { resolve(stdout); }
        });
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

/** Continue the same durable request after a bounded native join expires. */
export async function joinScheduledIndex<T>(initialArgs: string[],
    run: (command: string, args: string[]) => Promise<T>, onPending: () => void): Promise<T> {
    let revision: number | undefined;
    for (;;) {
        try { return await run(revision === undefined ? 'index' : 'schedule',
            revision === undefined ? initialArgs : ['wait', String(revision), '--progress']); }
        catch (error) {
            if (!(error instanceof NativeProcessError) || error.pendingIndexRevision === undefined) { throw error; }
            if (revision !== undefined && error.pendingIndexRevision !== revision) { throw new Error('Native indexing wait returned a different request revision.'); }
            revision = error.pendingIndexRevision;
            // The caller rechecks trust, shutdown and collection binding between joins.
            onPending();
        }
    }
}

/** Real bounded stdio initialization/tool-list/status probe, independent of any coding agent. */
export function probeMcp(spec: LaunchSpec, expectedTools: 14 | 15 | 16 = 15, clientVersion = 'development'): Promise<{ tools: string[]; status: unknown }> {
    return new Promise((resolve, reject) => {
        const child = spawn(spec.command, launchArgs(spec), { cwd: spec.root, env: { ...process.env, AIRK_CLIENT: 'vscode' }, windowsHide: true, stdio: 'pipe' });
        let buffer = '', stderr = '', total = 0, done = false;
        const requests = new Map<number, { resolve(value: any): void; reject(error: Error): void }>();
        const finish = (error?: Error, result?: { tools: string[]; status: unknown }) => {
            if (done) { return; } done = true; clearTimeout(timer);
            child.stdin.end(); child.kill();
            for (const request of requests.values()) { request.reject(error ?? new Error('Probe finished.')); }
            requests.clear();
            if (error) { reject(error); } else { resolve(result!); }
        };
        const timer = setTimeout(() => finish(new Error('AIRK MCP probe timed out.')), 15000);
        child.on('error', error => finish(error));
        child.on('exit', code => { if (!done) { finish(new Error(`AIRK MCP exited (${code}): ${stderr}`)); } });
        child.stdin.on('error', error => finish(error));
        child.stdout.setEncoding('utf8');
        child.stderr.on('data', bytes => { stderr = (stderr + bytes.toString()).slice(-4096); });
        child.stdout.on('data', bytes => {
            total += Buffer.byteLength(bytes);
            if (total > 1024 * 1024) { finish(new Error('AIRK MCP probe response exceeded budget.')); return; }
            buffer += bytes.toString();
            let end: number;
            while ((end = buffer.indexOf('\n')) >= 0) {
                const line = buffer.slice(0, end); buffer = buffer.slice(end + 1);
                try {
                    const message = JSON.parse(line);
                    const request = requests.get(message.id);
                    if (request) {
                        requests.delete(message.id);
                        if (message.error) { request.reject(new Error(JSON.stringify(message.error))); }
                        else { request.resolve(message.result); }
                    }
                } catch { finish(new Error('AIRK MCP emitted invalid JSON.')); return; }
            }
        });
        const request = (id: number, method: string, params: unknown): Promise<any> => new Promise((resolveRequest, rejectRequest) => {
            requests.set(id, { resolve: resolveRequest, reject: rejectRequest });
            child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
        });
        void (async () => {
            await request(1, 'initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'airk-extension-check', version: clientVersion } });
            child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
            const listed = await request(2, 'tools/list', {});
            const tools: string[] = listed.tools.map((tool: { name: string }) => tool.name);
            const required = ['airk_status', 'airk_symbols', 'airk_code_facts', 'airk_relationships', ...(expectedTools >= 15 ? ['airk_changes'] : []), ...(expectedTools === 16 ? ['airk_history'] : [])];
            if (tools.length !== expectedTools || !required.every(name => tools.includes(name))) { throw new Error(`AIRK MCP tool inventory differs from the expected ${expectedTools} tools.`); }
            const status = await request(3, 'tools/call', { name: 'airk_status', arguments: {} });
            if (status.isError) {
                const message = status.content?.map((part: { text?: string }) => part.text ?? '').join('\n') ?? '';
                if (!message.includes('workspace is not indexed')) { throw new Error(`AIRK MCP status failed: ${message.slice(0, 300)}`); }
                finish(undefined, { tools, status: { state: 'not_indexed' } }); return;
            }
            finish(undefined, { tools, status });
        })().catch(error => finish(error instanceof Error ? error : new Error(String(error))));
    });
}
