// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const bundled = await build({ entryPoints: [fileURLToPath(new URL('../src/preparedMutations.ts', import.meta.url))], bundle: true, write: false, format: 'esm', platform: 'node' });
const { createPreparedMutations, preparationOf, preparedUnresolved, recoverPreparedResult, preparesMutation } = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);
const fixture = JSON.parse(await readFile(new URL('../../../fixtures/split/i3/mutation_preparation.json', import.meta.url)));
const operation = 'catalog.policy_resume', original = { catalog_policy_resume: { index_id: 'idx_' + 'a'.repeat(32), operation_id: 'b'.repeat(32) } };
const ok = result => ({ kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation, result } });
const ack = ok(fixture.preparations[0]), final = ok({ status: 'catalog_policy_resume', complete: { unknown_future_field: ['whole', 'outcome'] } });
const lost = { kind: 'transport', status: 200, message: 'body unavailable' };
const refused = (status = 400, errorClass = 'refused') => ({ kind: 'failed', status, envelope: { schema: 'airk-admin/1', ok: false, error: { code: 'test_refusal', class: errorClass, retryable: false } } });
function storage() { const values = new Map(); return { values, getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value), removeItem: key => values.delete(key) }; }
function host(script, extra = {}) { const calls = []; const owner = createPreparedMutations({ serviceInstance: 'a'.repeat(32), current: () => true, send: async (...args) => { calls.push(args); const next = script.shift(); return typeof next === 'function' ? next(...args) : next; }, ...extra }); return { owner, calls }; }

test('R17 exact six Admin effectful generated operations use prepare-first admission', () => {
  for (const id of ['jobs.retry', 'catalog.policy_resume', 'catalog.split_recover', 'recovery.plan_create', 'service.stop', 'service.restart']) assert.equal(preparesMutation(id), true);
  for (const id of ['requests.prepare', 'catalog.project_create', 'jobs.cancel', 'usage.document']) assert.equal(preparesMutation(id), false);
});
test('R17 full acknowledgment is retained synchronously before exact original submission; full final precedes ACK', async () => {
  const journal = storage();
  const held = host([ack, (_op, body) => { assert.deepEqual(body, fixture.submit); assert.equal(journal.values.size, 1); return final; },
    (_op, body) => { assert.deepEqual(body, fixture.acknowledge); assert.equal(journal.values.size, 0); return ok(body); }], { storage: journal });
  const result = await held.owner.run(operation, original);
  assert.equal(result, final); assert.equal(preparedUnresolved(result), false);
  assert.deepEqual(held.calls[0][1], original); assert.deepEqual(held.calls.map(call => call[0]), [operation, operation, operation]);
});
test('R17 unknown submit cannot generate another key; recovery uses only original handle and ACKs positive full result', async () => {
  const held = host([ack, lost, final, ok(fixture.acknowledge)]);
  const result = await held.owner.run(operation, original); assert.equal(preparedUnresolved(result), true);
  await held.owner.run(operation, { edited: true }); assert.equal(held.calls.length, 2);
  assert.equal(await recoverPreparedResult(result), final);
  assert.deepEqual(held.calls[2].slice(0, 2), [operation, fixture.recover]); assert.equal(preparedUnresolved(result), false);
});
test('R17 reload restores recover-only custody and immutable original input; READY recovery does not submit or ACK', async () => {
  const journal = storage(); const first = host([ack, lost], { storage: journal });
  await first.owner.run(operation, original);
  const next = host([ack], { storage: journal });
  assert.equal(next.owner.list()[0].restored, true); assert.deepEqual(next.owner.list()[0].body, original);
  await next.owner.run('service.restart', { lifecycle: { action: 'restart' } }); assert.equal(next.calls.length, 0);
  const ready = await next.owner.recover(fixture.preparations[0].preparation);
  assert.equal(preparationOf(ready).state, 'ready'); assert.equal(preparedUnresolved(ready), true);
  assert.deepEqual(next.calls.map(call => call.slice(0, 2)), [[operation, fixture.recover]]); assert.equal(journal.values.size, 1);
});
test('R17 storage write failure refuses dispatch after preparation acknowledgment', async () => {
  const held = host([ack], { storage: { getItem: () => null, setItem() { throw Error('quota'); }, removeItem() {} } });
  const result = await held.owner.run(operation, original);
  assert.equal(held.calls.length, 1); assert.equal(preparationOf(result).state, 'ready'); assert.equal(preparedUnresolved(result), true);
});
test('R17 inaccessible or malformed restore journal blocks generation instead of forgetting custody', async () => {
  for (const journal of [{ getItem() { throw Error('denied'); } }, { getItem: () => '{bad' }]) {
    const held = host([], { storage: journal }); await held.owner.run(operation, original); assert.equal(held.calls.length, 0);
  }
});
test('R17 pending admission is synchronous and altered caller input cannot mutate captured original', async () => {
  let release; const pending = new Promise(resolve => { release = resolve; });
  const body = structuredClone(original), held = host([() => pending, lost]);
  const first = held.owner.run(operation, body); body.catalog_policy_resume.index_id = 'edited';
  await held.owner.run(operation, original); assert.equal(held.calls.length, 1);
  release(ack); await first; assert.deepEqual(held.calls[0][1], original); assert.deepEqual(held.calls[1][1], fixture.submit);
});
test('R17 Internal after submit and unavailable uncached reply remain unknown and are never ACKed', async () => {
  for (const result of [refused(500, 'internal'), refused(503, 'unavailable'), lost]) {
    const held = host([ack, result]); assert.equal(await held.owner.run(operation, original), result);
    assert.equal(preparedUnresolved(result), true); assert.equal(held.calls.length, 2);
  }
});
test('R17 explicit Refused is retained before acknowledgment; ACK400 restores unresolved barrier', async () => {
  const result = refused(), journal = storage(), held = host([ack, result, refused()], { storage: journal });
  assert.equal(await held.owner.run(operation, original), result); assert.equal(preparedUnresolved(result), true);
  assert.equal(journal.values.size, 1); await held.owner.run(operation, original); assert.equal(held.calls.length, 3);
});
test('R17 ACK loss keeps complete known outcome and never retries mutation', async () => {
  const held = host([ack, final, lost, ok(fixture.acknowledge)]); assert.equal(await held.owner.run(operation, original), final);
  assert.equal(preparedUnresolved(final), false); assert.equal(held.calls.length, 3);
  assert.equal(await held.owner.acknowledge(fixture.preparations[0].preparation), final);
  assert.deepEqual(held.calls[3].slice(0, 2), [operation, fixture.acknowledge]);
  assert.deepEqual(held.owner.list(), []);
});
test('R17 changed native lifetime after acknowledgment never submits captured preparation', async () => {
  let current = true; const held = host([() => { current = false; return ack; }], { current: () => current });
  assert.equal(await held.owner.run(operation, original), ack); assert.equal(held.calls.length, 1);
  assert.equal(preparedUnresolved(ack), true);
});
test('R17 separate service instance does not inherit another tab lifetime journal', async () => {
  const journal = storage(); await host([ack, lost], { storage: journal }).owner.run(operation, original);
  const next = host([], { serviceInstance: 'b'.repeat(32), storage: journal }); assert.deepEqual(next.owner.list(), []);
});
test('R17 old acknowledgment and final replies cannot write or clear a replacement lifetime journal', async () => {
  for (const stage of ['prepare', 'submit']) {
    let current = true; const journal = storage(), replacement = 'replacement-cookie-public-journal';
    const change = result => () => { current = false; journal.values.set('airk-admin-preparations:' + 'a'.repeat(32), replacement); return result; };
    const held = host(stage === 'prepare' ? [change(ack)] : [ack, change(final)], { storage: journal, current: () => current });
    await held.owner.run(operation, original);
    assert.equal(journal.values.get('airk-admin-preparations:' + 'a'.repeat(32)), replacement);
    assert.equal(held.calls.length, stage === 'prepare' ? 1 : 2);
  }
});
test('R17 recovered known body cannot admit replacement during ACK; ACK400 keeps original unresolved', async () => {
  let release, observed;
  const waiting = new Promise(resolve => { release = resolve; }), started = new Promise(resolve => { observed = resolve; });
  const held = host([ack, lost, final, () => { observed(); return waiting; }]);
  const originalResult = await held.owner.run(operation, original);
  const recovery = held.owner.recover(fixture.preparations[0].preparation);
  await started;
  assert.equal(preparedUnresolved(originalResult), true);
  await held.owner.run(operation, { edited: true }); assert.equal(held.calls.length, 4);
  release(refused()); assert.equal(await recovery, final);
  assert.equal(preparedUnresolved(originalResult), true);
  await held.owner.run(operation, { edited: true }); assert.equal(held.calls.length, 4);
});
test('R17 independent surviving UI scopes retain separate unknown intents while same-scope edits stay blocked', async () => {
  const journal = storage(), scopeA = {}, scopeB = {}, second = ok({ preparation: '2'.repeat(32), receipt_operation: null });
  const held = host([ack, lost, second, lost], { storage: journal });
  await held.owner.run(operation, original, scopeA);
  await held.owner.run(operation, { edited: true }, scopeA); assert.equal(held.calls.length, 2);
  await held.owner.run(operation, { independent_index: true }, scopeB); assert.equal(held.calls.length, 4);
  await held.owner.run(operation, { raw_unscoped: true }); assert.equal(held.calls.length, 4, 'unscoped API remains conservative');
  assert.equal(held.owner.list().some(entry => 'scope' in entry), false, 'private admission identity is never a public result');
  assert.equal(JSON.parse([...journal.values.values()][0]).records.some(entry => 'scope' in entry), false, 'private admission identity is never persisted');
});
