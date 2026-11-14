// SPDX-License-Identifier: Apache-2.0
// Real extension adapter + framed airkctl session on an exclusively owned fixture.
// The VS Code host is injected; native replies and WindowScope logic are real.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const Module = require('node:module');
const { spawn } = require('node:child_process');
const { once, EventEmitter } = require('node:events');
const input = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const root = path.resolve(__dirname, '..');
const out = path.join(root, 'packages/vscode/out');
const { AirkctlSession } = require(path.join(out, 'airkctl/session'));
const { AirkctlToolSession, evidenceSelection } = require(path.join(out, 'airkctl/tools'));
const { WindowScope, openSelectionSpec } = require(path.join(out, 'airkctl/features/windowScope'));
const { setEvidenceTransport } = require(path.join(out, 'explorerEvidence'));
const { renderExport } = require(path.join(out, 'workbenchRender'));
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
assert.equal(digest(input.cli), input.cli_sha256, 'fixture must use its matched airkctl');
assert.equal(path.resolve(input.control), input.control, 'absolute isolated fixture control required');
const deliveries = [], children = [], changes = new EventEmitter();
const native = new AirkctlSession({ clientVersion: input.version, label: 'Workbench routing qualification',
  launch: image => {
    assert.equal(image.path, input.cli);
    const child = spawn(image.path, ['--service-runtime-dir', input.control, 'session', '--owner-watch'],
      { cwd: path.dirname(image.path), env: process.env, shell: false, windowsHide: true, stdio: 'pipe' });
    children.push(child); return child;
  },
});
const commands = { get closed() { return native.closed; }, invoke: async (argv, options) => {
  const result = await native.invoke(argv, options);
  deliveries.push({ argv: [...argv], result });
  changes.emit('delivery'); return result;
} };
const values = new Map(), store = { get: key => values.get(key), update: async (key, value) => {
  if (value === undefined) values.delete(key); else values.set(key, value);
} };
const base = { command: input.cli, installDir: input.runtime, root: input.project.root };
const scope = new WindowScope(native, () => native.hello?.capabilities, store, () => base);
native.onEvent(event => { if (event.kind === 'session_lost') scope.lost(); });
const spec = { command: input.cli, root: input.project.root, dataDir: input.project.data_root,
  collection: input.project.collection, name: 'Ordinary project', service: { installDir: input.runtime, project: input.project.binding } };
const bodyOf = envelope => {
  const text = envelope.content?.find(part => part.type === 'text')?.text;
  assert.equal(typeof text, 'string', JSON.stringify(envelope));
  return JSON.parse(text);
};
const nativeBody = delivery => bodyOf(JSON.parse(delivery.result.stdout));
let exactComparisons = 0;
setEvidenceTransport((launch, client) => {
  const adapter = new AirkctlToolSession(commands, () => evidenceSelection(launch, client, scope.active, scope.readRoute()),
    { windowReadScope: () => scope.retryScope(), windowReadFailed: code => scope.refused(code) });
  return { get closed() { return adapter.closed; }, readScope: () => adapter.readScope(), close: () => adapter.close(), request: async (...args) => {
    const before = deliveries.length;
    const result = await adapter.request(...args);
    const matching = deliveries.slice(before).filter(row => row.argv[2] === args[1].name);
    assert.equal(matching.length, 1, 'one adapter request has one complete native delivery');
    assert.deepEqual(result, JSON.parse(matching[0].result.stdout), 'adapter preserves every canonical native result field');
    exactComparisons++; return result;
  } };
});
class Emitter {
  constructor() { this.listeners = new Set(); this.event = listener => { this.listeners.add(listener); return { dispose: () => this.listeners.delete(listener) }; }; }
  fire(event) { for (const listener of this.listeners) listener(event); }
  dispose() { this.listeners.clear(); }
}
const vscode = { EventEmitter: Emitter, env: { clipboard: { writeText: async () => undefined } },
  workspace: { getConfiguration: () => ({ get: (_key, fallback) => fallback, inspect: () => ({ globalValue: true }) }),
    onDidChangeConfiguration: () => ({ dispose() {} }) }, window: {}, commands: { executeCommand: async () => undefined },
  Uri: { file: fsPath => ({ fsPath }), joinPath: (base, ...parts) => ({ fsPath: path.join(base.fsPath, ...parts) }) } };
const originalLoad = Module._load;
Module._load = function (name, ...args) { return name === 'vscode' ? vscode : originalLoad.call(this, name, ...args); };
let WorkbenchView;
try { ({ WorkbenchView } = require(path.join(out, 'workbenchView'))); } finally { Module._load = originalLoad; }
const host = { trusted: () => true, specs: () => [spec], activeProject: () => spec.service.project,
  onDidChangeSpecs: () => ({ dispose() {} }), version: input.version, windowScope: scope,
  readRoute: () => scope.readRoute(), openSelectionSpec: (selection, label) => openSelectionSpec(base, selection, label) };
const workbench = new WorkbenchView({ globalStorageUri: { fsPath: path.join(input.owner, 'workbench') },
  extensionUri: { fsPath: path.join(root, 'packages/vscode') } }, host);
workbench.sessions = { list: async () => [], save: async session => { session.revision++; return []; },
  findShortcut: async () => undefined, remove: async () => undefined };
const observations = [];
async function until(predicate, label) {
  const timeout = AbortSignal.timeout(60000);
  while (!predicate()) {
    await once(changes, 'delivery', { signal: timeout });
    await new Promise(resolve => setImmediate(resolve));
  }
  assert.ok(predicate(), label);
}
function parsedDelivery(tool, before) {
  const matches = deliveries.slice(before).filter(row => row.argv[2] === tool);
  assert.equal(matches.length, 1, `${tool} exact native invocation`);
  return matches[0];
}
async function runSlash(text, expectedSelection) {
  const before = deliveries.length;
  await workbench.run(text); await workbench.saveQueue;
  const turn = workbench.session.turns.at(-1);
  assert.ok(turn.envelope, JSON.stringify(turn)); assert.equal(turn.error, undefined, JSON.stringify(turn));
  assert.equal(turn.envelope.executed, true, JSON.stringify(turn.envelope));
  assert.equal(turn.envelope.status, 'complete', JSON.stringify(turn.envelope));
  assert.ok(Array.isArray(turn.envelope.steps) && turn.envelope.steps.length > 0);
  assert.ok(turn.envelope.steps.every(step => step.status === 'complete'), JSON.stringify(turn.envelope));
  const delivered = parsedDelivery('airk_parse', before);
  assert.deepEqual(turn.envelope, nativeBody(delivered), 'complete Workbench turn equals actual canonical parser result');
  if (expectedSelection === 'window') assert.ok(!delivered.argv.includes('--service-project') && !delivered.argv.includes('--index'));
  if (expectedSelection === 'explicit') assert.ok(delivered.argv.includes('--index'));
  const exported = JSON.parse(renderExport(workbench.session, 'json'));
  assert.deepEqual(exported.turns.at(-1).envelope, turn.envelope, 'complete JSON export retains every native field');
  observations.push({ text, selection: expectedSelection, delivery: delivered, turn: structuredClone(turn) });
}
async function readSibling(name, args, client = 'airk-workbench') {
  const before = deliveries.length;
  const adapter = new AirkctlToolSession(commands, () => evidenceSelection(spec, client, scope.active, scope.readRoute()),
    { windowReadScope: () => scope.retryScope(), windowReadFailed: code => scope.refused(code) });
  try {
    const result = await adapter.request('tools/call', { name, arguments: args }, 60000);
    const delivered = parsedDelivery(name, before);
    assert.deepEqual(result, JSON.parse(delivered.result.stdout));
    assert.notEqual(result.isError, true, JSON.stringify(result));
    const body = bodyOf(result);
    if (name === 'airk_search') assert.ok(Array.isArray(body.hits) && body.hits.length > 0, JSON.stringify(body));
    if (name === 'airk_symbols') assert.ok(Array.isArray(body.items) && body.items.some(item => item.name === 'workbench_fixture'), JSON.stringify(body));
    assert.ok(!delivered.argv.includes('--service-project'));
    observations.push({ client, name, arguments: args, delivery: delivered }); exactComparisons++;
  } finally { adapter.close(); }
}
async function main() {
  native.bind({ path: input.cli, sha256: input.cli_sha256, version: input.version });
  await native.control('health');
  assert.ok(native.hello.capabilities.includes('window-scope/1'), 'real W4 capability required');
  await scope.ensureDefault(input.project.binding);
  assert.equal(scope.active, true, JSON.stringify(scope.readRoute()));
  assert.equal(scope.readRoute().automatic, true, 'real automatic WindowScope owner obtained W4');
  await workbench.select(input.project.binding);
  await until(() => workbench.status !== undefined, 'initial native Workbench status');
  await runSlash('/status', 'window');
  await runSlash('/search workbenchneedle --mode lexical', 'window');
  await readSibling('airk_search', { query: 'workbenchneedle', mode: 'lexical', verification: 'off', response_mode: 'full' });
  await readSibling('airk_symbols', { name: 'workbench_fixture', compact: false, response_mode: 'full' });
  await readSibling('airk_search', { query: 'workbenchneedle', mode: 'lexical', verification: 'off', response_mode: 'full' }, 'airk-explorer');
  await readSibling('airk_symbols', { name: 'workbench_fixture', compact: false, response_mode: 'full' }, 'airk-explorer');
  const negative = await native.invoke(['tools', 'call', 'airk_search', '--args', '{"query":"workbenchneedle","mode":"lexical"}',
    '--service-project', input.project.binding, '--output', 'json'], { deadlineMs: 60000 });
  assert.notEqual(negative.class, 'success', 'reported legacy binding route remains refused');
  assert.match(JSON.stringify(negative), /scoped_read_unavailable/);
  observations.push({ negative_legacy_binding: negative });
  await scope.readSearchScope();
  await scope.applySearchScope({ index_ids: [input.selection.index_id, input.other_selection.index_id], project_ids: [] });
  assert.equal(scope.searchScopeSummary().automatic, undefined, 'applied scope replaces the automatic route');
  await runSlash('/status', 'window');
  await runSlash('/search workbenchneedle --mode lexical', 'window');
  await workbench.message({ type: 'openSelection', indexes: [input.selection.index_id], sets: [] });
  await runSlash('/status', 'explicit');
  await runSlash('/search workbenchneedle --mode lexical', 'explicit');
  fs.writeFileSync(input.output, JSON.stringify({ schema: 1, protocol: native.hello, exactComparisons,
    observations, source_before: input.source_hashes, host: 'actual Workbench callbacks; injected VS Code host; real native session' }, null, 2) + '\n');
  console.log(JSON.stringify({ tests: 1, passed: 1, exactComparisons, observations: observations.length, output: input.output }));
}
main().catch(error => { console.error(error.stack || error); process.exitCode = 1; }).finally(async () => {
  workbench.dispose(); setEvidenceTransport(undefined); native.dispose();
  await Promise.all(children.map(child => child.exitCode !== null || child.signalCode !== null ? undefined : once(child, 'exit', { signal: AbortSignal.timeout(10000) })));
});
