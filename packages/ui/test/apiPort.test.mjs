// SPDX-License-Identifier: Apache-2.0
// Actual common API/session facade; native host transport is injected, not impersonated.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ stdin: { contents: `export * from './packages/admin/src/api'; export * from './packages/admin/src/session'; export * from './packages/admin/src/generatedActions'; export * from './packages/ui/src/nativeSessionLifetime';`,
  resolveDir: fileURLToPath(new URL('../../../', import.meta.url)) }, bundle: true, write: false, platform: 'node', format: 'esm' });
let instance = 0;
const fresh = () => import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text + `\n// instance ${instance++}`).toString('base64')}`);
const fixture = JSON.parse(await readFile(new URL('../../../fixtures/split/ui_pilot/native_reads.json', import.meta.url), 'utf8'));
const native = fixture.cases.find(entry => entry.name === 'usage_unknown_fields');
const reply = { kind: 'ok', status: native.i3.status, envelope: native.i3.body };
const port = change => ({ kind: 'native', serviceInstance: 'a'.repeat(32), isCurrent: () => true,
  call: async () => reply, close: async () => {}, ...change });

test('actual API native port retains full I3 outcome and never reads browser credentials or launch data', async () => {
  const actual = await fresh(), saved = { window: globalThis.window, fetch: globalThis.fetch }; let calls = 0;
  globalThis.window = new Proxy({}, { get() { throw Error('Native UI touched browser auth/storage'); } });
  globalThis.fetch = () => { throw Error('Native UI attempted HTTP fetch'); };
  try {
    const lifetime = actual.createNativeSessionLifetime(port({ call: async (operation, body) => {
      calls++; assert.equal(operation, 'usage.document'); assert.deepEqual(body, native.request.admin_body); return reply;
    } }));
    assert.equal(actual.adoptNativeSession(lifetime), lifetime.session);
    assert.equal(await actual.call('usage.document', native.request.admin_body), reply); assert.equal(calls, 1);
    assert.equal(actual.takeLaunchCode(), null); assert.equal(actual.takeInitialRoute(), null);
    assert.equal(actual.restoreSession(), lifetime.session); assert.equal((await actual.exchange('not-a-native-secret')).kind, 'failed');
    assert.deepEqual(actual.currentSession(), { kind: 'native', serviceInstance: 'a'.repeat(32) });
  } finally { globalThis.window = saved.window; globalThis.fetch = saved.fetch; }
});
test('actual API native logout closes captured owner once without browser logout or token storage', async () => {
  const actual = await fresh(), saved = { window: globalThis.window, fetch: globalThis.fetch }; let closed = 0;
  globalThis.window = new Proxy({}, { get() { throw Error('Native logout touched browser storage'); } });
  globalThis.fetch = () => { throw Error('Native logout attempted HTTP'); };
  try {
    const lifetime = actual.createNativeSessionLifetime(port({ close: async () => { closed++; } }));
    actual.adoptNativeSession(lifetime); await actual.logout(); await actual.logout();
    assert.equal(closed, 1); assert.equal(actual.currentSession(), null); assert.equal(actual.restoreSession(), null);
    assert.deepEqual(await actual.call('service.health', 'health'), { kind: 'no_session' });
  } finally { globalThis.window = saved.window; globalThis.fetch = saved.fetch; }
});
test('actual API old native401 preserves replacement session and original received status', async () => {
  const actual = await fresh(); let release;
  const pending = new Promise(resolve => { release = resolve; });
  const first = actual.createNativeSessionLifetime(port({ call: async () => pending }));
  actual.adoptNativeSession(first); const waiting = actual.call('service.health', 'health');
  const replacement = actual.createNativeSessionLifetime(port({ serviceInstance: 'b'.repeat(32) }));
  actual.adoptNativeSession(replacement);
  release({ kind: 'transport', status: 401, message: 'Body unavailable' });
  assert.equal((await waiting).status, 401); assert.equal(actual.currentSession(), replacement.session);
});
test('actual API preserves independent job retry custody through private UI scopes without browser storage', async () => {
  const actual = await fresh(), savedWindow = globalThis.window, calls = [];
  globalThis.window = new Proxy({}, { get() { throw Error('Native custody touched browser storage'); } });
  try {
    const lifetime = actual.createNativeSessionLifetime(port({ call: async (operation, body) => {
      calls.push({ operation, body });
      if (body.prepared) return { kind: 'transport', status: 200, message: 'Synthetic original reply unavailable' };
      return { kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation,
        result: { preparation: body.retry.id.slice(0, 32), receipt_operation: null } } };
    } })); actual.adoptNativeSession(lifetime);
    const a = actual.generatedAction(lifetime.session, 'job-A'), b = actual.generatedAction(lifetime.session, 'job-B');
    const first = a.begin('jobs.retry', { retry: { id: '1'.repeat(64) } });
    a.settle(first, await actual.call(first.operation, first.method), 1);
    assert.equal(a.begin('jobs.retry', { retry: { id: '3'.repeat(64) } }), null);
    const second = b.begin('jobs.retry', { retry: { id: '2'.repeat(64) } });
    b.settle(second, await actual.call(second.operation, second.method), 2);
    assert.deepEqual(calls.map(entry => entry.body), [{ retry: { id: '1'.repeat(64) } }, { prepared: '1'.repeat(32) },
      { retry: { id: '2'.repeat(64) } }, { prepared: '2'.repeat(32) }]);
    assert.equal(a.snapshot().unknown, true); assert.equal(b.snapshot().unknown, true); a.dispose(); b.dispose();
  } finally { globalThis.window = savedWindow; }
});
