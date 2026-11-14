// SPDX-License-Identifier: Apache-2.0
// Local C9 admission cases. Native method/body and authority admission stay native-owned.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../../vscode/src/adminUiContract.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm' });
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const request = (change = {}) => ({ v: 1, type: 'admin.call', requestId: 'w1', epoch: 3, operation: 'service.health', payload: { body: 'health' }, ...change });
for (const [name, operation, body] of [
  ['health', 'service.health', 'health'], ['resources', 'service.resources', 'resources'],
  ['catalog', 'catalog.page', { catalog_page: { request: { kind: 'indexes' } } }],
  ['reviewed apply', 'catalog.apply', { method: { catalog_apply: { request: { future: true } } }, review: 'a'.repeat(32), confirm_text: 'Exact target' }],
  ['retry', 'jobs.retry', { retry: { id: '1'.repeat(64) } }], ['stop', 'service.stop', { stop: {} }],
  ['schema descriptions', 'search.call', { tools_call: { tool: 'help', arguments: { schema: { properties: { caller: { type: 'string' }, request_key: { type: 'object' } } } } } }],
]) test(`C9 Admin accepts closed ${name} transport and retains complete body without native DTO guesses`, () => {
  const parsed = actual.parseAdminUiRequest(request({ operation, payload: { body } }));
  assert.ok(parsed); assert.deepEqual(parsed.payload.body, body); assert.equal(Object.isFrozen(parsed.payload), true);
});

const invalid = [
  ['version', { v: 2 }], ['request identity', { requestId: 'https://example.test' }], ['negative epoch', { epoch: -1 }], ['fraction epoch', { epoch: 1.5 }],
  ['arbitrary operation', { operation: 'shell.execute' }], ['raw control operation', { operation: 'control' }],
  ['argv', { argv: ['anything'] }], ['URL', { url: 'https://example.test' }], ['cookie', { cookie: 'forbidden' }],
  ['CSRF', { csrf: 'forbidden' }], ['caller', { caller: 'forbidden' }], ['key', { key: 'forbidden' }], ['authority', { ready: true }],
  ['missing payload body', { payload: {} }], ['extra payload key', { payload: { body: 'health', csrf: 'forbidden' } }],
  ['primitive payload', { payload: 'health' }], ['undefined body', { payload: { body: undefined } }],
  ['NaN body', { payload: { body: NaN } }], ['Date body', { payload: { body: new Date(0) } }],
];
for (const [name, change] of invalid) test(`C9 Admin refuses ${name} before a host call`, () => assert.equal(actual.parseAdminUiRequest(request(change)), null));

test('C9 Admin exact64KiB UTF8 budget preserves the entire valid body and refuses the next bytes', () => {
  assert.ok(actual.parseAdminUiRequest(request({ payload: { body: 'é'.repeat(32767) } })));
  assert.equal(actual.parseAdminUiRequest(request({ payload: { body: 'é'.repeat(32768) } })), null);
  assert.equal(actual.ADMIN_UI_BODY_BYTES, 65536); assert.equal(actual.ADMIN_UI_PAYLOAD_BYTES, 65545);
});
test('C9 Admin captures immutable original JSON and rejects cycles, accessors and array property loss', () => {
  const body = { future: { complete: [true, null, 'original'] } }, parsed = actual.parseAdminUiRequest(request({ payload: { body } }));
  body.future.complete[2] = 'edited'; assert.equal(parsed.payload.body.future.complete[2], 'original');
  assert.equal(Object.isFrozen(parsed.payload.body.future.complete), true);
  const cycle = {}; cycle.self = cycle; assert.equal(actual.captureAdminBody(cycle), null);
  let invoked = 0; const getter = { get field() { invoked++; return 1; } };
  assert.equal(actual.captureAdminBody(getter), null); assert.equal(invoked, 0);
  const array = [1]; array.future = 'must not disappear'; assert.equal(actual.captureAdminBody(array), null);
  assert.equal(actual.captureAdminBody(Array(1)), null);
});
const ready = { epoch: 3, ready: true, trusted: true, nativeEnabled: true, management: true };
for (const [name, change, expected] of [
  ['ready', {}, null], ['untrusted', { trusted: false }, 'admin_ui_untrusted'],
  ['native disabled', { nativeEnabled: false }, 'admin_ui_native_disabled'], ['disconnected', { ready: false }, 'admin_ui_not_ready'],
  ['capability absent', { management: false }, 'admin_ui_unsupported'], ['stale generation', { epoch: 4 }, 'admin_ui_context_changed'],
]) test(`C9 Admin private host admission ${name}`, () => assert.equal(actual.admitAdminUiRequest(actual.parseAdminUiRequest(request()), { ...ready, ...change }), expected));

test('C9 Admin Close is strict and may revoke captured ownership without starting native execution', () => {
  const close = { v: 1, type: 'admin.close', requestId: 'close_1', epoch: 3 };
  assert.ok(actual.parseAdminUiRequest(close)); assert.equal(actual.parseAdminUiRequest({ ...close, body: {} }), null);
  assert.equal(actual.admitAdminUiRequest(close, { epoch: 3, ready: false, trusted: false, nativeEnabled: false, management: false }), null);
  assert.equal(actual.admitAdminUiRequest(close, { ...ready, epoch: 4 }), 'admin_ui_context_changed');
});
