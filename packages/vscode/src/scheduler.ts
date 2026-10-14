// SPDX-License-Identifier: Apache-2.0
import { IndexMode, IndexReport, LaunchSpec } from './model';

export interface Options { mode: IndexMode; quietMs: number; idleMs: number; maxDelayMs: number; minIntervalMs: number }
export interface Clock { now(): number; set(fn: () => void, ms: number): unknown; clear(timer: unknown): void }
const systemClock: Clock = { now: () => Date.now(), set: (fn, ms) => setTimeout(fn, ms), clear: timer => clearTimeout(timer as NodeJS.Timeout) };
export class CapacityError extends Error {}
export class PausedError extends Error {}
type State = 'queued' | 'indexing' | 'current' | 'partial' | 'failed' | 'capacity-limited' | 'paused';
interface Waiter { revision: number; resolve(): void; reject(error: unknown): void }
interface Job {
    spec: LaunchSpec; revision: number; completed: number; first: number; last: number;
    ended: number; manual: boolean; full: boolean; blocked: boolean; state: State; waiters: Waiter[];
    runningRevision?: number; runningFull?: boolean;
}

/** Batches UI events; the native kernel remains the sole index writer/publisher. */
export class Scheduler {
    private jobs = new Map<string, Job>();
    private timer: unknown;
    private busy = false;
    private paused = false;
    private stopped = false;
    private lastActivity: number;
    constructor(private options: Options,
        private readonly run: (spec: LaunchSpec, full: boolean, automatic: boolean) => Promise<IndexReport>,
        private readonly changed: () => void,
        private readonly failed: (spec: LaunchSpec, error: unknown) => void,
        private readonly clock: Clock = systemClock) { this.lastActivity = clock.now(); }

    configure(options: Options): void { this.options = options; this.schedule(); }
    activity(): void { this.lastActivity = this.clock.now(); this.schedule(); }
    pause(): void { this.paused = true; this.schedule(); }
    resume(): void { this.paused = false; for (const job of this.jobs.values()) { job.blocked = false; } this.schedule(); }
    get isPaused(): boolean { return this.paused; }
    get isBusy(): boolean { return this.busy; }
    states(): { collection: string; name: string; state: string; pending: boolean }[] {
        return [...this.jobs.values()].map(job => ({ collection: job.spec.collection, name: job.spec.name,
            state: this.paused && job.state !== 'indexing' ? 'paused' : job.state,
            pending: job.revision > job.completed }));
    }
    forget(collection: string): void {
        const job = this.jobs.get(collection);
        // An in-flight native write finishes atomically; no further jobs run for a closed root.
        for (const waiter of job?.waiters ?? []) { waiter.reject(new Error('Workspace folder closed.')); }
        this.jobs.delete(collection); this.schedule();
    }
    request(spec: LaunchSpec, manual = false, full = false): Promise<void> {
        if (this.stopped) { return Promise.reject(new Error('AIRK scheduler stopped.')); }
        const now = this.clock.now();
        let job = this.jobs.get(spec.collection);
        if (!job) {
            job = { spec, revision: 0, completed: 0, first: now, last: now, ended: -Infinity,
                manual: false, full: false, blocked: false, state: 'queued', waiters: [] };
            this.jobs.set(spec.collection, job);
        }
        // Repeated explicit clicks share the existing up-to-date pass. Filesystem
        // events still advance the revision, including while a write is running.
        const coalesced = manual && job.revision > job.completed && (!full || job.full || job.runningFull);
        if (job.revision === job.completed) { job.first = now; }
        job.spec = spec;
        if (!coalesced) { job.revision++; job.last = now; }
        const capacityBlocked = job.blocked && job.state === 'capacity-limited' && !manual;
        if (!capacityBlocked) { job.blocked = false; }
        const alreadyRunning = job.runningRevision === job.revision;
        job.manual ||= manual && !alreadyRunning; job.full ||= full && !alreadyRunning;
        if (job.state !== 'indexing' && !capacityBlocked) { job.state = 'queued'; }
        const promise = manual ? new Promise<void>((resolve, reject) => job!.waiters.push({ revision: job!.revision, resolve, reject })) : Promise.resolve();
        this.schedule();
        return promise;
    }
    private due(job: Job): number {
        if (job.manual) { return this.clock.now(); }
        if (this.paused || this.options.mode === 'manual' || job.blocked) { return Infinity; }
        const quiet = job.last + this.options.quietMs;
        const batch = this.options.mode === 'idle'
            ? Math.max(quiet, this.lastActivity + this.options.idleMs)
            : Math.min(quiet, job.first + this.options.maxDelayMs);
        return Math.max(batch, job.ended + this.options.minIntervalMs);
    }
    private schedule(): void {
        if (this.timer !== undefined) { this.clock.clear(this.timer); this.timer = undefined; }
        this.changed();
        if (this.busy || this.stopped) { return; }
        const pending = [...this.jobs.values()].filter(job => job.revision > job.completed);
        pending.sort((a, b) => this.due(a) - this.due(b));
        const job = pending[0];
        if (!job || !Number.isFinite(this.due(job))) { return; }
        this.timer = this.clock.set(() => { this.timer = undefined; void this.execute(job); }, Math.max(0, this.due(job) - this.clock.now()));
    }
    private async execute(job: Job): Promise<void> {
        if (this.stopped) { return; }
        this.busy = true;
        const revision = job.revision, full = job.full, automatic = !job.manual;
        job.runningRevision = revision; job.runningFull = full;
        job.manual = false; job.full = false; job.state = 'indexing'; this.changed();
        try {
            const report = await this.run(job.spec, full, automatic);
            job.completed = revision;
            job.state = job.revision > revision ? 'queued' : report.skipped.length ? 'partial' : 'current';
            for (const waiter of job.waiters.filter(w => w.revision <= revision)) { waiter.resolve(); }
            job.waiters = job.waiters.filter(w => w.revision > revision);
            if (job.revision > revision) { job.first = job.last; }
        } catch (error) {
            job.blocked = true; job.state = error instanceof CapacityError ? 'capacity-limited' : error instanceof PausedError ? 'paused' : 'failed';
            for (const waiter of job.waiters.filter(w => w.revision <= revision)) { waiter.reject(error); }
            job.waiters = job.waiters.filter(w => w.revision > revision);
            this.failed(job.spec, error);
        } finally { this.busy = false; job.runningRevision = undefined; job.runningFull = undefined; job.ended = this.clock.now(); this.schedule(); }
    }
    dispose(): void {
        this.stopped = true;
        if (this.timer !== undefined) { this.clock.clear(this.timer); }
        for (const job of this.jobs.values()) { for (const waiter of job.waiters) { waiter.reject(new Error('AIRK extension closed.')); } }
    }
}
