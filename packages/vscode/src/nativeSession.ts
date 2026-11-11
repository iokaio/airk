// SPDX-License-Identifier: Apache-2.0
//! The one temporary `airk service session` child of an editor window.
//!
//! Every native command and metadata relay of the window goes through this
//! child, so the window never starts one `airk` process per operation. The
//! child is never the service: it exits on stdin EOF (dispose) or when its
//! owner process ends, and it serves no evidence.
//!
//! Wire contract: crates/airk-cli/src/session_protocol.rs. Frames are a 4-byte
//! big-endian length followed by UTF-8 JSON. Reviewed cases shared with the
//! native definition: fixtures/wire/session_frames.json.
import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { createReadStream } from 'node:fs';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { Readable, Writable } from 'node:stream';
import { NativeProcessError, runText } from './native';
import { NativeExecutionDisabled, requireNativeExecution } from './nativeGate';
import { ControlTransport, NativeControlTransport, RecordValue, ServiceError, record } from './serviceClient';

export const SESSION_PROTOCOL = 1;
export const MAX_REQUEST_FRAME_BYTES = 192 * 1024;
export const MAX_INVOKE_STDIN_BYTES = 64 * 1024;
export const MAX_INVOKE_ARGS = 64;
export const MAX_INVOKE_ARG_BYTES = 32 * 1024;
export const MAX_RESPONSE_FRAME_BYTES = 16 * 1024 * 1024;
/** Retained diagnostic stderr per invocation (native keeps the tail). */
export const MAX_INVOKE_STDERR_BYTES = 64 * 1024;
export const MAX_IN_FLIGHT = 8;
export const MAX_INVOKE_DEADLINE_MS = 600_000;
/** A session that stayed up this long no longer counts toward restart backoff. */
const HEALTHY_MS = 60_000;

export type SessionMethod =
    | { hello: { protocol: number } }
    | { control: { runtime_dir: string; runtime_sha256?: string; method: unknown } }
    | { invoke: { argv: string[]; stdin?: string; deadline_ms: number } }
    | { cancel: { target: string } };
export type SessionOutcome =
    | { status: 'hello'; protocol: number; version: string; executable_sha256: string }
    | { status: 'control'; response: RecordValue }
    | { status: 'invoked'; success: boolean; stdout: string; stderr: string }
    | { status: 'failed'; code: string; retryable: boolean }
    | { status: 'cancelled' };
export interface SessionResponse { id: string; outcome: SessionOutcome }

/** Request builders. Optional fields are omitted, never sent as null. */
export const sessionRequests = {
    hello: (): SessionMethod => ({ hello: { protocol: SESSION_PROTOCOL } }),
    control: (runtimeDir: string, method: unknown, runtimeSha256?: string): SessionMethod =>
        ({ control: { runtime_dir: runtimeDir, ...(runtimeSha256 === undefined ? {} : { runtime_sha256: runtimeSha256 }), method } }),
    invoke: (argv: readonly string[], deadlineMs: number, stdin?: string): SessionMethod =>
        ({ invoke: { argv: [...argv], ...(stdin === undefined ? {} : { stdin }), deadline_ms: deadlineMs } }),
    cancel: (target: string): SessionMethod => ({ cancel: { target } }),
};

const ID = /^[0-9a-f]{32}$/;
const newId = () => randomUUID().replaceAll('-', '');
const utf8 = (value: unknown) => typeof value === 'string' ? Buffer.byteLength(value, 'utf8') : Infinity;
function fields(value: RecordValue, required: string[], optional: string[] = []): boolean {
    return required.every(key => Object.hasOwn(value, key) && value[key] !== undefined)
        && Object.keys(value).every(key => required.includes(key) || optional.includes(key));
}
/** Refuses any request the native definition would reject for its shape or
 * budgets, so this window never sends one. */
export function checkSessionRequest(input: unknown): { id: string; request: SessionMethod } {
    const invalid = () => new ServiceError('native_session_invalid_request');
    let value: RecordValue, request: RecordValue, body: RecordValue;
    try { value = record(input); request = record(value.request); } catch { throw invalid(); }
    if (!fields(value, ['id', 'request']) || typeof value.id !== 'string' || !ID.test(value.id)) { throw invalid(); }
    const kinds = Object.keys(request);
    if (kinds.length !== 1) { throw invalid(); }
    try { body = record(request[kinds[0]]); } catch { throw invalid(); }
    switch (kinds[0]) {
        case 'hello':
            if (!fields(body, ['protocol']) || body.protocol !== SESSION_PROTOCOL) { throw invalid(); }
            break;
        case 'control':
            if (!fields(body, ['runtime_dir', 'method'], ['runtime_sha256']) || typeof body.runtime_dir !== 'string' || !body.runtime_dir || utf8(body.runtime_dir) > 4096
                || body.runtime_sha256 !== undefined && (typeof body.runtime_sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(body.runtime_sha256))) { throw invalid(); }
            break;
        case 'invoke': {
            const argv = body.argv;
            if (!fields(body, ['argv', 'deadline_ms'], ['stdin']) || !Array.isArray(argv) || argv.length > MAX_INVOKE_ARGS || argv.some(arg => utf8(arg) > MAX_INVOKE_ARG_BYTES)
                || body.stdin !== undefined && utf8(body.stdin) > MAX_INVOKE_STDIN_BYTES
                || !Number.isSafeInteger(body.deadline_ms) || Number(body.deadline_ms) < 1 || Number(body.deadline_ms) > MAX_INVOKE_DEADLINE_MS) { throw invalid(); }
            break;
        }
        case 'cancel':
            if (!fields(body, ['target']) || typeof body.target !== 'string' || !ID.test(body.target)) { throw invalid(); }
            break;
        default: throw invalid();
    }
    return value as unknown as { id: string; request: SessionMethod };
}
/** Exact JSON body of one request frame. */
export function sessionRequestJson(id: string, request: SessionMethod): string {
    const text = JSON.stringify(checkSessionRequest({ id, request }));
    if (Buffer.byteLength(text, 'utf8') > MAX_REQUEST_FRAME_BYTES) { throw new ServiceError('native_session_request_oversize'); }
    return text;
}
export function encodeSessionRequest(id: string, request: SessionMethod): Buffer {
    const body = Buffer.from(sessionRequestJson(id, request), 'utf8');
    const frame = Buffer.alloc(4 + body.length); frame.writeUInt32BE(body.length); body.copy(frame, 4);
    return frame;
}
export function decodeSessionResponse(input: unknown): SessionResponse {
    const invalid = () => new ServiceError('native_session_invalid_response', true);
    let value: RecordValue, outcome: RecordValue;
    try { value = record(input); outcome = record(value.outcome); } catch { throw invalid(); }
    if (!fields(value, ['id', 'outcome']) || typeof value.id !== 'string' || !ID.test(value.id)) { throw invalid(); }
    switch (outcome.status) {
        case 'hello':
            if (!Number.isSafeInteger(outcome.protocol) || typeof outcome.version !== 'string' || outcome.version.length > 64
                || typeof outcome.executable_sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(outcome.executable_sha256)) { throw invalid(); }
            break;
        case 'control': {
            let response: RecordValue;
            try { response = record(outcome.response); } catch { throw invalid(); }
            if (typeof response.status !== 'string' || response.status.length > 64) { throw invalid(); }
            break;
        }
        case 'invoked':
            if (typeof outcome.success !== 'boolean' || typeof outcome.stdout !== 'string' || typeof outcome.stderr !== 'string') { throw invalid(); }
            break;
        case 'failed':
            if (typeof outcome.code !== 'string' || !/^[a-z0-9_]{1,64}$/.test(outcome.code) || typeof outcome.retryable !== 'boolean') { throw invalid(); }
            break;
        case 'cancelled': break;
        default: throw invalid();
    }
    return { id: value.id, outcome: outcome as unknown as SessionOutcome };
}
/** Incremental frame decoder; a zero or oversized length is a protocol violation. */
export class SessionFrameReader {
    private readonly prefix = Buffer.alloc(4);
    private prefixBytes = 0;
    private body?: Buffer;
    private bodyBytes = 0;
    constructor(private readonly limit = MAX_RESPONSE_FRAME_BYTES) {}
    push(bytes: Buffer): Buffer[] {
        const frames: Buffer[] = [];
        let offset = 0;
        while (offset < bytes.length) {
            if (!this.body) {
                const copied = Math.min(4 - this.prefixBytes, bytes.length - offset);
                bytes.copy(this.prefix, this.prefixBytes, offset, offset + copied);
                this.prefixBytes += copied; offset += copied;
                if (this.prefixBytes < 4) { continue; }
                const length = this.prefix.readUInt32BE();
                if (!length || length > this.limit) { throw new ServiceError('native_session_invalid_response', true); }
                this.body = Buffer.allocUnsafe(length); this.bodyBytes = 0;
            }
            const copied = Math.min(this.body.length - this.bodyBytes, bytes.length - offset);
            bytes.copy(this.body, this.bodyBytes, offset, offset + copied);
            this.bodyBytes += copied; offset += copied;
            if (this.bodyBytes === this.body.length) { frames.push(this.body); this.body = undefined; this.prefixBytes = 0; }
        }
        return frames;
    }
}

/** A failure of the session itself. `sent` reports whether the request reached
 * the child; only then can its native outcome be unknown. */
export class NativeSessionError extends ServiceError {
    constructor(code: string, readonly sent: boolean, outcomeUnknown = sent) { super(code, outcomeUnknown); this.name = 'NativeSessionError'; }
}
/** Session refusals that cannot have started the command (session_protocol.rs). */
const NO_EFFECT_FAILURES = new Set(['session_invalid_request', 'session_protocol_mismatch', 'session_command_refused', 'session_busy']);

/** The verified VSIX bundle image; `sha256` is its main executable digest. */
export interface SessionImage { readonly path: string; readonly sha256: string; readonly version: string }
export interface SessionChild {
    readonly stdin: Writable; readonly stdout: Readable; readonly stderr: Readable;
    readonly exitCode: number | null; readonly signalCode: NodeJS.Signals | null;
    kill(signal?: NodeJS.Signals): boolean;
    on(event: 'exit', listener: () => void): unknown;
    on(event: 'error', listener: (error: Error) => void): unknown;
}
/** Injected clock and timers; `background` timers never keep the host alive. */
export interface SessionTimers { set(callback: () => void, ms: number, background?: boolean): unknown; clear(handle: unknown): void; now(): number }
const systemTimers: SessionTimers = {
    set: (callback, ms, background) => { const timer = setTimeout(callback, ms); if (background) { timer.unref(); } return timer; },
    clear: handle => clearTimeout(handle as NodeJS.Timeout),
    now: () => Date.now(),
};
export interface NativeSessionOptions {
    launch?: (image: SessionImage) => SessionChild;
    timers?: SessionTimers;
    /** After stdin EOF, a child that has not exited is killed. */
    closeGraceMs?: number;
    /** A cancelled request must be answered within this grace, or the session is replaced. */
    cancelGraceMs?: number;
    helloTimeoutMs?: number;
    maxQueued?: number;
}
function launchSession(image: SessionImage): SessionChild {
    return spawn(image.path, ['service', 'session'], { cwd: path.dirname(image.path), windowsHide: true, stdio: 'pipe',
        env: { ...process.env, AIRK_CLIENT: 'vscode', AIRK_ERROR_FORMAT: 'json' } });
}
function samePath(left: string, right: string): boolean {
    const a = path.resolve(left), b = path.resolve(right);
    return process.platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b;
}
const restartDelay = (failures: number) => Math.min(30_000, 500 * 2 ** Math.max(0, failures - 1));

interface Exchange {
    id: string; request: SessionMethod; kind: 'invoke' | 'control'; deadlineAt: number; busyRetries: number;
    state: 'queued' | 'sent' | 'abandoned' | 'settled';
    timer?: unknown; retryTimer?: unknown; cancelTimer?: unknown;
    resolve(outcome: SessionOutcome): void; reject(error: Error): void;
}

/**
 * One `airk service session` child per activation. It starts lazily on first
 * use, multiplexes up to eight requests by id, and restarts after an
 * unexpected exit only on the next use, with backoff.
 */
export class NativeSession {
    private image?: SessionImage;
    private refused?: SessionImage;
    private child?: SessionChild;
    private ready = false;
    private helloId?: string;
    private helloTimer?: unknown;
    private readyAt = 0;
    private reader = new SessionFrameReader();
    private readonly queue: Exchange[] = [];
    private readonly live = new Set<Exchange>();
    private readonly sent = new Map<string, Exchange>();
    /** Cancel request id → its target. Each id is kept until its own answer
     * (`cancelled`, or a refusal when the target already finished). */
    private readonly cancels = new Map<string, string>();
    /** Children after stdin EOF that still owe answers for started work. */
    private readonly draining = new Map<SessionChild, { reader: SessionFrameReader; pending: Map<string, number>; timer?: unknown }>();
    private readonly digests = new Map<string, { key: string; sha256: string }>();
    private failures = 0;
    private notBefore = 0;
    private startTimer?: unknown;
    private disposed = false;
    private spawned = 0;
    private readonly timers: SessionTimers;
    private readonly launch: (image: SessionImage) => SessionChild;
    constructor(private readonly options: NativeSessionOptions = {}) {
        this.timers = options.timers ?? systemTimers;
        this.launch = options.launch ?? launchSession;
    }
    /** Children started by this session (diagnostic and test observation). */
    get launches(): number { return this.spawned; }
    get closed(): boolean { return this.disposed; }

    /** Bind the image this session runs: the SHA-verified VSIX bundle. A
     * different verified image retires the running child after its EOF. */
    offer(image: SessionImage): void {
        if (this.disposed) { return; }
        if (!path.isAbsolute(image.path) || !/^[0-9a-f]{64}$/.test(image.sha256) || typeof image.version !== 'string' || !image.version) {
            throw new ServiceError('invalid_session_image');
        }
        const current = this.image;
        if (current && current.sha256 === image.sha256 && current.version === image.version && samePath(current.path, image.path)) { return; }
        this.image = Object.freeze({ path: image.path, sha256: image.sha256, version: image.version });
        const child = this.child;
        if (child) {
            const pending = this.inFlight();
            this.detach(child, 'native_session_closed');
            this.release(child, pending);
            this.start();
        }
    }
    /** True when `command` is this session's image or a byte-identical copy
     * (the installed runtime of the same build). Anything else is a different
     * build that this session must not impersonate. */
    async serves(command: string): Promise<boolean> {
        const image = this.image;
        if (!image) { throw new NativeSessionError('native_session_image_unverified', false); }
        if (samePath(command, image.path)) { return true; }
        return await this.digest(command) === image.sha256;
    }
    /** Run one allowlisted command in the session; same stdout and error
     * semantics as the one-shot process. */
    async text(command: string, args: readonly string[], timeout = 300000, input?: string): Promise<string> {
        if (input !== undefined && Buffer.byteLength(input, 'utf8') > MAX_INVOKE_STDIN_BYTES) { throw new Error('AIRK native control input exceeds 64 KiB.'); }
        const deadline = Math.max(1, Math.min(timeout, MAX_INVOKE_DEADLINE_MS)), started = this.timers.now();
        const elapsed = () => Math.max(0, Math.round(this.timers.now() - started));
        let outcome: SessionOutcome;
        try { outcome = await this.exchange(sessionRequests.invoke(args, deadline, input), deadline); }
        catch (error) {
            if (error instanceof NativeSessionError && error.code === 'native_session_deadline_exceeded') { throw NativeProcessError.fromSession(command, '', elapsed(), timeout, true); }
            throw error;
        }
        if (outcome.status === 'invoked') {
            if (outcome.success) { return outcome.stdout; }
            throw NativeProcessError.fromSession(command, outcome.stderr, elapsed(), timeout);
        }
        if (outcome.status === 'failed') {
            if (outcome.code === 'session_deadline_exceeded') { throw NativeProcessError.fromSession(command, '', elapsed(), timeout, true); }
            throw new NativeSessionError(outcome.code, true, !NO_EFFECT_FAILURES.has(outcome.code));
        }
        throw new NativeSessionError('native_session_cancelled', true);
    }
    /** Instance-fenced metadata relay; returns the complete ControlResponse. */
    async control(runtimeDir: string, runtimeSha256: string | undefined, method: unknown, timeout: number, signal?: AbortSignal): Promise<RecordValue> {
        const outcome = await this.exchange(sessionRequests.control(runtimeDir, method, runtimeSha256), timeout, signal);
        if (outcome.status === 'control') { return outcome.response; }
        if (outcome.status === 'failed') { throw new NativeSessionError(outcome.code, true, !NO_EFFECT_FAILURES.has(outcome.code)); }
        throw new NativeSessionError('native_session_cancelled', true);
    }
    /** Queue one request; `session_busy` alone is retried, with backoff, within its deadline. */
    exchange(request: SessionMethod, timeoutMs: number, signal?: AbortSignal): Promise<SessionOutcome> {
        if (this.disposed) { return Promise.reject(new NativeSessionError('native_session_disposed', false)); }
        try { requireNativeExecution(); } catch (error) { return Promise.reject(error); }
        const kind = 'invoke' in request ? 'invoke' : 'control' in request ? 'control' : undefined;
        if (!kind) { return Promise.reject(new ServiceError('native_session_invalid_request')); }
        if (this.queue.length >= (this.options.maxQueued ?? 64)) { return Promise.reject(new NativeSessionError('native_session_queue_full', false)); }
        if (signal?.aborted) { return Promise.reject(new NativeSessionError('native_session_aborted', false)); }
        // Shape and budget refusals happen before anything is queued or sent.
        try { encodeSessionRequest(newId(), request); } catch (error) { return Promise.reject(error); }
        return new Promise((resolve, reject) => {
            const entry: Exchange = { id: '', request, kind, deadlineAt: this.timers.now() + timeoutMs, busyRetries: 0, state: 'queued', resolve, reject };
            entry.timer = this.timers.set(() => { entry.timer = undefined; this.abandon(entry, 'native_session_deadline_exceeded'); }, Math.max(1, timeoutMs));
            signal?.addEventListener('abort', () => this.abandon(entry, 'native_session_aborted'), { once: true });
            this.live.add(entry); this.queue.push(entry);
            this.start(); this.pump();
        });
    }
    /** Close stdin (EOF ends the session) and kill it after a bounded grace. */
    dispose(): void {
        if (this.disposed) { return; }
        this.disposed = true;
        if (this.startTimer !== undefined) { this.timers.clear(this.startTimer); this.startTimer = undefined; }
        for (const entry of [...this.live]) { const sent = entry.state === 'sent'; this.settle(entry); entry.reject(new NativeSessionError('native_session_disposed', sent)); }
        this.queue.splice(0);
        const child = this.child;
        if (child) { const pending = this.inFlight(); this.detach(child, 'native_session_disposed'); this.release(child, pending); }
    }

    private settle(entry: Exchange): void {
        if (entry.state !== 'abandoned') { entry.state = 'settled'; }
        this.live.delete(entry);
        for (const timer of [entry.timer, entry.retryTimer]) { if (timer !== undefined) { this.timers.clear(timer); } }
        entry.timer = entry.retryTimer = undefined;
        const index = this.queue.indexOf(entry);
        if (index >= 0) { this.queue.splice(index, 1); }
    }
    /** Deadline or caller abort. Unsent work is withdrawn; sent work is
     * cancelled cooperatively and keeps its slot until the child answers. */
    private abandon(entry: Exchange, code: string): void {
        if (entry.state === 'queued') { this.settle(entry); entry.reject(new NativeSessionError(code, false)); return; }
        if (entry.state !== 'sent') { return; }
        this.settle(entry); entry.state = 'abandoned';
        entry.reject(new NativeSessionError(code, true));
        const child = this.child;
        if (!child || !this.ready) { return; }
        const id = newId();
        // Bounded: a child that never answers cancels cannot grow this map.
        if (this.cancels.size >= 64) { this.cancels.delete(this.cancels.keys().next().value!); }
        this.cancels.set(id, entry.id);
        child.stdin.write(encodeSessionRequest(id, sessionRequests.cancel(entry.id)), error => { if (error) { this.lost(child, 'native_session_closed'); } });
        entry.cancelTimer = this.timers.set(() => {
            entry.cancelTimer = undefined;
            if (this.sent.get(entry.id) === entry) { this.lost(child, 'native_session_unresponsive'); }
        }, this.options.cancelGraceMs ?? 30_000);
    }
    private start(): void {
        if (this.disposed || this.child || this.startTimer !== undefined || !this.queue.length) { return; }
        const wait = this.notBefore - this.timers.now();
        if (wait > 0) { this.startTimer = this.timers.set(() => { this.startTimer = undefined; this.start(); this.pump(); }, wait); return; }
        const image = this.image;
        if (!image) { this.failQueued(() => new NativeSessionError('native_session_image_unverified', false)); return; }
        // A child that answered a different identity is never started again for that image.
        if (this.refused === image) { this.failQueued(() => new NativeSessionError('native_session_identity_mismatch', false)); return; }
        let child: SessionChild;
        try { requireNativeExecution(); child = this.launch(image); }
        catch (error) {
            this.failures++; this.notBefore = this.timers.now() + restartDelay(this.failures);
            this.failQueued(() => error instanceof NativeExecutionDisabled ? error
                : Object.assign(new NativeSessionError('native_session_unavailable', false), { cause: error }));
            return;
        }
        this.spawned++; this.child = child; this.ready = false; this.reader = new SessionFrameReader();
        child.stdout.on('data', (bytes: Buffer) => this.receive(child, bytes));
        // Session diagnostics arrive as structured results; raw stderr is never forwarded.
        child.stderr.resume();
        child.on('error', () => this.lost(child, 'native_session_unavailable'));
        child.on('exit', () => this.lost(child, 'native_session_closed'));
        child.stdin.on('error', () => this.lost(child, 'native_session_closed'));
        const helloId = newId(); this.helloId = helloId;
        this.helloTimer = this.timers.set(() => this.lost(child, 'native_session_unresponsive', true), this.options.helloTimeoutMs ?? 30_000);
        child.stdin.write(encodeSessionRequest(helloId, sessionRequests.hello()), error => { if (error) { this.lost(child, 'native_session_closed'); } });
    }
    private pump(): void {
        const child = this.child;
        if (!child || !this.ready || this.disposed) { return; }
        while (this.queue.length && this.sent.size < MAX_IN_FLIGHT) {
            const entry = this.queue.shift()!;
            if ('invoke' in entry.request) {
                // The native deadline is what remains of this request's own deadline.
                const remaining = Math.max(1, Math.min(entry.request.invoke.deadline_ms, Math.round(entry.deadlineAt - this.timers.now())));
                entry.request = { invoke: { ...entry.request.invoke, deadline_ms: remaining } };
            }
            entry.id = newId(); entry.state = 'sent'; this.sent.set(entry.id, entry);
            child.stdin.write(encodeSessionRequest(entry.id, entry.request), error => { if (error) { this.lost(child, 'native_session_closed'); } });
        }
    }
    private receive(child: SessionChild, bytes: Buffer): void {
        if (child !== this.child) { this.drained(child, bytes); return; }
        let frames: Buffer[];
        try { frames = this.reader.push(bytes); } catch { this.lost(child, 'native_session_invalid_response'); return; }
        for (const frame of frames) {
            if (child !== this.child) { return; }
            let response: SessionResponse;
            try { response = decodeSessionResponse(JSON.parse(frame.toString('utf8'))); }
            catch { this.lost(child, 'native_session_invalid_response'); return; }
            this.dispatch(child, response);
        }
    }
    private dispatch(child: SessionChild, { id, outcome }: SessionResponse): void {
        if (!this.ready) {
            const image = this.image;
            if (id !== this.helloId || outcome.status !== 'hello' || !image || outcome.protocol !== SESSION_PROTOCOL
                || outcome.version !== image.version || outcome.executable_sha256 !== image.sha256) {
                this.refused = image;
                this.lost(child, 'native_session_identity_mismatch', true); return;
            }
            if (this.helloTimer !== undefined) { this.timers.clear(this.helloTimer); this.helloTimer = undefined; }
            this.ready = true; this.readyAt = this.timers.now(); this.pump(); return;
        }
        if (this.cancels.has(id)) {
            // The answer to a cancel itself, in either order relative to its target.
            if (outcome.status !== 'cancelled' && outcome.status !== 'failed') { this.lost(child, 'native_session_invalid_response'); return; }
            this.cancels.delete(id); return;
        }
        const entry = this.sent.get(id);
        const allowed = entry?.kind === 'invoke' ? ['invoked', 'failed', 'cancelled'] : ['control', 'failed', 'cancelled'];
        if (!entry || !allowed.includes(outcome.status)) { this.lost(child, 'native_session_invalid_response'); return; }
        this.sent.delete(id);
        if (entry.cancelTimer !== undefined) { this.timers.clear(entry.cancelTimer); entry.cancelTimer = undefined; }
        if (entry.state === 'sent' && outcome.status === 'failed' && outcome.code === 'session_busy' && outcome.retryable) {
            const delay = Math.min(1000, 25 * 2 ** entry.busyRetries++);
            if (this.timers.now() + delay < entry.deadlineAt) {
                entry.state = 'queued';
                entry.retryTimer = this.timers.set(() => {
                    entry.retryTimer = undefined;
                    if (entry.state === 'queued' && this.live.has(entry)) { this.queue.unshift(entry); this.start(); this.pump(); }
                }, delay);
                this.pump(); return;
            }
        }
        if (entry.state === 'sent') { this.settle(entry); entry.resolve(outcome); }
        this.pump();
    }
    /** Fail what the child received, keep unsent work for the next child. */
    private detach(child: SessionChild, code: string): void {
        if (child !== this.child) { return; }
        this.child = undefined; this.ready = false; this.helloId = undefined;
        if (this.helloTimer !== undefined) { this.timers.clear(this.helloTimer); this.helloTimer = undefined; }
        for (const entry of this.sent.values()) {
            if (entry.cancelTimer !== undefined) { this.timers.clear(entry.cancelTimer); entry.cancelTimer = undefined; }
            if (entry.state === 'sent') { this.settle(entry); entry.reject(new NativeSessionError(code, true)); }
        }
        this.sent.clear(); this.cancels.clear();
    }
    private lost(child: SessionChild, code: string, failQueued = false): void {
        if (child !== this.child) { return; }
        const wasReady = this.ready;
        this.detach(child, code);
        if (child.exitCode === null && child.signalCode === null) { child.kill(); }
        const now = this.timers.now();
        if (wasReady && now - this.readyAt >= HEALTHY_MS) { this.failures = 0; }
        this.failures++; this.notBefore = now + restartDelay(this.failures);
        if (failQueued) { this.failQueued(() => new NativeSessionError(code, false)); }
        this.start();
    }
    private failQueued(error: () => Error): void {
        for (const entry of [...this.live]) {
            if (entry.state === 'queued') { this.settle(entry); entry.reject(error()); }
        }
    }
    /** Started work and its deadline: the child drains it after EOF. */
    private inFlight(): Map<string, number> {
        return new Map([...this.sent].map(([id, entry]) => [id, entry.deadlineAt]));
    }
    /** Close stdin (EOF). Work the child already started finishes within its
     * own deadline (native drain contract), so the child is killed only if it
     * is still alive a bounded grace after its last started request answered
     * or passed its deadline. */
    private release(child: SessionChild, pending: Map<string, number>): void {
        const drain: { reader: SessionFrameReader; pending: Map<string, number>; timer?: unknown } = { reader: this.reader, pending };
        this.draining.set(child, drain);
        this.armDrain(child, drain);
        child.on('exit', () => { if (drain.timer !== undefined) { this.timers.clear(drain.timer); } this.draining.delete(child); });
        child.stdin.end();
    }
    private armDrain(child: SessionChild, drain: { pending: Map<string, number>; timer?: unknown }): void {
        if (drain.timer !== undefined) { this.timers.clear(drain.timer); }
        const now = this.timers.now(), latest = Math.max(now, ...drain.pending.values());
        drain.timer = this.timers.set(() => {
            this.draining.delete(child);
            if (child.exitCode === null && child.signalCode === null) { child.kill(); }
        }, latest - now + (this.options.closeGraceMs ?? 5000), true);
    }
    /** Answers from a draining child only shorten its grace; callers were already settled. */
    private drained(child: SessionChild, bytes: Buffer): void {
        const drain = this.draining.get(child);
        if (!drain) { return; }
        let frames: Buffer[];
        try { frames = drain.reader.push(bytes); } catch { return; }
        let answered = false;
        for (const frame of frames) {
            try { answered = drain.pending.delete(decodeSessionResponse(JSON.parse(frame.toString('utf8'))).id) || answered; } catch { return; }
        }
        if (answered && !drain.pending.size) { this.armDrain(child, drain); }
    }
    private async digest(file: string): Promise<string | undefined> {
        const resolved = path.resolve(file);
        let info;
        try { info = await fs.stat(resolved); } catch { return undefined; }
        if (!info.isFile() || info.size > 2 * 1024 * 1024 * 1024) { return undefined; }
        const key = `${info.dev}:${info.ino}:${info.size}:${info.mtimeMs}`, cached = this.digests.get(resolved);
        if (cached?.key === key) { return cached.sha256; }
        const hash = createHash('sha256');
        try { for await (const block of createReadStream(resolved)) { hash.update(block as Buffer); } } catch { return undefined; }
        const sha256 = hash.digest('hex');
        if (this.digests.size >= 16) { this.digests.delete(this.digests.keys().next().value!); }
        this.digests.set(resolved, { key, sha256 });
        return sha256;
    }
}

const CONSOLE_CODES: Record<string, string> = {
    native_session_deadline_exceeded: 'console_deadline_exceeded',
    native_session_invalid_response: 'invalid_console_response',
    native_session_queue_full: 'console_queue_full',
    native_session_aborted: 'console_client_closed',
    native_session_image_unverified: 'console_transport_unavailable',
    native_session_unavailable: 'console_transport_unavailable',
};
/**
 * One ServiceClient's view of the window session: requests stay sequential
 * per view, with the former console's codes, queue bound and deadline. A
 * runtime that is not this session's build keeps the former per-client
 * console process (explicit different-build fallback).
 */
export class SessionControlTransport implements ControlTransport {
    private readonly queue: { method: unknown; resolve(value: RecordValue): void; reject(error: Error): void }[] = [];
    private active?: AbortController;
    private stopped = false;
    private route?: Promise<ControlTransport | undefined>;
    private fallback?: ControlTransport;
    constructor(private readonly session: NativeSession, private readonly executable: string, private readonly runtimeDir: string,
        private readonly runtimeSha256: string, private readonly timeout = 20000,
        private readonly differentBuild: () => ControlTransport = () => new NativeControlTransport(executable,
            ['service', 'console', '--runtime-dir', runtimeDir, '--runtime-sha256', runtimeSha256], timeout)) {
        requireNativeExecution();
    }
    request(method: unknown): Promise<RecordValue> {
        if (this.stopped) { return Promise.reject(new ServiceError('console_transport_closed')); }
        if (this.queue.length + (this.active ? 1 : 0) >= 8) { return Promise.reject(new ServiceError('console_queue_full')); }
        // The service control frame keeps its 64 KiB request budget.
        if (Buffer.byteLength(JSON.stringify({ id: '0'.repeat(32), method }), 'utf8') > 65536) { return Promise.reject(new ServiceError('console_request_oversize')); }
        return new Promise((resolve, reject) => { this.queue.push({ method, resolve, reject }); void this.pump(); });
    }
    dispose(): void {
        if (this.stopped) { return; }
        this.stopped = true;
        for (const pending of this.queue.splice(0)) { pending.reject(new ServiceError('console_client_closed')); }
        this.active?.abort();
        this.fallback?.dispose();
    }
    private async pump(): Promise<void> {
        if (this.stopped || this.active || !this.queue.length) { return; }
        const next = this.queue.shift()!, active = new AbortController(); this.active = active;
        let value: RecordValue;
        try {
            this.route ??= this.session.serves(this.executable).then(same => same ? undefined : (this.fallback = this.differentBuild()));
            const fallback = await this.route;
            if (active.signal.aborted) { throw new NativeSessionError('native_session_aborted', false); }
            value = fallback ? await fallback.request(next.method)
                : await this.session.control(this.runtimeDir, this.runtimeSha256, next.method, this.timeout, active.signal);
        } catch (error) { this.active = undefined; this.fail(next, error); return; }
        this.active = undefined; next.resolve(value); void this.pump();
    }
    private fail(active: { reject(error: Error): void }, error: unknown): void {
        let code: string, failure: Error;
        if (error instanceof NativeSessionError) {
            code = CONSOLE_CODES[error.code] ?? (error.code.startsWith('native_session_') ? 'console_transport_closed' : error.code);
            failure = Object.assign(new ServiceError(code, error.outcomeUnknown), { cause: error });
        } else if (error instanceof ServiceError) { code = error.code; failure = error; }
        else { code = 'console_transport_unavailable'; failure = error instanceof Error ? error : new ServiceError(code); }
        active.reject(failure);
        if (this.stopped) { return; }
        this.stopped = true;
        for (const pending of this.queue.splice(0)) { pending.reject(new ServiceError(code)); }
        this.fallback?.dispose();
    }
}

let current: NativeSession | undefined;
/** The extension activation owns exactly one session; disposing it ends the child. */
export function openWindowSession(options?: NativeSessionOptions): { dispose(): void } {
    current?.dispose();
    const session = new NativeSession(options); current = session;
    return { dispose: () => { if (current === session) { current = undefined; } session.dispose(); } };
}
export function closeWindowSession(): void { const session = current; current = undefined; session?.dispose(); }
export function windowNativeSession(): NativeSession | undefined { return current; }

/** Run one native AIRK command for this window and return its stdout. */
export async function nativeText(command: string, args: readonly string[], timeout = 300000, input?: string): Promise<string> {
    const session = current;
    if (!session) { return windowlessOneShot(command, args, timeout, input); }
    if (await session.serves(command)) { return session.text(command, args, timeout, input); }
    return differentBuildOneShot(command, args, timeout, input);
}
/** Outside an editor window (the uninstall worker) there is no session: the
 * command is one temporary process that exits with it. */
function windowlessOneShot(command: string, args: readonly string[], timeout: number, input?: string): Promise<string> {
    return runText(command, [...args], path.dirname(command), timeout, undefined, undefined, input);
}
/** Explicit fallback for an image that is not this session's build, such as a
 * different-version predecessor being inspected or retired: this session
 * cannot run its code, so the command is one temporary process that exits. */
function differentBuildOneShot(command: string, args: readonly string[], timeout: number, input?: string): Promise<string> {
    return runText(command, [...args], path.dirname(command), timeout, undefined, undefined, input);
}
