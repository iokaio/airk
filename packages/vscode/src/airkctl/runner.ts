// SPDX-License-Identifier: Apache-2.0
//! The only place the extension starts a process (plan §4.4 "one spawning
//! module"). Reviewed roster, pinned by src/test/explorerBoundaries.test.ts:
//! the verified `airkctl` (one-shots and `session`), Windows PowerShell for
//! the Authenticode and ACL observation and macOS `codesign` before the first
//! `airkctl` execution. Explicit bootstrap additionally runs its verified official
//! installer and fixed OS archive/mount/ACL tools. None is another AIRK transport.
import { spawn } from 'node:child_process';
import type { ErrorLine, ExitClass } from '../generated/i2';

export const EXIT_CLASSES: readonly ExitClass[] = ['success', 'refused', 'usage', 'service_missing', 'incompatible_pair',
    'unsupported_capability', 'outcome_unknown', 'installation_unverified', 'internal'];
/** I2 exit code → class (contracts §3.4). Unknown codes are `internal`. */
export function exitClass(code: number | null): ExitClass {
    return code !== null && Number.isInteger(code) && code >= 0 && code < EXIT_CLASSES.length ? EXIT_CLASSES[code] : 'internal';
}
/** Whether a mutation may have happened after this class (contracts §3.4). */
export function mutationPossible(klass: ExitClass): 'yes' | 'no' | 'maybe' | 'unknown' {
    return klass === 'success' ? 'yes' : klass === 'outcome_unknown' ? 'maybe' : klass === 'internal' ? 'unknown' : 'no';
}

export const MAX_ERROR_MESSAGE_CHARS = 4096;
const ERROR_KEYS = new Set(['airkctl_error', 'class', 'code', 'retryable', 'retry_after_ms', 'message', 'native']);
/** Decode and validate one error line exactly as `ErrorLine::validate` does;
 * returns undefined for anything else (the line is then plain stderr). */
export function parseErrorLine(input: unknown): ErrorLine | undefined {
    if (!input || typeof input !== 'object' || Array.isArray(input)) { return undefined; }
    const line = input as Record<string, unknown>;
    if (Object.keys(line).some(key => !ERROR_KEYS.has(key))) { return undefined; }
    if (line.airkctl_error !== 1 || typeof line.code !== 'string' || typeof line.message !== 'string' || typeof line.retryable !== 'boolean') { return undefined; }
    if (typeof line.class !== 'string' || !EXIT_CLASSES.includes(line.class as ExitClass) || line.class === 'success') { return undefined; }
    if (!/^[a-z0-9_]{1,128}$/.test(line.code) || [...line.message].length > MAX_ERROR_MESSAGE_CHARS) { return undefined; }
    if (line.retry_after_ms !== undefined && line.retry_after_ms !== null
        && (typeof line.retry_after_ms !== 'number' || !Number.isSafeInteger(line.retry_after_ms) || line.retry_after_ms < 0)) { return undefined; }
    return line as unknown as ErrorLine;
}
/** Split stderr into its text and the trailing error line, as session.rs does. */
export function splitErrorLine(stderr: string): { stderr: string; error?: ErrorLine } {
    const body = stderr.endsWith('\n') ? stderr.slice(0, -1) : stderr;
    const start = body.lastIndexOf('\n') + 1;
    let error: ErrorLine | undefined;
    try { error = parseErrorLine(JSON.parse(body.slice(start))); } catch { error = undefined; }
    return error ? { stderr: stderr.slice(0, start), error } : { stderr };
}

export interface ProcessResult {
    readonly exitCode: number | null; readonly signal: string | null;
    readonly stdout: Buffer; readonly stderr: Buffer;
    readonly timedOut: boolean; readonly overflow: boolean;
}
export interface ProcessOptions {
    readonly env: NodeJS.ProcessEnv; readonly cwd?: string; readonly input?: string;
    readonly timeoutMs: number; readonly maxStdout: number; readonly maxStderr: number;
    /** Admitted setup owns cancellation/recovery; observation limits must not kill it. */
    readonly preserveProcessOnLimit?: boolean;
}
export type Spawner = (file: string, args: readonly string[], options: ProcessOptions) => Promise<ProcessResult>;

/** One bounded child process; no shell or window. Setup may retain process ownership. */
export const spawnBounded: Spawner = (file, args, options) => new Promise(resolve => {
    let child: ReturnType<typeof spawn>;
    try {
        child = spawn(file, [...args], { cwd: options.cwd, env: options.env, shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    } catch {
        resolve({ exitCode: null, signal: null, stdout: Buffer.alloc(0), stderr: Buffer.from('spawn failed'), timedOut: false, overflow: false });
        return;
    }
    const out: Buffer[] = [], err: Buffer[] = [];
    let outBytes = 0, errBytes = 0, overflow = false, timedOut = false, settled = false;
    const finish = (exitCode: number | null, signal: string | null) => {
        if (settled) { return; } settled = true; clearTimeout(timer);
        resolve({ exitCode, signal, stdout: Buffer.concat(out), stderr: Buffer.concat(err), timedOut, overflow });
    };
    const limit = () => {
        if (options.preserveProcessOnLimit) {
            child.unref();
            (child.stdout as unknown as {unref?():void}).unref?.();
            (child.stderr as unknown as {unref?():void}).unref?.();
            finish(null,null);
        }
        else { child.kill(); }
    };
    const timer = setTimeout(() => { timedOut = true; limit(); }, Math.max(1, options.timeoutMs));
    child.stdout!.on('data', (bytes: Buffer) => {
        if (settled) { return; }
        outBytes += bytes.length;
        if (outBytes > options.maxStdout) { overflow = true; limit(); return; }
        out.push(bytes);
    });
    child.stderr!.on('data', (bytes: Buffer) => {
        if (settled) { return; }
        // Keep the tail: the error line is the last stderr line.
        err.push(bytes); errBytes += bytes.length;
        while (errBytes > options.maxStderr && err.length > 1) { errBytes -= err.shift()!.length; }
    });
    child.on('error', () => finish(null, null));
    child.on('close', (code, signal) => finish(code, signal));
    child.stdin!.on('error', () => { /* Reported by the exit status. */ });
    child.stdin!.end(options.input ?? '');
});

/** The environment every `airkctl` child receives: the caller's, with the
 * client marker and JSON error lines. */
export function airkctlEnvironment(base: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
    return { ...base, AIRK_CLIENT: 'vscode', AIRK_ERROR_FORMAT: 'json' };
}
