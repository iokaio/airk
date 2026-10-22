// SPDX-License-Identifier: Apache-2.0
import { Clock } from './scheduler';
import { LaunchSpec } from './model';

export interface Coverage {
    collection: string; root: string; generation: number; indexed: boolean; inventory_revision: number;
    scanned_at_unix_ms: number; scan_complete: boolean; eligible: number; current: number; stale: number;
    missing: number; extraction_failed: number; capacity_blocked: number; unreadable: number; excluded: number;
    unsupported: number; pending_removals: number; budget_exhausted: boolean; percent: number | null;
    source_configuration_pending?: boolean;
    eligible_source_bytes?: number; extracted_bytes?: number;
    scan_limits?: { corpus_bytes: number; extracted_bytes: number };
}
export interface CoverageView { report?: Coverage; scanning: boolean; pending: boolean; error?: string }
export function coverageLabel(view: CoverageView | undefined, lastIndexed?: string): string {
    if (view?.scanning) { return 'Scanning'; }
    if (view?.error) { return 'Coverage unknown'; }
    const report = view?.report;
    if (!report || view?.pending) { return 'Coverage pending rescan'; }
    const indexedAt = lastIndexed ? Date.parse(lastIndexed) : NaN;
    if (Number.isFinite(indexedAt) && report.scanned_at_unix_ms < indexedAt) { return 'Coverage pending rescan'; }
    if (!report.scan_complete || report.source_configuration_pending) { return 'Coverage unknown'; }
    if (!report.eligible) { return 'No eligible files'; }
    if (!report.indexed) { return 'Not indexed'; }
    // Calculate independently using floor: 999/1000 must never display 100%.
    return `${Math.floor(Math.min(report.current, report.eligible) * 100 / report.eligible)}%`;
}
export function coverageDetails(name: string, view: CoverageView | undefined, unsaved: number, lastIndexed?: string): string {
    const report = view?.report;
    const rows = [`${name} — click to open AIRK controls`];
    if (report) {
        rows.push(`${report.current} of ${report.eligible} eligible files current; ${report.stale} stale; ${report.missing} awaiting indexing`,
            `${report.extraction_failed} extraction failed; ${report.capacity_blocked} capacity blocked; ${report.unreadable} unreadable`,
            `${report.pending_removals} pending removals; ${report.excluded} per-file exclusions; ${report.unsupported} unsupported`,
            report.scanned_at_unix_ms ? `Inventory measured ${new Date(report.scanned_at_unix_ms).toLocaleString()}` : 'Inventory not yet measured');
        if (!report.scan_complete) { rows.push('Partial inventory; coverage is unknown.'); }
        if (report.eligible_source_bytes !== undefined && report.scan_limits) {
            rows.push(`Eligible source: ${(report.eligible_source_bytes / 1048576).toFixed(1)} MiB / ${(report.scan_limits.corpus_bytes / 1048576).toFixed(1)} MiB limit${report.scan_complete ? '' : ' (partial scan)'}. Use AIRK: Show Ingestion Limits for details.`);
        }
        if (report.source_configuration_pending) { rows.push('Source folders changed; index to publish their new bindings.'); }
    }
    if (coverageLabel(view, lastIndexed) === 'Coverage pending rescan') {
        rows.push('Coverage pending rescan; inventories refresh every five minutes or on explicit refresh.');
    }
    if (unsaved) { rows.push(`${unsaved} unsaved document${unsaved === 1 ? '' : 's'}; AIRK indexes saved files.`); }
    if (view?.error) { rows.push(view.error); }
    return rows.join('\n');
}

const clock: Clock = { now: () => Date.now(), set: (fn, ms) => setTimeout(fn, ms), clear: timer => clearTimeout(timer as NodeJS.Timeout) };
interface Pending { spec: LaunchSpec; revision: number; first: number; last: number }
/** One bounded native scan at a time; edits invalidate an in-flight observation. */
export class CoverageQueue {
    private views = new Map<string, CoverageView>();
    private pending = new Map<string, Pending>();
    private revisions = new Map<string, number>();
    private timer?: unknown;
    private busy = false;
    private stopped = false;
    private sequence = 0;
    private quietMs = 2000;
    private idleMs = 0;
    private lastActivity = 0;
    constructor(private readonly run: (spec: LaunchSpec) => Promise<Coverage>, private readonly changed: () => void,
        private readonly timing: Clock = clock) {}
    get(collection: string): CoverageView | undefined { return this.views.get(collection); }
    configure(quietMs: number, idleMs: number): void { this.quietMs = quietMs; this.idleMs = idleMs; this.schedule(); }
    activity(): void { this.lastActivity = this.timing.now(); if (this.idleMs) { this.schedule(); } }
    /** An edit invalidates coverage without launching a separate source inventory. */
    invalidate(spec: LaunchSpec): void {
        if (this.stopped) { return; }
        this.revisions.set(spec.collection, ++this.sequence);
        const prior = this.views.get(spec.collection);
        this.views.set(spec.collection, { ...prior, scanning: prior?.scanning ?? false, pending: true });
        this.changed();
    }
    request(spec: LaunchSpec, immediate = false): void {
        if (this.stopped) { return; }
        const revision = ++this.sequence;
        this.revisions.set(spec.collection, revision);
        const now = this.timing.now();
        const existing = this.pending.get(spec.collection);
        this.pending.set(spec.collection, { spec, revision, first: immediate ? -Infinity : existing?.first ?? now, last: immediate ? -Infinity : now });
        const prior = this.views.get(spec.collection);
        this.views.set(spec.collection, { ...prior, scanning: prior?.scanning ?? false, pending: true });
        this.schedule();
    }
    forget(collection: string): void { this.revisions.delete(collection); this.pending.delete(collection); this.views.delete(collection); this.schedule(); }
    private due(job: Pending): number {
        if (job.first === -Infinity) { return this.timing.now(); }
        return this.idleMs ? Math.max(job.last + this.quietMs, this.lastActivity + this.idleMs)
            : Math.min(job.last + this.quietMs, job.first + 30000);
    }
    private schedule(): void {
        if (this.timer !== undefined) { this.timing.clear(this.timer); this.timer = undefined; }
        this.changed();
        if (this.stopped || this.busy) { return; }
        const next = [...this.pending.values()].sort((a, b) => this.due(a) - this.due(b))[0];
        if (next) { this.timer = this.timing.set(() => { this.timer = undefined; void this.execute(next); }, Math.max(0, this.due(next) - this.timing.now())); }
    }
    private async execute(job: Pending): Promise<void> {
        if (this.stopped) { return; }
        this.busy = true; this.pending.delete(job.spec.collection);
        this.views.set(job.spec.collection, { ...this.views.get(job.spec.collection), scanning: true, pending: true }); this.changed();
        try {
            const report = await this.run(job.spec);
            if (this.revisions.has(job.spec.collection)) {
                const unchanged = this.revisions.get(job.spec.collection) === job.revision;
                this.views.set(job.spec.collection, unchanged ? { report, scanning: false, pending: false }
                    : { ...this.views.get(job.spec.collection), scanning: false, pending: true });
            }
        } catch (error) {
            if (this.revisions.get(job.spec.collection) === job.revision) {
                this.views.set(job.spec.collection, { ...this.views.get(job.spec.collection), scanning: false, pending: true, error: String(error) });
            } else if (this.revisions.has(job.spec.collection)) {
                this.views.set(job.spec.collection, { ...this.views.get(job.spec.collection), scanning: false, pending: true });
            }
        } finally { this.busy = false; this.schedule(); }
    }
    dispose(): void { this.stopped = true; if (this.timer !== undefined) { this.timing.clear(this.timer); } }
}
