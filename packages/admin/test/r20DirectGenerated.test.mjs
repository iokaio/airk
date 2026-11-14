// SPDX-License-Identifier: Apache-2.0
// Direct keyed mutations must retain their original unresolved intent, including body loss.
import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/activityRequests.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm' });
const { createJobRequests } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const custodyBundle = await build({ entryPoints: [fileURLToPath(new URL('../src/generatedActions.ts', import.meta.url))], bundle: true, write: false, platform: 'node', format: 'esm' });
const { generatedAction } = await import(`data:text/javascript;base64,${Buffer.from(custodyBundle.outputFiles[0].text).toString('base64')}`);
const id = '1'.repeat(64), replacement = '2'.repeat(64);
const unknown = () => ({ kind: 'transport', status: 200, message: 'Body read rejected; no native outcome received.' });
const ok = job => ({ kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation: 'jobs.get', result: { status: 'job', job } } });
function owner(context, action) {
  let record, selected;
  const requests = createJobRequests({ currentContext: () => context, runJob: async () => ok({ id, state: 'failed', kind: 'index' }),
    runJobAction: action, setJobId: value => { selected = value; }, setJobRecord: value => { record = value; } });
  return { requests, state: () => ({ record, selected }) };
}

test('R20 direct job retry cannot generate a second key after a body-loss unknown', async () => {
  let calls = 0;
  const held = owner({}, async () => { calls++; return unknown(); });
  await held.requests.openJob(id); await held.requests.jobChange(id, 'retry'); await held.requests.jobChange(id, 'retry');
  assert.equal(calls, 1);
});

test('R20 direct job retry admits one in-flight mutation synchronously', async () => {
  let calls = 0, release;
  const pending = new Promise(resolve => { release = resolve; });
  const held = owner({}, async () => { calls++; return pending; });
  await held.requests.openJob(id);
  const first = held.requests.jobChange(id, 'retry'), second = held.requests.jobChange(id, 'retry');
  const observed = calls; release(ok({ id: replacement, state: 'pending', kind: 'index' }));
  await Promise.all([first, second]); assert.equal(observed, 1);
});

test('R20 direct job retry unknown survives owner disposal and same-session navigation', async () => {
  const session = {}; let calls = 0;
  const action = async () => { calls++; return unknown(); };
  const first = owner(session, action); await first.requests.openJob(id); await first.requests.jobChange(id, 'retry'); first.requests.dispose();
  const next = owner(session, action); await next.requests.openJob(id); await next.requests.jobChange(id, 'retry');
  assert.equal(calls, 1);
});

test('R20 direct job retry custody isolates exact session objects', async () => {
  let calls = 0;
  const action = async () => { calls++; return unknown(); };
  const first = owner({}, action); await first.requests.openJob(id); await first.requests.jobChange(id, 'retry'); first.requests.dispose();
  const next = owner({}, action); await next.requests.openJob(id); await next.requests.jobChange(id, 'retry');
  assert.equal(calls, 2);
});

test('R20 direct retry original pending reply remains visible after same-session owner remount', async () => {
  const session = {}; let release, calls = 0, changes = 0;
  const pending = new Promise(resolve => { release = resolve; });
  const action = async () => { calls++; return pending; };
  const first = owner(session, action); await first.requests.openJob(id); const waiting = first.requests.jobChange(id, 'retry'); first.requests.dispose();
  const next = createJobRequests({ currentContext: () => session, changed: () => { changes++; }, runJob: async () => ok({ id, state: 'failed' }),
    runJobAction: action, setJobId() {}, setJobRecord() {} });
  await next.openJob(id); await next.jobChange(id, 'retry'); assert.equal(calls, 1);
  const reply = ok({ id: replacement, state: 'pending', future: { retained: [true, null] } });
  release(reply); await waiting;
  assert.deepEqual(next.snapshot().retry.result, reply); assert.equal(next.snapshot().retry.pending, false); assert.ok(changes > 0);
});

test('R20 direct action custody binds immutable original method and ignores foreign settlement', () => {
  const held = generatedAction({}, 'service.lifecycle');
  const method = { lifecycle: { action: 'stop', target: { instance: 'original' } } };
  const intent = held.begin('service.stop', method);
  method.lifecycle.action = 'restart'; method.lifecycle.target.instance = 'edited';
  assert.deepEqual(intent.method, { lifecycle: { action: 'stop', target: { instance: 'original' } } });
  assert.equal(Object.isFrozen(intent.method.lifecycle.target), true);
  held.settle({ ...intent }, ok({ id }), 1); assert.equal(held.snapshot().pending, true);
  held.settle(intent, unknown(), 2); assert.equal(held.begin('service.restart', 'restart'), null);
});

test('R20 direct action custody preserves 32 outstanding slots and reclaims a known settled unmounted slot', () => {
  const session = {}, owners = [], intents = [];
  for (let i = 0; i < 32; i++) {
    const held = generatedAction(session, `retry:${i}`), intent = held.begin('jobs.retry', { retry: { id: i.toString(16).padStart(64, '0') } });
    assert.ok(intent); held.dispose(); owners.push(held); intents.push(intent);
  }
  const extra = generatedAction(session, 'retry:33'); assert.equal(extra.begin('jobs.retry', { retry: { id } }), null);
  assert.equal(extra.snapshot().capacity, true);
  owners[0].settle(intents[0], ok({ id: replacement }), 5);
  assert.ok(extra.begin('jobs.retry', { retry: { id } }));
  const original = generatedAction(session, 'retry:1'); assert.equal(original.snapshot().pending, true);
  assert.deepEqual(original.snapshot().intent.method, { retry: { id: '1'.padStart(64, '0') } });
});
