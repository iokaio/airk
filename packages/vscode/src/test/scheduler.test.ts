// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { CapacityError, Clock, Options, Scheduler, SemanticResult } from '../scheduler';
import { IndexReport, LaunchSpec } from '../model';

const spec: LaunchSpec = { command: '/fixture/airk', root: '/fixture/project', dataDir: '/fixture/state', collection: 'project-fixture', name: 'Fixture' };
const report: IndexReport = { generation: 1, indexed: 1, reused: 0, removed: 0, skipped: [] };
const options: Options = { mode: 'debounced', quietMs: 100, maxDelayMs: 500, idleMs: 1000, minIntervalMs: 0 };
const flush = () => new Promise<void>(resolve => setImmediate(resolve));
class FakeClock implements Clock {
    time = 0; sequence = 0; timers = new Map<number, { fn(): void; at: number }>();
    now(): number { return this.time; }
    set(fn: () => void, ms: number): number { const id = ++this.sequence; this.timers.set(id, { fn, at: this.time + ms }); return id; }
    clear(timer: unknown): void { this.timers.delete(timer as number); }
    async fireNextEarly(ms: number): Promise<void> {
        const next = [...this.timers.entries()].sort((a, b) => a[1].at - b[1].at)[0];
        assert.ok(next && ms > 0 && next[1].at - ms >= this.time, 'Expected a future timer');
        this.time = next[1].at - ms; this.timers.delete(next[0]); next[1].fn(); await flush();
    }
    async advance(ms: number): Promise<void> {
        const end = this.time + ms;
        for (let count = 0; count < 1000; count++) {
            const next = [...this.timers.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
            if (!next) { this.time = end; await flush(); return; }
            this.time = next[1].at; this.timers.delete(next[0]); next[1].fn(); await flush();
        }
        throw new Error('Scheduler timer loop');
    }
}

test('an early source timer preserves its deadline without starting unrequested semantic work', async () => {
    for (const hasEnrichment of [true, false]) {
        const clock = new FakeClock(); let scans = 0, batches = 0, failures = 0;
        const scheduler = new Scheduler({ ...options, quietMs: 250 }, async () => { scans++; return report; },
            () => {}, () => { failures++; }, clock, hasEnrichment ? async () => {
                batches++; throw new Error('Unrequested semantic work must not block the source scan');
            } : undefined);
        try {
            await scheduler.request(spec);
            await clock.fireNextEarly(1);
            assert.equal(clock.now(), 249);
            assert.deepEqual({ scans, batches, failures }, { scans: 0, batches: 0, failures: 0 });
            assert.equal(scheduler.states()[0].state, 'queued');
            assert.equal(scheduler.states()[0].pending, true);
            assert.equal(clock.timers.size, 1, 'Early delivery must rearm pending work');
            await clock.advance(1);
            assert.deepEqual({ scans, batches, failures }, { scans: 1, batches: 0, failures: 0 });
            assert.equal(scheduler.states()[0].state, 'current');
            assert.equal(scheduler.states()[0].pending, false);
            await clock.advance(10000);
            assert.equal(scans, 1); assert.equal(batches, 0);
        } finally { scheduler.dispose(); }
    }
});

test('an early semantic timer keeps its retry deadline and yields to due source work', async () => {
    const clock = new FakeClock(); const ran: { kind: string; collection: string; at: number }[] = [];
    const other = { ...spec, collection: 'second', root: '/fixture/second' }; let batches = 0;
    const scheduler = new Scheduler({ ...options, mode: 'manual' }, async current => {
        ran.push({ kind: 'source', collection: current.collection, at: clock.now() }); return report;
    }, () => {}, () => {}, clock, async current => {
        ran.push({ kind: 'semantic', collection: current.collection, at: clock.now() });
        return ++batches === 1 ? 'pending' : 'ready';
    });
    try {
        scheduler.observeSemantic(spec, true); await clock.advance(0);
        await clock.fireNextEarly(1);
        assert.equal(clock.now(), 999); assert.equal(batches, 1);
        assert.equal(scheduler.states()[0].state, 'semantic-pending');
        const indexed = scheduler.request(other, true);
        await clock.advance(0); await indexed;
        await clock.advance(1);
        assert.deepEqual(ran, [
            { kind: 'semantic', collection: spec.collection, at: 0 },
            { kind: 'source', collection: other.collection, at: 999 },
            { kind: 'semantic', collection: spec.collection, at: 1000 },
        ]);
        assert.ok(scheduler.states().every(state => !state.pending));
        assert.equal(clock.timers.size, 0);
    } finally { scheduler.dispose(); }
});

test('semantic batches finish after Index Now without another source edit or background service', async () => {
    const clock = new FakeClock(); let scans = 0, batches = 0;
    const scheduler = new Scheduler({ ...options, mode: 'manual' }, async () => {
        scans++; scheduler.observeSemantic(spec, true); return report;
    }, () => {}, () => {}, clock, async () => ++batches < 3 ? 'pending' : 'ready');
    const indexed = scheduler.request(spec, true);
    await clock.advance(0); await indexed;
    assert.equal(scans, 1); assert.equal(batches, 1);
    assert.equal(scheduler.states()[0].state, 'semantic-pending');
    // Manual-mode edits must not strand the already requested semantic build.
    await scheduler.request(spec);
    await clock.advance(2000);
    assert.equal(batches, 3); assert.equal(scans, 1);
    assert.equal(scheduler.states()[0].pending, true, 'The manual source edit still awaits Index Now');
    await clock.advance(10000); assert.equal(batches, 3);
    scheduler.dispose();
});

test('reopened semantic jobs run in manual mode, yield between collections, and honor pause', async () => {
    const clock = new FakeClock(), ran: string[] = [];
    const other = { ...spec, collection: 'second', root: '/fixture/second' };
    const scheduler = new Scheduler({ ...options, mode: 'manual' }, async () => { throw new Error('Unexpected source scan'); },
        () => {}, () => {}, clock, async current => { ran.push(current.collection); return ran.length < 4 ? 'pending' : 'ready'; });
    scheduler.pause(); scheduler.observeSemantic(spec, true); scheduler.observeSemantic(other, true);
    await clock.advance(5000); assert.equal(ran.length, 0);
    scheduler.resume(); await clock.advance(0);
    assert.deepEqual(ran, [spec.collection, other.collection]);
    scheduler.pause(); await clock.advance(5000); assert.equal(ran.length, 2);
    scheduler.resume(); await clock.advance(2000);
    assert.deepEqual(ran, [spec.collection, other.collection, spec.collection, other.collection, spec.collection]);
    assert.ok(scheduler.states().every(item => !item.pending));
    scheduler.dispose();
});

test('semantic failures and native pause stop retries until explicit resume', async () => {
    for (const outcome of ['error', 'paused'] as const) {
        const clock = new FakeClock(); let calls = 0, failures = 0;
        const scheduler = new Scheduler(options, async () => report, () => {}, () => { failures++; }, clock, async () => {
            calls++;
            if (calls === 1) { if (outcome === 'error') { throw new Error('model unavailable'); } return 'paused'; }
            return 'ready';
        });
        scheduler.observeSemantic(spec, true); await clock.advance(0);
        scheduler.observeSemantic(spec, true); await clock.advance(10000);
        assert.equal(calls, 1); assert.equal(failures, outcome === 'error' ? 1 : 0);
        scheduler.resume(); await clock.advance(0);
        assert.equal(calls, 2); assert.equal(scheduler.states()[0].pending, false);
        scheduler.dispose();
    }
});

test('storage selection waits for a semantic batch and removed collections never continue', async () => {
    const clock = new FakeClock(); let batches = 0, release!: (result: SemanticResult) => void;
    let held = false;
    const scheduler = new Scheduler(options, async () => report, () => {}, () => {}, clock, async () => {
        batches++; return new Promise<SemanticResult>(resolve => { release = resolve; });
    });
    scheduler.observeSemantic(spec, true); await clock.advance(0);
    assert.equal(scheduler.isBusy, true);
    const selecting = scheduler.holdWhile(async idle => {
        await idle; held = true; scheduler.forget(spec.collection);
    });
    await flush(); assert.equal(held, false);
    release('pending'); await selecting; await clock.advance(10000);
    assert.equal(held, true); assert.equal(batches, 1); assert.equal(scheduler.states().length, 0);
    scheduler.dispose();
});

test('a late native pause response cannot undo a newer Resume request', async () => {
    const clock = new FakeClock(); let calls = 0, release!: (result: SemanticResult) => void;
    const scheduler = new Scheduler(options, async () => report, () => {}, () => {}, clock, async () => {
        calls++;
        return calls === 1 ? new Promise<SemanticResult>(resolve => { release = resolve; }) : 'ready';
    });
    scheduler.observeSemantic(spec, true); await clock.advance(0);
    scheduler.resume(); release('paused'); await flush();
    await clock.advance(1000);
    assert.equal(calls, 2); assert.equal(scheduler.states()[0].pending, false);
    scheduler.dispose();
});

test('closing the extension during a semantic batch retains work without starting another batch', async () => {
    const clock = new FakeClock(); let batches = 0, release!: (result: SemanticResult) => void;
    const scheduler = new Scheduler(options, async () => report, () => {}, () => {}, clock, async () => {
        batches++; return new Promise<SemanticResult>(resolve => { release = resolve; });
    });
    scheduler.observeSemantic(spec, true); await clock.advance(0);
    scheduler.dispose(); release('pending'); await clock.advance(10000);
    scheduler.observeSemantic(spec, true); await clock.advance(10000);
    assert.equal(batches, 1); assert.equal(clock.timers.size, 0);
});

test('storage selection holds automatic and explicit jobs until the current write settles', async () => {
    for (const fail of [false, true]) {
        const clock = new FakeClock(); let finish!: () => void, finishSelection!: () => void;
        const ran: string[] = [];
        const scheduler = new Scheduler(options, async current => {
            ran.push(current.dataDir);
            if (ran.length === 1) {
                await new Promise<void>(resolve => { finish = resolve; });
                if (fail) { throw new Error('fixture indexing failure'); }
            }
            return report;
        }, () => {}, () => {}, clock);
        await scheduler.request(spec); await clock.advance(100);
        let idle = false;
        const held = scheduler.holdWhile(async ready => {
            await ready; idle = true;
            await new Promise<void>(resolve => { finishSelection = resolve; });
        });
        await scheduler.request(spec);
        const explicit = scheduler.request({ ...spec, collection: 'other' }, true);
        await clock.advance(1000); assert.equal(idle, false); assert.equal(ran.length, 1);
        finish(); await flush(); await clock.advance(1000);
        assert.equal(idle, true); assert.equal(ran.length, 1, 'No queued writer can start while choosing storage');
        scheduler.forget(spec.collection);
        await scheduler.request({ ...spec, dataDir: '/fixture/new-storage' });
        finishSelection(); await held; await clock.advance(100); await explicit;
        assert.deepEqual(ran, [spec.dataDir, spec.dataDir, '/fixture/new-storage']);
        scheduler.dispose();
    }
});

test('cancelled or failed storage selection restores scheduling and preserves an existing pause', async () => {
    for (const paused of [false, true]) {
        const clock = new FakeClock(); let count = 0;
        const scheduler = new Scheduler(options, async () => { count++; return report; }, () => {}, () => {}, clock);
        if (paused) { scheduler.pause(); }
        await scheduler.request(spec);
        await assert.rejects(scheduler.holdWhile(async idle => {
            await idle; await clock.advance(1000); assert.equal(count, 0);
            throw new Error('folder selection failed');
        }), /folder selection failed/);
        assert.equal(scheduler.isPaused, paused);
        await clock.advance(1000); assert.equal(count, paused ? 0 : 1);
        scheduler.dispose();
    }
});

test('idle maintenance waits for activity, queued indexing and storage holds', async () => {
    const clock = new FakeClock(); let finish!: () => void;
    const scheduler = new Scheduler(options, async () => { await new Promise<void>(resolve => { finish = resolve; }); return report; }, () => {}, () => {}, clock);
    assert.equal(scheduler.canRunMaintenance, false);
    await clock.advance(10000); assert.equal(scheduler.canRunMaintenance, true);
    scheduler.activity(); assert.equal(scheduler.canRunMaintenance, false);
    await clock.advance(10000);
    await scheduler.request(spec); assert.equal(scheduler.canRunMaintenance, false);
    await clock.advance(100); assert.equal(scheduler.canRunMaintenance, false);
    finish(); await flush(); assert.equal(scheduler.canRunMaintenance, true);
    await scheduler.holdWhile(async idle => { await idle; assert.equal(scheduler.canRunMaintenance, false); });
    assert.equal(scheduler.canRunMaintenance, true);
    scheduler.dispose(); assert.equal(scheduler.canRunMaintenance, false);
});

test('100 rapid edits coalesce into one job after the quiet period', async () => {
    const clock = new FakeClock(); let count = 0;
    const scheduler = new Scheduler(options, async () => { count++; return report; }, () => {}, () => {}, clock);
    for (let edit = 0; edit < 100; edit++) { await scheduler.request(spec); await clock.advance(1); }
    await clock.advance(98); assert.equal(count, 0);
    await clock.advance(1); assert.equal(count, 1); assert.equal(scheduler.states()[0].pending, false);
    scheduler.dispose();
});

test('edits during indexing survive and collections never run concurrently', async () => {
    const clock = new FakeClock(); let release!: (value: IndexReport) => void; const ran: string[] = [];
    const scheduler = new Scheduler(options, async current => {
        ran.push(current.collection);
        return ran.length === 1 ? new Promise<IndexReport>(resolve => { release = resolve; }) : report;
    }, () => {}, () => {}, clock);
    await scheduler.request(spec); await clock.advance(100);
    await scheduler.request(spec);
    await scheduler.request({ ...spec, collection: 'second', name: 'Second' });
    await clock.advance(1000); assert.equal(ran.length, 1);
    release(report); await flush(); await clock.advance(0);
    assert.equal(ran.length, 3); assert.equal(ran.filter(id => id === spec.collection).length, 2);
    assert.ok(scheduler.states().every(state => !state.pending)); scheduler.dispose();
});

test('manual mode and pause preserve pending work; an explicit request overrides pause', async () => {
    const clock = new FakeClock(); let count = 0;
    const scheduler = new Scheduler({ ...options, mode: 'manual' }, async () => { count++; return report; }, () => {}, () => {}, clock);
    await scheduler.request(spec); await clock.advance(5000); assert.equal(count, 0);
    scheduler.configure(options); scheduler.pause(); await clock.advance(5000); assert.equal(count, 0);
    const manual = scheduler.request(spec, true); await clock.advance(0); await manual;
    assert.equal(count, 1); assert.equal(scheduler.isPaused, true);
    await scheduler.request(spec); scheduler.resume(); await clock.advance(100); assert.equal(count, 2); scheduler.dispose();
});

test('capacity failure preserves pending work without retry loops and manual indexing can proceed', async () => {
    const clock = new FakeClock(); let count = 0;
    const scheduler = new Scheduler(options, async (_spec, _full, automatic) => {
        count++; if (automatic) { throw new CapacityError('fixture budget'); } return report;
    }, () => {}, () => {}, clock);
    await scheduler.request(spec); await clock.advance(10000);
    assert.equal(count, 1); assert.equal(scheduler.states()[0].state, 'capacity-limited'); assert.ok(scheduler.states()[0].pending);
    const manual = scheduler.request(spec, true); await clock.advance(0); await manual; assert.equal(count, 2); scheduler.dispose();
});

test('idle activity delays indexing; maximum delay and minimum interval bound debounced jobs', async () => {
    const clock = new FakeClock(); const times: number[] = [];
    const scheduler = new Scheduler({ ...options, mode: 'idle' }, async () => { times.push(clock.now()); return report; }, () => {}, () => {}, clock);
    await scheduler.request(spec); await clock.advance(900); scheduler.activity(); await clock.advance(999); assert.equal(times.length, 0);
    await clock.advance(1); assert.equal(times[0], 1900);
    scheduler.configure({ ...options, minIntervalMs: 1000 });
    for (let i = 0; i < 8; i++) { await scheduler.request(spec); await clock.advance(80); }
    assert.equal(times.length, 1); await clock.advance(360); assert.equal(times[1], 2900); scheduler.dispose();
});

test('closed collections lose queued work without deleting source or starting a job', async () => {
    const clock = new FakeClock(); let count = 0;
    const scheduler = new Scheduler(options, async () => { count++; return report; }, () => {}, () => {}, clock);
    await scheduler.request(spec); scheduler.forget(spec.collection); await clock.advance(1000);
    assert.equal(count, 0); assert.equal(scheduler.states().length, 0); scheduler.dispose();
});

test('repeated Index Now clicks share an in-flight pass while edits queue another pass', async () => {
    const clock = new FakeClock(); let calls = 0; let release!: (value: IndexReport) => void;
    const scheduler = new Scheduler(options, async () => { calls++; return new Promise<IndexReport>(resolve => { release = resolve; }); }, () => {}, () => {}, clock);
    const first = scheduler.request(spec, true); await clock.advance(0);
    const clicks = Array.from({ length: 50 }, () => scheduler.request(spec, true));
    release(report); await flush(); await Promise.all([first, ...clicks]); await clock.advance(1000);
    assert.equal(calls, 1);
    const next = scheduler.request(spec, true); await clock.advance(0);
    await scheduler.request(spec); const changedClick = scheduler.request(spec, true);
    release(report); await flush(); await next; await clock.advance(0);
    assert.equal(calls, 3); release(report); await flush(); await changedClick; scheduler.dispose();
});

test('a full rebuild requested during an incremental pass remains a distinct pass', async () => {
    const clock = new FakeClock(); const modes: boolean[] = []; let release!: (value: IndexReport) => void;
    const scheduler = new Scheduler(options, async (_spec, full) => { modes.push(full); return new Promise<IndexReport>(resolve => { release = resolve; }); }, () => {}, () => {}, clock);
    const first = scheduler.request(spec, true); await clock.advance(0);
    const full = scheduler.request(spec, true, true); release(report); await flush(); await first;
    await clock.advance(0); assert.deepEqual(modes, [false, true]); release(report); await flush(); await full; scheduler.dispose();
});
