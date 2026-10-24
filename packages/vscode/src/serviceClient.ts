// SPDX-License-Identifier: Apache-2.0
import { ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import * as path from 'node:path';

export type RecordValue = Record<string, unknown>;
export function record(value: unknown): RecordValue {
    if (!value || typeof value !== 'object' || Array.isArray(value)) { throw new Error('Invalid AIRK service object.'); }
    return value as RecordValue;
}
export function identifier(value: unknown, bytes: 16 | 32): string {
    if (typeof value !== 'string' || !(bytes === 16 ? /^[0-9a-f]{32}$/ : /^[0-9a-f]{64}$/).test(value)) {
        throw new Error('Invalid AIRK service identity.');
    }
    return value;
}
export function count(value: unknown): number {
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) { throw new Error('Invalid AIRK service count.'); }
    return value;
}
export class ServiceError extends Error {
    constructor(readonly code: string, readonly outcomeUnknown = false) { super(`AIRK service: ${code}`); }
}
export interface ControlTransport { request(method: unknown): Promise<RecordValue>; dispose(): void }
interface Pending {
    id: string; bytes: Buffer; resolve(value: RecordValue): void; reject(error: Error): void;
    timer?: NodeJS.Timeout;
}
/** One persistent native metadata adapter per client. It never carries evidence or credentials. */
export class NativeControlTransport implements ControlTransport {
    private readonly child: ChildProcessWithoutNullStreams;
    private readonly queue: Pending[] = [];
    private active?: Pending;
    private stopped = false;
    private prefix = Buffer.alloc(4);
    private prefixBytes = 0;
    private body?: Buffer;
    private bodyBytes = 0;
    constructor(command: string, args: string[], private readonly timeout = 20000,
        launch: (command: string, args: string[]) => ChildProcessWithoutNullStreams = (file, argv) =>
            spawn(file, argv, { cwd: path.isAbsolute(file) ? path.dirname(file) : undefined, windowsHide: true, stdio: 'pipe', env: { ...process.env, AIRK_CLIENT: 'vscode', AIRK_ERROR_FORMAT: 'json' } })) {
        this.child = launch(command, args);
        this.child.stdout.on('data', (bytes: Buffer) => { try { this.read(bytes); } catch { this.fail('invalid_console_response'); } });
        // Diagnostics arrive as bounded structured results. Do not forward raw stderr/environment/argv.
        this.child.stderr.resume();
        this.child.on('error', () => this.fail('console_transport_unavailable'));
        this.child.on('exit', () => this.fail('console_transport_closed'));
        this.child.stdin.on('error', () => this.fail('console_transport_closed'));
    }
    request(method: unknown): Promise<RecordValue> {
        if (this.stopped) { return Promise.reject(new ServiceError('console_transport_closed')); }
        if (this.queue.length + (this.active ? 1 : 0) >= 8) { return Promise.reject(new ServiceError('console_queue_full')); }
        const id = randomUUID().replaceAll('-', '');
        const body = Buffer.from(JSON.stringify({ id, method }), 'utf8');
        if (!body.length || body.length > 65536) { return Promise.reject(new ServiceError('console_request_oversize')); }
        const bytes = Buffer.alloc(4 + body.length); bytes.writeUInt32BE(body.length); body.copy(bytes, 4);
        return new Promise((resolve, reject) => { this.queue.push({ id, bytes, resolve, reject }); this.pump(); });
    }
    private pump(): void {
        if (this.stopped || this.active || !this.queue.length) { return; }
        const pending = this.queue.shift()!; this.active = pending;
        pending.timer = setTimeout(() => this.fail('console_deadline_exceeded'), this.timeout);
        this.child.stdin.write(pending.bytes, error => { if (error) { this.fail('console_transport_closed'); } });
    }
    private read(bytes: Buffer): void {
        let offset = 0;
        while (offset < bytes.length && !this.stopped) {
            if (!this.body) {
                const copied = Math.min(4 - this.prefixBytes, bytes.length - offset);
                bytes.copy(this.prefix, this.prefixBytes, offset, offset + copied);
                this.prefixBytes += copied; offset += copied;
                if (this.prefixBytes < 4) { continue; }
                const length = this.prefix.readUInt32BE();
                if (!length || length > 65536) { throw new Error('Invalid frame length'); }
                this.body = Buffer.allocUnsafe(length); this.bodyBytes = 0;
            }
            const copied = Math.min(this.body.length - this.bodyBytes, bytes.length - offset);
            bytes.copy(this.body, this.bodyBytes, offset, offset + copied);
            this.bodyBytes += copied; offset += copied;
            if (this.bodyBytes === this.body.length) {
                const value = record(JSON.parse(this.body.toString('utf8')));
                const active = this.active;
                if (!active || value.id !== active.id) { throw new Error('Crossed console request'); }
                const response = record(value.response);
                if (typeof response.status !== 'string' || response.status.length > 64) { throw new Error('Invalid console result'); }
                clearTimeout(active.timer); this.active = undefined; this.body = undefined; this.prefixBytes = 0;
                active.resolve(response); this.pump();
            }
        }
    }
    private fail(code: string): void {
        if (this.stopped) { return; } this.stopped = true;
        if (this.active) { clearTimeout(this.active.timer); this.active.reject(new ServiceError(code, true)); this.active = undefined; }
        for (const pending of this.queue.splice(0)) { pending.reject(new ServiceError(code)); }
        this.body = undefined;
        this.child.stdin.end();
        if (this.child.exitCode === null && this.child.signalCode === null) { this.child.kill(); }
    }
    dispose(): void { this.fail('console_client_closed'); }
}
export interface RequestKey { namespace: string; epoch: number; nonce: string }
export interface JobIntent { project: string; caller: string; key: RequestKey; kind: JobKind }
export type JobKind = 'index' | 'reconcile' | 'vectors' | 'model_setup' | 'verify' | 'inspect' | 'recover' | 'maintain';
export interface RetryIntent { id: string; caller: string; key: RequestKey }
export interface PlanIntent { caller: string; key: RequestKey; request: { selection: 'all' | { selected: string[] }; operation: RepairOperation } }
export type MutationIntent = { enqueue: JobIntent } | { retry: RetryIntent } | { plan_create: PlanIntent };
export type RepairOperation = 'auto' | 'verify' | 'recover' | 'reconcile' | 'vectors' | 'rebuild';
export interface ConsolePage { revision: number; cached: true; observed_unix_ms: number; projects: RecordValue[]; after: string | null }
export class ServiceClient {
    private readonly caller = randomUUID().replaceAll('-', '');
    constructor(readonly transport: ControlTransport) {}
    async rpc(method: unknown, expected?: string): Promise<RecordValue> {
        const response = await this.transport.request(method);
        if (response.status === 'failure') {
            const code = typeof response.code === 'string' && /^[a-z0-9_]{1,128}$/.test(response.code) ? response.code : 'invalid_service_failure';
            throw new ServiceError(code, code.endsWith('_outcome_unknown') || code === 'registry_response_unknown_retry_with_key');
        }
        if (expected && response.status !== expected) { throw new ServiceError('unexpected_service_response', true); }
        return response;
    }
    async projects(after: string | null = null, revision: number | null = null, limit = 16): Promise<ConsolePage> {
        if (after !== null) { identifier(after,32); } if (revision !== null) { count(revision); }
        if (!Number.isInteger(limit) || limit < 1 || limit > 32) { throw new ServiceError('invalid_project_page_limit'); }
        const page = record((await this.rpc({ console: { after, revision, limit } }, 'console')).page);
        count(page.revision); count(page.observed_unix_ms);
        if (page.cached !== true || !Array.isArray(page.projects) || page.projects.length > limit) { throw new ServiceError('invalid_console_page'); }
        const projects = page.projects.map(value => { const row = record(value); identifier(record(record(row.project).registration).binding,32); return row; });
        if (page.after !== null) { identifier(page.after,32); }
        return { revision: page.revision as number, observed_unix_ms: page.observed_unix_ms as number, cached: true, projects, after: page.after as string | null };
    }
    async workers(projects: string[]): Promise<RecordValue> {
        if (projects.length > 32 || new Set(projects).size !== projects.length) { throw new ServiceError('invalid_worker_observation_scope'); }
        projects.forEach(project => identifier(project,32));
        const result = await this.rpc({ workers: { projects } },'workers');
        count(result.age_ms);
        if (!Array.isArray(result.observations) || result.observations.length !== projects.length) { throw new ServiceError('invalid_worker_observations'); }
        result.observations.forEach((value,index) => {
            const observation = record(value);
            if (observation.project !== projects[index] || typeof observation.restarting !== 'boolean' || typeof observation.automatic_start_blocked !== 'boolean') { throw new ServiceError('crossed_worker_observation'); }
            if (observation.identity !== null) {
                const identity = record(observation.identity);
                identifier(identity.service,16); identifier(identity.worker,16);
                if (identity.project !== observation.project || count(identity.fencing_generation) === 0) { throw new ServiceError('crossed_worker_identity'); }
            }
        });
        return result;
    }
    async key(): Promise<RequestKey> {
        const response = await this.rpc('request_epoch','request_epoch');
        return { namespace: identifier(response.namespace,16), epoch: count(response.epoch), nonce: randomUUID().replaceAll('-','') };
    }
    async prepareJob(project: string, kind: JobKind): Promise<JobIntent> {
        return { project: identifier(project,32), caller: this.caller, key: await this.key(), kind };
    }
    // Preserve this exact intent before sending. An uncertain result must be checked
    // or retried with the same key; never manufacture a fresh mutation automatically.
    async submitJob(intent: JobIntent): Promise<RecordValue> { return this.submitMutation({ enqueue: intent }); }
    async prepareRetry(id: string): Promise<MutationIntent> {
        return { retry: { id: identifier(id,32), caller: this.caller, key: await this.key() } };
    }
    async preparePlan(selection: 'all' | string[], operation: RepairOperation): Promise<MutationIntent> {
        if (selection !== 'all') {
            if (!selection.length || selection.length > 256 || new Set(selection).size !== selection.length) { throw new ServiceError('invalid_repair_selection'); }
            selection.forEach(value => identifier(value,32));
        }
        return { plan_create: { caller: this.caller, key: await this.key(), request: {
            selection: selection === 'all' ? 'all' : { selected: [...selection] }, operation } } };
    }
    async submitMutation(intent: MutationIntent): Promise<RecordValue> {
        const value = 'enqueue' in intent ? intent.enqueue : 'retry' in intent ? intent.retry : intent.plan_create;
        identifier(value.caller,16); identifier(value.key.namespace,16); count(value.key.epoch); identifier(value.key.nonce,16);
        if ('enqueue' in intent) {
            identifier(intent.enqueue.project,32);
            if (!['index','reconcile','vectors','model_setup','verify','inspect','recover','maintain'].includes(intent.enqueue.kind)) { throw new ServiceError('invalid_job_intent'); }
        } else if ('retry' in intent) { identifier(intent.retry.id,32); }
        // The host persists the complete prepared intent before this call and
        // retires it only after receiving a complete, matching native result.
        const response = await this.rpc(intent, 'plan_create' in intent ? 'plan' : 'job');
        if ('enqueue' in intent && record(response.job).project !== intent.enqueue.project) { throw new ServiceError('crossed_job_receipt',true); }
        identifier(record('plan_create' in intent ? response.plan : response.job).id,32);
        return response;
    }
    dispose(): void { this.transport.dispose(); }
}
