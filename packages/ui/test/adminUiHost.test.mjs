// SPDX-License-Identifier: Apache-2.0
// Actual C9 host control flow over an injected native session; not real I2 parity.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const built = await build({ entryPoints: [fileURLToPath(new URL('../../vscode/src/adminUiHost.ts', import.meta.url))], bundle: true, write: false, format: 'esm', platform: 'node', packages: 'external' });
const { AdminUiHost } = await import(`data:text/javascript;base64,${Buffer.from(built.outputFiles[0].text).toString('base64')}`);
const identity = { serviceInstance: 'a'.repeat(32) }, token = 'b'.repeat(32);
const context = { epoch: 4, ready: true, trusted: true, nativeEnabled: true, management: true };
const message = (type, rest = {}) => ({ v: 1, type, requestId: 'w1', epoch: 4, ...rest });
const call = (operation, body) => message('admin.call', { operation, payload: { body } });
const envelope = (operation, result) => ({ schema: 'airk-admin/1', ok: true, operation, result });
function fixture(send) {
  let open = true; const calls = []; const session = { hello: {}, captureAdmin: () => identity, adminCurrent: held => open && held === identity,
    control() { throw Error('unexpected spawn/control'); }, adminCall: async (held, operation, body, options) => { calls.push({ held, operation, body }); return send(operation, body, options); },
    adminClose: async held => { assert.equal(held, identity); open = false; } };
  const owner = new AdminUiHost(session); return { owner, calls, session };
}
test('C9 actual host preserves full envelope/body and refuses arbitrary operation before native dispatch', async () => {
  const full = envelope('service.health', { future_native: { text: 'é中', all: ['retained'] } });
  const held = fixture(async () => ({ httpStatus: 200, body: JSON.stringify(full) }));
  await held.owner.request(message('admin.session'), () => context);
  const outcome = await held.owner.request(call('service.health', 'health'), () => context);
  assert.deepEqual(JSON.parse(outcome.reply.body), full); assert.equal(outcome.current, true);
  await assert.rejects(held.owner.request(call('shell.execute', {}), () => context)); assert.equal(held.calls.length, 1);
});
test('C9 actual host retains original preparation before renderer return and READY recover cannot replace immutable body', async () => {
  const preparation = { preparation: token, receipt_operation: null }, body = { lifecycle: { action: 'stop' } };
  const held = fixture(async () => ({ httpStatus: 200, body: JSON.stringify(envelope('service.stop', preparation)) }));
  await held.owner.request(message('admin.session'), () => context);
  await held.owner.request(call('service.stop', body), () => context);
  await held.owner.request(call('service.stop', { recover: token }), () => context);
  const reopened = await held.owner.request(message('admin.session'), () => context);
  assert.equal(reopened.pending.length, 1); assert.deepEqual(reopened.pending[0].body, body);
  assert.deepEqual(reopened.pending[0].acknowledgment.envelope, envelope('service.stop', preparation));
});
test('C9 actual host requires current trust/native/epoch before dispatch, yet close revokes without native startup', async () => {
  const held = fixture(async () => { throw Error('unexpected dispatch'); });
  await held.owner.request(message('admin.session'), () => context);
  for (const state of [{ ...context, trusted: false }, { ...context, nativeEnabled: false }, { ...context, epoch: 5 }])
    await assert.rejects(held.owner.request(call('service.stop', {}), () => state));
  await held.owner.request(message('admin.close'), () => ({ ...context, trusted: false, nativeEnabled: false }));
  assert.equal(held.session.adminCurrent(identity), false); assert.equal(held.calls.length, 0);
});
test('C9 submission/recovery/acknowledgment require original host-retained preparation and matching operation', async () => {
  const held = fixture(async () => ({ httpStatus: 200, body: JSON.stringify(envelope('service.stop', { preparation: token, receipt_operation: null })) }));
  await held.owner.request(message('admin.session'), () => context);
  for (const body of [{ prepared: token }, { recover: token }, { acknowledged: token }])
    await assert.rejects(held.owner.request(call('service.stop', body), () => context));
  assert.equal(held.calls.length, 0);
  await held.owner.request(call('service.stop', { lifecycle: { action: 'stop' } }), () => context);
  await assert.rejects(held.owner.request(call('service.restart', { prepared: token }), () => context));
  assert.equal(held.calls.length, 1);
});
test('C9 observed401 is delivered before failed body and no fabricated I3 envelope appears', async () => {
  const observed = [], held = fixture(async (_operation, _body, options) => { options.onStatus(401); assert.deepEqual(observed, [401]); throw Error('body broken'); });
  await held.owner.request(message('admin.session'), () => context);
  const result = await held.owner.request(call('service.health', 'health'), () => context, status => observed.push(status));
  assert.equal(result.reply.httpStatus, 401); assert.equal(result.reply.transport, true); assert.equal('body' in result.reply, false);
});
test('C9 old401 after confirmed close cannot end a replacement context in the same view epoch', async () => {
  const replacement = { serviceInstance: identity.serviceInstance }; let current = identity, pendingOptions, fail;
  const pending = new Promise((_resolve, reject) => { fail = reject; });
  const session = { hello: {}, captureAdmin: () => current, adminCurrent: value => value === current,
    adminCall: async (_identity, _operation, _body, options) => { pendingOptions = options; return pending; },
    adminClose: async () => { current = replacement; } };
  const owner = new AdminUiHost(session), observed = [];
  await owner.request(message('admin.session'), () => context);
  const waiting = owner.request(call('service.health', 'health'), () => context, status => observed.push(status));
  await owner.request(message('admin.close'), () => context);
  await owner.request(message('admin.session'), () => context);
  pendingOptions.onStatus(401); fail(Error('old body rejected'));
  const old = await waiting;
  assert.deepEqual(observed, []); assert.equal(old.current, false); assert.equal(old.reply.httpStatus, 401);
  assert.equal(session.adminCurrent(replacement), true);
});
