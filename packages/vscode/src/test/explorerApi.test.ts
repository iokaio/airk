// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import * as vm from 'node:vm';
import { test } from 'node:test';
import { buildSync } from 'esbuild';

function client() {
    const file = path.resolve(__dirname, '../../src/explorerWebview/api.ts'), events = new Map<string, (event: any) => void>(), timers = new Map<number, { fn(): void; delay: number }>(), sent: any[] = [];
    let next = 0, broken = false;
    const module = { exports: {} as any };
    const built = buildSync({ entryPoints: [file], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text;
    vm.runInNewContext(built, { module, exports: module.exports, require: createRequire(file),
        acquireVsCodeApi: () => ({ postMessage(message: any) { if (broken) { throw new Error('closed'); } sent.push(message); }, getState() {}, setState() {} }),
        window: { addEventListener(name: string, callback: (event: any) => void) { events.set(name, callback); } },
        setTimeout: (fn: () => void, delay: number) => { const id = ++next; timers.set(id, { fn, delay }); return id; }, clearTimeout: (id: number) => timers.delete(id) });
    return { api: module.exports, sent, timers, events, break: () => { broken = true; }, answer: (message: any) => events.get('message')!({ data: message }) };
}
test('lost reads release waiting state, explain recovery and discard late replies', async () => {
    const c = client(), pending = c.api.request('help.engine', { topic: 'indexing' });
    const timer = [...c.timers.values()][0]; assert.equal(timer.delay, 180000); timer.fn();
    await assert.rejects(pending, (error: any) => error.error.code === 'REQUEST_TIMEOUT' && error.message.includes('retry the read'));
    assert.equal(c.timers.size, 0);
    c.answer({ v: 1, type: 'result', requestId: c.sent[0].requestId, ok: true, data: 'late' });
    const next = c.api.request('recipes.list', {}); c.answer({ v: 1, type: 'result', requestId: c.sent[1].requestId, ok: true, data: [] });
    assert.deepEqual(await next, []); assert.equal(c.timers.size, 0);
});
test('lost action acknowledgments report uncertainty and never replay the action', async () => {
    const c = client(), pending = c.api.request('manage.action', { action: 'pause' });
    const timer = [...c.timers.values()][0]; assert.equal(timer.delay, 900000); timer.fn();
    await assert.rejects(pending, (error: any) => error.error.code === 'REQUEST_OUTCOME_UNKNOWN' && error.message.includes('before retrying'));
    assert.equal(c.sent.length, 1); assert.equal(c.timers.size, 0);
});
test('revision notifications reserve no pending slot; closed bridges and request overload fail promptly', async () => {
    const c = client(); for (let i = 0; i < 100; i++) { c.api.notify('suggestions.revisions', { studio: i, planner: 0 }); }
    assert.equal(c.timers.size, 0);
    const promises = Array.from({ length: 64 }, () => c.api.request('recipes.list', {}));
    await assert.rejects(c.api.request('recipes.list', {}), (error: any) => error.error.code === 'RATE_LIMITED');
    c.events.get('pagehide')!({});
    const results = await Promise.allSettled(promises); assert.ok(results.every(result => result.status === 'rejected')); assert.equal(c.timers.size, 0);
    c.break(); await assert.rejects(c.api.request('recipes.list', {}), (error: any) => error.error.code === 'HOST_UNAVAILABLE'); assert.equal(c.timers.size, 0);
});


test('new native and local writes keep outcome uncertainty after timeout or bridge closure without replay', async () => {
    const c = client();
    const actions: Array<[string, Record<string, unknown>]> = [
        ['indexing.setup.apply', { generation: 1 }], ['indexing.setup.resume', { id: 'a'.repeat(32), generation: 1 }],
        ['indexing.source.resolve', { generation: 1 }], ['indexing.policy.resume', { generation: 1 }],
        ['indexing.split.apply', { generation: 1 }], ['indexing.split.recover', { action: 'cancel', generation: 1 }],
        ['indexing.retirement.stopTracking', { generation: 1 }],
        ['indexing.grant.use', { generation: 1, choice: { grant_id: 'grt_' + 'a'.repeat(32), expected_grant_revision: 1, project_ids: ['prj_' + 'b'.repeat(32)] } }],
        ['indexing.recommendations.use', {}], ['indexing.recommendations.copy', {}],
        ['strategy.run.export', { runId: 'a'.repeat(32), digest: 'b'.repeat(64), format: 'json' }],
        ['indexing.apply', { generation: 1 }], ['indexing.export', { format: 'json' }],
        ['details.copy', { section: 'service' }], ['clipboard.write', { text: 'complete native observation' }],
    ];
    for (const [type, fields] of actions) {
        const before = c.sent.length, pending = c.api.request(type, fields);
        assert.equal(c.sent.length, before + 1, type);
        const posted = c.sent.at(-1); assert.equal(posted.type, type);
        for (const [key, value] of Object.entries(fields)) { assert.equal(JSON.stringify(posted[key]), JSON.stringify(value), `${type}.${key}`); }
        const timer = [...c.timers.values()][0]; assert.equal(timer.delay, 900000, type); timer.fn();
        await assert.rejects(pending, (error: any) => error.error.code === 'REQUEST_OUTCOME_UNKNOWN' && error.message.includes('before retrying'), type);
        c.answer({ v: 1, type: 'result', requestId: posted.requestId, ok: true, data: { late: true } });
        timer.fn(); assert.equal(c.sent.length, before + 1, `${type} must not replay`); assert.equal(c.timers.size, 0);
        const closed = c.api.request(type, fields); c.events.get('pagehide')!({});
        await assert.rejects(closed, (error: any) => error.error.code === 'REQUEST_OUTCOME_UNKNOWN', `${type} pagehide`);
        assert.equal(c.sent.length, before + 2, type); assert.equal(c.timers.size, 0);
    }
});

test('native draft and receipt reads retain read deadlines while actual pickers retain interactive deadlines', async () => {
    const c = client();
    const reads = [
        'indexing.recommendations', 'indexing.recommendations.dismiss', 'indexing.page', 'indexing.detail',
        'indexing.preview', 'indexing.create', 'indexing.reconcile', 'indexing.context.read',
        'indexing.context.preview', 'indexing.context.delete', 'indexing.grant.draft', 'indexing.grant.revoke',
        'indexing.setup.draft', 'indexing.source.draft', 'indexing.source.read', 'indexing.source.pending',
        'indexing.split.open', 'indexing.split.sourceRead', 'indexing.split.draft', 'indexing.split.read',
        'indexing.policy.read', 'indexing.retirement.read',
    ];
    for (const type of reads) {
        const before = c.sent.length, pending = c.api.request(type, {}), timer = [...c.timers.values()][0];
        assert.equal(timer.delay, 180000, type); timer.fn();
        await assert.rejects(pending, (error: any) => error.error.code === 'REQUEST_TIMEOUT' && error.message.includes('retry the read'), type);
        assert.equal(c.sent.length, before + 1, type); assert.equal(c.timers.size, 0);
    }
    for (const type of ['workspace.manageTrust', 'source.choose', 'source.open', 'help.choose', 'instructions.open', 'planner.openDraft', 'planner.import', 'strategy.import', 'recipes.import']) {
        const pending = c.api.request(type, {}), timer = [...c.timers.values()][0];
        assert.equal(timer.delay, 900000, type); timer.fn();
        await assert.rejects(pending, (error: any) => error.error.code === 'REQUEST_TIMEOUT', type);
        assert.equal(c.timers.size, 0);
    }
    const pending = c.api.request('indexing.split.read', {}); c.events.get('pagehide')!({});
    await assert.rejects(pending, (error: any) => error.error.code === 'HOST_UNAVAILABLE');
    assert.equal(c.timers.size, 0);
});
