// SPDX-License-Identifier: Apache-2.0
// Local adapter mechanics with synthetic opaque payloads, not native semantic proof.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../admin/package.json', import.meta.url))('esbuild');
const source = new URL('../src/', import.meta.url);
const bundle = await build({
  stdin: { contents: `export * from './nativePilot'; export * from './browserPilot'; export * from './pilotPresentation';`,
    resolveDir: fileURLToPath(source), sourcefile: 'pilot-adapter-observation.ts' },
  bundle: true, write: false, format: 'esm', platform: 'node', target: 'node24',
  plugins: [{ name: 'private-session-observer', setup(plugin) {
    plugin.onResolve({ filter: /(?:^|\/)session$/ }, () => ({ path: 'session', namespace: 'pilot-fixture' }));
    plugin.onLoad({ filter: /.*/, namespace: 'pilot-fixture' }, () => ({ contents: `
      export function currentSession(){return globalThis.__pilotSession;}
      export function dropSession(){globalThis.__pilotSession=null;}
    ` }));
  } }],
});
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const binding = '11'.repeat(32);
const request = (read, id = 'read_1', epoch = 1) => ({ v: 1, type: 'pilot.read', requestId: id, epoch, read });
const usage = () => request({ kind: 'usage-json', binding, days: 7 });
const help = () => request({ kind: 'help-topic', topic: 'usage', startLine: 31 });
const ready = host => ({ host, epoch: 1, ready: true, trusted: true, nativeEnabled: true, usageBinding: binding });
function opaqueNative() { return { status: 'invoked', class: 'success', exit_code: 0,
  stdout: '{"synthetic":"opaque transport fixture","future":{"all":[1,true,null]}}\r\n', stderr: '' }; }

test('pilot native Usage uses fixed argv and retains every invocation channel', async () => {
  const native = opaqueNative(), calls = [], session = {};
  const read = actual.createNativePilot({ context: () => ready('vscode'), session: () => session,
    async invoke(argv, options) { calls.push({ argv, options }); return native; } });
  const observation = await read(usage());
  assert.deepEqual(calls, [{ argv: ['usage', 'report', '--days', '7', '--format', 'json', '--service-project', binding, '--output', 'json'], options: { deadlineMs: 60000 } }]);
  assert.equal(observation.result.outcome, native);
  assert.equal(observation.applicability, 'current');
});
test('pilot native metadata Help uses no project or window scope arguments', async () => {
  const calls = [], context = ready('vscode'); delete context.usageBinding;
  const read = actual.createNativePilot({ context: () => context, session: () => context,
    async invoke(argv) { calls.push(argv); return opaqueNative(); } });
  await read(help());
  assert.deepEqual(calls, [['tools', 'call', 'airk_help', '--args', '{"topic":"usage","start_line":31,"max_bytes":32768}', '--output', 'json']]);
});
test('pilot native refuses malformed/untrusted requests before invoking', async () => {
  const context = { ...ready('vscode'), trusted: false };
  const read = actual.createNativePilot({ context: () => context, session: () => context,
    async invoke() { assert.fail('refused reads never invoke'); } });
  assert.equal(await read({ ...usage(), argv: ['service', 'stop'] }), null);
  assert.equal((await read(usage())).result.code, 'pilot_untrusted');
});
test('pilot native context change retains full late evidence as superseded', async () => {
  const context = ready('vscode'), session = {}, native = opaqueNative(); let release;
  const wait = new Promise(resolve => { release = resolve; });
  const read = actual.createNativePilot({ context: () => context, session: () => session,
    async invoke() { await wait; return native; } });
  const pending = read(usage()); context.epoch++; release();
  const observation = await pending;
  assert.equal(observation.applicability, 'superseded');
  assert.equal(observation.result.outcome, native);
  assert.equal(observation.request.epoch, 1);
});
test('pilot native older same-context request cannot replace its successor', async () => {
  const context = ready('vscode'); let release, count = 0;
  const wait = new Promise(resolve => { release = resolve; });
  const read = actual.createNativePilot({ context: () => context, session: () => context,
    async invoke() { if (++count === 1) await wait; return opaqueNative(); } });
  const older = read(usage()); const newer = await read(request({ kind: 'usage-json', binding, days: 14 }, 'read_2'));
  release(); assert.equal((await older).applicability, 'superseded');
  assert.equal(newer.applicability, 'current');
});
test('pilot native preserves a returned failure and never retries', async () => {
  const context = ready('vscode'); let calls = 0;
  const failure = { ...opaqueNative(), class: 'refused', exit_code: 1, stderr: 'native stderr retained\r\n',
    error: { native: { future: ['whole refusal'] } } };
  const read = actual.createNativePilot({ context: () => context, session: () => context,
    async invoke() { calls++; return failure; } });
  assert.equal((await read(usage())).result.outcome, failure);
  assert.equal(calls, 1);
});
test('pilot native cancellation before dispatch performs no work', async () => {
  const context = ready('vscode'), controller = new AbortController(); controller.abort();
  const read = actual.createNativePilot({ context: () => context, session: () => context,
    async invoke() { assert.fail('canceled read invoked'); } });
  const result = await read(usage(), controller.signal);
  assert.equal(result.result.kind, 'transport');
  assert.match(result.result.message, /canceled before dispatch/);
});

for (const kind of ['usage', 'help']) {
  test(`pilot browser ${kind} delegates to the existing canonical I3 request body`, async () => {
    const fetch = globalThis.fetch; globalThis.__pilotSession = { csrf: 'fixture-only' };
    try {
      const calls = [], context = ready('browser');
      const envelope = { schema: 'airk-admin/1', ok: true, operation: kind === 'usage' ? 'usage.document' : 'search.call',
        result: { synthetic: 'opaque native fixture', future: { retained: [1, true, null] } } };
      globalThis.fetch = async (url, init) => { calls.push({ url, body: JSON.parse(init.body) }); return { status: 200, text: async () => JSON.stringify(envelope) }; };
      const read = actual.createBrowserPilot(() => context);
      const observation = await read(kind === 'usage' ? usage() : help());
      assert.deepEqual(calls, [{ url: `/admin/api/v1/call/${kind === 'usage' ? 'usage.document' : 'search.call'}`, body: kind === 'usage'
        ? { usage_document: { project: binding, read: { report: { days: 7, format: 'json' } } } }
        : { tools_call: { tool: 'airk_help', arguments: { topic: 'usage', start_line: 31, max_bytes: 32768 }, selection: 'unselected', response_mode: 'terse', usage_client: 'unknown' } } }]);
      assert.deepEqual(observation.result, { kind: 'i3', status: 200, envelope });
      assert.equal(observation.applicability, 'current');
    } finally { globalThis.fetch = fetch; delete globalThis.__pilotSession; }
  });
}
test('pilot browser body failure settles with observed status and no retry', async () => {
  const fetch = globalThis.fetch; globalThis.__pilotSession = { csrf: 'fixture-only' };
  try {
    const context = ready('browser'); let calls = 0;
    globalThis.fetch = async () => { calls++; return { status: 502, text: async () => { throw new Error('private diagnostic'); } }; };
    const observation = await actual.createBrowserPilot(() => context)(usage());
    assert.equal(observation.result.kind, 'transport'); assert.equal(observation.result.status, 502);
    assert.doesNotMatch(observation.result.message, /private diagnostic/); assert.equal(calls, 1);
  } finally { globalThis.fetch = fetch; delete globalThis.__pilotSession; }
});
test('pilot browser session change retains received evidence as superseded', async () => {
  const fetch = globalThis.fetch; globalThis.__pilotSession = { csrf: 'fixture-only' }; let release;
  try {
    const wait = new Promise(resolve => { release = resolve; });
    const envelope = { schema: 'airk-admin/1', ok: true, operation: 'usage.document', result: { future: 'retained' } };
    globalThis.fetch = async () => { await wait; return { status: 200, text: async () => JSON.stringify(envelope) }; };
    const pending = actual.createBrowserPilot(() => ready('browser'))(usage());
    globalThis.__pilotSession = { csrf: 'replacement-only' }; release();
    const observation = await pending;
    assert.equal(observation.applicability, 'superseded'); assert.deepEqual(observation.result.envelope, envelope);
  } finally { release?.(); globalThis.fetch = fetch; delete globalThis.__pilotSession; }
});
test('pilot shared presentation uses only text/fixed tags and retains complete opaque output', () => {
  const outcome = opaqueNative(), observation = { request: usage(), result: { kind: 'i2', outcome }, applicability: 'superseded' };
  const tree = actual.pilotPresentation(observation, '<img src=x onerror=never_run>');
  const rendered = actual.renderPilotNode((tag, children) => ({ tag, children }), tree);
  assert.deepEqual(rendered, tree);
  const text = JSON.stringify(tree);
  assert.match(text, /earlier context/); assert.match(text, /future/); assert.match(text, /onerror=never_run/);
  assert.deepEqual(JSON.parse(JSON.stringify(observation.result)), observation.result, 'the native payload is unchanged');
  assert.doesNotMatch(text, /dangerouslySetInnerHTML|innerHTML/);
});
