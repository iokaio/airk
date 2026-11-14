// SPDX-License-Identifier: Apache-2.0
//! The window's `airkctl session --owner-watch` child, session protocol 2
//! (contracts §3.7–§3.8; native owner crates/airkctl/src/cli/session.rs).
//!
//! Frames are a 4-byte big-endian length followed by UTF-8 JSON. Replies are
//! `{id, outcome}`; unsolicited events are `{event}` (disjoint keys). The
//! window lease, its renewal and re-opening after a service restart live in
//! the child; this client sees only `hello.window_id` and `resync` events.
//! Rules kept from protocol 1: `hello` first; at most eight requests in
//! flight; `session_busy` alone is retried within the caller's deadline; a
//! deadline or abort sends `cancel` and keeps the slot until the child
//! answers; replies above 16 MiB are refused by the child, never truncated.
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import * as path from 'node:path';
import type { Readable, Writable } from 'node:stream';
import type {
    ErrorLine, ExitClass, SessionCursor, SessionEvent, SessionMethod, SessionOutcome, WindowSelection, EventTopic,
} from '../generated/i2';
import { airkctlEnvironment, EXIT_CLASSES, parseErrorLine } from './runner';
import { createAdminChunks } from './adminChunks';

export const SESSION_PROTOCOL = 2;
export const CLIENT_NAME = 'airk-vscode';
export const MAX_REQUEST_FRAME_BYTES = 192 * 1024;
export const MAX_INVOKE_STDIN_BYTES = 64 * 1024;
export const MAX_INVOKE_ARGS = 64;
export const MAX_INVOKE_ARG_BYTES = 32 * 1024;
export const MAX_RESPONSE_FRAME_BYTES = 16 * 1024 * 1024;
export const MAX_IN_FLIGHT = 8;
export const MAX_INVOKE_DEADLINE_MS = 120_000;
const HEALTHY_MS = 60_000;
const ID = /^[0-9a-f]{32}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const newId = () => randomUUID().replaceAll('-', '');
const utf8 = (value: string) => Buffer.byteLength(value, 'utf8');

/** A failure of the session or the request, with the I2 code. `sent` tells
 * whether the child received the request (only then can a mutation's outcome
 * be unknown). */
export class SessionError extends Error {
    constructor(readonly code: string, readonly sent: boolean, readonly retryable = false, readonly outcomeUnknown = sent) {
        super(code); this.name = 'SessionError';
    }
}
/** Session refusals that cannot have started the command. */
const NO_EFFECT = new Set(['session_invalid_request', 'session_protocol_mismatch', 'session_command_refused', 'session_busy', 'event_cursor_scope_mismatch']);

/** The verified image the session runs (from ./verify.ts). */
export interface SessionImage { readonly path: string; readonly sha256: string; readonly version: string }
export interface SessionChild {
    readonly stdin: Writable; readonly stdout: Readable; readonly stderr: Readable;
    readonly exitCode: number | null; readonly signalCode: NodeJS.Signals | null;
    kill(signal?: NodeJS.Signals): boolean;
    on(event: 'exit', listener: () => void): unknown;
    on(event: 'error', listener: (error: Error) => void): unknown;
}
export interface SessionTimers { set(callback: () => void, ms: number, background?: boolean): unknown; clear(handle: unknown): void; now(): number }
const systemTimers: SessionTimers = {
    set: (callback, ms, background) => { const timer = setTimeout(callback, ms); if (background) { timer.unref(); } return timer; },
    clear: handle => clearTimeout(handle as NodeJS.Timeout),
    now: () => Date.now(),
};
export interface Greeting {
    readonly version: string; readonly i2Contract: number; readonly capabilities: readonly string[];
    readonly serviceInstance?: string; readonly windowId?: string;
}
/** Host-local identity only; the native session keeps all Admin credentials. */
export type AdminIdentity = Readonly<{ serviceInstance: string }>;
type AdminOwner = { child: SessionChild; greeting: Greeting; closed: boolean; ended: boolean; confirmed: boolean; closing: Promise<void> | null };
export interface SessionOptions {
    readonly clientVersion: string;
    readonly contracts?: readonly number[];
    /** Window label shown to agents (at most 128 bytes). */
    readonly label?: string;
    readonly launch?: (image: SessionImage, label: string | undefined) => SessionChild;
    readonly timers?: SessionTimers;
    readonly closeGraceMs?: number;
    readonly cancelGraceMs?: number;
    readonly helloTimeoutMs?: number;
    readonly maxQueued?: number;
}

function launchSession(image: SessionImage, label: string | undefined): SessionChild {
    const args = ['session', '--owner-watch', ...(label ? ['--label', label] : [])];
    return spawn(image.path, args, { cwd: path.dirname(image.path), windowsHide: true, shell: false, stdio: 'pipe', env: airkctlEnvironment() });
}

// ---- Frames ----------------------------------------------------------------

type Rec = Record<string, unknown>;
const isRec = (value: unknown): value is Rec => !!value && typeof value === 'object' && !Array.isArray(value);
/** `deny_unknown_fields`: exactly the required keys plus any optional ones. */
const keys = (value: unknown, required: string[], optional: string[] = []): value is Rec => isRec(value)
    && required.every(key => Object.hasOwn(value, key)) && Object.keys(value).every(key => required.includes(key) || optional.includes(key));
const u64 = (value: unknown) => Number.isSafeInteger(value) && (value as number) >= 0;
const cursorValid = (value: unknown) => keys(value, ['service_instance', 'window_id', 'native'])
    && typeof value.service_instance === 'string' && ID.test(value.service_instance) && typeof value.window_id === 'string' && ID.test(value.window_id);
/** The request shapes `SessionMethod` accepts (contract/mod.rs), checked before sending. */
function requestShapeValid(request: unknown): boolean {
    if (!isRec(request)) { return false; }
    const [kind, ...rest] = Object.keys(request);
    if (!kind || rest.length) { return false; }
    const body = request[kind];
    switch (kind) {
        case 'hello': return keys(body, ['protocol', 'client', 'i2_contracts']) && body.protocol === SESSION_PROTOCOL
            && keys(body.client, ['name', 'version']) && typeof body.client.name === 'string' && typeof body.client.version === 'string'
            && Array.isArray(body.i2_contracts) && body.i2_contracts.every(u64);
        case 'invoke': {
            if (!keys(body, ['argv', 'deadline_ms'], ['stdin'])) { return false; }
            const argv = body.argv, deadline = body.deadline_ms;
            return Array.isArray(argv) && argv.length <= MAX_INVOKE_ARGS && argv.every(arg => typeof arg === 'string' && utf8(arg) <= MAX_INVOKE_ARG_BYTES)
                && (body.stdin === undefined || typeof body.stdin === 'string' && utf8(body.stdin) <= MAX_INVOKE_STDIN_BYTES)
                && Number.isSafeInteger(deadline) && (deadline as number) >= 1 && (deadline as number) <= MAX_INVOKE_DEADLINE_MS;
        }
        case 'cancel': return keys(body, ['target']) && typeof body.target === 'string' && ID.test(body.target);
        case 'control': return keys(body, ['method']);
        case 'admin_call': {
            if (!keys(body, ['operation', 'body', 'deadline_ms']) || typeof body.operation !== 'string'
                || !/^[a-z][a-z0-9_.]{0,127}$/.test(body.operation) || !Number.isSafeInteger(body.deadline_ms)
                || (body.deadline_ms as number) < 1 || (body.deadline_ms as number) > MAX_INVOKE_DEADLINE_MS) return false;
            try { const encoded = JSON.stringify(body.body); return typeof encoded === 'string' && utf8(encoded) <= 64 * 1024; }
            catch { return false; }
        }
        case 'admin_read': return keys(body, ['target', 'offset']) && typeof body.target === 'string' && ID.test(body.target) && u64(body.offset);
        case 'admin_cancel': return keys(body, ['target']) && typeof body.target === 'string' && ID.test(body.target);
        case 'admin_close': return keys(body, []);
        case 'subscribe': return keys(body, ['topics'], ['cursor']) && Array.isArray(body.topics)
            && body.topics.every(topic => ['status', 'jobs', 'assistance'].includes(topic as string)) && (body.cursor === undefined || cursorValid(body.cursor));
        case 'unsubscribe': case 'scope_release': return keys(body, []);
        case 'scope_select': {
            if (!keys(body, ['selection'])) { return false; }
            const selection = body.selection;
            return keys(selection, ['namespace', 'expected_catalog_revision', 'index_ids', 'project_ids'])
                && typeof selection.namespace === 'string' && ID.test(selection.namespace) && u64(selection.expected_catalog_revision)
                && Array.isArray(selection.index_ids) && selection.index_ids.every(item => typeof item === 'string')
                && Array.isArray(selection.project_ids) && selection.project_ids.every(item => typeof item === 'string');
        }
        case 'assistance_ack': return keys(body, ['through']) && u64(body.through);
        case 'handoff_publish': return keys(body, ['handoff', 'ttl_ms']) && u64(body.ttl_ms);
        default: return false;
    }
}
/** A complete request frame as the child decodes it (`SessionRequest`, deny_unknown_fields). */
export function requestFrameValid(value: unknown): boolean {
    return keys(value, ['id', 'request']) && typeof value.id === 'string' && ID.test(value.id) && requestShapeValid(value.request);
}
/** Exact JSON body of one request frame; refuses what the child would refuse. */
export function encodeRequest(id: string, request: SessionMethod): Buffer {
    if (!ID.test(id) || !requestShapeValid(request)) { throw new SessionError('session_invalid_request', false); }
    const body = Buffer.from(JSON.stringify({ id, request }), 'utf8');
    if (body.length > MAX_REQUEST_FRAME_BYTES) { throw new SessionError('session_request_oversize', false); }
    const frame = Buffer.alloc(4 + body.length); frame.writeUInt32BE(body.length); body.copy(frame, 4);
    return frame;
}

const OUTCOMES = new Set(['hello', 'control', 'invoked', 'subscribed', 'unsubscribed', 'scope', 'scope_released', 'acknowledged', 'handoff', 'refused', 'failed', 'cancelled', 'admin_chunk', 'admin_closed']);
const EVENTS = new Set(['status', 'job', 'assistance_draft', 'resync', 'closing']);
export type Frame = { kind: 'reply'; id: string; outcome: SessionOutcome } | { kind: 'event'; event: SessionEvent };
/** Decode one reply or event frame; anything else is a protocol violation. */
export function decodeFrame(input: unknown): Frame {
    const invalid = () => new SessionError('session_invalid_response', true, true);
    if (!input || typeof input !== 'object' || Array.isArray(input)) { throw invalid(); }
    const value = input as Record<string, unknown>;
    if (Object.hasOwn(value, 'event')) {
        const event = value.event as Record<string, unknown>;
        if (Object.keys(value).length !== 1 || !event || typeof event !== 'object' || !EVENTS.has(event.kind as string)) { throw invalid(); }
        if (event.kind === 'resync' && !['overflow', 'native_gap', 'service_restarted'].includes(event.reason as string)) { throw invalid(); }
        return { kind: 'event', event: event as unknown as SessionEvent };
    }
    const outcome = value.outcome as Record<string, unknown>;
    if (Object.keys(value).length !== 2 || typeof value.id !== 'string' || !ID.test(value.id) || !outcome || typeof outcome !== 'object') { throw invalid(); }
    if (!OUTCOMES.has(outcome.status as string)) { throw invalid(); }
    switch (outcome.status) {
        case 'hello':
            if (!Number.isSafeInteger(outcome.protocol) || typeof outcome.version !== 'string' || outcome.version.length > 64
                || !Number.isSafeInteger(outcome.i2_contract) || !Array.isArray(outcome.capabilities)
                || typeof outcome.executable_sha256 !== 'string' || !SHA256.test(outcome.executable_sha256)) { throw invalid(); }
            break;
        case 'invoked':
            if (!Number.isInteger(outcome.exit_code) || !EXIT_CLASSES.includes(outcome.class as ExitClass)
                || typeof outcome.stdout !== 'string' || typeof outcome.stderr !== 'string') { throw invalid(); }
            if (outcome.error !== undefined && outcome.error !== null && !parseErrorLine(outcome.error)) { throw invalid(); }
            break;
        case 'failed': case 'refused':
            if (typeof outcome.code !== 'string' || !/^[a-z0-9_]{1,128}$/.test(outcome.code) || typeof outcome.retryable !== 'boolean') { throw invalid(); }
            break;
    }
    return { kind: 'reply', id: value.id, outcome: outcome as unknown as SessionOutcome };
}

/** Incremental frame decoder; a zero or oversized length is a protocol violation. */
export class FrameReader {
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
                if (!length || length > this.limit) { throw new SessionError('session_invalid_response', true, true); }
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

// ---- Client ----------------------------------------------------------------

export interface InvokeResult {
    readonly exitCode: number; readonly class: ExitClass; readonly stdout: string; readonly stderr: string; readonly error?: ErrorLine;
}
/** A non-success `invoked` result, carried complete (I2 read-parity rule). */
export class AirkctlCommandError extends Error {
    constructor(readonly argv: readonly string[], readonly result: InvokeResult) {
        super(result.error?.message ?? `airkctl ${argv[0] ?? ''} exited with ${result.class}`);
        this.name = 'AirkctlCommandError';
    }
    get code(): string { return this.result.error?.code ?? this.result.class; }
    get retryable(): boolean { return this.result.error?.retryable === true; }
}

type Kind = 'invoke' | 'control' | 'window' | 'admin';
const ALLOWED: Record<Kind, readonly string[]> = {
    invoke: ['invoked', 'failed', 'cancelled'],
    control: ['control', 'failed', 'cancelled'],
    window: ['subscribed', 'unsubscribed', 'scope', 'scope_released', 'acknowledged', 'handoff', 'refused', 'failed', 'cancelled'],
    admin: ['admin_chunk', 'admin_closed', 'refused', 'failed', 'cancelled'],
};
interface Exchange {
    id: string; request: SessionMethod; kind: Kind; deadlineAt: number; busyRetries: number;
    state: 'queued' | 'sent' | 'abandoned' | 'settled';
    timer?: unknown; retryTimer?: unknown; cancelTimer?: unknown;
    resolve(outcome: SessionOutcome): void; reject(error: Error): void;
    owner?: SessionChild; originalId?: string; adminTarget?: string; admit?: () => boolean;
}
/** Native events, plus `session_lost` when the child ended (not a native event). */
export type ClientEvent = SessionEvent | { kind: 'session_lost'; code: string };
export type EventListener = (event: ClientEvent) => void;
export type SessionGate = () => void;
const restartDelay = (failures: number) => Math.min(30_000, 500 * 2 ** Math.max(0, failures - 1));

/**
 * One `airkctl session` child per window. It starts lazily on first use,
 * multiplexes up to eight requests by id and restarts after an unexpected
 * exit only on the next use, with backoff. It never sees a bearer token:
 * `agents credential` and `admin url|open` are one-shots (contracts §3.8).
 */
export class AirkctlSession {
    private image?: SessionImage;
    private refused?: SessionImage;
    private child?: SessionChild;
    private ready = false;
    private helloId?: string;
    private helloTimer?: unknown;
    private readyAt = 0;
    private reader = new FrameReader();
    private readonly queue: Exchange[] = [];
    private readonly live = new Set<Exchange>();
    private readonly sent = new Map<string, Exchange>();
    private readonly cancels = new Map<string, string>();
    private readonly listeners = new Set<EventListener>();
    private failures = 0;
    private notBefore = 0;
    private startTimer?: unknown;
    private disposed = false;
    private spawned = 0;
    private greeting?: Greeting;
    private readonly adminOwners = new WeakMap<AdminIdentity, AdminOwner>();
    private currentAdmin?: AdminIdentity;
    private readonly timers: SessionTimers;
    private readonly launch: (image: SessionImage, label: string | undefined) => SessionChild;
    constructor(private readonly options: SessionOptions, private readonly gate: SessionGate = () => {}) {
        this.timers = options.timers ?? systemTimers;
        this.launch = options.launch ?? launchSession;
    }
    get launches(): number { return this.spawned; }
    get closed(): boolean { return this.disposed; }
    /** The last `hello` answer: capabilities, service instance and window. */
    get hello(): Greeting | undefined { return this.greeting; }

    /** Bind the verified image. A different image retires the running child. */
    bind(image: SessionImage): void {
        if (this.disposed) { return; }
        if (!path.isAbsolute(image.path) || !SHA256.test(image.sha256) || !image.version) { throw new SessionError('session_image_invalid', false); }
        const current = this.image;
        if (current && current.sha256 === image.sha256 && current.version === image.version && current.path === image.path) { return; }
        this.image = Object.freeze({ ...image });
        if (this.child) { this.retire(this.child, 'session_image_changed'); this.start(); }
    }
    /** Forget the image (the installation changed or was removed); the child ends. */
    unbind(): void {
        this.image = undefined;
        if (this.child) { this.retire(this.child, 'session_image_changed'); }
        this.failQueued(() => new SessionError('session_image_unverified', false));
    }
    onEvent(listener: EventListener): { dispose(): void } {
        this.listeners.add(listener);
        return { dispose: () => { this.listeners.delete(listener); } };
    }

    /** Run one allowlisted `airkctl` command; the complete three channels return. */
    async invoke(argv: readonly string[], options: { stdin?: string; deadlineMs?: number; signal?: AbortSignal } = {}): Promise<InvokeResult> {
        const deadline = Math.max(1, Math.min(options.deadlineMs ?? 60_000, MAX_INVOKE_DEADLINE_MS));
        const outcome = await this.exchange({ invoke: { argv: [...argv], ...(options.stdin === undefined ? {} : { stdin: options.stdin }), deadline_ms: deadline } }, 'invoke', deadline, options.signal);
        if (outcome.status === 'invoked') {
            return { exitCode: outcome.exit_code, class: outcome.class, stdout: outcome.stdout, stderr: outcome.stderr, ...(outcome.error ? { error: outcome.error } : {}) };
        }
        throw this.failure(outcome);
    }
    /** `invoke` with `--output json`; a non-success class throws the complete result. */
    async json(argv: readonly string[], options: { stdin?: string; deadlineMs?: number; signal?: AbortSignal } = {}): Promise<unknown> {
        const full = [...argv, '--output', 'json'];
        const result = await this.invoke(full, options);
        if (result.class !== 'success') { throw new AirkctlCommandError(full, result); }
        try { return JSON.parse(result.stdout); } catch { throw new AirkctlCommandError(full, { ...result, class: 'internal' }); }
    }
    /** I1 metadata relay; the installation's service, no control directory named. */
    async control(method: unknown, timeoutMs = 20_000, signal?: AbortSignal): Promise<Record<string, unknown>> {
        const outcome = await this.exchange({ control: { method } }, 'control', timeoutMs, signal);
        if (outcome.status === 'control') { return outcome.response as Record<string, unknown>; }
        throw this.failure(outcome);
    }
    /** Capture the existing verified child; this never starts or rebinds one. */
    captureAdmin(): AdminIdentity {
        const child = this.child, greeting = this.greeting;
        if (!this.ready || !child || !greeting?.serviceInstance || !greeting.capabilities.includes('admin-management/1'))
            throw new SessionError('admin_context_unavailable', false);
        const prior = this.currentAdmin && this.adminOwners.get(this.currentAdmin);
        if (prior?.child === child && prior.greeting === greeting) {
            if (!prior.closed) return this.currentAdmin!;
            if (prior.ended || !prior.confirmed) throw new SessionError('admin_close_unconfirmed', false);
        }
        const identity = Object.freeze({ serviceInstance: greeting.serviceInstance });
        this.adminOwners.set(identity, { child, greeting, closed: false, ended: false, confirmed: false, closing: null });
        this.currentAdmin = identity;
        return identity;
    }
    adminCurrent(identity: AdminIdentity): boolean {
        const owner = this.adminOwners.get(identity);
        return !!owner && this.currentAdmin === identity && this.ready && this.child === owner.child
            && this.greeting === owner.greeting && !owner.closed;
    }
    async adminCall(identity: AdminIdentity, operation: string, body: unknown, options: {
        deadlineMs?: number; signal?: AbortSignal; onStatus?: (status: number) => void; admit?: () => boolean;
    } = {}): Promise<{ httpStatus: number; body: string }> {
        if (!this.adminCurrent(identity)) throw new SessionError('admin_context_changed', false);
        const owner = this.adminOwners.get(identity)!, child = owner.child, target = newId();
        const deadline = Math.max(1, Math.min(options.deadlineMs ?? 60_000, MAX_INVOKE_DEADLINE_MS));
        const deadlineAt = this.timers.now() + deadline;
        const chunks = createAdminChunks(status => {
            if (status === 401) { owner.closed = true; owner.ended = true; }
            options.onStatus?.(status);
        });
        const sameOwner = () => this.child === child && this.ready && this.greeting === owner.greeting && owner.closing === null;
        const current = () => { try { return sameOwner() && options.admit?.() !== false; } catch { return false; } };
        try {
            let outcome = await this.exchange({ admin_call: { operation, body, deadline_ms: deadline } }, 'admin', deadline,
                options.signal, { owner: child, originalId: target, adminTarget: target, admit: () => this.adminCurrent(identity) && current() });
            for (;;) {
                if (outcome.status === 'failed' && outcome.code === 'admin_session_ended_outcome_unknown') {
                    owner.closed = true; owner.ended = true; options.onStatus?.(401);
                    const error = new SessionError('admin_session_ended_outcome_unknown', true, false, true);
                    Object.assign(error, { observedStatus: 401 }); throw error;
                }
                if (outcome.status !== 'admin_chunk') throw this.failure(outcome);
                const full = chunks.accept(outcome.chunk);
                if (full) return full;
                if (!current() || options.signal?.aborted || this.timers.now() >= deadlineAt) throw new SessionError('admin_context_changed', true);
                outcome = await this.exchange({ admin_read: { target, offset: chunks.offset } }, 'admin', deadlineAt - this.timers.now(),
                    options.signal, { owner: child, adminTarget: target, admit: current });
            }
        } catch (error) {
            if (sameOwner()) this.cancelAdmin(child, target);
            // Preserve already observed status even when a later frame/body fails.
            if (error instanceof Error && !('observedStatus' in error)) Object.assign(error, { observedStatus: chunks.status });
            throw error;
        }
    }
    adminClose(identity: AdminIdentity): Promise<void> {
        const owner = this.adminOwners.get(identity);
        if (!owner) return Promise.resolve();
        owner.closed = true; // revocation precedes queueing or awaiting.
        if (owner.closing) return owner.closing;
        owner.closing = Promise.resolve().then(async () => {
            if (this.child !== owner.child || !this.ready || this.greeting !== owner.greeting) return;
            const outcome = await this.exchange({ admin_close: {} }, 'admin', 20_000, undefined, { owner: owner.child });
            if (outcome.status !== 'admin_closed') throw this.failure(outcome);
            owner.confirmed = true;
        });
        return owner.closing;
    }
    subscribe(topics: EventTopic[], cursor?: SessionCursor): Promise<SessionOutcome> {
        return this.window({ subscribe: { topics, ...(cursor ? { cursor } : {}) } });
    }
    unsubscribe(): Promise<SessionOutcome> { return this.window({ unsubscribe: {} }); }
    scopeSelect(selection: WindowSelection): Promise<SessionOutcome> { return this.window({ scope_select: { selection } }); }
    scopeRelease(): Promise<SessionOutcome> { return this.window({ scope_release: {} }); }
    assistanceAck(through: number): Promise<SessionOutcome> { return this.window({ assistance_ack: { through } }); }
    handoffPublish(handoff: unknown, ttlMs: number): Promise<SessionOutcome> { return this.window({ handoff_publish: { handoff, ttl_ms: ttlMs } }); }
    private async window(request: SessionMethod, timeoutMs = 20_000): Promise<SessionOutcome> {
        const outcome = await this.exchange(request, 'window', timeoutMs);
        if (outcome.status === 'failed' || outcome.status === 'cancelled') { throw this.failure(outcome); }
        return outcome;
    }
    private failure(outcome: SessionOutcome): SessionError {
        if (outcome.status === 'failed') { return new SessionError(outcome.code, true, outcome.retryable, !NO_EFFECT.has(outcome.code)); }
        return new SessionError('session_cancelled', true);
    }

    private exchange(request: SessionMethod, kind: Kind, timeoutMs: number, signal?: AbortSignal,
        captured?: Pick<Exchange, 'owner' | 'originalId' | 'adminTarget' | 'admit'>): Promise<SessionOutcome> {
        if (this.disposed) { return Promise.reject(new SessionError('session_disposed', false)); }
        // Revocation may use an already captured child after native execution
        // is disabled. It may never start a child or address a replacement.
        const revocation = kind === 'admin' && 'admin_close' in request && captured?.owner === this.child && this.ready;
        if (!revocation) { try { this.gate(); } catch (error) { return Promise.reject(error); } }
        if (this.queue.length >= (this.options.maxQueued ?? 64)) { return Promise.reject(new SessionError('session_queue_full', false)); }
        if (signal?.aborted) { return Promise.reject(new SessionError('session_aborted', false)); }
        try { encodeRequest(newId(), request); } catch (error) { return Promise.reject(error); }
        return new Promise((resolve, reject) => {
            const entry: Exchange = { id: '', request, kind, deadlineAt: this.timers.now() + timeoutMs, busyRetries: 0, state: 'queued', resolve, reject, ...captured };
            entry.timer = this.timers.set(() => { entry.timer = undefined; this.abandon(entry, 'session_deadline_exceeded'); }, Math.max(1, timeoutMs));
            signal?.addEventListener('abort', () => this.abandon(entry, 'session_aborted'), { once: true });
            this.live.add(entry); this.queue.push(entry);
            this.start(); this.pump();
        });
    }
    /** Close stdin (EOF ends the session; the child closes its window) and kill after a grace. */
    dispose(): void {
        if (this.disposed) { return; }
        this.disposed = true;
        if (this.startTimer !== undefined) { this.timers.clear(this.startTimer); this.startTimer = undefined; }
        for (const entry of [...this.live]) { const sent = entry.state === 'sent'; this.settle(entry); entry.reject(new SessionError('session_disposed', sent)); }
        this.queue.splice(0);
        if (this.child) { this.retire(this.child, 'session_disposed'); }
        this.listeners.clear();
    }

    private settle(entry: Exchange): void {
        if (entry.state !== 'abandoned') { entry.state = 'settled'; }
        this.live.delete(entry);
        for (const timer of [entry.timer, entry.retryTimer]) { if (timer !== undefined) { this.timers.clear(timer); } }
        entry.timer = entry.retryTimer = undefined;
        const index = this.queue.indexOf(entry);
        if (index >= 0) { this.queue.splice(index, 1); }
    }
    private abandon(entry: Exchange, code: string): void {
        if (entry.state === 'queued') { this.settle(entry); entry.reject(new SessionError(code, false)); return; }
        if (entry.state !== 'sent') { return; }
        this.settle(entry); entry.state = 'abandoned';
        entry.reject(new SessionError(code, true));
        const child = this.child;
        if (!child || !this.ready) { return; }
        const id = newId();
        if (this.cancels.size >= 64) { this.cancels.delete(this.cancels.keys().next().value!); }
        this.cancels.set(id, entry.id);
        entry.cancelTimer = this.timers.set(() => {
            entry.cancelTimer = undefined;
            if (this.sent.get(entry.id) === entry) { this.lost(child, 'session_unresponsive'); }
        }, this.options.cancelGraceMs ?? 30_000);
        child.stdin.write(encodeRequest(id, entry.adminTarget ? { admin_cancel: { target: entry.adminTarget } }
            : { cancel: { target: entry.id } }), error => { if (error) { this.lost(child, 'session_closed'); } });
    }
    private cancelAdmin(child: SessionChild, target: string): void {
        if (child !== this.child || !this.ready) return;
        if ([...this.cancels.values()].includes(target)) return;
        const id = newId();
        if (this.cancels.size >= 64) this.cancels.delete(this.cancels.keys().next().value!);
        this.cancels.set(id, target);
        child.stdin.write(encodeRequest(id, { admin_cancel: { target } }), error => { if (error) this.lost(child, 'session_closed'); });
    }
    private start(): void {
        if (this.disposed || this.child || this.startTimer !== undefined || !this.queue.length) { return; }
        const wait = this.notBefore - this.timers.now();
        if (wait > 0) { this.startTimer = this.timers.set(() => { this.startTimer = undefined; this.start(); this.pump(); }, wait); return; }
        const image = this.image;
        if (!image) { this.failQueued(() => new SessionError('session_image_unverified', false)); return; }
        if (this.refused === image) { this.failQueued(() => new SessionError('session_identity_mismatch', false)); return; }
        let child: SessionChild;
        try { this.gate(); child = this.launch(image, this.options.label); }
        catch (error) {
            this.failures++; this.notBefore = this.timers.now() + restartDelay(this.failures);
            this.failQueued(() => Object.assign(new SessionError('session_unavailable', false, true), { cause: error }));
            return;
        }
        this.spawned++; this.child = child; this.ready = false; this.reader = new FrameReader();
        child.stdout.on('data', (bytes: Buffer) => this.receive(child, bytes));
        child.stderr.resume();
        child.on('error', () => this.lost(child, 'session_unavailable'));
        child.on('exit', () => this.lost(child, 'session_closed'));
        child.stdin.on('error', () => this.lost(child, 'session_closed'));
        const helloId = newId(); this.helloId = helloId;
        this.helloTimer = this.timers.set(() => this.lost(child, 'session_unresponsive', true), this.options.helloTimeoutMs ?? 30_000);
        const hello: SessionMethod = { hello: { protocol: SESSION_PROTOCOL, client: { name: CLIENT_NAME, version: this.options.clientVersion }, i2_contracts: [...(this.options.contracts ?? [1])] } };
        child.stdin.write(encodeRequest(helloId, hello), error => { if (error) { this.lost(child, 'session_closed'); } });
    }
    private pump(): void {
        const child = this.child;
        if (!child || !this.ready || this.disposed) { return; }
        while (this.queue.length && this.sent.size < MAX_IN_FLIGHT) {
            const entry = this.queue.shift()!;
            if (entry.owner && entry.owner !== child || entry.admit && !entry.admit()) {
                this.settle(entry); entry.reject(new SessionError('admin_context_changed', false)); continue;
            }
            if (entry.deadlineAt <= this.timers.now()) { this.abandon(entry, 'session_deadline_exceeded'); continue; }
            if ('invoke' in entry.request) {
                const remaining = Math.max(1, Math.min(entry.request.invoke.deadline_ms, Math.round(entry.deadlineAt - this.timers.now())));
                entry.request = { invoke: { ...entry.request.invoke, deadline_ms: remaining } };
            }
            if ('admin_call' in entry.request) entry.request = { admin_call: { ...entry.request.admin_call,
                deadline_ms: Math.max(1, Math.min(entry.request.admin_call.deadline_ms, Math.floor(entry.deadlineAt - this.timers.now()))) } };
            entry.id = entry.originalId ?? newId(); entry.state = 'sent'; this.sent.set(entry.id, entry);
            child.stdin.write(encodeRequest(entry.id, entry.request), error => { if (error) { this.lost(child, 'session_closed'); } });
        }
    }
    private receive(child: SessionChild, bytes: Buffer): void {
        if (child !== this.child) { return; }
        let frames: Buffer[];
        try { frames = this.reader.push(bytes); } catch { this.lost(child, 'session_invalid_response'); return; }
        for (const frame of frames) {
            if (child !== this.child) { return; }
            let decoded: Frame;
            try { decoded = decodeFrame(JSON.parse(frame.toString('utf8'))); } catch { this.lost(child, 'session_invalid_response'); return; }
            if (decoded.kind === 'event') { this.event(decoded.event); continue; }
            this.dispatch(child, decoded.id, decoded.outcome);
        }
    }
    private event(event: ClientEvent): void {
        if (event.kind === 'resync' && event.reason === 'service_restarted' && this.currentAdmin) {
            const owner = this.adminOwners.get(this.currentAdmin);
            if (owner) { owner.closed = true; owner.ended = true; }
        }
        for (const listener of [...this.listeners]) { try { listener(event); } catch { /* A listener failure never breaks the session. */ } }
    }
    private dispatch(child: SessionChild, id: string, outcome: SessionOutcome): void {
        if (!this.ready) {
            const image = this.image;
            const contracts = this.options.contracts ?? [1];
            if (id !== this.helloId || outcome.status !== 'hello' || !image || outcome.protocol !== SESSION_PROTOCOL
                || outcome.version !== image.version || outcome.executable_sha256 !== image.sha256 || !contracts.includes(outcome.i2_contract)) {
                this.refused = image;
                this.lost(child, 'session_identity_mismatch', true); return;
            }
            if (this.helloTimer !== undefined) { this.timers.clear(this.helloTimer); this.helloTimer = undefined; }
            this.greeting = Object.freeze({
                version: outcome.version, i2Contract: outcome.i2_contract, capabilities: Object.freeze([...outcome.capabilities]),
                ...(outcome.service_instance ? { serviceInstance: outcome.service_instance } : {}),
                ...(outcome.window_id ? { windowId: outcome.window_id } : {}),
            });
            this.ready = true; this.readyAt = this.timers.now(); this.pump(); return;
        }
        if (this.cancels.has(id)) {
            if (outcome.status !== 'cancelled' && outcome.status !== 'failed') { this.lost(child, 'session_invalid_response'); return; }
            this.cancels.delete(id); return;
        }
        const entry = this.sent.get(id);
        if (!entry || !ALLOWED[entry.kind].includes(outcome.status)) { this.lost(child, 'session_invalid_response'); return; }
        this.sent.delete(id);
        if (entry.cancelTimer !== undefined) { this.timers.clear(entry.cancelTimer); entry.cancelTimer = undefined; }
        if (entry.kind !== 'admin' && entry.state === 'sent' && outcome.status === 'failed' && outcome.code === 'session_busy' && outcome.retryable) {
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
    private detach(child: SessionChild, code: string): void {
        if (child !== this.child) { return; }
        this.child = undefined; this.ready = false; this.helloId = undefined; this.greeting = undefined;
        if (this.helloTimer !== undefined) { this.timers.clear(this.helloTimer); this.helloTimer = undefined; }
        // Captured Admin work cannot cause a replacement process to start just
        // so its owner fence can subsequently refuse the queued request.
        for (const entry of [...this.live]) if (entry.state === 'queued' && entry.owner === child) {
            this.settle(entry); entry.reject(new SessionError('admin_context_changed', false));
        }
        for (const entry of this.sent.values()) {
            if (entry.cancelTimer !== undefined) { this.timers.clear(entry.cancelTimer); entry.cancelTimer = undefined; }
            if (entry.state === 'sent') { this.settle(entry); entry.reject(new SessionError(code, true)); }
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
        if (failQueued) { this.failQueued(() => new SessionError(code, false)); }
        this.event({ kind: 'session_lost', code });
        this.start();
    }
    /** EOF, then kill if the child outlives a bounded grace. Started work is
     * failed to its callers; accepted mutations are recovered by receipt. */
    private retire(child: SessionChild, code: string): void {
        if (child !== this.child) { return; }
        this.detach(child, code);
        this.event({ kind: 'session_lost', code });
        const timer = this.timers.set(() => { if (child.exitCode === null && child.signalCode === null) { child.kill(); } }, this.options.closeGraceMs ?? 5000, true);
        child.on('exit', () => this.timers.clear(timer));
        child.stdin.end();
    }
    private failQueued(error: () => Error): void {
        for (const entry of [...this.live]) {
            if (entry.state === 'queued') { this.settle(entry); entry.reject(error()); }
        }
    }
}
