// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { Coverage, CoverageQueue, coverageDetails, coverageLabel } from '../coverage';
import { LaunchSpec } from '../model';
import { Clock } from '../scheduler';

const spec: LaunchSpec = { command: '/fixture/airk', root: '/fixture/root', dataDir: '/fixture/state', collection: 'fixture', name: 'Fixture' };
const report: Coverage = { collection: 'fixture', root: spec.root, generation: 1, indexed: true, inventory_revision: 1,
    scanned_at_unix_ms: 1000, scan_complete: true, eligible: 1000, current: 999, stale: 1, missing: 0, extraction_failed: 0,
    capacity_blocked: 0, unreadable: 0, excluded: 0, unsupported: 0, pending_removals: 0, budget_exhausted: false, percent: 99 };
const flush = () => new Promise<void>(resolve => setImmediate(resolve));
class FakeClock implements Clock {
    time = 0; sequence = 0; timers = new Map<number, { fn(): void; at: number }>();
    now(): number { return this.time; }
    set(fn: () => void, ms: number): number { const id = ++this.sequence; this.timers.set(id, { fn, at: this.time + ms }); return id; }
    clear(timer: unknown): void { this.timers.delete(timer as number); }
    async advance(ms: number): Promise<void> {
        this.time += ms;
        for (const [id, timer] of [...this.timers]) { if (timer.at <= this.time) { this.timers.delete(id); timer.fn(); } }
        await flush();
    }
}

test('coverage labels never round partial, empty, changed or nearly complete inventories to 100%', () => {
    const view = { report, scanning: false, pending: false };
    assert.equal(coverageLabel(view), '99%');
    assert.equal(coverageLabel({ ...view, report: { ...report, current: 1000 } }), '100%');
    assert.equal(coverageLabel({ ...view, report: { ...report, eligible: 0 } }), 'No eligible files');
    assert.equal(coverageLabel({ ...view, report: { ...report, scan_complete: false } }), 'Coverage unknown');
    assert.equal(coverageLabel({ ...view, report: { ...report, source_configuration_pending: true } }), 'Coverage unknown');
    assert.equal(coverageLabel({ ...view, pending: true }), 'Coverage unknown');
    assert.equal(coverageLabel({ ...view, report: { ...report, indexed: false } }), 'Not indexed');
    assert.match(coverageDetails('Fixture', view, 2), /2 unsaved documents/);
    assert.match(coverageDetails('Fixture', view, 0), /999 of 1000 eligible files current/);
});

test('coverage scans coalesce events and invalidate observations when edits arrive during a scan', async () => {
    const clock = new FakeClock(); let calls = 0; let release!: (value: Coverage) => void;
    const queue = new CoverageQueue(async () => { calls++; return new Promise<Coverage>(resolve => { release = resolve; }); }, () => {}, clock);
    for (let i = 0; i < 100; i++) { queue.request(spec); }
    await clock.advance(1999); assert.equal(calls, 0);
    await clock.advance(1); assert.equal(calls, 1);
    queue.request(spec); release(report); await flush();
    assert.equal(coverageLabel(queue.get(spec.collection)), 'Coverage unknown');
    await clock.advance(2000); assert.equal(calls, 2);
    release(report); await flush(); assert.equal(coverageLabel(queue.get(spec.collection)), '99%'); queue.dispose();
});

test('closing and reopening a folder discards scans for its previous storage binding', async () => {
    const clock = new FakeClock(); let release!: (value: Coverage) => void;
    const queue = new CoverageQueue(async () => new Promise<Coverage>(resolve => { release = resolve; }), () => {}, clock);
    queue.request(spec, true); await clock.advance(0);
    queue.forget(spec.collection); queue.request({ ...spec, dataDir: '/fixture/new-state' });
    release(report); await flush();
    assert.equal(coverageLabel(queue.get(spec.collection)), 'Coverage unknown');
    assert.ok(queue.get(spec.collection)?.pending); queue.dispose();
});
