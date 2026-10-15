// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { HistoryControls, HistoryPlan, historyControl, historySummary, parsePurgeRequest } from '../history';

const revision = `airk-r1:${'1'.repeat(32)}:7:${'2'.repeat(64)}`;
const digest = 'a'.repeat(64);
const plan: HistoryPlan = { complete: false, projected_history_bytes: null, byte_limit: 4096, over_target_protected: true, retained: [{ protected: ['reader'] }] };
function fixture() {
    const calls: string[][] = [], writes: string[][] = [], shown: { title: string; value: any }[] = [], events: string[] = [];
    let confirm = false, choice: string | undefined = 'on', input: string | undefined = '--original checkpoint';
    let verify = true, failWrite = false, previewDigest = digest;
    const ui: HistoryControls = {
        collection: 'Original (isolated)',
        async read<T>(args: string[]): Promise<T> {
            calls.push(args); events.push(`read:${args[0]}:${args[1]}`);
            let value: unknown;
            if (args[0] === 'checkpoint') { value = [{ label: '--original checkpoint', publication: { token: revision }, admitted_dependency_bytes: 123 }]; }
            else if (args[0] === 'retention') { value = { protected_bytes: null, pending_intents: 1 }; }
            else if (args[1] === 'show') { value = { policy: { version: 1, enabled: false, recent: 2, hourly_hours: 8, max_packages: 5, max_bytes: 4096, quota_basis_points: 100 }, sequence: 1, digest, explicit: true }; }
            else if (args[1] === 'preview') { value = plan; }
            else if (args[1] === 'available') { value = { revisions: verify ? [revision] : [], complete: true, representation_verification: args.includes('--revision') ? 'exact representation verified' : 'receipt identities only' }; }
            else if (args[1] === 'purge') { value = { digest: previewDigest, targets: [{ revision, protected: ['reader'] }], clear_lineage: false }; }
            else { throw new Error(`unexpected read ${args}`); }
            return value as T;
        },
        async write<T>(args: string[]): Promise<T> {
            writes.push(args); events.push('write');
            if (failWrite) { throw new Error('stale plan or unavailable revision'); }
            return { pending: true, error: 'original bounded cleanup error', protected: [revision] } as T;
        },
        async show(title, value) { shown.push({ title, value }); events.push('show'); },
        async confirm() { events.push('confirm'); return confirm; },
        async pick() { return choice; },
        async input() { return input; }
    };
    return { ui, calls, writes, shown, events, allow: () => { confirm = true; }, choose: (value: string | undefined) => { choice = value; }, input: (value: string | undefined) => { input = value; }, unavailable: () => { verify = false; }, fail: () => { failWrite = true; }, badDigest: () => { previewDigest = 'not-a-digest'; } };
}

test('history status preserves unknown bytes, partial/protected coverage and performs no writes', async () => {
    const f = fixture(); await historyControl(f.ui, 'show');
    assert.equal(f.writes.length, 0);
    assert.match(historySummary(plan), /unknown/); assert.match(historySummary(plan), /partial/); assert.match(historySummary(plan), /protected evidence exceeds target/);
    assert.equal(f.shown[0].value.retention.protected_bytes, null);
});

test('history policy cancellation has no mutation and saving preserves operator limits', async () => {
    const f = fixture(); await historyControl(f.ui, 'policy'); assert.equal(f.writes.length, 0);
    f.allow(); await historyControl(f.ui, 'policy');
    assert.deepEqual(JSON.parse(f.writes[0][3]), { version: 1, enabled: true, recent: 2, hourly_hours: 8, max_packages: 5, max_bytes: 4096, quota_basis_points: 100 });
    assert.equal(f.writes[0][1], 'set'); assert.equal(f.writes.length, 1);
    assert.ok(f.shown.some(item => item.value.explanation?.includes('current policy')));
    assert.equal(f.events[f.events.indexOf('write') - 1], 'confirm');
});

test('history maintenance previews before apply and exposes pending/error result', async () => {
    const f = fixture(); f.allow(); await historyControl(f.ui, 'run');
    assert.deepEqual(f.writes, [['history', 'run', '--apply']]);
    assert.ok(f.calls.some(call => call[1] === 'preview'));
    assert.equal(f.shown.at(-1)?.value.pending, true); assert.match(f.shown.at(-1)?.value.error, /cleanup error/);
});

test('checkpoint creation verifies committed evidence and treats labels as positional data', async () => {
    const f = fixture(); f.choose(revision); f.allow(); await historyControl(f.ui, 'create');
    assert.ok(f.calls.some(args => args.includes('--revision') && args.includes(revision)));
    assert.deepEqual(f.writes, [['checkpoint', 'create', '--revision', revision, '--', '--original checkpoint']]);
    const unavailable = fixture(); unavailable.choose(revision); unavailable.allow(); unavailable.unavailable();
    await assert.rejects(historyControl(unavailable.ui, 'create'), /unavailable/); assert.equal(unavailable.writes.length, 0);
});

test('checkpoint removal binds exact listed revision and cancellation leaves pin intact', async () => {
    const f = fixture(); f.choose('--original checkpoint'); await historyControl(f.ui, 'remove'); assert.equal(f.writes.length, 0);
    f.allow(); await historyControl(f.ui, 'remove');
    assert.deepEqual(f.writes, [['checkpoint', 'remove', '--revision', revision, '--', '--original checkpoint']]);
});

test('purge applies captured explicit request and preview digest, never retries a stale plan', async () => {
    const f = fixture(); f.input(JSON.stringify({ revisions: [revision], checkpoints: [{ label: 'original', expected_revision: revision }], clear_lineage: true }));
    await historyControl(f.ui, 'purge'); assert.equal(f.writes.length, 0);
    f.allow(); await historyControl(f.ui, 'purge');
    const preview = f.calls.filter(args => args[1] === 'purge').at(-1)!;
    assert.equal(f.writes[0][3], preview[3]); assert.deepEqual(f.writes[0].slice(4), ['--apply', '--expected-plan', digest]);
    assert.equal(JSON.parse(f.writes[0][3]).clear_lineage, true);
    f.fail(); await assert.rejects(historyControl(f.ui, 'purge'), /stale plan/); assert.equal(f.writes.length, 2);
    const invalid = fixture(); invalid.input('{"revisions":[]}'); invalid.allow(); invalid.badDigest();
    await assert.rejects(historyControl(invalid.ui, 'purge'), /valid plan digest/); assert.equal(invalid.writes.length, 0);
});

test('purge rejects hidden fields and excessive work without inferring checkpoint removal', () => {
    assert.deepEqual(parsePurgeRequest('{"revisions":[]}'), { revisions: [], checkpoints: [], clear_lineage: false });
    for (const value of [{ revisions: [], recursive: true }, { revisions: Array(129).fill(revision) }, { revisions: [], checkpoints: [{ label: 'missing revision' }] }, { revisions: [], clear_lineage: 'yes' }]) {
        assert.throws(() => parsePurgeRequest(JSON.stringify(value)));
    }
    assert.throws(() => parsePurgeRequest(' '.repeat(65537)));
});

test('availability preserves cursor and receipt-only interpretation across bounded pages', async () => {
    const f = fixture(); const calls: string[][] = []; let page = 0;
    f.ui.read = async <T>(args: string[]) => { calls.push(args); return { revisions: [revision], complete: ++page > 1, next_cursor: page === 1 ? 'opaque original cursor' : null, representation_verification: 'receipt identities only' } as T; };
    f.choose('next'); await historyControl(f.ui, 'available');
    assert.equal(calls.length, 2); assert.deepEqual(calls[1].slice(-2), ['--cursor', 'opaque original cursor']);
    assert.match(f.shown[0].value.explanation, /not historical search/); assert.equal(f.writes.length, 0);
});
