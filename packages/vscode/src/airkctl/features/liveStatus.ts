// SPDX-License-Identifier: Apache-2.0
//! Live index status in the Explorer (U103; contracts §4.3c): the native
//! `Progress` and `Watch` reads of this window's indexes through the session's
//! metadata relay, the session's `status` and `jobs` events, and a modest poll
//! while a job runs and the Explorer is visible. Each read keeps its complete
//! native response; nothing here starts, changes or retries work.
import type { RecordValue } from '../../serviceClient';
import type { WorkAction } from '../../explorerContracts';
import type { ClientEvent, InvokeResult } from '../session';

/** `airkctl` argv of one Explorer action on index `binding` (U103), selected with
 * `--service-project` as `tools.ts` does. Each prints the native response with `--output json`. */
export function workActionArgv(action: WorkAction, binding: string, job?: string): string[] {
    if (!/^[0-9a-f]{64}$/.test(binding)) { throw Object.assign(new Error('The index identity is invalid.'), { code: 'invalid_project' }); }
    const select = ['--service-project', binding, '--output', 'json'];
    switch (action) {
        case 'index': return ['index', ...select];
        case 'pause': return ['pause', ...select];
        case 'resume': return ['resume', ...select];
        case 'watch_on': return ['watch', 'on', ...select];
        case 'watch_off': return ['watch', 'off', ...select];
        case 'retry': case 'cancel':
            if (!job || !/^[0-9a-f]{64}$/.test(job)) { throw Object.assign(new Error('Choose the job to retry or cancel.'), { code: 'invalid_job' }); }
            return ['jobs', action, job, ...select];
    }
}
/** `airkctl usage report` for one index (JSON document). */
export function usageArgv(binding: string, days: number): string[] {
    if (!/^[0-9a-f]{64}$/.test(binding)) { throw Object.assign(new Error('The index identity is invalid.'), { code: 'invalid_project' }); }
    return ['usage', 'report', '--days', String(days), '--format', 'json', '--service-project', binding, '--output', 'json'];
}
/** Recent service log records from the running service. */
export const LOGS_ARGV: readonly string[] = ['service', 'logs', '--limit', '32', '--format', 'json', '--output', 'json'];

/** One `airkctl` command's complete outcome: the three channels, the error line and the parsed JSON stdout. */
export interface CommandOutcome {
    readonly ok: boolean;
    readonly argv: readonly string[];
    readonly class: string;
    readonly exitCode: number;
    readonly result?: unknown;
    readonly stdout?: string;
    readonly stderr?: string;
    readonly error?: unknown;
}
export function commandOutcome(argv: readonly string[], invoked: InvokeResult): CommandOutcome {
    let result: unknown, parsed = false;
    try { result = JSON.parse(invoked.stdout); parsed = true; } catch { /* Kept as text below. */ }
    return { ok: invoked.class === 'success', argv: [...argv], class: invoked.class, exitCode: invoked.exitCode,
        ...(parsed ? { result } : invoked.stdout ? { stdout: invoked.stdout } : {}),
        ...(invoked.stderr ? { stderr: invoked.stderr } : {}), ...(invoked.error ? { error: invoked.error } : {}) };
}

/** The relay method this module uses (`AirkctlSession.control`). */
export interface LiveSession { control(method: unknown): Promise<Record<string, unknown>> }
/** Injected timers: tests drive polling with a manual clock, never sleeps. */
export interface LiveTimers { set(callback: () => void, ms: number): unknown; clear(handle: unknown): void; now(): number }
export const systemLiveTimers: LiveTimers = {
    set: (callback, ms) => { const timer = setTimeout(callback, ms); timer.unref?.(); return timer; },
    clear: handle => clearTimeout(handle as NodeJS.Timeout),
    now: () => Date.now(),
};
/** Session event topics the Explorer subscribes to (contracts §3.7). */
export const LIVE_TOPICS = ['status', 'jobs'] as const;
export const POLL_INTERVAL_MS = 2000;
/** Indexes of this window read per refresh (Progress and Watch each). */
export const MAX_LIVE_BINDINGS = 8;
const EVENT_LIMIT = 64;

/** One index's latest native reads, complete; a failed read keeps its code, never an empty result. */
export interface LiveObservation {
    readonly progress?: Record<string, unknown>;
    readonly watch?: Record<string, unknown>;
    readonly progressError?: string;
    readonly watchError?: string;
    readonly observedAt: number;
}

const obj = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const errorCode = (error: unknown, fallback: string) => {
    const code = (error as { code?: unknown })?.code;
    return typeof code === 'string' && /^[a-z0-9_]{1,128}$/.test(code) ? code : fallback;
};

/** A console page row for one registered project with its live reads, in the shape
 * `projectSummaries` and `managementView` read: `progress` is the native
 * ProgressObservation, `watch` the native watch observation. */
export function liveRow(registration: { binding: string; root: string; data_root: string; collection: string }, live?: LiveObservation): RecordValue {
    const progress = live?.progress, watch = live?.watch;
    const paused = progress && typeof progress.project_paused === 'boolean' ? { paused: progress.project_paused } : {};
    return {
        project: { registration: { ...registration }, ...paused },
        ...(progress && progress.observation !== null && progress.observation !== undefined ? { progress: progress.observation as RecordValue } : {}),
        ...(progress && progress.pending_job !== null && progress.pending_job !== undefined ? { pending_job: progress.pending_job as RecordValue } : {}),
        ...(progress && Array.isArray(progress.queued_jobs) ? { queued_jobs: progress.queued_jobs, queue_truncated: progress.queue_truncated === true } : {}),
        ...(progress && typeof progress.pending_paused === 'boolean' ? { pending_paused: progress.pending_paused } : {}),
        ...(watch && watch.observation !== null && watch.observation !== undefined ? { watch: watch.observation as RecordValue } : {}),
        ...(live ? { live: { observed_unix_ms: live.observedAt, ...(progress ? { progress } : {}), ...(watch ? { watch } : {}),
            ...(live.progressError ? { progress_error: live.progressError } : {}), ...(live.watchError ? { watch_error: live.watchError } : {}) } } : {}),
    };
}

/** True while the observation shows a running or queued job (the poll continues). */
export function workActive(live: LiveObservation | undefined): boolean {
    const progress = obj(live?.progress), job = obj(obj(progress.observation).job), pending = obj(progress.pending_job);
    return job.state === 'running' || job.state === 'pending' || typeof pending.id === 'string';
}

/** A session `status`/`job` event as one bounded activity record (the complete event is kept). */
export function activityRecord(event: ClientEvent, now: number): RecordValue | undefined {
    if (event.kind === 'job') {
        const job = obj(event.job);
        return { kind: 'job', observed_unix_ms: now, ...(typeof job.project === 'string' ? { project: job.project } : {}),
            ...(typeof job.kind === 'string' ? { job_kind: job.kind } : {}), ...(typeof job.state === 'string' ? { state: job.state } : {}),
            ...(typeof job.id === 'string' ? { job: job.id } : {}), ...(typeof obj(job.outcome).code === 'string' ? { code: obj(job.outcome).code } : {}),
            native: structuredClone(event) as unknown as RecordValue };
    }
    if (event.kind === 'status') {
        const status = obj(event.status);
        return { kind: 'status', observed_unix_ms: now, ...(typeof status.project === 'string' ? { project: status.project } : {}),
            native: structuredClone(event) as unknown as RecordValue };
    }
    if (event.kind === 'resync') { return { kind: 'resync', observed_unix_ms: now, code: event.reason }; }
    return undefined;
}

export class LiveStatus {
    private readonly observations = new Map<string, LiveObservation>();
    private bindings: string[] = [];
    private retainers = 0;
    /** An event arrived while hidden; the next retain reads again. */
    private outdated = false;
    private timer?: unknown;
    private reading?: Promise<void>;
    private again = false;
    private disposed = false;
    /** Session status/job events of this window, newest last (bounded). */
    readonly events: RecordValue[] = [];
    eventsOmitted = false;

    constructor(private readonly session: LiveSession, private readonly changed: () => void,
        private readonly timers: LiveTimers = systemLiveTimers, private readonly intervalMs = POLL_INTERVAL_MS,
        private readonly log: (line: string) => void = () => {}) {}

    /** The indexes to observe (this window's projects); others are forgotten. */
    track(bindings: readonly string[]): void {
        this.bindings = [...new Set(bindings.filter(binding => /^[0-9a-f]{64}$/.test(binding)))].slice(0, MAX_LIVE_BINDINGS);
        for (const binding of [...this.observations.keys()]) { if (!this.bindings.includes(binding)) { this.observations.delete(binding); } }
    }
    observation(binding: string): LiveObservation | undefined { return this.observations.get(binding); }
    clear(): void { this.observations.clear(); this.stop(); }

    /** One Progress and one Watch read per tracked index; concurrent calls share one pass and run once more after it. */
    read(): Promise<void> {
        if (this.disposed) { return Promise.resolve(); }
        if (this.reading) { this.again = true; return this.reading; }
        const pass = (async () => {
            do {
                this.again = false;
                for (const binding of [...this.bindings]) {
                    const [progress, watch] = await Promise.all([
                        this.one({ progress: { project: binding } }, 'progress', binding),
                        this.one({ watch: { project: binding } }, 'watch', binding),
                    ]);
                    if (this.disposed || !this.bindings.includes(binding)) { continue; }
                    const prior = this.observations.get(binding);
                    this.observations.set(binding, {
                        // A failed read keeps the previous complete response beside its error code.
                        ...(progress.value ? { progress: progress.value } : prior?.progress ? { progress: prior.progress } : {}),
                        ...(watch.value ? { watch: watch.value } : prior?.watch ? { watch: prior.watch } : {}),
                        ...(progress.error ? { progressError: progress.error } : {}), ...(watch.error ? { watchError: watch.error } : {}),
                        observedAt: this.timers.now(),
                    });
                }
            } while (this.again && !this.disposed);
        })().finally(() => { this.reading = undefined; if (!this.disposed) { this.changed(); this.schedule(); } });
        this.reading = pass;
        return pass;
    }
    private async one(method: Record<string, unknown>, status: 'progress' | 'watch', binding: string): Promise<{ value?: Record<string, unknown>; error?: string }> {
        try {
            const response = await this.session.control(method);
            if (response?.status === status && response.project === binding) { return { value: response }; }
            const code = typeof response?.code === 'string' ? response.code : `unexpected_${typeof response?.status === 'string' ? response.status : 'response'}`;
            this.log(`AIRK: the ${status} read of ${binding.slice(0, 12)} was refused (${code}).`);
            return { error: code };
        } catch (error) {
            const code = errorCode(error, `${status}_read_failed`);
            this.log(`AIRK: the ${status} read of ${binding.slice(0, 12)} failed (${code}).`);
            return { error: code };
        }
    }

    /** A session event: status and job events are recorded and the observations read again. */
    event(event: ClientEvent): void {
        if (this.disposed) { return; }
        const record = activityRecord(event, this.timers.now());
        if (record) {
            this.events.push(record);
            if (this.events.length > EVENT_LIMIT) { this.events.splice(0, this.events.length - EVENT_LIMIT); this.eventsOmitted = true; }
        }
        if (event.kind === 'status' || event.kind === 'job' || event.kind === 'resync') {
            // While the Explorer is hidden an event only marks the observations old; showing it reads them.
            if (this.retainers > 0) { void this.read(); } else { this.outdated = true; }
        }
    }

    /** While retained (the Explorer is visible), poll at the interval as long as work runs or waits. */
    retain(): { dispose(): void } {
        this.retainers++;
        if (this.outdated) { this.outdated = false; void this.read(); }
        this.schedule();
        let released = false;
        return { dispose: () => { if (released) { return; } released = true; this.retainers = Math.max(0, this.retainers - 1); if (!this.retainers) { this.stop(); } } };
    }
    get polling(): boolean { return this.timer !== undefined; }
    private active(): boolean { return this.bindings.some(binding => workActive(this.observations.get(binding))); }
    private schedule(): void {
        if (this.disposed || this.timer !== undefined || this.reading || this.retainers < 1 || !this.active()) { return; }
        this.timer = this.timers.set(() => { this.timer = undefined; void this.read(); }, this.intervalMs);
    }
    private stop(): void { if (this.timer !== undefined) { this.timers.clear(this.timer); this.timer = undefined; } }
    dispose(): void { this.disposed = true; this.stop(); this.observations.clear(); }
}
