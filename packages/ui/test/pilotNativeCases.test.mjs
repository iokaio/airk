// SPDX-License-Identifier: Apache-2.0
// Actual Rust-emitted parser/formatter/Admin-handler cases. No live transport claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../admin/package.json', import.meta.url))('esbuild');
const bundle = await build({ stdin: { contents: `export * from './nativePilot'; export * from './browserPilot'; export * from './pilotProjection';`,
  resolveDir: fileURLToPath(new URL('../src/', import.meta.url)), sourcefile: 'native-case-observation.ts' }, bundle: true, write: false,
  platform: 'node', format: 'esm', plugins: [{ name: 'session-owner-observation', setup(plugin) {
    plugin.onResolve({ filter: /(?:^|\/)session$/ }, () => ({ path: 'session', namespace: 'native-cases' }));
    plugin.onLoad({ filter: /.*/, namespace: 'native-cases' }, () => ({ contents: `export function currentSession(){return globalThis.__nativeCaseSession;}export function dropSession(){globalThis.__nativeCaseSession=null;}` }));
  } }],
});
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const cases = JSON.parse(await readFile(new URL('../../../fixtures/split/ui_pilot/native_reads.json', import.meta.url), 'utf8'));
const exits = JSON.parse(await readFile(new URL('../../../fixtures/split/i2/exit_codes.json', import.meta.url), 'utf8'));
assert.equal(cases.schema, 'airk.split.ui-pilot.reads/1');
assert.equal(cases.cases.length, 12);
const refused = new Set(['usage_days_31_refused', 'usage_native_failure', 'help_unknown_topic', 'help_bad_line']);
function request(row) {
  const value = row.request.admin_body;
  return { v: 1, type: 'pilot.read', requestId: row.name, epoch: 1, read: row.surface === 'usage'
    ? { kind: 'usage-json', binding: value.usage_document.project, days: value.usage_document.read.report.days }
    : { kind: 'help-topic', topic: value.tools_call.arguments.topic ?? '', startLine: value.tools_call.arguments.start_line } };
}
function invocation(row) {
  const classification = exits.codes.find(value => value.code === row.i2.exit_code);
  assert.ok(classification);
  let error;
  try { error = JSON.parse(row.i2.stderr.trim().split('\n').at(-1)); } catch { /* Native success has no error line. */ }
  return { status: 'invoked', ...row.i2, class: classification.class, ...(error ? { error } : {}) };
}
function argvValues(argv) {
  return argv.map((value, index) => index > 0 && argv[index - 1] === '--args' ? JSON.parse(value) : value);
}
for (const row of cases.cases) {
  test(`Rust native pilot ${row.name} uses equivalent inputs and whole outcomes in both adapters`, async () => {
    const input = request(row), session = {}, native = invocation(row), calls = [];
    const context = host => ({ host, epoch: 1, ready: true, trusted: true, nativeEnabled: true,
      ...(input.read.kind === 'usage-json' ? { usageBinding: input.read.binding } : {}) });
    const readNative = actual.createNativePilot({ context: () => context('vscode'), session: () => session,
      async invoke(argv) { calls.push(argv); return native; } });
    const i2 = await readNative(input);
    assert.equal(calls.length, 1);
    assert.deepEqual(argvValues(calls[0]), argvValues(row.request.cli_args), 'only the documented JSON argument property order is normalized');
    assert.equal(i2.result.outcome, native, 'all stdout/stderr/ErrorLine data remains intact');
    const fetch = globalThis.fetch; globalThis.__nativeCaseSession = { csrf: 'fixture-only' };
    let i3;
    try {
      const browserCalls = [];
      globalThis.fetch = async (url, init) => { browserCalls.push({ url, body: JSON.parse(init.body) });
        return { status: row.i3.status, text: async () => JSON.stringify(row.i3.body) }; };
      i3 = await actual.createBrowserPilot(() => context('browser'))(input);
      assert.deepEqual(browserCalls, [{ url: `/admin/api/v1/call/${row.request.admin_operation}`, body: row.request.admin_body }]);
      assert.deepEqual(i3.result, { kind: 'i3', status: row.i3.status, envelope: row.i3.body });
    } finally { globalThis.fetch = fetch; delete globalThis.__nativeCaseSession; }
    const left = actual.projectPilotObservation(i2), right = actual.projectPilotObservation(i3);
    if (refused.has(row.name)) {
      assert.equal(left.kind, 'failure'); assert.equal(right.kind, 'failure');
    } else {
      assert.equal(left.kind, row.surface); assert.equal(right.kind, row.surface);
      const expected = row.surface === 'usage' ? row.native_response.document : row.native_response.result;
      assert.deepEqual(left.payload, expected);
      assert.deepEqual(right.payload, expected);
      if (row.surface === 'help') {
        assert.equal(left.content, right.content);
        assert.deepEqual(left.article, right.article);
      }
    }
    assert.deepEqual(i3.result.envelope, row.i3.body, 'projection never mutates or strips original native fields');
    assert.equal(i2.result.outcome.stdout, row.i2.stdout);
    assert.equal(i2.result.outcome.stderr, row.i2.stderr);
  });
}
