// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/nativeSessionLifetime.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm' });
const { createNativeSessionLifetime } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const fixture = JSON.parse(await readFile(new URL('../../../fixtures/split/ui_pilot/native_reads.json', import.meta.url), 'utf8'));
const raw = fixture.cases.find(entry => entry.name === 'usage_unknown_fields').i3;
const reply = { kind: 'ok', status: raw.status, envelope: raw.body };
const port = (change = {}) => ({ kind: 'native', serviceInstance: 'a'.repeat(32), isCurrent: () => true,
  call: async () => reply, close: async () => {}, ...change });

test('native UI lifetime contains no fake CSRF, credential, window lease or durable storage', () => {
  const lifetime = createNativeSessionLifetime(port());
  assert.deepEqual(lifetime.session, { kind: 'native', serviceInstance: 'a'.repeat(32) });
  assert.equal(Object.isFrozen(lifetime.session), true); assert.equal(lifetime.isOpen(), true);
  assert.notEqual(lifetime.session, createNativeSessionLifetime(port()).session);
  assert.throws(() => createNativeSessionLifetime(port({ serviceInstance: '' })), /invalid_native_ui_session/);
});
test('native UI lifetime preserves the entire actual I3 outcome including unknown fields', async () => {
  let calls = 0;
  const lifetime = createNativeSessionLifetime(port({ call: async (operation, body) => {
    calls++; assert.equal(operation, 'usage.document'); assert.deepEqual(body, { native_body: 'unchanged' }); return reply;
  } }));
  assert.equal(await lifetime.call('usage.document', { native_body: 'unchanged' }), reply); assert.equal(calls, 1);
});
test('native UI close stops admission synchronously and invokes its captured owner exactly once', async () => {
  let calls = 0, closed = 0;
  const lifetime = createNativeSessionLifetime(port({ call: async () => { calls++; return reply; }, close: async () => { closed++; } }));
  const first = lifetime.close(), second = lifetime.close(); assert.equal(first, second);
  assert.equal(lifetime.isOpen(), false); assert.deepEqual(await lifetime.call('service.stop', 'stop'), { kind: 'no_session' });
  await Promise.all([first, second]); assert.equal(closed, 1); assert.equal(calls, 0);
});
test('native UI late full reply remains historical evidence without reopening or replay', async () => {
  let release, calls = 0, valid = true, notifications = 0;
  const pending = new Promise(resolve => { release = resolve; });
  const lifetime = createNativeSessionLifetime(port({ isCurrent: () => valid, call: async () => { calls++; return pending; } }));
  lifetime.subscribe(() => { notifications++; });
  const waiting = lifetime.call('usage.document', {}); valid = false; assert.equal(lifetime.isOpen(), false);
  release(reply); assert.equal(await waiting, reply); assert.equal(lifetime.isOpen(), false); assert.equal(calls, 1); assert.equal(notifications, 1);
  assert.deepEqual(await lifetime.call('usage.document', {}), { kind: 'no_session' });
});
test('native UI observed401 ends only its own lifetime even when the envelope is broken', async () => {
  const first = createNativeSessionLifetime(port({ call: async () => ({ kind: 'transport', status: 401, message: 'Malformed body' }) }));
  const replacement = createNativeSessionLifetime(port());
  assert.equal((await first.call('service.health', 'health')).status, 401);
  assert.equal(first.isOpen(), false); assert.equal(replacement.isOpen(), true);
});
test('native UI lost host response settles unknown without exposing private exception text or replay', async () => {
  let calls = 0;
  const lifetime = createNativeSessionLifetime(port({ call: async () => { calls++; throw Error('private_auth_sentinel'); } }));
  const result = await lifetime.call('service.stop', 'stop');
  assert.equal(result.kind, 'transport'); assert.equal(result.status, null); assert.equal(calls, 1);
  assert.doesNotMatch(JSON.stringify(result), /private_auth_sentinel/);
});
