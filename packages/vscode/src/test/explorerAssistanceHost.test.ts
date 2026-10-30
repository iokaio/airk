// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { parseWindowDescriptor, WINDOW_SCHEMA } from '../explorerContracts';
import { AssistanceIpcHost, restrictWindowsDirectory, sweepExpired, windowsDirectory } from '../explorerAssistanceHost';
import { assistanceDirectory, LAUNCH_SCHEMA, launchDescriptorPath, serverInvocation, writeLaunchDescriptor } from '../explorerAssistanceLaunch';
import { SuggestionStore } from '../explorerSuggestions';
import { serviceTestHost } from './serviceTestHost';
import { contains } from '../explorerAssistance/attach';

const bundle = path.resolve(__dirname, '..', 'explorerAssistance.js');

function scratch(t: { after(fn: () => void): void }): string {
    const directory = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assist-host-')));
    t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
    return directory;
}

/** ui_status from the real bundle, run from cwd with the given assistance directory. */
async function uiStatus(cwd: string, directory: string): Promise<any> {
    const child = spawn(process.execPath, [bundle, '--assistance-dir', directory], { cwd, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
    child.stdin.end([
        JSON.stringify({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-11-25' } }),
        JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'airk_explorer_ui_status' } }),
    ].join('\n') + '\n');
    await new Promise(resolve => child.on('close', resolve));
    const reply = stdout.split('\n').filter(Boolean).map(line => JSON.parse(line)).find(entry => entry.id === 1);
    return { text: reply.result.content[0].text, ui: reply.result.structuredContent.ui };
}

function descriptorFiles(directory: string): string[] {
    try { return fs.readdirSync(windowsDirectory(directory)); } catch { return []; }
}

test('the launch descriptor names the bundled server by digest and is rewritten only when something changed', t => {
    const root = scratch(t), storage = path.join(root, 'storage'), extension = path.join(root, 'extension');
    fs.mkdirSync(path.join(extension, 'out'), { recursive: true });
    fs.writeFileSync(path.join(extension, 'out', 'explorerAssistance.js'), 'one');
    const runtime = { path: process.execPath, electron: true };
    assert.equal(writeLaunchDescriptor(storage, extension, '1.5.0', runtime), true);
    const file = launchDescriptorPath(storage);
    assert.equal(file, path.join(storage, 'explorer-assistance', 'launch-1.json'), 'the native forwarder accepts only this location');
    const descriptor = JSON.parse(fs.readFileSync(file, 'utf8'));
    assert.deepEqual(Object.keys(descriptor).sort(), ['extension_root', 'extension_version', 'runtime', 'schema', 'server', 'server_sha256', 'written_unix_ms']);
    assert.equal(descriptor.schema, LAUNCH_SCHEMA);
    assert.equal(descriptor.server, path.join(extension, 'out', 'explorerAssistance.js'));
    assert.equal(descriptor.server_sha256, createHash('sha256').update('one').digest('hex'));
    assert.deepEqual(descriptor.runtime, runtime);
    if (process.platform !== 'win32') { assert.equal(fs.statSync(file).mode & 0o077, 0); assert.equal(fs.statSync(path.dirname(file)).mode & 0o077, 0); }
    assert.equal(writeLaunchDescriptor(storage, extension, '1.5.0', runtime), false, 'unchanged inputs keep the existing file');
    fs.writeFileSync(path.join(extension, 'out', 'explorerAssistance.js'), 'two');
    assert.equal(writeLaunchDescriptor(storage, extension, '1.5.0', runtime), true);
    assert.equal(JSON.parse(fs.readFileSync(file, 'utf8')).server_sha256, createHash('sha256').update('two').digest('hex'));
    assert.deepEqual(fs.readdirSync(path.dirname(file)), ['launch-1.json'], 'no temporary files remain');

    const invocation = serverInvocation(extension, storage, runtime);
    assert.equal(invocation.command, process.execPath);
    assert.deepEqual(invocation.args, [path.join(extension, 'out', 'explorerAssistance.js'), '--assistance-dir', assistanceDirectory(storage)]);
    assert.deepEqual(invocation.env, { ELECTRON_RUN_AS_NODE: '1', NODE_OPTIONS: null, NODE_PATH: null });
    assert.equal(serverInvocation(extension, storage, { path: '/usr/bin/node', electron: false }).env.ELECTRON_RUN_AS_NODE, undefined);
});

test('a window attaches only for working directories inside its folders, and detaches on dispose', { timeout: 30000 }, async t => {
    assert.ok(fs.existsSync(bundle), 'compile builds out/explorerAssistance.js');
    const root = scratch(t), directory = path.join(root, 'assistance'), workspace = path.join(root, 'workspace'), elsewhere = path.join(root, 'elsewhere');
    fs.mkdirSync(path.join(workspace, 'src'), { recursive: true });
    fs.mkdirSync(elsewhere);
    let open = true;
    const host = await AssistanceIpcHost.start({ directory, version: '1.5.0', roots: [workspace], explorerOpen: () => open });
    assert.ok(host);
    t.after(() => host.dispose());
    const [file] = descriptorFiles(directory);
    const descriptor = parseWindowDescriptor(JSON.parse(fs.readFileSync(path.join(windowsDirectory(directory), file), 'utf8')));
    assert.ok(descriptor, 'the published descriptor satisfies the shared contract');
    assert.deepEqual(descriptor.roots, [workspace]);
    if (process.platform !== 'win32') { assert.equal(fs.statSync(path.join(windowsDirectory(directory), file)).mode & 0o077, 0); }

    const attached = await uiStatus(path.join(workspace, 'src'), directory);
    assert.deepEqual(attached.ui, { attached: true, binding_fingerprint: descriptor.binding_fingerprint, explorer_open: true, target_revisions: {} });
    assert.ok(!attached.text.includes(descriptor.credential) && !attached.text.includes(workspace.replaceAll('\\', '\\\\')), 'no credential or path reaches the agent');
    open = false;
    assert.equal((await uiStatus(workspace, directory)).ui.explorer_open, false);
    assert.deepEqual((await uiStatus(elsewhere, directory)).ui.code, 'UI_NOT_ATTACHED');

    // A crashed window leaves a descriptor whose endpoint no longer answers; it must not make the live window ambiguous.
    const stale = { ...descriptor, session_id: 'f'.repeat(32), endpoint: process.platform === 'win32' ? `\\\\.\\pipe\\airk-explorer-${'e'.repeat(32)}` : '/tmp/airk-explorer-gone/airk-explorer.sock' };
    fs.writeFileSync(path.join(windowsDirectory(directory), `${'f'.repeat(32)}.json`), JSON.stringify(stale));
    assert.equal((await uiStatus(workspace, directory)).ui.attached, true, 'only windows that answer count');
    fs.rmSync(path.join(windowsDirectory(directory), `${'f'.repeat(32)}.json`));
    const second = await AssistanceIpcHost.start({ directory, version: '1.5.0', roots: [workspace], explorerOpen: () => true });
    assert.equal((await uiStatus(workspace, directory)).ui.code, 'AMBIGUOUS_TARGET', 'two windows on one folder never guess');
    second!.dispose();
    assert.equal((await uiStatus(workspace, directory)).ui.attached, true);

    host.dispose();
    assert.deepEqual(descriptorFiles(directory), [], 'dispose withdraws the descriptor');
    assert.equal((await uiStatus(workspace, directory)).ui.code, 'UI_NOT_ATTACHED');
});

test('the endpoint answers only the current credential and only typed requests', { timeout: 30000 }, async t => {
    const root = scratch(t), directory = path.join(root, 'assistance');
    const host = (await AssistanceIpcHost.start({ directory, version: '1.5.0', roots: [root], explorerOpen: () => false }))!;
    t.after(() => host.dispose());
    const read = () => JSON.parse(fs.readFileSync(path.join(windowsDirectory(directory), descriptorFiles(directory)[0]), 'utf8'));
    const ask = (endpoint: string, line: string) => new Promise<string>(resolve => {
        let text = '';
        const socket = net.createConnection(endpoint, () => socket.write(line + '\n'));
        socket.setEncoding('utf8').on('data', chunk => { text += chunk; }).on('close', () => resolve(text)).on('error', () => resolve(text));
    });
    const first = read();
    assert.match(await ask(first.endpoint, JSON.stringify({ v: 1, credential: first.credential, type: 'status' })), /"ok":true/);
    assert.equal(await ask(first.endpoint, JSON.stringify({ v: 1, credential: '0'.repeat(64), type: 'status' })), '', 'a wrong credential gets no reply');
    assert.match(await ask(first.endpoint, JSON.stringify({ v: 1, credential: first.credential, type: 'run_command' })), /INVALID_SCHEMA/);
    assert.match(await ask(first.endpoint, JSON.stringify({ v: 2, credential: first.credential, type: 'status' })), /SCHEMA_UNSUPPORTED/);
    host.rotate();
    const rotated = read();
    assert.notEqual(rotated.credential, first.credential);
    assert.ok(rotated.expires_unix_ms >= first.expires_unix_ms);
    assert.equal(await ask(first.endpoint, JSON.stringify({ v: 1, credential: first.credential, type: 'status' })), '', 'a rotated credential is refused');
    assert.match(await ask(rotated.endpoint, JSON.stringify({ v: 1, credential: rotated.credential, type: 'status' })), /"ok":true/);
});

test('window descriptors are strict and stale ones are swept', t => {
    const root = scratch(t), directory = path.join(root, 'assistance');
    const valid = { schema: WINDOW_SCHEMA, session_id: 'a'.repeat(32), endpoint: process.platform === 'win32' ? `\\\\.\\pipe\\airk-explorer-${'b'.repeat(32)}` : '/tmp/airk-explorer-x/airk-explorer.sock',
        credential: 'c'.repeat(64), roots: [root], binding_fingerprint: 'd'.repeat(16), extension_version: '1.5.0', expires_unix_ms: Date.now() + 60000 };
    assert.ok(parseWindowDescriptor(valid));
    for (const patch of [{ extra: 1 }, { endpoint: 'tcp://127.0.0.1:80' }, { endpoint: '\\\\.\\pipe\\other' }, { roots: [] }, { credential: 'short' }, { schema: 'x/1' }]) {
        assert.equal(parseWindowDescriptor({ ...valid, ...patch }), undefined, JSON.stringify(patch));
    }
    fs.mkdirSync(windowsDirectory(directory), { recursive: true });
    const fresh = path.join(windowsDirectory(directory), `${'a'.repeat(32)}.json`), stale = path.join(windowsDirectory(directory), `${'e'.repeat(32)}.json`);
    fs.writeFileSync(fresh, JSON.stringify(valid));
    fs.writeFileSync(stale, JSON.stringify({ ...valid, session_id: 'e'.repeat(32), expires_unix_ms: Date.now() - 1 }));
    fs.writeFileSync(path.join(windowsDirectory(directory), 'unrelated.txt'), 'kept');
    sweepExpired(directory);
    assert.deepEqual(fs.readdirSync(windowsDirectory(directory)).sort(), [path.basename(fresh), 'unrelated.txt']);
});

test('trusted activation withdraws the old provider and attaches UI to one native endpoint; untrusted windows publish nothing', { timeout: 30000 }, async t => {
    const f = await serviceTestHost(t, { assistance: true });
    try {
        const provider = f.assistance.get('airk.explorerAssistance');
        assert.ok(provider, 'a separate provider id from airk.collections');
        assert.deepEqual(await provider.provideMcpServerDefinitions(),[],'retired provider withdraws cached legacy definitions');
        const [definition]=await f.api.getServerDefinitions();
        assert.equal(definition.label,'AIRK');assert.equal(definition.command,f.binding.launcher);
        assert.ok(definition.args.includes('--explorer-descriptor'));
        const directory = path.join(f.root, 'editor-storage', 'explorer-assistance');
        await assert.rejects(async()=>provider.resolveMcpServerDefinition(definition),/legacy_mcp_definition_retired/);
        await f.refreshFolders();
        await new Promise(resolve => setTimeout(resolve, 200));
        assert.equal(descriptorFiles(directory).length, 1, 'one descriptor per window, replaced when folders change');
        assert.ok(f.calls.some(line => line.startsWith('Explorer Assistance launch descriptor was not written')), 'a missing bundle is reported, not hidden');
        await f.close();
        assert.equal(fs.existsSync(windowsDirectory(directory)) ? descriptorFiles(directory).length : 0, 0);
    } catch (error) { await f.close(); throw error; }

    const untrusted = await serviceTestHost(t, { assistance: true, trusted: false });
    try {
        assert.deepEqual(await untrusted.assistance.get('airk.explorerAssistance').provideMcpServerDefinitions(), []);
        assert.equal(fs.existsSync(path.join(untrusted.root, 'editor-storage', 'explorer-assistance')), false);
    } finally { await untrusted.close(); }
});

/** Run several MCP calls against the real bundle from cwd and return structured results in order. */
async function calls(cwd: string, directory: string, requests: { name: string; args: object }[]): Promise<any[]> {
    const child = spawn(process.execPath, [bundle, '--assistance-dir', directory], { cwd, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
    child.stdin.end([JSON.stringify({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-11-25', clientInfo: { name: 'test-agent', version: '9' } } }),
        ...requests.map((request, index) => JSON.stringify({ jsonrpc: '2.0', id: index + 1, method: 'tools/call', params: { name: request.name, arguments: request.args } }))].join('\n') + '\n');
    await new Promise(resolve => child.on('close', resolve));
    const replies = stdout.split('\n').filter(Boolean).map(line => JSON.parse(line));
    return requests.map((_, index) => replies.find(entry => entry.id === index + 1).result.structuredContent);
}

test('drafts travel over the window endpoint into the Inbox store, idempotently, with the agent label (AT-05, AT-14)', { timeout: 30000 }, async t => {
    const root = scratch(t), directory = path.join(root, 'assistance');
    let fingerprint = '';
    const store = new SuggestionStore({ binding: () => fingerprint, revisions: () => ({ planner: 2 }), publication: () => undefined, ttlMs: () => 3600_000 });
    const host = (await AssistanceIpcHost.start({ directory, version: '1.5.0', roots: [root], explorerOpen: () => false, targetRevisions: () => ({ planner: 2 }),
        suggest: (kind, draft, client) => store.submit(kind, draft, client), proposalStatus: id => store.status(id) }))!;
    t.after(() => host.dispose());
    fingerprint = host.bindingFingerprint;
    const draft = { schema_version: '1.0', idempotency_key: 'renew-1', title: 'Find renewals', workspace_binding: fingerprint, draft: { query: 'session renew', path_prefix: 'src/' } };
    const [status, first, replay, conflict, stale] = await calls(root, directory, [
        { name: 'airk_explorer_ui_status', args: {} },
        { name: 'airk_explorer_suggest_search', args: draft },
        { name: 'airk_explorer_suggest_search', args: draft },
        { name: 'airk_explorer_suggest_search', args: { ...draft, title: 'Changed' } },
        { name: 'airk_explorer_suggest_plan', args: { schema_version: '1.0', idempotency_key: 'p', title: 'P', expected_target_revision: 1, tasks: [{ task_id: 'T1', title: 'a' }] } },
    ]);
    assert.deepEqual(status.ui.target_revisions, { planner: 2 });
    assert.equal(first.status, 'pending_review');
    assert.equal(first.executed, false);
    assert.equal(replay.proposal_id, first.proposal_id);
    assert.equal(conflict.error.code, 'PROPOSAL_CONFLICT');
    assert.equal(stale.error.code, 'PROPOSAL_CONFLICT', 'a stale planner revision is refused');
    assert.equal(store.list().length, 1);
    assert.equal(store.list()[0].client.name, 'test-agent', 'the Inbox shows the self-reported client');
    const body = store.list()[0].draft.body;
    assert.equal(body.kind === 'search' && body.search.query, 'session renew', 'nothing ran: the query only sits in the draft');
    store.resolve(first.proposal_id, 'applied');
    const [after] = await calls(root, directory, [{ name: 'airk_explorer_proposal_status', args: { proposal_id: first.proposal_id } }]);
    assert.deepEqual({ status: after.status, applied: after.applied, executed: after.executed }, { status: 'applied', applied: true, executed: false });
    assert.equal(after.query, undefined, 'status never returns the body');
});

test('native Windows namespace roots match editor paths without admitting sibling projects',()=>{
    assert.equal(contains('D:\\work\\Book','\\\\?\\D:\\work\\book\\src','win32'),true);
    assert.equal(contains('\\\\?\\D:\\work\\book','D:\\work\\Book','win32'),true);
    assert.equal(contains('D:\\work\\book','D:\\work\\book-other','win32'),false);
    assert.equal(contains('/work/book','/work/book-other','linux'),false);
});

test('Windows descriptor directories replace inherited profile entries with user, SYSTEM and Administrators', { skip: process.platform !== 'win32' }, () => {
    const calls: { file: string; args: readonly string[] }[] = [];
    const run = ((file: string, args: readonly string[]) => {
        calls.push({ file, args });
        return file.endsWith('whoami.exe') ? '"host\fixture","S-1-5-21-1-2-3-1001"\r\n' : '';
    }) as unknown as typeof import('node:child_process').execFileSync;
    const directory = path.join(os.tmpdir(), `airk-acl-fixture-${process.pid}`);
    restrictWindowsDirectory(directory, run);
    restrictWindowsDirectory(directory, run);
    assert.equal(calls.length, 2, 'each directory is restricted once per process');
    assert.match(calls[1].file, /icacls\.exe$/);
    assert.deepEqual(calls[1].args, [directory, '/inheritance:r', '/grant:r', '*S-1-5-21-1-2-3-1001:(OI)(CI)F', '*S-1-5-18:(OI)(CI)F', '*S-1-5-32-544:(OI)(CI)F']);
});
