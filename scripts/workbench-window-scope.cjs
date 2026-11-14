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
const { renderExport, evidenceCards } = require(path.join(out, 'workbenchRender'));
const {answerPartitions} = require(path.join(out,'workbenchAnswerNavigation'));
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
let exactComparisons = 0, answerComparisons = 0, answerSourceOpens = 0;
let copied; const editorOpens = [], exportsWritten = [];
const exportFile = path.join(input.owner, 'workbench-answer-export.json');
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
const vscode = { EventEmitter: Emitter, env: { clipboard: { writeText: async value => { copied = value; } } },
  ViewColumn: {Active: 1, Beside: 2}, Range: class { constructor(...coordinates) { this.coordinates = coordinates; } },
  workspace: { textDocuments: [], workspaceFolders: [{uri:{scheme:'file',fsPath:input.project.root}}],
    fs: {writeFile: async (uri, bytes) => { assert.equal(uri.fsPath, exportFile); fs.writeFileSync(uri.fsPath,bytes); exportsWritten.push(uri.fsPath); }}, getConfiguration: () => ({ get: (_key, fallback) => fallback, inspect: () => ({ globalValue: true }) }),
    onDidChangeConfiguration: () => ({ dispose() {} }) }, window: {showSaveDialog:async()=>({fsPath:exportFile}), showTextDocument:async(uri,options)=>{editorOpens.push({uri,options});}}, commands: { executeCommand: async () => undefined },
  Uri: { file: fsPath => ({ fsPath, scheme:'file' }), joinPath: (base, ...parts) => ({ fsPath: path.join(base.fsPath, ...parts) }) } };
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
async function runAnswer(expectedSelection) {
  const before = deliveries.length, original = structuredClone(input.answer_request);
  await workbench.message({type:'answer',question:original.question,budget:original.budget}); await workbench.saveQueue;
  const turn = workbench.session.turns.at(-1), delivered = parsedDelivery('airk_answer',before);
  assert.equal(turn.error,undefined,JSON.stringify(turn));
  assert.equal(turn.envelope?.schema,'airk.combined-read/1',JSON.stringify(turn.envelope));
  assert.equal(turn.envelope.context?.schema,'airk.combined-context/1');
  const partitions=answerPartitions(turn.envelope);
  assert.equal(partitions.length,1,'these automatic and explicit fixtures each select exactly one index');
  assert.equal(partitions[0].outcome,'complete');assert.equal(partitions[0].packet?.schema,'airk.arbiter.context/2');
  assert.deepEqual(turn.request,original,'Workbench uses the complete unchanged Rust-emitted current request');
  assert.deepEqual(JSON.parse(delivered.argv[delivered.argv.indexOf('--args')+1]),original,'real framed native request preserves every input field');
  assert.deepEqual(turn.envelope,nativeBody(delivered),'complete native /2 result including all work, history, provenance and records is retained');
  if(expectedSelection==='window')assert.ok(!delivered.argv.includes('--index')&&!delivered.argv.includes('--service-project'));
  else assert.ok(delivered.argv.includes('--index'));
  assert.ok(partitions[0].packet.records.length>0,'actual question search must retain canonical evidence');
  assert.ok(partitions[0].packet.records.some(record=>record.canonical_result?.hits?.length>0),'indexed exact Rust fixture question must return actual hits');
  await workbench.message({type:'copy',turn:turn.id,format:'json'});
  assert.deepEqual(JSON.parse(copied).turns[0],turn,'actual host copy preserves entire original turn/request/result');
  await workbench.message({type:'export',turn:turn.id,format:'json'});
  assert.equal(exportsWritten.at(-1),exportFile);
  assert.deepEqual(JSON.parse(fs.readFileSync(exportFile,'utf8')).turns[0],turn,'actual host export preserves entire original turn/request/result');
  const cards=evidenceCards(turn.envelope),card=cards.find(card=>card.step&&/^\/hits\/\d+$/.test(card.resultPointer));
  assert.ok(card,'actual native source verification must provide an unambiguous Answer citation');
  const sourceBefore=deliveries.length,openBefore=editorOpens.length;
  await workbench.message({type:'openAnswerCitation',turn:turn.id,record:card.step,hit:Number(card.resultPointer.split('/').at(-1)),member:card.answerMember});
  const info=parsedDelivery('airk_file_info',sourceBefore),args=JSON.parse(info.argv[info.argv.indexOf('--args')+1]);
  assert.equal(args.path,card.path);assert.equal(args.known_hash,card.hash);assert.equal(args.revision,card.publication);
  assert.deepEqual(args.index_ids,[card.indexId]);assert.deepEqual(args.project_ids,card.projectIds);
  assert.equal(editorOpens.length,openBefore+1,'native file_info and source/turn/window fences precede injected editor effect');
  const opened=editorOpens.at(-1);assert.ok(Object.hasOwn(input.source_hashes,opened.uri.fsPath),'source path belongs to this actual read-only fixture');
  assert.equal(digest(opened.uri.fsPath),card.hash);assert.equal(opened.options.selection.coordinates[0],card.start-1);
  observations.push({name:'airk_answer',selection:expectedSelection,request:original,delivery:delivered,turn:structuredClone(turn),source:{card,delivery:info,editor:opened},
    host_effect:'injected editor effect after real canonical file_info/source verification; not installed-editor qualification'});
  answerComparisons++; answerSourceOpens++;
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
  await runAnswer('window');
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
  await runAnswer('explicit');
  fs.writeFileSync(input.output, JSON.stringify({ schema: 1, protocol: native.hello, exactComparisons, answerComparisons, answerSourceOpens,
    observations, source_before: input.source_hashes, host: 'actual Workbench callbacks; injected VS Code host; real native session' }, null, 2) + '\n');
  console.log(JSON.stringify({ tests: 1, passed: 1, exactComparisons, observations: observations.length, output: input.output }));
}
main().catch(error => { console.error(error.stack || error); process.exitCode = 1; }).finally(async () => {
  workbench.dispose(); setEvidenceTransport(undefined); native.dispose();
  await Promise.all(children.map(child => child.exitCode !== null || child.signalCode !== null ? undefined : once(child, 'exit', { signal: AbortSignal.timeout(10000) })));
});
