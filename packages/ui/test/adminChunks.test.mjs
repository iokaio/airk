// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { PassThrough } from 'node:stream';
import { EventEmitter } from 'node:events';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const built = await build({ stdin: { contents: `export * from './packages/vscode/src/airkctl/adminChunks'; export * from './packages/vscode/src/airkctl/session';`, resolveDir: fileURLToPath(new URL('../../../', import.meta.url)) }, bundle: true, write: false, platform: 'node', format: 'esm', packages: 'external' });
const actual = await import(`data:text/javascript;base64,${Buffer.from(built.outputFiles[0].text).toString('base64')}`);
const fixture = JSON.parse(await readFile(new URL('../../../fixtures/split/native_admin/chunks.json', import.meta.url), 'utf8'));
for (const row of fixture.cases) test(`native emitted chunk case: ${row.name}`, () => {
  const assembler = actual.createAdminChunks(); let complete;
  const run = () => { for (const chunk of row.chunks) complete = assembler.accept(chunk); if (!complete) assembler.finish(); };
  if (row.accepted) { run(); assert.deepEqual(JSON.parse(complete.body), row.payload); }
  else assert.throws(run, actual.AdminChunkError);
  if (Object.hasOwn(row, 'observed_status')) assert.equal(assembler.status, row.observed_status);
});
test('chunk first401 survives invalid UTF8 and changed later status', () => {
  for (const chunks of [[{ http_status: 401, offset: 0, total_bytes: 1, data: [255], done: true }],
    [{ http_status: 401, offset: 0, total_bytes: 2, data: [123], done: false }, { http_status: 200, offset: 1, total_bytes: 2, data: [125], done: true }]]) {
    const observed = [], owner = actual.createAdminChunks(status => observed.push(status));
    assert.throws(() => chunks.forEach(chunk => owner.accept(chunk)), error => error.observedStatus === 401); assert.deepEqual(observed, [401]);
  }
});

class Child extends EventEmitter {
  stdin = new PassThrough(); stdout = new PassThrough(); stderr = new PassThrough(); exitCode = null; signalCode = null; requests = [];
  reader = new actual.FrameReader();
  constructor(answer) { super(); this.stdin.on('data', data => { for (const frame of this.reader.push(data)) { const value = JSON.parse(frame.toString()); this.requests.push(value); answer(value, this); } }); }
  send(id, outcome) { const body = Buffer.from(JSON.stringify({ id, outcome })), prefix = Buffer.alloc(4); prefix.writeUInt32BE(body.length); this.stdout.write(Buffer.concat([prefix, body])); }
  event(event) { const body = Buffer.from(JSON.stringify({ event })), prefix = Buffer.alloc(4); prefix.writeUInt32BE(body.length); this.stdout.write(Buffer.concat([prefix, body])); }
  kill() { this.exitCode = 1; this.emit('exit'); return true; }
}
const image = { path: process.platform === 'win32' ? 'C:\\verified\\airkctl.exe' : '/verified/airkctl', sha256: 'a'.repeat(64), version: '1.9.0' };
function session(answer, options = {}, gate = () => {}) {
  let child;
  const owner = new actual.AirkctlSession({ clientVersion: image.version, ...options, launch: () => child = new Child((frame, me) => {
    if (frame.request.hello) me.send(frame.id, { status: 'hello', protocol: 2, version: image.version, executable_sha256: image.sha256, i2_contract: 1,
      capabilities: ['session/2', 'admin-management/1'], service_instance: 'b'.repeat(32) });
    else if (frame.request.control) me.send(frame.id, { status: 'control', response: { status: 'pong' } });
    else answer(frame, me);
  }) }, gate); owner.bind(image); return { owner, child: () => child };
}
test('actual Node session assembles native multibyte chunks with original target and preserves full unknown fields', async () => {
  const row = fixture.cases.find(item => item.name === 'multibyte_split'); let position = 0, target;
  const held = session((frame, child) => {
    if (frame.request.admin_call) { target = frame.id; assert.equal(frame.request.admin_call.operation, 'usage.document'); }
    else if (frame.request.admin_read) { assert.equal(frame.request.admin_read.target, target); assert.equal(frame.request.admin_read.offset, row.chunks[position].offset); }
    child.send(frame.id, { status: 'admin_chunk', chunk: row.chunks[position++] });
  });
  try { await held.owner.control('ping'); const reply = await held.owner.adminCall(held.owner.captureAdmin(), 'usage.document', { original: true });
    assert.deepEqual(JSON.parse(reply.body), row.payload); assert.equal(position, row.chunks.length); }
  finally { held.owner.dispose(); }
});
test('actual Node session abort cancels original initial target before first reply and never retries AdminCall', async () => {
  const controller = new AbortController(); let target;
  const held = session((frame, child) => { if (frame.request.admin_call) target = frame.id;
    else if (frame.request.admin_cancel) { assert.equal(frame.request.admin_cancel.target, target); child.send(frame.id, { status: 'cancelled' }); child.send(target, { status: 'cancelled' }); } });
  try { await held.owner.control('ping'); const waiting = held.owner.adminCall(held.owner.captureAdmin(), 'service.stop', {}, { signal: controller.signal });
    controller.abort(); await assert.rejects(waiting); assert.equal(held.child().requests.filter(frame => frame.request.admin_call).length, 1);
    assert.ok(held.child().requests.some(frame => frame.request.admin_cancel?.target === target)); }
  finally { held.owner.dispose(); }
});
test('actual Node session401 clears admission before broken body and does not reopen captured context', async () => {
  const statuses = [];
  const held = session((frame, child) => { if (frame.request.admin_call) child.send(frame.id, { status: 'admin_chunk', chunk: { http_status: 401, offset: 0, total_bytes: 1, data: [255], done: true } });
    else if (frame.request.admin_cancel) child.send(frame.id, { status: 'cancelled' });
    else if (frame.request.admin_close) child.send(frame.id, { status: 'admin_closed' }); });
  try { await held.owner.control('ping'); const identity = held.owner.captureAdmin();
    await assert.rejects(held.owner.adminCall(identity, 'service.health', {}, { onStatus: status => statuses.push(status) }), error => error.observedStatus === 401);
    assert.deepEqual(statuses, [401]); assert.equal(held.owner.adminCurrent(identity), false);
    assert.throws(() => held.owner.captureAdmin()); await held.owner.adminClose(identity);
    assert.throws(() => held.owner.captureAdmin(), 'even a confirmed cleanup cannot reopen an auth-ended child');
    await assert.rejects(held.owner.adminCall(identity, 'service.health', {})); assert.equal(held.child().requests.filter(frame => frame.request.admin_call).length, 1); }
  finally { held.owner.dispose(); }
});
test('actual Node close revokes synchronously and sends one native close', async () => {
  const held = session((frame, child) => { if (frame.request.admin_close) child.send(frame.id, { status: 'admin_closed' }); });
  try { await held.owner.control('ping'); const identity = held.owner.captureAdmin(), closing = held.owner.adminClose(identity);
    assert.equal(held.owner.adminCurrent(identity), false); await closing; await held.owner.adminClose(identity);
    assert.equal(held.child().requests.filter(frame => frame.request.admin_close).length, 1); }
  finally { held.owner.dispose(); }
});
test('known close admits a fresh identity; old callbacks cannot rebind; unknown close stays fenced', async () => {
  for (const confirmed of [true, false]) {
    const held = session((frame, child) => { if (frame.request.admin_close) child.send(frame.id, confirmed ? { status: 'admin_closed' }
      : { status: 'failed', code: 'admin_outcome_unknown', retryable: false }); });
    try { await held.owner.control('ping'); const identity = held.owner.captureAdmin(), closing = held.owner.adminClose(identity);
      assert.throws(() => held.owner.captureAdmin());
      if (confirmed) { await closing; const replacement = held.owner.captureAdmin(); assert.notEqual(replacement, identity);
        assert.equal(replacement.serviceInstance, identity.serviceInstance); assert.equal(held.owner.adminCurrent(identity), false); }
      else { await assert.rejects(closing); assert.throws(() => held.owner.captureAdmin()); }
      await assert.rejects(held.owner.adminCall(identity, 'service.stop', {})); }
    finally { held.owner.dispose(); }
  }
});
test('native typed session-ended-after200 revokes captured owner and preserves unknown effects', async () => {
  const statuses = [];
  const held = session((frame, child) => {
    if (frame.request.admin_call) child.send(frame.id, { status: 'admin_chunk', chunk: { http_status: 200, total_bytes: 2, offset: 0, data: [123], done: false } });
    else if (frame.request.admin_read) child.send(frame.id, { status: 'failed', code: 'admin_session_ended_outcome_unknown', retryable: false });
    else if (frame.request.admin_cancel) child.send(frame.id, { status: 'cancelled' });
  });
  try { await held.owner.control('ping'); const identity = held.owner.captureAdmin();
    await assert.rejects(held.owner.adminCall(identity, 'service.stop', {}, { onStatus: status => statuses.push(status) }),
      error => error.observedStatus === 401 && error.outcomeUnknown === true && error.retryable === false);
    assert.deepEqual(statuses, [200, 401]); assert.equal(held.owner.adminCurrent(identity), false); }
  finally { held.owner.dispose(); }
});
test('queued AdminCall sends remaining original deadline and busy never replays the call', async () => {
  let now = 0;
  const held = session((frame, child) => {
    if (frame.request.admin_call) { assert.equal(frame.request.admin_call.deadline_ms, 600);
      child.send(frame.id, { status: 'failed', code: 'session_busy', retryable: true }); }
    else if (frame.request.admin_cancel) child.send(frame.id, { status: 'cancelled' });
  }, { timers: { now: () => now, set: (callback, ms, background) => { const timer = setTimeout(callback, ms); if (background) timer.unref(); return timer; }, clear: clearTimeout } });
  const blockers = [];
  try { await held.owner.control('ping'); for (let i = 0; i < 8; i++) blockers.push(held.owner.invoke(['status']).catch(error => error));
    const waiting = held.owner.adminCall(held.owner.captureAdmin(), 'service.stop', {}, { deadlineMs: 1000 });
    assert.equal(held.child().requests.filter(frame => frame.request.admin_call).length, 0);
    now = 400; const first = held.child().requests.find(frame => frame.request.invoke);
    held.child().send(first.id, { status: 'invoked', class: 'success', exit_code: 0, stdout: '{}', stderr: '' });
    await assert.rejects(waiting); assert.equal(held.child().requests.filter(frame => frame.request.admin_call).length, 1);
  } finally { held.owner.dispose(); await Promise.all(blockers); }
});
test('private admission is rechecked at queued frame write and revocation never spawns another child', async () => {
  let allowed = true;
  const held = session((frame, child) => { if (frame.request.admin_cancel) child.send(frame.id, { status: 'cancelled' }); });
  const blockers = [];
  try { await held.owner.control('ping'); for (let i = 0; i < 8; i++) blockers.push(held.owner.invoke(['status']).catch(error => error));
    const waiting = held.owner.adminCall(held.owner.captureAdmin(), 'service.stop', {}, { admit: () => allowed });
    allowed = false;
    const first = held.child().requests.find(frame => frame.request.invoke);
    held.child().send(first.id, { status: 'invoked', class: 'success', exit_code: 0, stdout: '{}', stderr: '' });
    await assert.rejects(waiting, error => error.sent === false);
    assert.equal(held.child().requests.filter(frame => frame.request.admin_call).length, 0); assert.equal(held.owner.launches, 1);
  } finally { held.owner.dispose(); await Promise.all(blockers); }
});
test('captured close is allowed after native gate revocation without any process start', async () => {
  let enabled = true;
  const held = session((frame, child) => { if (frame.request.admin_close) child.send(frame.id, { status: 'admin_closed' }); }, {}, () => { if (!enabled) throw Error('native_disabled'); });
  try { await held.owner.control('ping'); const identity = held.owner.captureAdmin(); enabled = false;
    await held.owner.adminClose(identity); assert.equal(held.owner.launches, 1);
    assert.equal(held.child().requests.filter(frame => frame.request.admin_close).length, 1); }
  finally { held.owner.dispose(); }
});
test('queued captured close is discarded on child loss instead of starting a replacement', async () => {
  const held = session(() => {}), blockers = [];
  try { await held.owner.control('ping'); for (let i = 0; i < 8; i++) blockers.push(held.owner.invoke(['status']).catch(error => error));
    const closing = held.owner.adminClose(held.owner.captureAdmin()).catch(error => error);
    await Promise.resolve(); held.child().kill();
    assert.ok(await closing instanceof actual.SessionError); assert.equal(held.owner.launches, 1);
    assert.equal(held.child().requests.filter(frame => frame.request.admin_close).length, 0); }
  finally { held.owner.dispose(); await Promise.all(blockers); }
});
test('service restart resync ends original Admin instance even when the same I2 child remains', async () => {
  const held = session(() => {});
  try { await held.owner.control('ping'); const identity = held.owner.captureAdmin();
    held.child().event({ kind: 'resync', reason: 'service_restarted' });
    assert.equal(held.owner.adminCurrent(identity), false); assert.throws(() => held.owner.captureAdmin()); assert.equal(held.owner.launches, 1);
  } finally { held.owner.dispose(); }
});
