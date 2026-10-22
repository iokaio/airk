// SPDX-License-Identifier: Apache-2.0
import { ScheduledIndexStillQueued } from './native';
import { IndexMode, IndexReport, LaunchSpec } from './model';

export interface Options { mode: IndexMode; quietMs: number; idleMs: number; maxDelayMs: number; minIntervalMs: number }
export interface Clock { now(): number; set(fn: () => void, ms: number): unknown; clear(timer: unknown): void }
const systemClock: Clock = { now: () => Date.now(), set: (fn, ms) => setTimeout(fn, ms), clear: timer => clearTimeout(timer as NodeJS.Timeout) };
export class CapacityError extends Error {}
export class PausedError extends Error {}
export type SemanticResult = 'pending' | 'busy' | 'ready' | 'paused' | 'yield';
type State = 'queued' | 'indexing' | 'enriching' | 'semantic-pending' | 'current' | 'partial' | 'failed' | 'capacity-limited' | 'paused';
interface Waiter { revision: number; resolve(): void; reject(error: unknown): void }
interface Job {
    spec: LaunchSpec; revision: number; completed: number; first: number; last: number;
    ended: number; manual: boolean; full: boolean; blocked: boolean; state: State; waiters: Waiter[];
    deferredRevision?: number; deferredUntil?: number; runningRevision?: number; runningFull?: boolean;
    runningAutomatic?: boolean;
    semanticPending: boolean; semanticAt: number; baseState: 'current' | 'partial';
    semanticBusyDelay: number; semanticResetRevision: number;
}

/** Batches UI events; the native kernel remains the sole index writer/publisher. */
export class Scheduler {
    private jobs = new Map<string, Job>();
    private timer: unknown;
    private busy = false;
    private paused = false;
    private stopped = false;
    private resumeRevision = 0;
    private holds = 0;
    private idleWaiters = new Set<() => void>();
    private lastActivity: number;
    constructor(private options: Options,
        private readonly run: (spec: LaunchSpec, full: boolean, automatic: boolean) => Promise<IndexReport>,
        private readonly changed: () => void,
        private readonly failed: (spec: LaunchSpec, error: unknown) => void,
        private readonly clock: Clock = systemClock,
        private readonly enrich?: (spec: LaunchSpec) => Promise<SemanticResult>) { this.lastActivity = clock.now(); }

    configure(options: Options): void { this.options = options; this.schedule(); }
    activity(): void { this.lastActivity = this.clock.now(); this.schedule(); }
    pause(): void { this.paused = true; this.schedule(); }
    resume(): void {
        this.resumeRevision++;
        this.paused = false;
        for (const job of this.jobs.values()) {
            job.blocked = false;
            this.resetSemanticBackoff(job);
            if (job.semanticPending && job.state === 'paused') { job.state = 'semantic-pending'; }
        }
        this.schedule();
    }
    get isPaused(): boolean { return this.paused; }
    get isBusy(): boolean { return this.busy; }
    get canRunMaintenance(): boolean {
        return !this.busy && !this.stopped && !this.holds
            && this.clock.now() - this.lastActivity >= Math.max(10000, this.options.idleMs)
            && ![...this.jobs.values()].some(job => Number.isFinite(this.nextDue(job)));
    }
    /** Hold new jobs during storage selection without changing the user's pause state. */
    async holdWhile<T>(operation: (idle: Promise<void>) => Promise<T>): Promise<T> {
        if (this.stopped) { throw new Error('AIRK scheduler stopped.'); }
        this.holds++;
        let ready!: () => void;
        const idle = new Promise<void>(resolve => { ready = resolve; });
        if (this.busy) { this.idleWaiters.add(ready); } else { ready(); }
        this.schedule();
        try { return await operation(idle); }
        finally { this.idleWaiters.delete(ready); this.holds--; this.schedule(); }
    }
    states(): { collection: string; name: string; state: string; pending: boolean }[] {
        return [...this.jobs.values()].map(job => ({ collection: job.spec.collection, name: job.spec.name,
            state: this.paused && !['indexing', 'enriching'].includes(job.state) ? 'paused' : job.state,
            pending: job.revision > job.completed || job.semanticPending }));
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
                manual: false, full: false, blocked: false, state: 'queued', waiters: [],
                semanticPending: false, semanticAt: now, baseState: 'current', semanticBusyDelay: 0, semanticResetRevision: 0 };
            this.jobs.set(spec.collection, job);
        }
        // Repeated explicit clicks share the existing up-to-date pass. Filesystem
        // events still advance the revision, including while a write is running.
        const coalesced = manual && job.revision > job.completed && (!full || job.full || job.runningFull)
            && !(job.runningRevision === job.revision && job.runningAutomatic);
        if (job.revision === job.completed) { job.first = now; }
        job.spec = spec;
        if (manual) {
            this.resetSemanticBackoff(job);
            if (this.paused) { job.deferredRevision = undefined; job.deferredUntil = undefined; }
        }
        if (!coalesced) { job.revision++; job.last = now; }
        const capacityBlocked = job.blocked && job.state === 'capacity-limited' && !manual;
        if (!capacityBlocked) { job.blocked = false; }
        const alreadyRunning = job.runningRevision === job.revision;
        job.manual ||= manual && !alreadyRunning; job.full ||= full && !alreadyRunning;
        if (!['indexing', 'enriching'].includes(job.state) && !capacityBlocked) { job.state = 'queued'; }
        const promise = manual ? new Promise<void>((resolve, reject) => job!.waiters.push({ revision: job!.revision, resolve, reject })) : Promise.resolve();
        this.schedule();
        return promise;
    }
    /** Observe durable work without requesting a new source scan or clearing failures. */
    observeSemantic(spec: LaunchSpec, pending: boolean): void {
        if (this.stopped || !this.enrich) { return; }
        let job = this.jobs.get(spec.collection);
        if (!job && pending) {
            const now = this.clock.now();
            job = { spec, revision: 0, completed: 0, first: now, last: now, ended: -Infinity,
                manual: false, full: false, blocked: false, state: 'semantic-pending', waiters: [],
                semanticPending: true, semanticAt: now, baseState: 'current', semanticBusyDelay: 0, semanticResetRevision: 0 };
            this.jobs.set(spec.collection, job);
        }
        if (!job) { return; }
        job.semanticPending = pending;
        if (!pending) { this.resetSemanticBackoff(job); }
        if (!job.blocked && job.revision === job.completed && !['indexing', 'enriching'].includes(job.state)) {
            job.state = pending ? 'semantic-pending' : job.baseState;
        }
        this.schedule();
    }
    private nextDue(job: Job): number {
        const source = job.revision > job.completed ? this.due(job) : Infinity;
        return Math.min(source, this.semanticDue(job));
    }
    private semanticDue(job: Job): number {
        // Manual/on-save modes control new source scans, not completion of an
        // already requested semantic build. Explicit Pause still stops batches.
        return this.enrich && job.semanticPending && !this.paused && !job.blocked ? job.semanticAt : Infinity;
    }
    private due(job: Job): number {
        if (job.deferredUntil !== undefined) { return this.paused || job.blocked ? Infinity : Math.max(this.clock.now(), job.deferredUntil); }
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
        if (this.busy || this.stopped || this.holds) { return; }
        const pending = [...this.jobs.values()].filter(job => Number.isFinite(this.nextDue(job)));
        pending.sort((a, b) => this.nextDue(a) - this.nextDue(b));
        const job = pending[0];
        if (!job) { return; }
        this.timer = this.clock.set(() => {
            this.timer = undefined;
            if (this.busy || this.stopped || this.holds || this.jobs.get(job.spec.collection) !== job) { return; }
            const source = job.revision > job.completed ? this.due(job) : Infinity;
            const semantic = this.semanticDue(job), now = this.clock.now();
            if (source <= now) { void this.execute(job); }
            else if (semantic <= now) { void this.executeSemantic(job); }
            // Timers can arrive before the wall-clock deadline. Keep the work
            // queued instead of treating an early source timer as semantic work.
            else { this.schedule(); }
        }, Math.max(0, this.nextDue(job) - this.clock.now()));
    }
    private async executeSemantic(job: Job): Promise<void> {
        if (this.stopped || !this.enrich) { return; }
        const resumeRevision = this.resumeRevision;
        const resetRevision = job.semanticResetRevision;
        let delay = 0;
        this.busy = true; job.state = 'enriching'; this.changed();
        try {
            const result = await this.enrich(job.spec);
            if (result === 'busy' && resetRevision === job.semanticResetRevision) {
                job.semanticBusyDelay = Math.min(30000, Math.max(1000, job.semanticBusyDelay * 2));
                delay = job.semanticBusyDelay;
            } else {
                job.semanticBusyDelay = 0;
                if (result === 'yield') { delay = 1000; }
                if (resetRevision !== job.semanticResetRevision) { delay = 0; }
            }
            job.semanticPending = result !== 'ready';
            job.blocked = result === 'paused' && resumeRevision === this.resumeRevision;
            job.state = job.blocked ? 'paused' : job.revision > job.completed ? 'queued'
                : job.semanticPending ? 'semantic-pending' : job.baseState;
        } catch (error) {
            job.blocked = true; job.state = 'failed'; this.failed(job.spec, error);
        } finally {
            this.busy = false; job.semanticAt = this.clock.now() + delay;
            // Rotate equal-deadline jobs so a ready collection gets the next turn
            // without forcing a one-second gap between successful native drains.
            if (this.jobs.get(job.spec.collection) === job) { this.jobs.delete(job.spec.collection); this.jobs.set(job.spec.collection, job); }
            for (const ready of this.idleWaiters) { ready(); }
            this.idleWaiters.clear(); this.schedule();
        }
    }
    private resetSemanticBackoff(job: Job): void {
        job.semanticBusyDelay = 0; job.semanticAt = this.clock.now(); job.semanticResetRevision++;
    }
    private async execute(job: Job): Promise<void> {
        if (this.stopped) { return; }
        this.busy = true;
        const continuing = job.deferredRevision !== undefined;
        const revision = job.deferredRevision ?? job.revision, full = !continuing && job.full, automatic = continuing || !job.manual;
        job.runningRevision = revision; job.runningFull = full; job.runningAutomatic = automatic;
        if (!continuing) { job.manual = false; job.full = false; }
        job.state = 'indexing'; this.changed();
        try {
            const report = await this.run(job.spec, full, automatic);
            this.resetSemanticBackoff(job);
            job.completed = revision; job.deferredUntil = undefined; job.deferredRevision = undefined;
            job.baseState = report.skipped.length ? 'partial' : 'current';
            job.state = job.revision > revision ? 'queued' : job.semanticPending ? 'semantic-pending' : job.baseState;
            for (const waiter of job.waiters.filter(w => w.revision <= revision)) { waiter.resolve(); }
            job.waiters = job.waiters.filter(w => w.revision > revision);
            if (job.revision > revision) { job.first = job.last; }
        } catch (error) {
            if (error instanceof ScheduledIndexStillQueued) {
                job.blocked = error.paused; job.state = error.paused ? 'paused' : 'queued';
                job.deferredRevision = revision; job.deferredUntil = this.clock.now() + 30000;
                for (const waiter of job.waiters.filter(w => w.revision <= revision)) { waiter.resolve(); }
            } else {
                job.deferredRevision = undefined; job.deferredUntil = undefined;
                job.blocked = true; job.state = error instanceof CapacityError ? 'capacity-limited' : error instanceof PausedError ? 'paused' : 'failed';
                for (const waiter of job.waiters.filter(w => w.revision <= revision)) { waiter.reject(error); }
                if (!(error instanceof PausedError)) { this.failed(job.spec, error); }
            }
            job.waiters = job.waiters.filter(w => w.revision > revision);
        } finally {
            this.busy = false; job.runningRevision = undefined; job.runningFull = undefined; job.runningAutomatic = undefined; job.ended = this.clock.now();
            for (const ready of this.idleWaiters) { ready(); }
            this.idleWaiters.clear(); this.schedule();
        }
    }
    dispose(): void {
        this.stopped = true;
        for (const ready of this.idleWaiters) { ready(); }
        this.idleWaiters.clear();
        if (this.timer !== undefined) { this.clock.clear(this.timer); }
        for (const job of this.jobs.values()) { for (const waiter of job.waiters) { waiter.reject(new Error('AIRK extension closed.')); } }
    }
}
